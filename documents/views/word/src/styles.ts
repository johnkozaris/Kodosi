export interface StyleLook {
  name?: string;
  /** Whom the style is for: a paragraph, a run of letters, a table or a list. */
  kind?: string;
  /** Word shows the style in its gallery. */
  quick?: boolean;
  bold?: boolean;
  italic?: boolean;
  size?: number;
  color?: string;
  font?: string;
  align?: string;
  before?: number;
  after?: number;
  line?: number;
  indent?: number;
  firstLine?: number;
  hanging?: number;
  keepNext?: boolean;
  pageBreak?: boolean;
}

export interface Level extends StyleLook {
  format?: string;
  text?: string;
  start?: number;
}

export interface WordView {
  styles: {
    defaults: StyleLook;
    defaultParagraph: string | null;
    styles: Record<string, StyleLook>;
  } | null;
  numbering: Record<string, Level[]> | null;
  page: {
    width: number | null;
    height: number | null;
    top: number | null;
    right: number | null;
    bottom: number | null;
    left: number | null;
  } | null;
}

const POINT_PIXELS = 96 / 72;
const PAGE_WIDTH = 595;
const PAGE_HEIGHT = 842;
const PAGE_MARGIN = 72;

/** The size of a page in CSS pixels. */
export function pageSize(view: WordView) {
  const page = view.page;
  return {
    width: (page?.width ?? PAGE_WIDTH) * POINT_PIXELS,
    height: (page?.height ?? PAGE_HEIGHT) * POINT_PIXELS,
    top: (page?.top ?? PAGE_MARGIN) * POINT_PIXELS,
    bottom: (page?.bottom ?? PAGE_MARGIN) * POINT_PIXELS,
  };
}

const COUNTER_STYLES: Record<string, string> = {
  decimal: "decimal",
  decimalZero: "decimal-leading-zero",
  lowerLetter: "lower-alpha",
  upperLetter: "upper-alpha",
  lowerRoman: "lower-roman",
  upperRoman: "upper-roman",
};

/** A bullet in a symbol font has a private letter that other fonts draw as a box. */
function bulletOf(text: string | undefined): string {
  const code = text?.codePointAt(0) ?? 0;
  return !text || (code >= 0xe000 && code <= 0xf8ff) ? "•" : text;
}

function cssText(text: string): string {
  return `"${text.replace(/["\\]/g, "\\$&").replace(/[\n\r\f]/g, " ")}"`;
}

function counterOf(list: string, level: number): string {
  return `list-${list.replace(/[^\w-]/g, "")}-${level}`;
}

/** The mark before an item: a bullet, or the level's words with each level's number in them. */
function markerOf(list: string, levels: readonly Level[], index: number): string {
  const level = levels[index];
  if (level?.format === "bullet") return cssText(bulletOf(level.text));
  const parts = (level?.text ?? `%${index + 1}.`)
    .split(/(%[1-9])/)
    .filter(Boolean)
    .map((part) => {
      const at = /^%([1-9])$/.exec(part)?.[1];
      if (at === undefined) return cssText(part);
      const depth = Number(at) - 1;
      const style = COUNTER_STYLES[levels[depth]?.format ?? "decimal"] ?? "decimal";
      return `counter(${counterOf(list, depth)},${style})`;
    });
  return parts.length > 0 ? parts.join(" ") : '""';
}

function alignOf(align: string | undefined): string | undefined {
  if (align === "both") return "justify";
  if (align === "start") return "left";
  if (align === "end") return "right";
  return align;
}

function rules(look: StyleLook): string {
  const parts = [
    look.bold === undefined ? "" : `font-weight:${look.bold ? 700 : 400}`,
    look.italic === undefined ? "" : `font-style:${look.italic ? "italic" : "normal"}`,
    look.size ? `font-size:${look.size}pt` : "",
    look.color ? `color:#${look.color}` : "",
    look.font ? `font-family:"${look.font}",sans-serif` : "",
    look.align ? `text-align:${alignOf(look.align)}` : "",
    look.before === undefined ? "" : `margin-top:${look.before}pt`,
    look.after === undefined ? "" : `margin-bottom:${look.after}pt`,
    look.indent === undefined ? "" : `margin-left:${look.indent}pt`,
    look.firstLine === undefined ? "" : `text-indent:${look.firstLine}pt`,
    look.hanging === undefined ? "" : `text-indent:-${look.hanging}pt`,
  ];
  return parts.filter(Boolean).join(";");
}

/** The look of a file, as a style sheet for its page. */
export function styleSheet(view: WordView): string {
  const page = view.page;
  const sheet = [
    `.word-page .tiptap{width:${page?.width ?? PAGE_WIDTH}pt;` +
      `padding:${page?.top ?? PAGE_MARGIN}pt ${page?.right ?? PAGE_MARGIN}pt 0 ` +
      `${page?.left ?? PAGE_MARGIN}pt;}`,
    `.word-page .tiptap{${rules(view.styles?.defaults ?? {})}}`,
    `.word-page p{${rules(view.styles?.defaults ?? {})}}`,
  ];
  const fallback = view.styles?.defaultParagraph;
  if (fallback && view.styles?.styles[fallback]) {
    sheet.push(`.word-page p:not([data-style]){${rules(view.styles.styles[fallback])}}`);
  }
  for (const [id, look] of Object.entries(view.styles?.styles ?? {})) {
    sheet.push(`.word-page p[data-style="${CSS.escape(id)}"]{${rules(look)}}`);
  }
  for (const [list, levels] of Object.entries(view.numbering ?? {})) {
    for (const [index, level] of levels.entries()) {
      const item =
        `.word-page p[data-list="${CSS.escape(list)}"]` +
        (index === 0 ? `:is([data-level="0"],:not([data-level]))` : `[data-level="${index}"]`);
      const deeper = levels
        .slice(index + 1)
        .map((_, offset) => counterOf(list, index + 1 + offset))
        .join(" ");
      sheet.push(
        `${item}{${rules(level)};counter-increment:${counterOf(list, index)};` +
          `${deeper ? `counter-reset:${deeper};` : ""}}`,
      );
      sheet.push(
        `${item}::before{content:${markerOf(list, levels, index)};display:inline-block;min-width:18pt;}`,
      );
    }
  }
  return sheet.join("\n");
}
