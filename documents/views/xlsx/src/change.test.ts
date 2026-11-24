import { expect, test } from "bun:test";
import { type Change, moveChange, SheetFitting, touches, withoutSheet } from "./change";

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

test("waiting changes fit the sheets that the document holds, in their order", () => {
  const fitting = new SheetFitting(["1", "2"]);
  const row = (sheet: string): Change => ({ kind: "rows", sheet, at: 3, count: 1 });
  const waiting: Change[] = [
    { kind: "group", changes: [row("1"), row("gone")] },
    { kind: "merges", sheet: "gone", add: [[0, 0, 1, 1]] },
    { kind: "addSheet", id: "3", name: "Notes" },
    { kind: "cells", sheet: "3", cells: [{ row: 0, column: 0, value: 1 }] },
    { kind: "removeSheet", sheet: "2" },
    row("2"),
  ];
  expect(waiting.map((change) => fitting.fit(change))).toEqual([
    { kind: "group", changes: [row("1")] },
    null,
    waiting[2] as Change,
    waiting[3] as Change,
    waiting[4] as Change,
    null,
  ]);
  expect(fitting.lost).toBe(true);
});

test("a sheet that is in already or gone already takes no change, and the last sheet stays", () => {
  const fitting = new SheetFitting(["1", "2"]);
  expect(fitting.fit({ kind: "addSheet", id: "2", name: "Again" })).toBeNull();
  expect(fitting.fit({ kind: "removeSheet", sheet: "gone" })).toBeNull();
  expect(fitting.lost).toBe(false);
  expect(fitting.fit({ kind: "removeSheet", sheet: "1" })).not.toBeNull();
  expect(fitting.fit({ kind: "removeSheet", sheet: "2" })).toBeNull();
  expect(fitting.lost).toBe(true);
});

test("what waits for a sheet that another person removed goes with it", () => {
  const cells: Change = { kind: "cells", sheet: "2", cells: [{ row: 0, column: 0, value: 1 }] };
  const added: Change = { kind: "addSheet", id: "3", name: "Notes" };
  expect(withoutSheet({ kind: "group", changes: [cells, added] }, "2")).toEqual({
    kind: "group",
    changes: [added],
  });
  expect(withoutSheet({ kind: "removeSheet", sheet: "2" }, "2")).toBeNull();
  expect(withoutSheet(cells, "1")).toBe(cells);
});

test("waiting sizes, hidden lines and panes move with the lines that others insert or delete", () => {
  const hidden: Change = {
    kind: "hidden",
    sheet,
    rows: { "2": true, "6": false },
    columns: { "1": true },
  };
  expect(moveChange(hidden, sheet, "rows", 0, 2)).toEqual({
    ...hidden,
    rows: { "4": true, "8": false },
  });
  expect(moveChange(hidden, sheet, "rows", 2, -1)).toEqual({ ...hidden, rows: { "5": false } });
  expect(moveChange(hidden, "2", "rows", 0, 2)).toBe(hidden);
  expect(
    moveChange({ kind: "hidden", sheet, columns: { "1": true } }, sheet, "columns", 1, -1),
  ).toBeNull();
  const sizes: Change = {
    kind: "size",
    sheet,
    widths: { "3": 20 },
    heights: { "2": 30, "5": null },
  };
  expect(moveChange(sizes, sheet, "rows", 3, -1)).toEqual({
    ...sizes,
    heights: { "2": 30, "4": null },
  });
  const panes: Change = { kind: "freeze", sheet, top: 0, left: 0, rows: 1, columns: 0 };
  expect(moveChange(panes, sheet, "rows", 0, 1)).toEqual({ ...panes, rows: 2 });
});
