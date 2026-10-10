import { still } from "./motion";

/**
 * Each step of a sign-in is a page of its own. A page hands the next one the box of its capsule,
 * the place of the cursor and its title, and the next one starts from them: the capsule changes
 * its shape, the cursor goes on from where it stood, and the title of the page before leaves
 * through the line that the new title comes through. index.html draws the capsule and the cursor
 * before this page has its own. The mark needs no such help: index.html draws it in the same
 * place on each page.
 */
export interface Carry {
  /** When the page left. */
  t: number;
  /** The capsule, in the window. A capsule that the person scrolled away has no width. */
  x: number;
  y: number;
  w: number;
  h: number;
  r: number;
  /** The cursor, in the window. */
  cx: number;
  cy: number;
  cw: number;
  ch: number;
  /** The title of the page. */
  title: string;
  /** The width of the window: the places are right only in a window of the same width. */
  vw: number;
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

const KEY = "kodosi.carry";
/** After this time, the page before is not what the person just left. index.html has the same number. */
const FRESH_MS = 6000;

let taken: Carry | null | undefined;

/** What the page before this one handed over. It is read one time, for the whole page. */
export function carried(): Carry | null {
  if (taken !== undefined) return taken;
  taken = null;
  try {
    const stored = sessionStorage.getItem(KEY);
    sessionStorage.removeItem(KEY);
    const carry = stored ? (JSON.parse(stored) as Carry) : null;
    if (carry && Date.now() - carry.t < FRESH_MS && carry.vw === innerWidth && !still())
      taken = carry;
  } catch {
    // A browser without storage starts each page from nothing.
  }
  return taken;
}

/** The capsule of the page before, when it was in the window. */
export function carriedShell(): Carry | null {
  const carry = carried();
  return carry && carry.w > 0 ? carry : null;
}

export function handOver(shell: Element | null, cursor: Box | null, title: string) {
  const box = shell?.getBoundingClientRect();
  // A capsule that the person scrolled away is not where the next page starts from. The time
  // still goes over, so the next page knows that the visit goes on.
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
  } catch {
    // The next page starts from nothing.
  }
}

/** Takes away the capsule that index.html drew. This page has its own now. */
export function dropDrawn() {
  document.getElementById("carry")?.remove();
}
