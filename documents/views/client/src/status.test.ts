import { expect, test } from "bun:test";
import { colorOf, initialsOf } from "./status";

test("a person has the color and the initials that the apps give them", () => {
  expect(colorOf("ana")).toBe("#2f92a3");
  expect(colorOf("maya")).toBe("#b362b6");
  expect(colorOf("Ünïcode 名前")).toBe("#2f92a3");
  expect(colorOf("0193b7f2-1c4e-7d6a-9e3b-5a2c8f1d4e60")).toBe("#3c8db5");
  expect(initialsOf("  Ana   Test ")).toBe("AT");
  expect(initialsOf("maya")).toBe("M");
  expect(initialsOf("")).toBe("·");
});
