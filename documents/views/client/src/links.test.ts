import { expect, test } from "bun:test";
import { passesOn } from "./links";

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
