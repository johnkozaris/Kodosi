import type { Node as ProseNode, Schema } from "@tiptap/pm/model";
import { type EditorState, TextSelection, type Transaction } from "@tiptap/pm/state";

/** A cell, with its first column in the grid of its table and the columns that it spans. */
interface Cell {
  node: ProseNode;
  pos: number;
  start: number;
  span: number;
}

interface Row {
  node: ProseNode;
  pos: number;
  cells: Cell[];
}

/** The table that holds the caret, its rows, and the row and the cell of the caret. */
interface Around {
  table: ProseNode;
  pos: number;
  rows: Row[];
  row: number;
  cell: Cell;
}

function typesOf(schema: Schema) {
  const { table, tableRow, tableCell, paragraph } = schema.nodes;
  return table && tableRow && tableCell && paragraph
    ? { table, tableRow, tableCell, paragraph }
    : null;
}

function cellDepth(state: EditorState): number {
  const { $from } = state.selection;
  for (let depth = $from.depth; depth > 0; depth--) {
    if ($from.node(depth).type.name === "tableCell") return depth;
  }
  return 0;
}

export function inTable(state: EditorState): boolean {
  return cellDepth(state) > 0;
}

function around(state: EditorState): Around | null {
  const depth = cellDepth(state);
  if (depth === 0) return null;
  const { $from } = state.selection;
  const table = $from.node(depth - 2);
  const pos = $from.before(depth - 2);
  const rows: Row[] = [];
  table.forEach((row, rowOffset) => {
    const rowPos = pos + 1 + rowOffset;
    const cells: Cell[] = [];
    let start = 0;
    row.forEach((cell, cellOffset) => {
      const span = Math.max(1, Number(cell.attrs.span) || 1);
      cells.push({ node: cell, pos: rowPos + 1 + cellOffset, start, span });
      start += span;
    });
    rows.push({ node: row, pos: rowPos, cells });
  });
  const row = $from.index(depth - 2);
  const cell = rows[row]?.cells[$from.index(depth - 1)];
  return cell ? { table, pos, rows, row, cell } : null;
}

function cellAt(row: Row | undefined, start: number): Cell | undefined {
  return row?.cells.find((cell) => cell.start === start);
}

function merged(cell: Cell | undefined): boolean {
  return cell?.node.attrs.merged === true;
}

/** A new empty cell with the look of another, and its paragraph with the look of the other's. */
function cellLike(schema: Schema, like: Cell, span: number, inMerge: boolean): ProseNode | null {
  const types = typesOf(schema);
  if (!types) return null;
  const first = like.node.firstChild;
  const look =
    first?.type === types.paragraph
      ? {
          style: first.attrs.style,
          align: first.attrs.align,
          line: first.attrs.line,
          indent: first.attrs.indent,
        }
      : null;
  return types.tableCell.create(
    { props: like.node.attrs.props, span, merged: inMerge },
    types.paragraph.create(look),
  );
}

/** A row above or below the caret's, with a cell for each of its cells, inside the merges that go on across it. */
export function rowAdded(state: EditorState, below: boolean): Transaction | null {
  const at = around(state);
  const row = at?.rows[at.row];
  if (!at || !row) return null;
  const next = at.rows[at.row + 1];
  const cells: ProseNode[] = [];
  for (const cell of row.cells) {
    const inMerge = below ? merged(cellAt(next, cell.start)) : merged(cell);
    const added = cellLike(state.schema, cell, cell.span, inMerge);
    if (added) cells.push(added);
  }
  const added = row.node.type.create(row.node.attrs, cells);
  return state.tr.insert(below ? row.pos + row.node.nodeSize : row.pos, added);
}

/**
 * A column to the left or the right of the caret's cell. A cell that spans across the new
 * column spans it too.
 */
export function columnAdded(state: EditorState, right: boolean): Transaction | null {
  const at = around(state);
  if (!at) return null;
  const column = right ? at.cell.start + at.cell.span : at.cell.start;
  const tr = state.tr;
  for (const row of [...at.rows].reverse()) {
    const across = row.cells.find((cell) => cell.start < column && column < cell.start + cell.span);
    if (across) {
      tr.setNodeMarkup(across.pos, undefined, { ...across.node.attrs, span: across.span + 1 });
      continue;
    }
    const after = row.cells.find((cell) => cell.start >= column);
    const like = right ? (row.cells.findLast((cell) => cell.start < column) ?? after) : after;
    const cell = cellLike(state.schema, like ?? row.cells.at(-1) ?? at.cell, 1, false);
    if (cell) tr.insert(after ? after.pos : row.pos + row.node.nodeSize - 1, cell);
  }
  return tr;
}

/**
 * Takes away the caret's row. A cell of it that starts a merge gives its words and the start
 * to the cell below it.
 */
export function rowRemoved(state: EditorState): Transaction | null {
  const at = around(state);
  const row = at?.rows[at.row];
  if (!at || !row) return null;
  if (at.rows.length === 1) return tableRemoved(state);
  const next = at.rows[at.row + 1];
  const tr = state.tr;
  for (const cell of [...row.cells].reverse()) {
    const below = cellAt(next, cell.start);
    if (merged(cell) || !below || !merged(below)) continue;
    const starts = below.node.type.create(
      { ...below.node.attrs, merged: false },
      cell.node.content,
    );
    tr.replaceWith(below.pos, below.pos + below.node.nodeSize, starts);
  }
  return tr.delete(row.pos, row.pos + row.node.nodeSize);
}

/** Takes away the columns of the caret's cell. A cell that spans more keeps the rest. */
export function columnRemoved(state: EditorState): Transaction | null {
  const at = around(state);
  if (!at) return null;
  const from = at.cell.start;
  const to = at.cell.start + at.cell.span;
  const emptied = at.rows.map((row) =>
    row.cells.every((cell) => cell.start >= from && cell.start + cell.span <= to),
  );
  if (emptied.every(Boolean)) return tableRemoved(state);
  const tr = state.tr;
  for (const [index, row] of [...at.rows.entries()].reverse()) {
    if (emptied[index]) {
      tr.delete(row.pos, row.pos + row.node.nodeSize);
      continue;
    }
    for (const cell of [...row.cells].reverse()) {
      const shared = Math.min(cell.start + cell.span, to) - Math.max(cell.start, from);
      if (shared <= 0) continue;
      if (shared >= cell.span) tr.delete(cell.pos, cell.pos + cell.node.nodeSize);
      else tr.setNodeMarkup(cell.pos, undefined, { ...cell.node.attrs, span: cell.span - shared });
    }
  }
  return tr;
}

/** Takes away the table that holds the caret. A place that needs a block keeps a paragraph. */
export function tableRemoved(state: EditorState): Transaction | null {
  const at = around(state);
  const paragraph = state.schema.nodes.paragraph;
  if (!at || !paragraph) return null;
  const tr = state.tr;
  const end = at.pos + at.table.nodeSize;
  if (state.doc.resolve(at.pos).parent.childCount === 1) {
    tr.replaceWith(at.pos, end, paragraph.create());
  } else {
    tr.delete(at.pos, end);
  }
  return tr.setSelection(TextSelection.near(tr.doc.resolve(at.pos)));
}

/**
 * A new table of empty cells at the caret, between the two halves of its paragraph, with a
 * paragraph after it to go on writing. The caret goes into its first cell.
 */
export function tableInserted(
  state: EditorState,
  columns: number,
  rows: number,
): Transaction | null {
  const types = typesOf(state.schema);
  if (!types || !(state.selection instanceof TextSelection)) return null;
  const tr = state.tr.deleteSelection();
  const $at = tr.selection.$from;
  if (!$at.parent.isTextblock) return null;
  let at = $at.after();
  if ($at.parentOffset === 0) {
    at = $at.before();
  } else if ($at.parentOffset < $at.parent.content.size) {
    tr.split($at.pos);
    at = $at.pos + 1;
  }
  const row = () =>
    types.tableRow.create(
      null,
      Array.from({ length: columns }, () => types.tableCell.create(null, types.paragraph.create())),
    );
  const table = types.table.create(null, Array.from({ length: rows }, row));
  const next = tr.doc.resolve(at).nodeAfter;
  tr.insert(at, next?.type === types.paragraph ? table : [table, types.paragraph.create()]);
  return tr.setSelection(TextSelection.create(tr.doc, at + 4));
}
