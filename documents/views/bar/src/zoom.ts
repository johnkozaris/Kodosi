import { say } from "@documents/words";
import { commandKey, keys, type ZoomPart, zoomOf } from "./parts";

export type Fitting = "width" | "page";

/** What sets the size of the pages: a way to fit them, or a scale that the person chose. */
type Sizing = Fitting | number;

/** A view of pages that a zoom sizes. */
interface Zoomable {
  /** The part that scrolls. A pinch over it zooms around the pointer. */
  area: HTMLElement;
  /** The space inside the area that keeps its size at each scale. */
  inset: number;
  /**
   * The scales at which the pages fill the width of the area, and at which a whole page fits.
   * A view without them has no fitting: it starts at its actual size.
   */
  fits?(): Record<Fitting, number>;
  /** Shows the pages at a scale. */
  scale(scale: number): void;
}

interface Point {
  clientX: number;
  clientY: number;
}

/** A pinch on a trackpad or a touch screen in WebKit. */
interface GestureEvent extends UIEvent, Point {
  scale: number;
}

/** The scales that the keys step through. */
const STEPS = [
  0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4,
] as const;
const LEAST = 0.1;
const MOST = 4;
/** Chromium sends a pinch as wheel steps with Ctrl, each 100 times the log of its scale. */
const PINCH_STEP = 100;
/** The most of one wheel step, so that a mouse wheel with Ctrl zooms in steps that one can follow. */
const MOST_WHEEL = 25;
const SAME = 0.001;

/** The next scale of the keys from a scale, which may lie between two of them. */
export function stepFrom(scale: number, larger: boolean): number | undefined {
  return larger
    ? STEPS.find((step) => step > scale + SAME)
    : STEPS.findLast((step) => step < scale - SAME);
}

/**
 * The zoom of a view of pages. The pages fit the width of the view, never above their actual
 * size, until the person zooms with the bar, the keys or a pinch.
 */
export class Zoom {
  private readonly part: ZoomPart;
  private readonly watching: ResizeObserver;
  private readonly listening = new AbortController();
  private readonly start: Sizing;
  private sizing: Sizing;
  private current = 1;
  private pinched = 1;

  constructor(private readonly view: Zoomable) {
    this.start = view.fits ? "width" : 1;
    this.sizing = this.start;
    this.part = zoomOf({
      step: (larger) => this.step(larger),
      entries: () => {
        const actual = {
          label: say("Actual size"),
          checked: this.sizing === 1,
          run: () => this.set(1),
        };
        if (!view.fits) return [{ ...actual, hint: keys("0") }];
        return [
          {
            label: say("Fit width"),
            hint: keys("0"),
            checked: this.sizing === "width",
            run: () => this.set("width"),
          },
          { label: say("Fit page"), checked: this.sizing === "page", run: () => this.set("page") },
          actual,
        ];
      },
    });
    const signal = this.listening.signal;
    const area = view.area;
    document.addEventListener("keydown", (event) => this.key(event), { signal });
    area.addEventListener("wheel", (event) => this.wheel(event), { passive: false, signal });
    area.addEventListener(
      "gesturestart",
      (event) => {
        event.preventDefault();
        this.pinched = this.current;
      },
      { signal },
    );
    area.addEventListener(
      "gesturechange",
      (event) => {
        event.preventDefault();
        const pinch = event as GestureEvent;
        this.set(this.pinched * pinch.scale, pinch);
      },
      { signal },
    );
    area.addEventListener("gestureend", (event) => event.preventDefault(), { signal });
    this.watching = new ResizeObserver(() => this.refit());
    this.watching.observe(area);
    this.show();
  }

  get element(): HTMLElement {
    return this.part.element;
  }

  /** The pages or the view changed. A fitting follows them, and the top of the view stays. */
  refit(): void {
    const area = this.view.area;
    if (area.clientWidth === 0 || area.clientHeight === 0) return;
    const box = area.getBoundingClientRect();
    this.apply(this.scaleOf(this.sizing), { clientX: box.left, clientY: box.top });
  }

  destroy(): void {
    this.listening.abort();
    this.watching.disconnect();
  }

  private scaleOf(sizing: Sizing): number {
    const scale =
      typeof sizing === "number" ? sizing : Math.min(1, this.view.fits?.()[sizing] ?? 1);
    return Math.min(MOST, Math.max(LEAST, scale)) || 1;
  }

  private set(sizing: Sizing, at?: Point): void {
    this.sizing = sizing;
    this.apply(this.scaleOf(sizing), at);
  }

  private step(larger: boolean): void {
    const next = stepFrom(this.current, larger);
    if (next !== undefined) this.set(next);
  }

  /** Shows the pages at a scale, and keeps the point under `at`, or the middle of the view. */
  private apply(scale: number, at?: Point): void {
    const area = this.view.area;
    const before = this.current;
    const box = area.getBoundingClientRect();
    const x = at ? at.clientX - box.left : area.clientWidth / 2;
    const y = at ? at.clientY - box.top : area.clientHeight / 2;
    const { inset } = this.view;
    const left = (area.scrollLeft + x - inset) / before;
    const top = (area.scrollTop + y - inset) / before;
    this.current = scale;
    this.view.scale(scale);
    if (Math.abs(scale - before) > SAME) {
      area.scrollLeft = left * scale + inset - x;
      area.scrollTop = top * scale + inset - y;
    }
    this.show();
  }

  private show(): void {
    this.part.show(
      Math.round(this.current * 100),
      stepFrom(this.current, false) !== undefined,
      stepFrom(this.current, true) !== undefined,
    );
  }

  private key(event: KeyboardEvent): void {
    if (event.defaultPrevented || !commandKey(event)) return;
    if (event.key === "=" || event.key === "+") this.step(true);
    else if (event.key === "-" || event.key === "_") this.step(false);
    else if (event.key === "0") this.set(this.start);
    else return;
    event.preventDefault();
  }

  private wheel(event: WheelEvent): void {
    if (!event.ctrlKey) return;
    event.preventDefault();
    const step = Math.min(MOST_WHEEL, Math.max(-MOST_WHEEL, event.deltaY));
    this.set(this.current * Math.exp(-step / PINCH_STEP), event);
  }
}
