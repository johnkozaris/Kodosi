// Schemes that execute or navigate inside the webview instead of delegating to the host.
const IN_PAGE = new Set(["javascript:", "data:", "blob:", "vbscript:", "about:"]);

/** Delegates external links to the host via a new-window navigation; ignores in-page URLs. */
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
 * Requires Cmd/Ctrl while editing on desktop so plain clicks still place the caret or select
 * shapes/cells. Plain clicks and taps open links directly in read-only mode and on touch devices.
 */
export function opensLink(
  keys: Pick<MouseEvent, "metaKey" | "ctrlKey">,
  editing: boolean,
  touch = globalThis.matchMedia?.("(pointer: coarse)").matches === true,
): boolean {
  return !editing || touch || keys.metaKey || keys.ctrlKey;
}
