import { ChevronDown, createElement, Eye, type IconNode, Minus, Plus } from "lucide";
import {
  closeFrom,
  closePopover,
  focusFirst,
  giveTip,
  type MenuEntry,
  menuOf,
  openPopover,
  viewOf,
} from "./popover";

const APPLE = /Mac|iPhone|iPad/.test(navigator.userAgent);

/** A chord as the computer shows it: ⌘ on a Mac and Ctrl elsewhere. */
export function keys(key: string, { shift = false } = {}): string {
  return APPLE ? `${shift ? "⇧" : ""}⌘${key}` : `Ctrl+${shift ? "Shift+" : ""}${key}`;
}

/** How much a bar holds: parts that need more room leave a narrower bar. */
export type Room = "wide" | "mid" | "narrow" | "tiny";

/** The narrowest room in which a part stays in the bar. */
export type Fit = Exclude<Room, "tiny">;

const ROOMS: readonly (readonly [Room, number])[] = [
  ["wide", 900],
  ["mid", 620],
  ["narrow", 440],
  ["tiny", 0],
];

function rank(room: string | undefined): number {
  const at = ROOMS.findIndex(([name]) => name === room);
  return at < 0 ? 0 : at;
}

/** Whether a part stays in the bar of the view that holds this element. */
export function fits(element: HTMLElement, part: Fit): boolean {
  return rank(viewOf(element).dataset.room) <= rank(part);
}

function fit<E extends HTMLElement>(element: E, fits?: Fit): E {
  if (fits) element.dataset.fits = fits;
  return element;
}

function icon(node: IconNode): SVGElement {
  return createElement(node);
}

/**
 * The bar of a document. A press on it keeps the focus where it is, so the caret or the cell
 * stays, and its width sets the room of the view.
 */
export class Bar {
  readonly element = document.createElement("div");
  private readonly sizing: ResizeObserver;

  constructor(view: HTMLElement, label: string) {
    this.element.className = "document-bar";
    this.element.role = "toolbar";
    this.element.ariaLabel = label;
    this.element.addEventListener("mousedown", (event) => {
      if (!(event.target as HTMLElement).closest("input")) event.preventDefault();
    });
    this.sizing = new ResizeObserver(() => {
      const width = this.element.clientWidth;
      view.dataset.room = ROOMS.find(([, least]) => width >= least)?.[0] ?? "tiny";
    });
    this.sizing.observe(this.element);
  }

  show(...parts: HTMLElement[]): void {
    closePopover();
    this.element.replaceChildren(...parts);
  }

  destroy(): void {
    closePopover();
    this.sizing.disconnect();
  }
}

export function readOnly(): HTMLElement {
  const label = document.createElement("span");
  label.className = "bar-read-only";
  label.append(icon(Eye), "Read only");
  return label;
}

export function rule(fits?: Fit): HTMLElement {
  const line = document.createElement("span");
  line.className = "bar-rule";
  line.ariaHidden = "true";
  return fit(line, fits);
}

export function gap(): HTMLElement {
  const space = document.createElement("span");
  space.className = "bar-gap";
  return space;
}

/** Whether a part can be used, and whether it is on. */
export interface Look {
  active?: boolean;
  enabled?: boolean;
}

export interface Key {
  element: HTMLButtonElement;
  show(look: Look): void;
}

/**
 * A key with a symbol: a quiet face on hover, the raised face while its menu is open, and the
 * raised face with a copper symbol while it is on.
 */
export function keyOf(options: {
  label: string;
  icon: IconNode;
  shortcut?: string;
  fits?: Fit;
  press(key: HTMLButtonElement, fromKeyboard: boolean): void;
}): Key {
  const key = fit(document.createElement("button"), options.fits);
  key.type = "button";
  key.className = "bar-key";
  key.append(icon(options.icon));
  giveTip(key, options.label, options.shortcut);
  key.addEventListener("click", (event) => options.press(key, event.detail === 0));
  return {
    element: key,
    show({ active, enabled = true }) {
      key.disabled = !enabled;
      if (active !== undefined) key.ariaPressed = String(active);
    },
  };
}

function opener(
  kind: "menu" | "panel",
  build: (done: () => void) => HTMLElement,
  align: "start" | "end",
): (key: HTMLElement, fromKeyboard: boolean) => void {
  return (key, fromKeyboard) => {
    if (key.dataset.open === "true") {
      closeFrom(key);
      return;
    }
    const content = build(() => closeFrom(key));
    openPopover(key, content, { kind, align });
    if (fromKeyboard) focusFirst(content);
  };
}

/** A key that opens a list of choices, which it builds each time from what is true then. */
export function menuKey(options: {
  label: string;
  icon: IconNode;
  fits?: Fit;
  align?: "start" | "end";
  entries(): readonly MenuEntry[];
}): Key {
  const open = opener("menu", (done) => menuOf(options.entries(), done), options.align ?? "start");
  return keyOf({ ...options, press: open });
}

/** Opens a panel from a part, such as the next step of a menu's choice. */
export function openPanel(anchor: HTMLElement, content: HTMLElement): void {
  openPopover(anchor, content, { kind: "panel" });
  const field = content.querySelector("input");
  field?.focus();
  field?.select();
}

/** A key that opens a panel of controls, which it builds each time it opens. */
export function panelKey(options: {
  label: string;
  icon: IconNode;
  fits?: Fit;
  align?: "start" | "end";
  build(): HTMLElement;
}): Key {
  return keyOf({
    ...options,
    press: opener("panel", () => options.build(), options.align ?? "start"),
  });
}

export interface Toggle {
  label: string;
  icon: IconNode;
  shortcut?: string;
  press(): void;
}

export interface Toggles {
  element: HTMLElement;
  show(looks: readonly Look[]): void;
}

/** Toggles side by side in one well, such as bold, italic and underline. */
export function wellOf(legend: string, toggles: readonly Toggle[], fits?: Fit): Toggles {
  const well = fit(document.createElement("div"), fits);
  well.className = "bar-well";
  well.role = "group";
  well.ariaLabel = legend;
  const keys = toggles.map((toggle) => {
    const key = well.appendChild(document.createElement("button"));
    key.type = "button";
    key.className = "bar-toggle";
    key.append(icon(toggle.icon));
    giveTip(key, toggle.label, toggle.shortcut);
    key.addEventListener("click", () => toggle.press());
    return key;
  });
  return {
    element: well,
    show(looks) {
      for (const [index, key] of keys.entries()) {
        const { active = false, enabled = true } = looks[index] ?? {};
        key.ariaPressed = String(active);
        key.disabled = !enabled;
      }
    },
  };
}

export interface Choice<V extends string> {
  value: V;
  label: string;
  icon: IconNode;
  shortcut?: string;
}

export interface Segments<V extends string> {
  element: HTMLElement;
  show(value: V | null, enabled?: boolean): void;
}

/** One choice of a few, with a raised thumb that slides to the chosen one. */
export function segmentsOf<V extends string>(
  label: string,
  choices: readonly Choice<V>[],
  pick: (value: V) => void,
  fits?: Fit,
): Segments<V> {
  const track = fit(document.createElement("div"), fits);
  track.className = "bar-segments";
  track.role = "radiogroup";
  track.ariaLabel = label;
  const thumb = track.appendChild(document.createElement("span"));
  thumb.className = "bar-thumb";
  thumb.hidden = true;
  const buttons = choices.map((choice) => {
    const button = track.appendChild(document.createElement("button"));
    button.type = "button";
    button.role = "radio";
    button.className = "bar-segment";
    button.append(icon(choice.icon));
    giveTip(button, choice.label, choice.shortcut);
    button.addEventListener("click", () => pick(choice.value));
    return button;
  });
  let chosen: HTMLButtonElement | undefined;
  const move = () => {
    thumb.hidden = !chosen || chosen.offsetWidth === 0;
    if (!chosen || thumb.hidden) return;
    thumb.style.width = `${chosen.offsetWidth}px`;
    thumb.style.transform = `translateX(${chosen.offsetLeft}px)`;
  };
  new ResizeObserver(move).observe(track);
  return {
    element: track,
    show(value, enabled = true) {
      chosen = buttons[choices.findIndex((choice) => choice.value === value)];
      for (const button of buttons) {
        button.ariaChecked = String(button === chosen);
        button.disabled = !enabled;
      }
      move();
    },
  };
}

export interface Picker {
  element: HTMLButtonElement;
  show(text: string, enabled?: boolean, look?: Partial<CSSStyleDeclaration>): void;
}

/** A face that names the current choice and lists the others, such as a style or a format. */
export function pickerOf(options: {
  label: string;
  width: number;
  fits?: Fit;
  entries(): readonly MenuEntry[];
}): Picker {
  const button = fit(document.createElement("button"), options.fits);
  button.type = "button";
  button.className = "bar-picker";
  button.style.width = `${options.width}px`;
  const text = button.appendChild(document.createElement("span"));
  text.className = "bar-picker-text";
  button.append(icon(ChevronDown));
  giveTip(button, options.label);
  const open = opener("menu", (done) => menuOf(options.entries(), done), "start");
  button.addEventListener("click", (event) => open(button, event.detail === 0));
  return {
    element: button,
    show(value, enabled = true, look = {}) {
      text.textContent = value;
      text.removeAttribute("style");
      Object.assign(text.style, look);
      button.disabled = !enabled;
    },
  };
}

export interface Stepper {
  element: HTMLElement;
  show(value: string, canShrink: boolean, canGrow: boolean): void;
}

/** A size between a key that makes it smaller and one that makes it larger. */
export function stepperOf(label: string, step: (larger: boolean) => void, fits?: Fit): Stepper {
  const well = fit(document.createElement("div"), fits);
  well.className = "bar-well bar-stepper";
  well.role = "group";
  well.ariaLabel = label;
  const key = (larger: boolean) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "bar-toggle";
    button.append(icon(larger ? Plus : Minus));
    giveTip(button, larger ? "Larger" : "Smaller");
    button.addEventListener("click", () => step(larger));
    return button;
  };
  const smaller = well.appendChild(key(false));
  const value = well.appendChild(document.createElement("span"));
  value.className = "bar-stepper-value";
  const larger = well.appendChild(key(true));
  return {
    element: well,
    show(text, canShrink, canGrow) {
      value.textContent = text;
      smaller.disabled = !canShrink;
      larger.disabled = !canGrow;
    },
  };
}

export interface Swatch {
  value: string | null;
  label: string;
  /** The color that the swatch shows. */
  fill: string;
  icon?: IconNode;
}

/** A row of colors, the chosen one with a copper ring. */
export function swatchesOf(
  legend: string,
  swatches: readonly Swatch[],
  chosen: string | null,
  pick: (value: string | null) => void,
): HTMLElement {
  const row = document.createElement("div");
  row.className = "bar-swatches";
  row.role = "radiogroup";
  row.ariaLabel = legend;
  const same = (value: string | null) => value?.toLowerCase() === chosen?.toLowerCase();
  for (const swatch of swatches) {
    const button = row.appendChild(document.createElement("button"));
    button.type = "button";
    button.role = "radio";
    button.className = "bar-swatch";
    button.style.backgroundColor = swatch.fill;
    button.ariaChecked = String(same(swatch.value));
    if (swatch.icon) button.append(icon(swatch.icon));
    giveTip(button, swatch.label);
    button.addEventListener("click", () => {
      for (const other of row.children) (other as HTMLElement).ariaChecked = "false";
      button.ariaChecked = "true";
      pick(swatch.value);
    });
  }
  return row;
}

/** A text well that takes its words with Enter. */
export function fieldOf(label: string, value: string, take: (text: string) => void): HTMLElement {
  const field = document.createElement("input");
  field.className = "bar-field";
  field.ariaLabel = label;
  field.value = value;
  field.spellcheck = false;
  field.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    const text = field.value.trim();
    if (text) take(text);
  });
  return field;
}

export type PanelRow = readonly [name: string, control: HTMLElement];

/** A panel's rows, each with a short name on the left of its control. */
export function panelOf(rows: readonly PanelRow[], foot?: HTMLElement): HTMLElement {
  const panel = document.createElement("div");
  const grid = panel.appendChild(document.createElement("div"));
  grid.className = "bar-panel-rows";
  for (const [name, control] of rows) {
    const label = grid.appendChild(document.createElement("span"));
    label.className = "bar-panel-name";
    label.textContent = name;
    grid.append(control);
  }
  if (foot) panel.append(foot);
  return panel;
}

/** A row of wide keys under a panel, each with its symbol over its name. */
export function footOf(
  keys: readonly { label: string; icon: IconNode; enabled?: boolean; press(): void }[],
): HTMLElement {
  const foot = document.createElement("div");
  foot.className = "bar-panel-foot";
  for (const key of keys) {
    const button = foot.appendChild(document.createElement("button"));
    button.type = "button";
    button.className = "bar-panel-key";
    button.disabled = key.enabled === false;
    button.append(icon(key.icon), key.label);
    button.addEventListener("click", () => {
      closePopover();
      key.press();
    });
  }
  return foot;
}
