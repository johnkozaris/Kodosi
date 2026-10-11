import { type ReactNode, type Ref, useImperativeHandle, useLayoutEffect, useRef } from "react";

import { cn } from "../parts/cn";
import { carriedShell, dropDrawn } from "./carry";
import { MOVES, play, springSoft, still } from "./motion";

/** The capsule that carried on from the page before: one capsule, one time. */
let carriedOn: Element | null = null;

/**
 * The object of a page: a lifted sheet, as the composer of the app. It starts from the box that
 * the capsule of the page before had, and it takes the size that this step needs.
 */
export function Shell({
  ref,
  busy,
  waits,
  wrong,
  carries = true,
  grows,
  className,
  children,
}: {
  ref?: Ref<HTMLDivElement> | undefined;
  /** The request is on its way: the warm rim turns, and the capsule holds what it has. */
  busy?: boolean;
  /** The page works on something of its own: the rim turns, and the capsule takes input. */
  waits?: boolean | undefined;
  /** What the person gave was refused. Each refusal has its own value, and the capsule shakes
      its head one time for each. */
  wrong?: string | undefined;
  /** This capsule carries on from the page before. A page with more capsules has one such. */
  carries?: boolean;
  /** What the capsule holds changes while the page is open: it grows and shrinks on a spring. */
  grows?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const node = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  useImperativeHandle(ref, () => node.current as HTMLDivElement);
  const shaken = useRef<string | undefined>(undefined);

  useLayoutEffect(() => {
    const el = node.current;
    if (!el || !carries) return;
    const from = carriedShell();
    dropDrawn();
    // A capsule that comes a second time, after "Try again", starts where it is.
    if (!from || (carriedOn && carriedOn !== el)) return;
    carriedOn = el;
    const to = el.getBoundingClientRect();
    const radius = getComputedStyle(el).borderTopLeftRadius;
    const morph = play(
      el,
      [
        {
          width: `${from.w}px`,
          height: `${from.h}px`,
          transform: `translate(${from.x + from.w / 2 - (to.left + to.width / 2)}px, ${from.y - to.top}px)`,
          borderRadius: `${from.r}px`,
        },
        {
          width: `${to.width}px`,
          height: `${to.height}px`,
          transform: "translate(0, 0)",
          borderRadius: radius,
        },
      ],
      springSoft,
    );
    return () => morph.cancel();
  }, [carries]);

  useLayoutEffect(() => {
    const el = node.current;
    const inside = body.current;
    if (!el || !inside || !grows) return;
    let last: number | null = null;
    let change: Animation | undefined;
    const watch = new ResizeObserver(() => {
      // The capsule is its body and its two lines.
      const now = inside.offsetHeight + 1;
      const before = last;
      last = now;
      if (before === null || Math.abs(before - now) < 1) return;
      // A change that is still on its way goes on from where it is.
      const from = el.getAnimations().length ? el.getBoundingClientRect().height : before;
      change?.cancel();
      change = play(el, [{ height: `${from}px` }, { height: `${now}px` }], springSoft);
    });
    watch.observe(inside);
    return () => {
      watch.disconnect();
      change?.cancel();
    };
  }, [grows]);

  // A wrong answer shakes the capsule one time, as the sign-in window of a Mac does.
  useLayoutEffect(() => {
    const el = node.current;
    if (!el || !wrong || shaken.current === wrong || still()) return;
    // The first refusal of a page comes with the page: the shake waits for the capsule.
    const first = shaken.current === undefined;
    shaken.current = wrong;
    document.dispatchEvent(new Event(MOVES));
    const shake = el.animate(
      { transform: [0, -10, 8, -5, 3, 0].map((x) => `translateX(${x}px)`) },
      {
        duration: 460,
        delay: first ? (carriedShell() ? 220 : 120) : 0,
        easing: "ease-out",
        composite: "add",
      },
    );
    return () => shake.cancel();
  }, [wrong]);

  return (
    <div
      ref={node}
      data-busy={busy || undefined}
      data-wrong={wrong ? "" : undefined}
      className={cn(
        "shell sheet-lift relative mx-auto w-full rounded-sheet transition-shadow duration-200",
        wrong
          ? "shadow-[var(--e-lifted),0_0_0_1.5px_var(--danger)]"
          : // The copper halo says "type here": it is for a field, not for a row that is pressed.
            "has-[:is(input:not([type=checkbox],[type=radio]),select,textarea):focus]:shadow-[var(--e-lifted),0_0_0_3px_color-mix(in_srgb,var(--accent)_22%,transparent)]",
        (busy || waits) && "working-rim",
        className,
      )}
    >
      {/* The body has the capsule's own ground, so the glow of the rim stays outside it. It has
          the capsule's height, so it cuts what does not fit yet while the capsule changes size. */}
      <div className="h-full overflow-hidden rounded-[inherit] bg-raised">
        <div ref={body} className={cn("shell-body", carries && carriedShell() && "resolve")}>
          {children}
        </div>
      </div>
    </div>
  );
}
