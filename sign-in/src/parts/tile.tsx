import type { ReactNode } from "react";

const TINT = {
  copper: "var(--accent)",
  violet: "#8c7bd1",
  teal: "#4f9bb0",
  shell: "#54433a",
  graphite: "#5f6b7a",
  green: "#4f9a76",
  red: "#c9524b",
} as const;

export type Tint = keyof typeof TINT;

export function IconTile({ tint, icon, waits }: { tint: Tint; icon: ReactNode; waits?: boolean }) {
  const colour = TINT[tint];
  return (
    <span
      aria-hidden
      className="squircle relative inline-grid shrink-0 place-items-center text-white [&_svg]:size-[0.52em] [&_svg]:stroke-[2.2]"
      style={{
        width: 30,
        height: 30,
        fontSize: 30,
        background: `linear-gradient(135deg, color-mix(in srgb, ${colour} 78%, white), ${colour} 52%, color-mix(in srgb, ${colour} 80%, black))`,
        boxShadow:
          "inset 0.5px 1px 0 rgb(255 255 255 / 0.4), inset -0.5px -1px 0 rgb(0 0 0 / 0.18), 1px 2px 4px rgb(0 0 0 / 0.22)",
      }}
    >
      {icon}
      {waits && (
        <span className="breathe absolute -top-0.5 -right-0.5 size-2 rounded-full bg-accent text-accent ring-2 ring-raised" />
      )}
    </span>
  );
}
