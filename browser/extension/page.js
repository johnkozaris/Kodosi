let shape = "";
let waiting = null;
let nextMenu = 1;

function toHost(message) {
  chrome.runtime.sendMessage({ to: "host", message }).catch(() => {});
}

function showShape(event) {
  const next = event.target instanceof Element ? getComputedStyle(event.target).cursor : "";
  if (next === shape) return;
  shape = next;
  toHost({ type: "shape", shape });
}

function pickFrom(select) {
  const id = nextMenu++;
  waiting = (choice) => {
    if (choice === null || choice === select.selectedIndex) return;
    select.selectedIndex = choice;
    select.dispatchEvent(new Event("input", { bubbles: true }));
    select.dispatchEvent(new Event("change", { bubbles: true }));
  };
  toHost({
    type: "menu",
    menu: id,
    items: Array.from(select.options, (option) => ({
      label: option.label || option.text,
      enabled: !option.disabled,
      chosen: option.selected,
    })),
  });
}

function pressed(event) {
  const select = event.target instanceof Element ? event.target.closest("select") : null;
  if (!select || select.multiple || select.size > 1 || select.disabled) return;
  event.preventDefault();
  select.focus();
  pickFrom(select);
}

function rightClicked(event) {
  if (event.defaultPrevented) return;
  event.preventDefault();
  const link = event.target instanceof Element ? event.target.closest("a[href]") : null;
  toHost({
    type: "context",
    selection: String(getSelection() ?? ""),
    link: link?.href ?? null,
  });
}

function copied() {
  const text = String(getSelection() ?? "");
  if (text) toHost({ type: "copied", text });
}

addEventListener("mousemove", showShape, { capture: true, passive: true });
addEventListener("mousedown", pressed, { capture: true });
addEventListener("contextmenu", rightClicked);
addEventListener("copy", copied, { capture: true, passive: true });
addEventListener("cut", copied, { capture: true, passive: true });

chrome.runtime.onMessage.addListener(({ choice }) => {
  waiting?.(choice);
  waiting = null;
});
