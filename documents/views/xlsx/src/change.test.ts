import { expect, test } from "bun:test";
import {
  type Change,
  freezes,
  hides,
  linkAt,
  linksOf,
  moveChange,
  moveFrozen,
  moveLink,
  movePlace,
  rangeOf,
  SheetFitting,
  touches,
  withoutSheet,
} from "./change";

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

test("a cell name names its cells from zero", () => {
  expect(rangeOf("A1")).toEqual({ row: 0, column: 0, endRow: 0, endColumn: 0 });
  expect(rangeOf("$C$4:B2")).toEqual({ row: 1, column: 1, endRow: 3, endColumn: 2 });
  expect(rangeOf("AA10")).toEqual({ row: 9, column: 26, endRow: 9, endColumn: 26 });
  expect(rangeOf("Data!A1")).toBeNull();
  expect(rangeOf("A0")).toBeNull();
});

test("a link belongs to its cells and moves with them", () => {
  const [web, place] = linksOf({
    id: sheet,
    name: "Data",
    rows: 4,
    columns: 2,
    cells: {},
    merges: [],
    widths: {},
    heights: {},
    hiddenRows: [],
    hiddenColumns: [],
    frozen: { top: 0, left: 0, rows: 0, columns: 0 },
    links: [
      { ranges: "A1", to: { href: "https://kodosi.com/docs" } },
      { ranges: "B2:B4", to: { location: "'Data'!A3" } },
    ],
  });
  if (!web || !place) throw new Error("two links");
  const links = [web, place];
  expect(linkAt(links, sheet, 0, 0)?.to).toEqual({ href: "https://kodosi.com/docs" });
  expect(linkAt(links, sheet, 2, 1)?.to).toEqual({ location: "'Data'!A3" });
  expect(linkAt(links, sheet, 0, 1)).toBeUndefined();
  expect(linkAt(links, "2", 0, 0)).toBeUndefined();

  expect(moveLink(web, { kind: "rows", sheet, at: 0, count: 2 })?.row).toBe(2);
  expect(moveLink(web, { kind: "rows", sheet, at: 0, count: -1 })).toBeNull();
  expect(moveLink(place, { kind: "rows", sheet, at: 2, count: -1 })).toMatchObject({
    row: 1,
    endRow: 2,
  });
  expect(moveLink(place, { kind: "rows", sheet, at: 0, count: -2 })).toMatchObject({
    row: 0,
    endRow: 1,
  });
  expect(moveLink(place, { kind: "columns", sheet: "2", at: 0, count: 1 })).toBe(place);
  expect(moveLink(place, { kind: "removeSheet", sheet })).toBeNull();
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

test("lines that come or go before frozen panes move them as the document does", () => {
  const frozen = { top: 3, left: 1, rows: 2, columns: 1 };
  expect(moveFrozen(frozen, "rows", 5, 4)).toEqual(frozen);
  const grown = moveFrozen(frozen, "rows", 4, 2);
  expect(grown).toEqual({ ...frozen, rows: 4 });
  const shrunk = moveFrozen(grown, "rows", 0, -2);
  expect(shrunk).toEqual({ ...frozen, rows: 2 });
  const kept = moveFrozen(shrunk, "rows", 2, -10);
  expect(kept).toEqual({ ...frozen, top: 1, rows: 1 });
  expect(moveFrozen(kept, "columns", 0, 3)).toEqual({ ...kept, columns: 4 });
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

test("an unanswered change keeps the lines that it hides or shows and the panes that it sets", () => {
  const waiting: Change = {
    kind: "group",
    changes: [
      { kind: "hidden", sheet, rows: { "4": false } },
      { kind: "freeze", sheet, top: 0, left: 0, rows: 1, columns: 0 },
    ],
  };
  expect(hides(waiting, sheet, "rows", 4)).toBe(true);
  expect(hides(waiting, sheet, "rows", 5)).toBe(false);
  expect(hides(waiting, sheet, "columns", 4)).toBe(false);
  expect(hides(waiting, "2", "rows", 4)).toBe(false);
  expect(freezes(waiting, sheet)).toBe(true);
  expect(freezes(waiting, "2")).toBe(false);
});
