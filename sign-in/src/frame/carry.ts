import { still } from "./motion";

export interface Carry {
  t: number;
  x: number;
  y: number;
  w: number;
  h: number;
  r: number;
  cx: number;
  cy: number;
  cw: number;
  ch: number;
  title: string;
  vw: number;
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

const KEY = "kodosi.carry";
// index.html reads the same key with the same age.
const FRESH_MS = 6000;

let taken: Carry | null | undefined;

export function carried(): Carry | null {
  if (taken !== undefined) return taken;
  taken = null;
  try {
    const stored = sessionStorage.getItem(KEY);
    sessionStorage.removeItem(KEY);
    const carry = stored ? (JSON.parse(stored) as Carry) : null;
    if (carry && Date.now() - carry.t < FRESH_MS && carry.vw === innerWidth && !still())
      taken = carry;
  } catch {}
  return taken;
}

export function carriedShell(): Carry | null {
  const carry = carried();
  return carry && carry.w > 0 ? carry : null;
}

export function handOver(shell: Element | null, cursor: Box | null, title: string) {
  const box = shell?.getBoundingClientRect();
  const seen = !!box && box.bottom > 0 && box.top < innerHeight;
  const carry: Carry = {
    t: Date.now(),
    x: box?.left ?? 0,
    y: box?.top ?? 0,
    w: seen ? box.width : 0,
    h: seen ? box.height : 0,
    r: (shell && Number.parseFloat(getComputedStyle(shell).borderTopLeftRadius)) || 22,
    cx: cursor?.x ?? 0,
    cy: cursor?.y ?? 0,
    cw: cursor?.w ?? 0,
    ch: cursor?.h ?? 0,
    title,
    vw: innerWidth,
  };
  try {
    sessionStorage.setItem(KEY, JSON.stringify(carry));
  } catch {}
}

export function dropDrawn() {
  document.getElementById("carry")?.remove();
}
