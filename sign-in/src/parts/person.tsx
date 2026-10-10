import { cn } from "./cn";

/** The letters of a person's circle: the first letters of two names, or the first of one. */
export function initials(name: string): string {
  return name
    .trim()
    .split(/\s+/)
    .map((part) => [...part.replace(/^@/, "")][0])
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

/**
 * The circle of the person who signs in. A person is a circle in the app, and your own circle is
 * copper. A circle with no letter yet is a slot that the person fills: a dashed line.
 */
export function Avatar({
  name,
  size = 28,
  className,
}: {
  name: string;
  size?: number;
  className?: string;
}) {
  const letters = initials(name);
  return (
    <span
      aria-hidden
      className={cn(
        "grid shrink-0 place-items-center rounded-full font-semibold transition-[background-color,box-shadow] duration-200",
        letters
          ? "bg-[linear-gradient(135deg,color-mix(in_srgb,var(--accent)_82%,white),var(--accent)_52%,color-mix(in_srgb,var(--accent)_84%,black))] text-on-accent shadow-[inset_0_0_0_0.5px_rgb(255_255_255/0.22),1px_2px_4px_rgb(0_0_0/0.22)]"
          : "border border-dashed border-ink-faint text-ink-faint",
        className,
      )}
      style={{ width: size, height: size, fontSize: size * 0.38 }}
    >
      {/* The letters come one time for each change, with the small squash of a mark of the app. */}
      {letters && (
        <span key={letters} className="pop leading-none">
          {letters}
        </span>
      )}
    </span>
  );
}
