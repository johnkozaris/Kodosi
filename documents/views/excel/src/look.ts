/** The look of a cell, as the server holds it. */
export interface Look {
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  color?: string;
  size?: number;
  font?: string;
  fill?: string;
  numberFormat?: string;
  align?: string;
  wrap?: boolean;
}

/** The look of a cell, as Univer holds it. */
export interface UniverStyle {
  bl?: number;
  it?: number;
  ul?: { s: number };
  cl?: { rgb: string };
  fs?: number;
  ff?: string;
  bg?: { rgb: string };
  n?: { pattern: string };
  ht?: number;
  tb?: number;
}

const ALIGN_TO_UNIVER: Record<string, number> = { left: 1, center: 2, right: 3 };
const WRAP = 3;

function hash(color: string): string {
  return `#${color}`;
}

function bare(color: string): string {
  return color.replace(/^#/, "").toUpperCase();
}

export function toUniver(look: Look): UniverStyle {
  const style: UniverStyle = {};
  if (look.bold) style.bl = 1;
  if (look.italic) style.it = 1;
  if (look.underline) style.ul = { s: 1 };
  if (look.color) style.cl = { rgb: hash(look.color) };
  if (look.size) style.fs = look.size;
  if (look.font) style.ff = look.font;
  if (look.fill) style.bg = { rgb: hash(look.fill) };
  if (look.numberFormat) style.n = { pattern: look.numberFormat };
  const align = look.align && ALIGN_TO_UNIVER[look.align];
  if (align) style.ht = align;
  if (look.wrap) style.tb = WRAP;
  return style;
}

export function fromUniver(style: UniverStyle): Look {
  const look: Look = {};
  if (style.bl) look.bold = true;
  if (style.it) look.italic = true;
  if (style.ul?.s) look.underline = true;
  if (style.cl?.rgb) look.color = bare(style.cl.rgb);
  if (style.fs) look.size = style.fs;
  if (style.ff) look.font = style.ff;
  if (style.bg?.rgb) look.fill = bare(style.bg.rgb);
  if (style.n?.pattern) look.numberFormat = style.n.pattern;
  for (const [name, value] of Object.entries(ALIGN_TO_UNIVER)) {
    if (style.ht === value) look.align = name;
  }
  if (style.tb === WRAP) look.wrap = true;
  return look;
}
