import { extendTailwindMerge } from "tailwind-merge";

// Register custom text-* font sizes so tailwind-merge does not treat them as conflicting with text
// color classes.
export const cn = extendTailwindMerge({
  extend: { theme: { text: ["caption", "footnote", "callout", "title"] } },
});
