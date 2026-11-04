/** Schemes that would run in the view's own page instead of going to the host. */
const IN_PAGE = new Set(["javascript:", "data:", "blob:", "vbscript:", "about:"]);

/**
 * Hands a link to the host, which opens it by the rules of terminal links: in the browser or an
 * app of the device, after a question, or not at all with the reason. A link that would run in
 * this page goes nowhere.
 */
export function followLink(address: string): void {
  if (!passesOn(address)) return;
  const link = document.createElement("a");
  link.href = address;
  link.target = "_blank";
  link.rel = "noopener";
  link.click();
}

export function passesOn(address: string): boolean {
  if (!URL.canParse(address)) return false;
  const url = new URL(address);
  return !IN_PAGE.has(url.protocol) && url.origin !== globalThis.location?.origin;
}

/**
 * Where a person edits, a click places the caret or picks a shape or a cell, so a link opens with
 * ⌘ or Ctrl, as in a terminal. Where they only look, and on a touch screen, which has no such
 * keys, a click or a tap opens it.
 */
export function opensLink(
  keys: Pick<MouseEvent, "metaKey" | "ctrlKey">,
  editing: boolean,
  touch = globalThis.matchMedia?.("(pointer: coarse)").matches === true,
): boolean {
  return !editing || touch || keys.metaKey || keys.ctrlKey;
}
