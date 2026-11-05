import { expect, test } from "bun:test";
import { type Change, linkAt, linksOf, moveLink, movePlace, rangeOf, touches } from "./change";

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
