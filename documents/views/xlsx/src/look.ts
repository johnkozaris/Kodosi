/** The look of a cell, as the server holds it. */
export interface Look {
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
  color?: string;
  size?: number;
  font?: string;
  fill?: string;
  numberFormat?: string;
  align?: string;
  vertical?: string;
  wrap?: boolean;
}

/** The look of a cell, as Univer holds it. */
export interface UniverStyle {
  bl?: number;
  it?: number;
  ul?: { s: number };
  st?: { s: number };
  cl?: { rgb: string };
  fs?: number;
  ff?: string;
  bg?: { rgb: string };
  n?: { pattern: string };
  ht?: number;
  vt?: number;
  tb?: number;
}

export const ALIGN_TO_UNIVER: Record<string, number> = { left: 1, center: 2, right: 3 };
export const VERTICAL_TO_UNIVER: Record<string, number> = { top: 1, center: 2, bottom: 3 };
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
  if (look.strike) style.st = { s: 1 };
  if (look.color) style.cl = { rgb: hash(look.color) };
  if (look.size) style.fs = look.size;
  if (look.font) style.ff = look.font;
  if (look.fill) style.bg = { rgb: hash(look.fill) };
  if (look.numberFormat) style.n = { pattern: look.numberFormat };
  const align = look.align && ALIGN_TO_UNIVER[look.align];
  if (align) style.ht = align;
  const vertical = look.vertical && VERTICAL_TO_UNIVER[look.vertical];
  if (vertical) style.vt = vertical;
  if (look.wrap) style.tb = WRAP;
  return style;
}

export function fromUniver(style: UniverStyle): Look {
  const look: Look = {};
  if (style.bl) look.bold = true;
  if (style.it) look.italic = true;
  if (style.ul?.s) look.underline = true;
  if (style.st?.s) look.strike = true;
  if (style.cl?.rgb) look.color = bare(style.cl.rgb);
  if (style.fs) look.size = style.fs;
  if (style.ff) look.font = style.ff;
  if (style.bg?.rgb) look.fill = bare(style.bg.rgb);
  if (style.n?.pattern) look.numberFormat = style.n.pattern;
  for (const [name, value] of Object.entries(ALIGN_TO_UNIVER)) {
    if (style.ht === value) look.align = name;
  }
  for (const [name, value] of Object.entries(VERTICAL_TO_UNIVER)) {
    if (style.vt === value) look.vertical = name;
  }
  if (style.tb === WRAP) look.wrap = true;
  return look;
}
