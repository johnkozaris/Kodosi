import { type ReactNode, useLayoutEffect, useRef } from "react";

import { play, springSnappy } from "../frame/motion";
import { cn } from "./cn";

/** The look of one option of a sliding pill. */
export function option(on: boolean) {
  return cn(
    "relative z-10 inline-flex h-7 items-center justify-center rounded-full px-3 text-footnote font-medium whitespace-nowrap transition-colors duration-150 outline-none focus-visible:ring-2 focus-visible:ring-accent pointer-coarse:h-9",
    on ? "text-ink" : "text-ink-muted hover:text-ink",
  );
}

/**
 * The chooser of the app: a well with a raised thumb that slides to the chosen option. The
 * chosen option has `data-on`. The thumb looks for it each time the options are drawn, and it
 * slides when that option has a different place or width.
 */
export function Pill({ children, className }: { children: ReactNode; className?: string }) {
  const track = useRef<HTMLDivElement>(null);
  const thumb = useRef<HTMLSpanElement>(null);
  const was = useRef<{ left: number; width: number } | null>(null);

  useLayoutEffect(() => {
    const chosen = track.current?.querySelector<HTMLElement>("[data-on]");
    const pill = thumb.current;
    if (!chosen || !pill) return;
    const now = { left: chosen.offsetLeft, width: chosen.offsetWidth };
    pill.style.left = `${now.left}px`;
    pill.style.width = `${now.width}px`;
    pill.style.opacity = "1";
    const from = was.current;
    was.current = now;
    if (!from || (from.left === now.left && from.width === now.width)) return;
    play(
      pill,
      [
        { left: `${from.left}px`, width: `${from.width}px` },
        { left: `${now.left}px`, width: `${now.width}px` },
      ],
      springSnappy,
    );
  });

  return (
    <div
      ref={track}
      className={cn(
        "well relative inline-flex max-w-full items-center gap-0.5 overflow-x-auto rounded-full p-[3px]",
        className,
      )}
    >
      <span
        ref={thumb}
        aria-hidden
        className="sheet absolute top-[3px] bottom-[3px] rounded-full opacity-0"
      />
      {children}
    </div>
  );
}
