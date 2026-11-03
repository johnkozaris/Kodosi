import { expect, test } from "bun:test";
import { type Change, movePlace, touches } from "./change";

const sheet = "1";

test("a change waits for an open cell editor only when it writes that cell or moves its line", () => {
  const cell = (row: number, column: number): Change => ({
    kind: "cells",
    sheet,
    cells: [{ row, column }],
  });
  expect(touches(cell(4, 3), sheet, 4, 3)).toBe(true);
  expect(touches(cell(4, 2), sheet, 4, 3)).toBe(false);
  expect(touches({ kind: "rows", sheet, at: 2, count: 1 }, sheet, 4, 3)).toBe(true);
  expect(touches({ kind: "rows", sheet, at: 5, count: 1 }, sheet, 4, 3)).toBe(false);
  expect(touches({ kind: "rows", sheet, at: 4, count: -1 }, sheet, 4, 3)).toBe(true);
  expect(touches({ kind: "columns", sheet, at: 4, count: 2 }, sheet, 4, 3)).toBe(false);
  expect(touches({ kind: "rows", sheet: "2", at: 0, count: 1 }, sheet, 4, 3)).toBe(false);
  expect(touches({ kind: "group", changes: [cell(0, 0), cell(4, 3)] }, sheet, 4, 3)).toBe(true);
  expect(touches({ kind: "removeSheet", sheet }, sheet, 4, 3)).toBe(true);
});

test("a place moves with the rows that come in above it", () => {
  const place = { sheet, row: 4, column: 3, endRow: 5, endColumn: 3, editing: true };
  const inserted: Change = { kind: "rows", sheet, at: 2, count: 1 };
  expect(movePlace(place, inserted)).toEqual({ ...place, row: 5, endRow: 6 });
  const removed: Change = { kind: "rows", sheet, at: 4, count: -1 };
  expect(movePlace(place, removed)).toEqual({ ...place, row: 4, endRow: 4 });
  const columns: Change = { kind: "group", changes: [{ kind: "columns", sheet, at: 0, count: 2 }] };
  expect(movePlace(place, columns)).toEqual({ ...place, column: 5, endColumn: 5 });
});
