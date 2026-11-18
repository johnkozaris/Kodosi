import type { Look } from "./look";

export type Value = number | boolean | string | { error: string };

export interface Cell {
  value?: Value;
  formula?: string;
  style?: number;
}

export interface Sheet {
  id: string;
  name: string;
  rows: number;
  columns: number;
  cells: Record<string, Record<string, Cell>>;
  merges: [number, number, number, number][];
  widths: Record<string, number>;
  heights: Record<string, number>;
  hiddenRows: number[];
  hiddenColumns: number[];
  frozen: Frozen;
  links: { ranges: string; to: LinkTarget }[];
}

/** The rows and the columns that stay in sight: `rows` rows from `top`, `columns` from `left`. */
export interface Frozen {
  top: number;
  left: number;
  rows: number;
  columns: number;
}

export type LinkTarget = { href: string } | { location: string };

export interface SheetLink {
  sheet: string;
  row: number;
  column: number;
  endRow: number;
  endColumn: number;
  to: LinkTarget;
}

export function rangeOf(
  name: string,
): { row: number; column: number; endRow: number; endColumn: number } | null {
  const cells = name.replaceAll("$", "").split(":").map(cellOf);
  const [first, last = first] = cells;
  if (!first || !last || cells.length > 2) return null;
  return {
    row: Math.min(first.row, last.row),
    column: Math.min(first.column, last.column),
    endRow: Math.max(first.row, last.row),
    endColumn: Math.max(first.column, last.column),
  };
}

function cellOf(name: string): { row: number; column: number } | null {
  const found = /^([A-Za-z]{1,3})([1-9][0-9]*)$/.exec(name);
  if (!found) return null;
  const column = [...(found[1] ?? "").toUpperCase()].reduce(
    (sum, letter) => sum * 26 + letter.charCodeAt(0) - 64,
    0,
  );
  return { row: Number(found[2]) - 1, column: column - 1 };
}

export function linksOf(sheet: Sheet): SheetLink[] {
  return sheet.links.flatMap(({ ranges, to }) =>
    ranges.split(" ").flatMap((name) => {
      const range = rangeOf(name);
      return range ? [{ sheet: sheet.id, ...range, to }] : [];
    }),
  );
}

export function linkAt(
  links: readonly SheetLink[],
  sheet: string,
  row: number,
  column: number,
): SheetLink | undefined {
  return links.find(
    (link) =>
      link.sheet === sheet &&
      row >= link.row &&
      row <= link.endRow &&
      column >= link.column &&
      column <= link.endColumn,
  );
}

export function moveLink(link: SheetLink, change: Change): SheetLink | null {
  if (change.kind === "group") {
    return change.changes.reduce<SheetLink | null>(
      (moved, inner) => (moved ? moveLink(moved, inner) : null),
      link,
    );
  }
  if (change.kind === "removeSheet") return change.sheet === link.sheet ? null : link;
  if ((change.kind !== "rows" && change.kind !== "columns") || change.sheet !== link.sheet) {
    return link;
  }
  const [start, end] =
    change.kind === "rows" ? (["row", "endRow"] as const) : (["column", "endColumn"] as const);
  const first = moveLine(link[start], change.at, change.count);
  const last = moveLine(link[end], change.at, change.count);
  if (first === null && last === null) return null;
  return { ...link, [start]: first ?? change.at, [end]: last ?? change.at - 1 };
}

export interface Snapshot {
  sheets: Sheet[];
  looks: Look[];
}

export interface CellChange {
  row: number;
  column: number;
  value?: Value | null;
  formula?: string | null;
  style?: number | null;
  look?: Look;
}

type Line = "rows" | "columns";

export type Change =
  | { kind: "cells"; sheet: string; cells: CellChange[] }
  | { kind: Line; sheet: string; at: number; count: number }
  | { kind: "addSheet"; id: string; name: string }
  | { kind: "renameSheet"; sheet: string; name: string }
  | { kind: "removeSheet"; sheet: string }
  | {
      kind: "size";
      sheet: string;
      widths?: Record<string, number>;
      /** `null` makes a row fit what it holds. */
      heights?: Record<string, number | null>;
    }
  | { kind: "merges"; sheet: string; add?: Merge[]; remove?: Merge[] }
  | {
      kind: "hidden";
      sheet: string;
      rows?: Record<string, boolean>;
      columns?: Record<string, boolean>;
    }
  | ({ kind: "freeze"; sheet: string } & Frozen)
  | { kind: "group"; changes: Change[] };

export type Merge = [number, number, number, number];

export interface Place {
  sheet?: string;
  row?: number;
  column?: number;
  endRow?: number;
  endColumn?: number;
  editing?: boolean;
}

export function moveLine(line: number, at: number, count: number): number | null {
  if (line < at) return line;
  if (count > 0) return line + count;
  return line < at - count ? null : line + count;
}

/**
 * Lines that come in before the first line that scrolls join the frozen ones, and lines that go
 * before it leave them, though one stays. Univer moves the panes this way for a person's own
 * change, and the document moves them alike.
 */
export function moveFrozen(frozen: Frozen, kind: Line, at: number, count: number): Frozen {
  const [start, lines] =
    kind === "rows" ? (["top", "rows"] as const) : (["left", "columns"] as const);
  const end = frozen[start] + frozen[lines];
  if (frozen[lines] === 0 || at >= end) return frozen;
  if (count > 0) return { ...frozen, [lines]: frozen[lines] + count };
  const gone = Math.min(end, at - count) - at;
  const kept = Math.max(frozen[lines] - gone, 1);
  return { ...frozen, [start]: Math.max(end - gone, 1) - kept, [lines]: kept };
}

function moveKeys<V>(record: Record<string, V>, at: number, count: number): Record<string, V> {
  return Object.fromEntries(
    Object.entries(record).flatMap(([line, value]) => {
      const moved = moveLine(Number(line), at, count);
      return moved === null ? [] : [[String(moved), value]];
    }),
  );
}

export function moveChange(
  change: Change,
  sheet: string,
  kind: Line,
  at: number,
  count: number,
): Change | null {
  if (change.kind === "group") {
    const changes = change.changes.flatMap(
      (inner) => moveChange(inner, sheet, kind, at, count) ?? [],
    );
    return changes.length > 0 ? { kind: "group", changes } : null;
  }
  if (!("sheet" in change) || change.sheet !== sheet) return change;
  if (change.kind === "cells") {
    const key = kind === "rows" ? "row" : "column";
    const cells = change.cells.flatMap((cell) => {
      const line = moveLine(cell[key], at, count);
      return line === null ? [] : [{ ...cell, [key]: line }];
    });
    return cells.length > 0 ? { ...change, cells } : null;
  }
  if (change.kind === kind) {
    const moved = moveLine(change.at, at, count);
    return moved === null ? null : { ...change, at: moved };
  }
  if (change.kind === "size") {
    const key = kind === "rows" ? "heights" : "widths";
    const sizes = change[key];
    return sizes ? { ...change, [key]: moveKeys(sizes, at, count) } : change;
  }
  if (change.kind === "hidden") {
    const lines = change[kind];
    if (!lines) return change;
    const moved = { ...change, [kind]: moveKeys(lines, at, count) };
    const left = Object.keys(moved.rows ?? {}).length + Object.keys(moved.columns ?? {}).length;
    return left > 0 ? moved : null;
  }
  if (change.kind === "freeze") return { ...change, ...moveFrozen(change, kind, at, count) };
  return change;
}

/**
 * Waiting changes go on only to sheets that the workbook holds, in their order, with the sheets
 * that they add and remove themselves. A workbook keeps its last sheet.
 */
export class SheetFitting {
  private readonly held: Set<string>;
  /** Whether an edit went because its sheet is gone. */
  lost = false;

  constructor(sheets: Iterable<string>) {
    this.held = new Set(sheets);
  }

  fit(change: Change): Change | null {
    switch (change.kind) {
      case "group": {
        const changes = change.changes.flatMap((inner) => this.fit(inner) ?? []);
        return changes.length > 0 ? { kind: "group", changes } : null;
      }
      case "addSheet":
        if (this.held.has(change.id)) return null;
        this.held.add(change.id);
        return change;
      case "removeSheet":
        if (!this.held.has(change.sheet)) return null;
        if (this.held.size === 1) return this.gone();
        this.held.delete(change.sheet);
        return change;
      default:
        return this.held.has(change.sheet) ? change : this.gone();
    }
  }

  private gone(): null {
    this.lost = true;
    return null;
  }
}

export function withoutSheet(change: Change, sheet: string): Change | null {
  if (change.kind === "group") {
    const changes = change.changes.flatMap((inner) => withoutSheet(inner, sheet) ?? []);
    return changes.length > 0 ? { kind: "group", changes } : null;
  }
  return "sheet" in change && change.sheet === sheet ? null : change;
}

export function movesLines(change: Change): boolean {
  if (change.kind === "group") return change.changes.some(movesLines);
  return change.kind === "rows" || change.kind === "columns";
}

export function movePlace(place: Place, change: Change): Place {
  if (change.kind === "group") {
    return change.changes.reduce((moved, inner) => movePlace(moved, inner), place);
  }
  if ((change.kind !== "rows" && change.kind !== "columns") || change.sheet !== place.sheet) {
    return place;
  }
  const keys =
    change.kind === "rows" ? (["row", "endRow"] as const) : (["column", "endColumn"] as const);
  const moved = { ...place };
  for (const key of keys) {
    const line = place[key];
    if (line !== undefined) moved[key] = moveLine(line, change.at, change.count) ?? change.at;
  }
  return moved;
}

export function touches(change: Change, sheet: string, row: number, column: number): boolean {
  switch (change.kind) {
    case "group":
      return change.changes.some((inner) => touches(inner, sheet, row, column));
    case "cells":
      return covers(change, sheet, row, column);
    case "rows":
      return change.sheet === sheet && moveLine(row, change.at, change.count) !== row;
    case "columns":
      return change.sheet === sheet && moveLine(column, change.at, change.count) !== column;
    case "removeSheet":
      return change.sheet === sheet;
    default:
      return false;
  }
}

/** Whether an unanswered change hides or shows a line, which then stays as that change has it. */
export function hides(change: Change, sheet: string, kind: Line, line: number): boolean {
  if (change.kind === "group") {
    return change.changes.some((inner) => hides(inner, sheet, kind, line));
  }
  return change.kind === "hidden" && change.sheet === sheet && String(line) in (change[kind] ?? {});
}

/** Whether an unanswered change sets the panes of a sheet, which then stay as it has them. */
export function freezes(change: Change, sheet: string): boolean {
  if (change.kind === "group") return change.changes.some((inner) => freezes(inner, sheet));
  return change.kind === "freeze" && change.sheet === sheet;
}

/** The value of an unanswered edit wins over the document's. */
export function covers(change: Change, sheet: string, row: number, column: number): boolean {
  if (change.kind === "group") {
    return change.changes.some((inner) => covers(inner, sheet, row, column));
  }
  return (
    change.kind === "cells" &&
    change.sheet === sheet &&
    change.cells.some((cell) => cell.row === row && cell.column === column)
  );
}
