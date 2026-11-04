import { expect, test } from "bun:test";
import { stepFrom } from "./zoom";

test("the zoom keys step to the next scale, also from a scale that fits the view", () => {
  expect(stepFrom(1, true)).toBe(1.1);
  expect(stepFrom(1, false)).toBe(0.9);
  expect(stepFrom(0.7329, true)).toBe(0.75);
  expect(stepFrom(0.7329, false)).toBe(0.67);
  expect(stepFrom(0.6705, false)).toBe(0.5);
  expect(stepFrom(0.18, true)).toBe(0.25);
  expect(stepFrom(0.25, false)).toBeUndefined();
  expect(stepFrom(4, true)).toBeUndefined();
});
