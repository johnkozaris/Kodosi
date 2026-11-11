import { Check, createElement, type IconNode } from "lucide";

const TIP_DELAY = 450;
const TIP_WARM = 600;
const GAP = 6;

/** Floating boxes go into the view of their part, so they keep its look. */
export function viewOf(element: HTMLElement): HTMLElement {
  return element.closest<HTMLElement>(".document-view") ?? document.body;
}

function place(box: HTMLElement, anchor: HTMLElement, align: "start" | "end" | "middle"): void {
  const room = anchor.getBoundingClientRect();
  const size = box.getBoundingClientRect();
  const below = room.bottom + GAP + size.height <= window.innerHeight;
  const top = below ? room.bottom + GAP : Math.max(GAP, room.top - GAP - size.height);
  const left =
    align === "start"
      ? room.left
      : align === "end"
        ? room.right - size.width
        : room.left + (room.width - size.width) / 2;
  box.style.top = `${top}px`;
  box.style.left = `${Math.min(Math.max(GAP, left), window.innerWidth - size.width - GAP)}px`;
}

let tip: HTMLElement | undefined;
let tipTimer = 0;
let tipClosed = 0;

function hideTip(): void {
  clearTimeout(tipTimer);
  if (!tip) return;
  tip.remove();
  tip = undefined;
  tipClosed = performance.now();
}

export function giveTip(element: HTMLElement, label: string, shortcut?: string): void {
  element.ariaLabel = label;
  element.addEventListener("pointerenter", () => {
    if (element.dataset.open === "true") return;
    clearTimeout(tipTimer);
    const warm = tip !== undefined || performance.now() - tipClosed < TIP_WARM;
    tipTimer = window.setTimeout(
      () => {
        tip?.remove();
        tip = document.createElement("div");
        tip.className = "bar-tip";
        tip.dataset.instant = String(warm);
        tip.append(label);
        if (shortcut) tip.appendChild(document.createElement("kbd")).textContent = shortcut;
        viewOf(element).append(tip);
        place(tip, element, "middle");
      },
      warm ? 0 : TIP_DELAY,
    );
  });
  element.addEventListener("pointerleave", hideTip);
  element.addEventListener("pointerdown", hideTip);
}

interface Open {
  box: HTMLElement;
  anchor: HTMLElement;
}

/** A menu that a panel opens lies over it. */
const opened: Open[] = [];

function closeTop(): void {
  const top = opened.pop();
  if (!top) return;
  top.box.remove();
  delete top.anchor.dataset.open;
  top.anchor.ariaExpanded = "false";
  if (opened.length === 0) listen(false);
}

function closeOver(node: Node | null): void {
  const holder = opened.findLastIndex(
    ({ box, anchor }) => node !== null && (box.contains(node) || anchor.contains(node)),
  );
  while (opened.length > holder + 1) closeTop();
}

export function closePopover(): void {
  while (opened.length > 0) closeTop();
}

const outside = (event: PointerEvent) => closeOver(event.target as Node);

const escape = (event: KeyboardEvent) => {
  const top = opened.at(-1);
  if (event.key !== "Escape" || !top) return;
  event.preventDefault();
  event.stopPropagation();
  const focused = top.box.contains(document.activeElement);
  closeTop();
  if (focused) top.anchor.focus();
};

function listen(on: boolean): void {
  if (on) {
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", escape, true);
    window.addEventListener("resize", closePopover);
    window.addEventListener("blur", closePopover);
  } else {
    document.removeEventListener("pointerdown", outside, true);
    document.removeEventListener("keydown", escape, true);
    window.removeEventListener("resize", closePopover);
    window.removeEventListener("blur", closePopover);
  }
}

export function openPopover(
  anchor: HTMLElement,
  content: HTMLElement,
  options: { align?: "start" | "end"; kind: "menu" | "panel" },
): void {
  closeOver(anchor);
  hideTip();
  const box = document.createElement("div");
  box.className = `bar-popover bar-${options.kind}`;
  box.append(content);
  box.addEventListener("mousedown", (event) => {
    if (!(event.target as HTMLElement).closest("input")) event.preventDefault();
  });
  viewOf(anchor).append(box);
  place(box, anchor, options.align ?? "start");
  anchor.dataset.open = "true";
  anchor.ariaExpanded = "true";
  if (opened.length === 0) listen(true);
  opened.push({ box, anchor });
}

export function closeFrom(anchor: HTMLElement): void {
  const at = opened.findIndex((open) => open.anchor === anchor);
  if (at < 0) return;
  while (opened.length > at) closeTop();
}

export type MenuEntry =
  | {
      label: string;
      icon?: IconNode;
      hint?: string;
      checked?: boolean;
      disabled?: boolean;
      look?: Partial<CSSStyleDeclaration>;
      run(): void;
    }
  | { heading: string }
  | "separator";

export function menuOf(entries: readonly MenuEntry[], done: () => void): HTMLElement {
  const list = document.createElement("div");
  list.role = "menu";
  const icons = entries.some((entry) => typeof entry === "object" && "icon" in entry);
  for (const entry of entries) {
    if (entry === "separator") {
      list.appendChild(document.createElement("div")).className = "bar-menu-separator";
      continue;
    }
    if ("heading" in entry) {
      const heading = list.appendChild(document.createElement("div"));
      heading.className = "bar-menu-heading";
      heading.textContent = entry.heading;
      continue;
    }
    const item = list.appendChild(document.createElement("button"));
    item.type = "button";
    item.className = "bar-menu-item";
    item.role = entry.checked === undefined ? "menuitem" : "menuitemradio";
    if (entry.checked !== undefined) item.ariaChecked = String(entry.checked);
    item.disabled = entry.disabled ?? false;
    if (icons) {
      const icon = item.appendChild(document.createElement("span"));
      icon.className = "bar-menu-icon";
      if (entry.icon) icon.append(createElement(entry.icon));
    }
    const label = item.appendChild(document.createElement("span"));
    label.className = "bar-menu-label";
    label.textContent = entry.label;
    Object.assign(label.style, entry.look ?? {});
    if (entry.hint) {
      const hint = item.appendChild(document.createElement("span"));
      hint.className = "bar-menu-hint";
      hint.textContent = entry.hint;
    }
    if (entry.checked !== undefined) {
      const check = item.appendChild(document.createElement("span"));
      check.className = "bar-menu-check";
      if (entry.checked) check.append(createElement(Check));
    }
    item.addEventListener("click", () => {
      done();
      entry.run();
    });
  }
  list.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const items = [...list.querySelectorAll<HTMLButtonElement>(".bar-menu-item:not(:disabled)")];
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    const step = event.key === "ArrowDown" ? 1 : -1;
    items[(at + step + items.length) % items.length]?.focus();
  });
  return list;
}

export function focusFirst(content: HTMLElement): void {
  content
    .querySelector<HTMLElement>(
      "[aria-checked='true'], .bar-menu-item:not(:disabled), input, button",
    )
    ?.focus();
}
