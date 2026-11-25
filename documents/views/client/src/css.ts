const HEX = /^#?([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
const NAMED = /^[a-z]+$/i;
const FUNCTION = /^(?:rgba?|hsla?)\((?:[\d\s.,%/+-]|deg|g?rad|turn|none)*\)$/i;

/**
 * Sanitizes an untrusted CSS color (hex with or without #, named color, rgb(), or hsl()).
 * Rejects anything else to prevent CSS injection.
 */
export function cssColor(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const hex = HEX.exec(value)?.[1];
  if (hex) return `#${hex}`;
  return NAMED.test(value) || FUNCTION.test(value) ? value : undefined;
}

/** Quotes and escapes untrusted text for safe use as a CSS string literal. */
export function cssString(text: string): string {
  return `"${text.replace(/["\\]/g, "\\$&").replace(/[\n\r\f]/g, " ")}"`;
}
