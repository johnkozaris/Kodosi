import { type ReactNode, type Ref, useImperativeHandle, useLayoutEffect, useRef } from "react";

import { cn } from "../parts/cn";
import { carriedShell, dropDrawn } from "./carry";
import { MOVES, play, springSoft, still } from "./motion";

let carriedOn: Element | null = null;

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
  busy?: boolean;
  waits?: boolean | undefined;
  wrong?: string | undefined;
  carries?: boolean;
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
      const now = inside.offsetHeight + 1;
      const before = last;
      last = now;
      if (before === null || Math.abs(before - now) < 1) return;
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

  useLayoutEffect(() => {
    const el = node.current;
    if (!el || !wrong || shaken.current === wrong || still()) return;
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
          : "has-[:is(input:not([type=checkbox],[type=radio]),select,textarea):focus]:shadow-[var(--e-lifted),0_0_0_3px_color-mix(in_srgb,var(--accent)_22%,transparent)]",
        (busy || waits) && "working-rim",
        className,
      )}
    >
      <div className="h-full overflow-hidden rounded-[inherit] bg-raised">
        <div ref={body} className={cn("shell-body", carries && carriedShell() && "resolve")}>
          {children}
        </div>
      </div>
    </div>
  );
}
