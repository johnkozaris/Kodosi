import { cssColor, cssString } from "@documents/client";
import { type Extensions, Mark, Node } from "@tiptap/core";
import { MOST_SPAN } from "./generated/writable";
import { cssAlign } from "./styles";

/**
 * Non-rendered attribute ignored on paste. Explicit parseHTML is required because Tiptap
 * otherwise auto-parses matching HTML attributes (such as `id` or `style`) from pasted HTML.
 */
const attribute = (empty: unknown = null) => ({
  default: empty,
  rendered: false,
  parseHTML: () => null,
});

// Key referencing raw backing XML; paste.ts drops it unless the document holds that piece.
const pieceAttribute = () => ({
  default: null,
  rendered: false,
  parseHTML: (element: HTMLElement) => element.getAttribute("data-raw"),
});

const shownText = () => ({
  default: "",
  rendered: false,
  parseHTML: (element: HTMLElement) => element.textContent ?? "",
});

const WHOLE = /^\d+$/;
const SIGNED = /^-?\d+$/;
const ALIGNS = new Set([
  "left",
  "start",
  "center",
  "right",
  "end",
  "both",
  "distribute",
  "mediumKashida",
  "highKashida",
  "lowKashida",
  "thaiDistribute",
  "numTab",
]);

// Baseline single-line height multiplier matching typical CSS "normal" line-height.
const SINGLE_LINE = 1.2;

// OOXML highlight color values, which differ from CSS named colors (e.g. green is #00ff00).
const HIGHLIGHTS: Record<string, string> = {
  yellow: "#ffff00",
  green: "#00ff00",
  cyan: "#00ffff",
  magenta: "#ff00ff",
  blue: "#0000ff",
  red: "#ff0000",
  darkBlue: "#000080",
  darkCyan: "#008080",
  darkGreen: "#008000",
  darkMagenta: "#800080",
  darkRed: "#800000",
  darkYellow: "#808000",
  darkGray: "#808080",
  lightGray: "#c0c0c0",
  black: "#000000",
  white: "#ffffff",
};

/**
 * Attribute round-tripped via a data-* attribute and validated on paste against `allowed`,
 * since clipboard HTML can come from any source.
 */
const dataAttribute = (
  name: string,
  data: string,
  allowed: (value: string) => boolean,
  style?: (value: string) => string | null,
) => ({
  default: null,
  parseHTML: (element: HTMLElement) => {
    const value = element.getAttribute(data);
    return value !== null && allowed(value) ? value : null;
  },
  renderHTML: (attrs: Record<string, unknown>) => {
    const value = attrs[name];
    if (!value) return {};
    const css = style?.(String(value));
    return css ? { [data]: value, style: css } : { [data]: value };
  },
});

const changeAttribute = (name: "inserted" | "deleted") => ({
  default: null,
  parseHTML: () => null,
  renderHTML: (attrs: Record<string, unknown>) => (attrs[name] ? { [`data-${name}`]: "" } : {}),
});

function pixels(element: HTMLElement, name: string): number | null {
  const value = Number(element.getAttribute(name));
  return Number.isFinite(value) && value > 0 ? value : null;
}

export function cellSpan(value: unknown): number {
  const span = Number(value);
  return Number.isInteger(span) && span > 0 ? Math.min(span, MOST_SPAN) : 1;
}

function columns(element: HTMLElement): number {
  return cellSpan(element.getAttribute("colspan"));
}

// Node and mark order must match crates/docx/schema.json.
const nodes = [
  Node.create({ name: "doc", topNode: true, content: "block+" }),
  Node.create({
    name: "paragraph",
    group: "block",
    content: "inline*",
    addAttributes: () => ({
      id: attribute(),
      style: dataAttribute("style", "data-style", Boolean),
      align: dataAttribute(
        "align",
        "data-align",
        (value) => ALIGNS.has(value),
        (value) => `text-align:${cssAlign(value)}`,
      ),
      numId: dataAttribute("numId", "data-list", (value) => WHOLE.test(value)),
      ilvl: dataAttribute("ilvl", "data-level", (value) => WHOLE.test(value)),
      line: dataAttribute(
        "line",
        "data-line",
        (value) => SIGNED.test(value),
        (value) => {
          const lines = Number(value) / 240;
          return lines > 0 ? `line-height:${lines * SINGLE_LINE}` : null;
        },
      ),
      indent: dataAttribute(
        "indent",
        "data-indent",
        (value) => SIGNED.test(value),
        (value) => `margin-left:${Number(value) / 20}pt`,
      ),
      props: attribute(),
      pageBreak: attribute(false),
      inserted: changeAttribute("inserted"),
      deleted: changeAttribute("deleted"),
    }),
    parseHTML: () => [{ tag: "p" }],
    renderHTML: ({ HTMLAttributes }) => ["p", HTMLAttributes, 0],
  }),
  Node.create({
    name: "table",
    group: "block",
    content: "tableRow+",
    isolating: true,
    addAttributes: () => ({ id: attribute(), props: attribute() }),
    parseHTML: () => [{ tag: "table" }],
    renderHTML: () => ["table", ["tbody", 0]],
  }),
  Node.create({
    name: "tableRow",
    content: "tableCell+",
    addAttributes: () => ({ props: attribute() }),
    parseHTML: () => [{ tag: "tr" }],
    renderHTML: () => ["tr", 0],
  }),
  Node.create({
    name: "tableCell",
    content: "block+",
    isolating: true,
    addAttributes: () => ({
      props: attribute(),
      span: { default: 1, parseHTML: columns, renderHTML: (attrs) => ({ colspan: attrs.span }) },
      merged: attribute(false),
    }),
    parseHTML: () => [{ tag: "td" }],
    renderHTML: ({ HTMLAttributes }) => ["td", HTMLAttributes, 0],
  }),
  Node.create({
    name: "opaque",
    group: "block",
    atom: true,
    addAttributes: () => ({
      id: attribute(),
      raw: pieceAttribute(),
      text: shownText(),
    }),
    parseHTML: () => [{ tag: "div[data-opaque]" }],
    renderHTML: ({ node }) => [
      "div",
      { "data-opaque": "", "data-raw": node.attrs.raw },
      node.attrs.text,
    ],
  }),
  Node.create({ name: "text", group: "inline" }),
  Node.create({
    name: "hardBreak",
    group: "inline",
    inline: true,
    parseHTML: () => [{ tag: "br" }],
    renderHTML: () => ["br"],
  }),
  Node.create({
    name: "pageBreak",
    group: "inline",
    inline: true,
    selectable: false,
    parseHTML: () => [{ tag: "span[data-page-break]:not([data-opaque])" }],
    renderHTML: () => ["span", { "data-page-break": "" }],
  }),
  Node.create({
    name: "tab",
    group: "inline",
    inline: true,
    parseHTML: () => [{ tag: "span[data-tab]" }],
    renderHTML: () => ["span", { "data-tab": "" }, "\t"],
  }),
  Node.create({
    name: "image",
    group: "inline",
    inline: true,
    atom: true,
    addAttributes: () => ({
      asset: { rendered: false, parseHTML: (element) => element.getAttribute("data-asset") },
      width: { rendered: false, parseHTML: (element) => pixels(element, "width") },
      height: { rendered: false, parseHTML: (element) => pixels(element, "height") },
      raw: pieceAttribute(),
    }),
    parseHTML: () => [{ tag: "img[data-asset]" }],
    renderHTML: ({ node }) => [
      "img",
      {
        "data-asset": node.attrs.asset,
        "data-raw": node.attrs.raw,
        width: node.attrs.width,
        height: node.attrs.height,
      },
    ],
  }),
  Node.create({
    name: "inlineOpaque",
    group: "inline",
    inline: true,
    atom: true,
    addAttributes: () => ({
      raw: pieceAttribute(),
      text: shownText(),
    }),
    parseHTML: () => [{ tag: "span[data-opaque]" }],
    renderHTML: ({ node }) => [
      "span",
      {
        "data-opaque": "",
        "data-raw": node.attrs.raw,
        ...(node.attrs.text.includes("\f") ? { "data-page-break": "" } : {}),
      },
      node.attrs.text,
    ],
  }),
];

const marks = [
  Mark.create({
    name: "run",
    excludes: "",
    addAttributes: () => ({ props: { rendered: false } }),
    renderHTML: () => ["span", 0],
    parseHTML: () => [],
  }),
  Mark.create({
    name: "bold",
    parseHTML: () => [{ tag: "strong" }, { tag: "b" }],
    renderHTML: () => ["strong", 0],
  }),
  Mark.create({
    name: "italic",
    parseHTML: () => [{ tag: "em" }, { tag: "i" }],
    renderHTML: () => ["em", 0],
  }),
  Mark.create({
    name: "underline",
    parseHTML: () => [{ tag: "u" }],
    renderHTML: () => ["u", 0],
  }),
  Mark.create({
    name: "strike",
    parseHTML: () => [{ tag: "s" }],
    renderHTML: () => ["s", 0],
  }),
  Mark.create({
    name: "superscript",
    excludes: "subscript",
    parseHTML: () => [{ tag: "sup" }],
    renderHTML: () => ["sup", 0],
  }),
  Mark.create({
    name: "subscript",
    excludes: "superscript",
    parseHTML: () => [{ tag: "sub" }],
    renderHTML: () => ["sub", 0],
  }),
  Mark.create({
    name: "textStyle",
    addAttributes: () => ({
      color: attribute(),
      size: attribute(),
      font: attribute(),
      highlight: attribute(),
    }),
    parseHTML: () => [{ tag: "span[data-look]" }],
    renderHTML: ({ mark }) => {
      const { color, size, font, highlight } = mark.attrs;
      const shown = cssColor(color);
      const parts = [
        shown ? `color:${shown}` : "",
        size ? `font-size:${Number(size) / 2}pt` : "",
        font ? `font-family:${cssString(String(font))}` : "",
        highlight && HIGHLIGHTS[highlight] ? `background:${HIGHLIGHTS[highlight]}` : "",
      ];
      return ["span", { "data-look": "", style: parts.filter(Boolean).join(";") }, 0];
    },
  }),
  Mark.create({
    name: "link",
    addAttributes: () => ({
      href: { rendered: false, parseHTML: (element) => element.getAttribute("data-href") },
      relation: attribute(),
    }),
    parseHTML: () => [{ tag: "a[data-link][data-href]" }],
    renderHTML: ({ mark }) => [
      "a",
      { "data-link": "", "data-href": mark.attrs.href, title: mark.attrs.href },
      0,
    ],
  }),
  Mark.create({
    name: "insertion",
    addAttributes: () => ({
      author: attribute(""),
      date: attribute(""),
    }),
    parseHTML: () => [{ tag: "ins" }],
    renderHTML: ({ mark }) => ["ins", { title: mark.attrs.author }, 0],
  }),
  Mark.create({
    name: "deletion",
    addAttributes: () => ({
      author: attribute(""),
      date: attribute(""),
    }),
    parseHTML: () => [{ tag: "del" }],
    renderHTML: ({ mark }) => ["del", { title: mark.attrs.author }, 0],
  }),
];

export const extensions: Extensions = [...nodes, ...marks];
