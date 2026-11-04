import { expect, test } from "bun:test";
import { opensLink, passesOn } from "./links";

test("a link goes to the host unless it would run in the page", () => {
  for (const address of [
    "https://kodosi.com/docs",
    "mailto:ana@example.com",
    "http://localhost:3000/",
    "vscode://file/home/ana/main.rs",
    "file:///home/ana/plan.md",
  ]) {
    expect(passesOn(address)).toBe(true);
  }
  for (const address of [
    "javascript:alert(1)",
    "JavaScript:alert(1)",
    "data:text/html,<script>alert(1)</script>",
    "blob:https://kodosi.com/1",
    "about:blank",
    "plan.md",
    "",
  ]) {
    expect(passesOn(address)).toBe(false);
  }
});

test("a link opens on a click where a person only looks, and with a key where they edit", () => {
  const plain = { metaKey: false, ctrlKey: false };
  expect(opensLink(plain, false, false)).toBe(true);
  expect(opensLink(plain, true, false)).toBe(false);
  expect(opensLink({ ...plain, metaKey: true }, true, false)).toBe(true);
  expect(opensLink({ ...plain, ctrlKey: true }, true, false)).toBe(true);
});

test("a tap opens a link on a touch screen, which has no keys to hold", () => {
  expect(opensLink({ metaKey: false, ctrlKey: false }, true, true)).toBe(true);
});
