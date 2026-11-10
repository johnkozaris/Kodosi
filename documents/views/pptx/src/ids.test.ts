import { expect, test } from "bun:test";
import { newId } from "./ids";

test("a new id is a number that a file can hold and none of the slide's", () => {
  const draws = [0, 0.5];
  const taken = ["256", "257"];
  const id = newId(taken, () => draws.shift() ?? 0.5);
  expect(id).toMatch(/^\d+$/);
  expect(taken).not.toContain(id);
  expect(Number(id)).toBeGreaterThanOrEqual(256);
  expect(Number(id)).toBeLessThanOrEqual(2_147_483_647);
});
