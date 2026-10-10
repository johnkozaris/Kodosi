import { type ClassValue, clsx } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

// The text styles of styles.css are sizes, not colours. Without this, a size and a colour on the
// same element cancel each other.
const merge = extendTailwindMerge({
  extend: { theme: { text: ["caption", "footnote", "body", "callout", "headline", "title"] } },
});

export function cn(...inputs: ClassValue[]) {
  return merge(clsx(inputs));
}
