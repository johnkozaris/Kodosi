import { type Box, carried } from "./carry";
import { MOVES, spring, still } from "./motion";

export interface Cursor {
  waits(on: boolean): void;
  works(on: boolean): void;
  box(): Box | null;
  destroy(): void;
}

const TYPED = ".row-input, .handle-input, .code-input";
const IDLE_MS = 600;
const INTRO_MS = 950;
const SIDES = ["x", "y", "w", "h"] as const;

const idle: Cursor = { waits() {}, works() {}, box: () => null, destroy() {} };

interface Metrics {
  type: string;
  font: string;
  spacing: string;
  left: number;
  right: number;
  top: number;
  height: number;
  cell: number;
  dot: number;
}

let ruler: CanvasRenderingContext2D | null | undefined;

function widthOf(text: string, font: string, spacing: string): number {
  ruler ??= document.createElement("canvas").getContext("2d");
  if (!ruler) return 0;
  ruler.font = font;
  if ("letterSpacing" in ruler) ruler.letterSpacing = spacing === "normal" ? "0px" : spacing;
  return ruler.measureText(text).width;
}

// Each browser draws its own password dot, so measure ten of them in a field one pixel wide.
function dotWidth(font: string, spacing: string): number {
  const probe = document.createElement("input");
  probe.type = "password";
  probe.tabIndex = -1;
  probe.autocomplete = "off";
  probe.setAttribute("aria-hidden", "true");
  probe.value = "0000000000";
  probe.style.cssText = `position:absolute;visibility:hidden;width:1px;padding:0;border:0;font:${font};letter-spacing:${spacing}`;
  document.body.appendChild(probe);
  const wide = probe.scrollWidth;
  probe.remove();
  return wide > 10 ? wide / 10 : widthOf("•", font, spacing);
}

function measure(field: HTMLInputElement): Metrics {
  const style = getComputedStyle(field);
  const size = Number.parseFloat(style.fontSize) || 15;
  const font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
  const spacing = style.letterSpacing;
  const px = (value: string) => Number.parseFloat(value) || 0;
  const above = px(style.borderTopWidth) + px(style.paddingTop);
  const below = px(style.borderBottomWidth) + px(style.paddingBottom);
  const height = Math.round(size * 1.3);
  return {
    type: field.type,
    font,
    spacing,
    left: px(style.borderLeftWidth) + px(style.paddingLeft),
    right: px(style.borderRightWidth) + px(style.paddingRight),
    top: above + (field.offsetHeight - above - below - height) / 2,
    height,
    cell: widthOf("0", font, spacing) || size * 0.6,
    dot: field.type === "password" ? dotWidth(font, spacing) : 0,
  };
}

export function liveCursor(): Cursor {
  const home = document.getElementById("cursor-home");
  if (!home) return idle;
  const root = document.documentElement;
  const reduced = still();
  const fine = matchMedia("(pointer: fine)").matches;
  const { stiffness, damping, mass = 1 } = spring;

  const homeBox = (): Box => {
    const box = home.getBoundingClientRect();
    return { x: box.left + scrollX, y: box.top + scrollY, w: box.width, h: box.height };
  };

  let node = document.getElementById("cursor");
  const from = carried();
  let at: Box = node && from ? { x: from.cx, y: from.cy, w: from.cw, h: from.ch } : homeBox();
  // StrictMode mounts the page twice in development: go on from the cursor of the first mount.
  if (node?.dataset.x) {
    at = {
      x: Number(node.dataset.x),
      y: Number(node.dataset.y),
      w: Number(node.dataset.w),
      h: Number(node.dataset.h),
    };
  }
  const arrives = root.classList.contains("arrive") && !reduced;
  const arrived = Number.parseFloat(root.style.getPropertyValue("--arrived")) || 0;
  if (!node) {
    node = document.createElement("div");
    node.id = "cursor";
    node.setAttribute("aria-hidden", "true");
    if (arrives) node.style.animationDelay = `${0.52 - arrived - performance.now() / 1000}s`;
    document.body.appendChild(node);
  }
  root.classList.add("cursor-live");
  const cursor = node;
  const holdUntil = arrives ? INTRO_MS - arrived * 1000 : 0;

  const speed: Box = { x: 0, y: 0, w: 0, h: 0 };
  let field: HTMLInputElement | null = null;
  let metrics: Metrics | null = null;
  let composing = false;
  let waiting = true;
  let working = false;
  let quiet = false;
  let frame = 0;
  let last = 0;
  let watchUntil = 0;
  let quietTimer = 0;

  const caretBox = (): Box | null => {
    if (!field || composing || !field.isConnected) return null;
    const host = field.closest("[data-cursor-host]");
    if (host) {
      const slot = host.querySelector("[data-cursor-slot]")?.getBoundingClientRect();
      return slot
        ? { x: slot.left + scrollX, y: slot.top + scrollY, w: slot.width, h: slot.height }
        : null;
    }
    if (!metrics || metrics.type !== field.type) metrics = measure(field);
    const box = field.getBoundingClientRect();
    if (box.width === 0) return null;
    const end =
      (field.selectionDirection === "backward" ? field.selectionStart : field.selectionEnd) ??
      field.value.length;
    const typed = metrics.dot
      ? metrics.dot * end
      : widthOf(field.value.slice(0, end), metrics.font, metrics.spacing);
    const x = Math.min(
      Math.max(metrics.left + typed - field.scrollLeft, metrics.left),
      box.width - metrics.right,
    );
    return {
      x: box.left + scrollX + x,
      y: box.top + scrollY + metrics.top,
      w: metrics.cell,
      h: metrics.height,
    };
  };

  const held = () => performance.now() < holdUntil;
  const goal = (): Box => (held() ? null : caretBox()) ?? homeBox();

  const draw = () => {
    const long = Math.min(Math.abs(speed.x) * 0.012, at.w * 2);
    const tall = Math.min(Math.abs(speed.y) * 0.012, at.h * 1.2);
    const x = speed.x > 0 ? at.x - long : at.x;
    const y = speed.y > 0 ? at.y - tall : at.y;
    const style = cursor.style;
    style.transform = `translate(${x.toFixed(2)}px,${y.toFixed(2)}px)`;
    style.width = `${(at.w + long).toFixed(2)}px`;
    style.height = `${(at.h + tall).toFixed(2)}px`;
    style.borderRadius = `${Math.max(1, at.w * 0.1).toFixed(1)}px`;
    cursor.dataset.x = `${at.x}`;
    cursor.dataset.y = `${at.y}`;
    cursor.dataset.w = `${at.w}`;
    cursor.dataset.h = `${at.h}`;
  };

  const step = (now: number) => {
    let time = reduced ? 0 : Math.min((now - last) / 1000, 0.6);
    last = now;
    const to = goal();
    let moves = false;
    while (time > 0) {
      const dt = Math.min(time, 1 / 120);
      time -= dt;
      for (const side of SIDES) {
        speed[side] += ((stiffness * (to[side] - at[side]) - damping * speed[side]) / mass) * dt;
        at[side] += speed[side] * dt;
      }
    }
    for (const side of SIDES)
      if (Math.abs(to[side] - at[side]) > 0.05 || Math.abs(speed[side]) > 2) moves = true;
    if (!moves || reduced) {
      at = { ...to };
      speed.x = speed.y = speed.w = speed.h = 0;
    }
    draw();
    frame = moves || now < watchUntil ? requestAnimationFrame(step) : 0;
  };

  const watch = (ms = 700) => {
    watchUntil = Math.max(watchUntil, performance.now() + ms);
    if (frame) return;
    last = performance.now();
    frame = requestAnimationFrame(step);
  };

  const mood = () => {
    const inField = !!field && !held() && !composing;
    field?.toggleAttribute("data-block", inField);
    cursor.toggleAttribute("data-works", working);
    cursor.toggleAttribute("data-blinks", !working && (inField ? quiet : waiting));
    cursor.toggleAttribute("data-glows", !inField);
    root.classList.toggle("cursor-away", inField);
  };

  const typed = () => {
    quiet = false;
    clearTimeout(quietTimer);
    quietTimer = window.setTimeout(() => {
      quiet = true;
      mood();
    }, IDLE_MS);
    mood();
    watch();
  };

  const enter = (event: FocusEvent) => {
    const target = event.target;
    if (!fine || !(target instanceof HTMLInputElement) || !target.matches(TYPED)) return;
    if (getComputedStyle(target).direction === "rtl") return;
    field = target;
    metrics = null;
    typed();
  };
  const leave = (event: FocusEvent) => {
    if (event.target !== field) return;
    field?.removeAttribute("data-block");
    field = null;
    composing = false;
    mood();
    watch();
  };
  const changed = () => {
    if (field) typed();
  };
  const compose = (event: CompositionEvent) => {
    composing = event.type === "compositionstart";
    mood();
    watch();
  };
  const resized = () => {
    metrics = null;
    watch();
  };
  const moved = () => watch(1200);

  document.addEventListener(MOVES, moved);
  document.addEventListener("focusin", enter);
  document.addEventListener("focusout", leave);
  document.addEventListener("input", changed);
  document.addEventListener("keydown", changed);
  document.addEventListener("pointerup", changed);
  document.addEventListener("selectionchange", changed);
  document.addEventListener("compositionstart", compose);
  document.addEventListener("compositionend", compose);
  document.addEventListener("animationstart", moved);
  document.addEventListener("transitionstart", moved);
  document.addEventListener("scroll", moved, true);
  window.addEventListener("resize", resized);
  void document.fonts?.ready.then(resized);

  const first = document.activeElement;
  if (first instanceof HTMLInputElement) enter({ target: first } as unknown as FocusEvent);
  const release = window.setTimeout(
    () => {
      mood();
      watch();
    },
    Math.max(0, holdUntil - performance.now()),
  );
  draw();
  mood();
  watch(1400);

  return {
    waits(on) {
      waiting = on;
      mood();
    },
    works(on) {
      working = on;
      mood();
    },
    box: () => ({ x: at.x - scrollX, y: at.y - scrollY, w: at.w, h: at.h }),
    destroy() {
      document.removeEventListener(MOVES, moved);
      document.removeEventListener("focusin", enter);
      document.removeEventListener("focusout", leave);
      document.removeEventListener("input", changed);
      document.removeEventListener("keydown", changed);
      document.removeEventListener("pointerup", changed);
      document.removeEventListener("selectionchange", changed);
      document.removeEventListener("compositionstart", compose);
      document.removeEventListener("compositionend", compose);
      document.removeEventListener("animationstart", moved);
      document.removeEventListener("transitionstart", moved);
      document.removeEventListener("scroll", moved, true);
      window.removeEventListener("resize", resized);
      clearTimeout(quietTimer);
      clearTimeout(release);
      cancelAnimationFrame(frame);
      field?.removeAttribute("data-block");
      root.classList.remove("cursor-away");
    },
  };
}
