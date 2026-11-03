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

function moveLine(line: number, at: number, count: number): number | null {
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
