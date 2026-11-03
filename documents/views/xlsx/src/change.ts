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
      heights?: Record<string, number>;
    }
  | { kind: "merges"; sheet: string; add?: Merge[]; remove?: Merge[] }
  | { kind: "group"; changes: Change[] };

export type Merge = [number, number, number, number];

/** A range of cells where a person is. */
export interface Place {
  sheet?: string;
  row?: number;
  column?: number;
  endRow?: number;
  endColumn?: number;
  /** The person's cell editor is open. */
  editing?: boolean;
}

/** Where a line goes when lines come in or go at `at`, or null when it goes. */
export function moveLine(line: number, at: number, count: number): number | null {
  if (line < at) return line;
  if (count > 0) return line + count;
  return line < at - count ? null : line + count;
}

/** Moves an unanswered edit across rows or columns that the document ordered first. */
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
  return change;
}

/** Whether a change adds or removes rows or columns. */
export function movesLines(change: Change): boolean {
  if (change.kind === "group") return change.changes.some(movesLines);
  return change.kind === "rows" || change.kind === "columns";
}

/** Moves a place across the lines that a change adds or removes, as its cells move. */
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

/** Whether a change writes this cell, moves its line or removes its sheet. */
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

/** Whether an unanswered edit holds this cell, so its value wins over the document's. */
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
