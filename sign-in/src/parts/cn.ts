import { extendTailwindMerge } from "tailwind-merge";

// The text styles of styles.css are sizes: without this, a size and a colour cancel each other.
export const cn = extendTailwindMerge({
  extend: { theme: { text: ["caption", "footnote", "callout", "title"] } },
});
