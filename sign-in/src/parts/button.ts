import { cn } from "./cn";

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md" | "lg";

const base =
  "relative inline-flex select-none items-center justify-center gap-2 whitespace-nowrap rounded-full font-medium " +
  "transition-[background-color,box-shadow,color,opacity,scale] duration-200 ease-out active:scale-[0.97] " +
  "disabled:pointer-events-none disabled:opacity-45 [&_svg]:shrink-0";

const variants: Record<Variant, string> = {
  primary: "bg-accent-fill text-on-accent shadow-press hover:bg-accent-hover",
  secondary: "sheet text-ink hover:bg-lifted",
  ghost: "text-ink-muted hover:bg-well/70 hover:text-ink",
  danger: "bg-danger text-on-danger shadow-press hover:opacity-90",
};

const sizes: Record<Size, string> = {
  sm: "h-8 px-3 text-footnote [&_svg]:size-4 pointer-coarse:h-11",
  md: "h-10 px-4 text-[14.5px] [&_svg]:size-[17px] pointer-coarse:h-11",
  lg: "h-11 px-5 text-[15px] [&_svg]:size-[18px]",
};

/** The look of an action: a capsule, as each action of the app. */
export function buttonClass({
  variant = "secondary",
  size = "md",
  className,
}: {
  variant?: Variant;
  size?: Size;
  className?: string;
} = {}) {
  return cn(base, variants[variant], sizes[size], className);
}
