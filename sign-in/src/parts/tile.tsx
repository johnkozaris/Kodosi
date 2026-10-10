import type { ReactNode } from "react";

import { cn } from "./cn";

// The tints of the app's marks: the copper of an action, the tints of rooms and of programs.
const TINT = {
  copper: "var(--accent)",
  amber: "#d9a441",
  rose: "#c96f7e",
  violet: "#8c7bd1",
  teal: "#4f9bb0",
  blue: "#607fcc",
  shell: "#54433a",
  graphite: "#5f6b7a",
  green: "#4f9a76",
  red: "#c9524b",
} as const;

export type Tint = keyof typeof TINT;

/** A tile names a kind of thing: a rounded square, lit from the top left. It is no button. */
export function IconTile({
  tint,
  icon,
  size = 30,
  className,
}: {
  tint: Tint;
  icon: ReactNode;
  size?: number;
  className?: string;
}) {
  const colour = TINT[tint];
  return (
    <span
      aria-hidden
      className={cn(
        "squircle relative inline-grid shrink-0 place-items-center text-white [&_svg]:size-[0.52em] [&_svg]:stroke-[2.2]",
        className,
      )}
      style={{
        width: size,
        height: size,
        fontSize: size,
        background: `linear-gradient(135deg, color-mix(in srgb, ${colour} 78%, white), ${colour} 52%, color-mix(in srgb, ${colour} 80%, black))`,
        boxShadow:
          "inset 0.5px 1px 0 rgb(255 255 255 / 0.4), inset -0.5px -1px 0 rgb(0 0 0 / 0.18), 1px 2px 4px rgb(0 0 0 / 0.22)",
      }}
    >
      {icon}
    </span>
  );
}
