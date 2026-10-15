import { type ClassValue, clsx } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

const merge = extendTailwindMerge({
  extend: { theme: { text: ["caption", "footnote", "body", "callout", "headline", "title"] } },
});

export function cn(...inputs: ClassValue[]) {
  return merge(clsx(inputs));
}
