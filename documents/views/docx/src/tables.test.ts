import { expect, test } from "bun:test";
import { getSchema } from "@tiptap/core";
import type { Node as ProseNode } from "@tiptap/pm/model";
import { EditorState, TextSelection, type Transaction } from "@tiptap/pm/state";
import { extensions } from "./schema";
import { columnAdded, columnRemoved, rowAdded, rowRemoved, tableInserted } from "./tables";

const schema = getSchema(extensions);

interface Spec {
  text?: string;
  span?: number;
  merged?: boolean;
}

function paragraph(text = ""): ProseNode {
  return schema.node("paragraph", null, text ? [schema.text(text)] : []);
}

function tableOf(rows: Spec[][]): ProseNode {
  return schema.node(
    "table",
    null,
    rows.map((cells) =>
      schema.node(
        "tableRow",
        null,
        cells.map(({ text, span = 1, merged = false }) =>
          schema.node("tableCell", { span, merged }, [paragraph(text)]),
        ),
      ),
    ),
  );
}

/**
 * A by B over a cell merged down from A, over D across both columns:
 *
 *   | A | B |
 *   | ^ | C |
 *   |   D   |
 */
const MERGED = tableOf([
  [{ text: "A" }, { text: "B" }],
  [{ merged: true }, { text: "C" }],
  [{ text: "D", span: 2 }],
]);

function caretIn(table: ProseNode, words: string): EditorState {
  const doc = schema.node("doc", null, [table, paragraph()]);
  let caret = -1;
  doc.descendants((node, position) => {
    if (node.isText && node.text === words) caret = position;
    return caret < 0;
  });
  return EditorState.create({ doc, selection: TextSelection.create(doc, caret) });
}

function cellsOf(state: EditorState, change: Transaction | null): string[][] {
  const table = state.apply(change ?? state.tr).doc.child(0);
  const rows: string[][] = [];
  table.forEach((row) => {
    const cells: string[] = [];
    row.forEach((cell) => {
      const span = cell.attrs.span > 1 ? `/${cell.attrs.span}` : "";
      cells.push(`${cell.textContent}${span}${cell.attrs.merged ? "^" : ""}`);
    });
    rows.push(cells);
  });
  return rows;
}

test("a new row inside a merge goes on with it, and keeps the spans of its row", () => {
  const top = caretIn(MERGED, "A");
  expect(cellsOf(top, rowAdded(top, true))).toEqual([["A", "B"], ["^", ""], ["^", "C"], ["D/2"]]);
  const end = caretIn(MERGED, "C");
  expect(cellsOf(end, rowAdded(end, false))).toEqual([["A", "B"], ["^", ""], ["^", "C"], ["D/2"]]);
  expect(cellsOf(end, rowAdded(end, true))).toEqual([["A", "B"], ["^", "C"], ["", ""], ["D/2"]]);
  const wide = caretIn(MERGED, "D");
  expect(cellsOf(wide, rowAdded(wide, false))).toEqual([["A", "B"], ["^", "C"], ["/2"], ["D/2"]]);
});

test("a new column widens a cell that spans across it", () => {
  const first = caretIn(MERGED, "A");
  expect(cellsOf(first, columnAdded(first, true))).toEqual([
    ["A", "", "B"],
    ["^", "", "C"],
    ["D/3"],
  ]);
  const last = caretIn(MERGED, "B");
  expect(cellsOf(last, columnAdded(last, true))).toEqual([
    ["A", "B", ""],
    ["^", "C", ""],
    ["D/2", ""],
  ]);
  expect(cellsOf(first, columnAdded(first, false))).toEqual([
    ["", "A", "B"],
    ["", "^", "C"],
    ["", "D/2"],
  ]);
});

test("a row that starts a merge leaves its words and the start to the cell below", () => {
  const top = caretIn(MERGED, "A");
  expect(cellsOf(top, rowRemoved(top))).toEqual([["A", "C"], ["D/2"]]);
});

test("a removed column narrows a cell that spans it, and the last one takes the table", () => {
  const last = caretIn(MERGED, "B");
  expect(cellsOf(last, columnRemoved(last))).toEqual([["A"], ["^"], ["D"]]);
  const alone = caretIn(tableOf([[{ text: "A" }], [{ text: "B" }]]), "A");
  const doc = alone.apply(columnRemoved(alone) ?? alone.tr).doc;
  expect(doc.childCount).toBe(1);
  expect(doc.child(0).type.name).toBe("paragraph");
});

test("a new table goes between the halves of the caret's paragraph, with the caret in it", () => {
  const doc = schema.node("doc", null, [paragraph("Hello world")]);
  const middle = EditorState.create({
    doc,
    selection: TextSelection.create(doc, 1 + "Hello ".length),
  });
  const split = middle.apply(tableInserted(middle, 3, 2) ?? middle.tr);
  expect(split.doc.childCount).toBe(3);
  expect(split.doc.child(0).textContent).toBe("Hello ");
  expect(split.doc.child(1).childCount).toBe(2);
  expect(split.doc.child(1).child(0).childCount).toBe(3);
  expect(split.doc.child(2).textContent).toBe("world");
  expect(split.selection.$from.node(-1).type.name).toBe("tableCell");
  const end = EditorState.create({ doc, selection: TextSelection.atEnd(doc) });
  const after = end.apply(tableInserted(end, 1, 1) ?? end.tr).doc;
  expect(after.childCount).toBe(3);
  expect(after.child(2).type.name).toBe("paragraph");
});
