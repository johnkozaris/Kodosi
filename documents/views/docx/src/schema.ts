import { type Extensions, Mark, Node } from "@tiptap/core";
import { cssAlign } from "./styles";

const attribute = () => ({ default: null, rendered: false });

/** A line of single spacing, as a multiple of the size of the letters, near what `normal` gives. */
const SINGLE_LINE = 1.2;

/** The colors of the highlight names of .docx, which differ from the CSS colors of the same names. */
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

const dataAttribute = (name: string, data: string) => ({
  default: null,
  renderHTML: (attrs: Record<string, unknown>) => (attrs[name] ? { [data]: attrs[name] } : {}),
});

/** Who made the mark that ends a paragraph a tracked insertion or deletion. */
const changeAttribute = (name: "inserted" | "deleted") => ({
  default: null,
  renderHTML: (attrs: Record<string, unknown>) => (attrs[name] ? { [`data-${name}`]: "" } : {}),
});

/** The nodes and marks of a document body, in the order of `crates/docx/schema.json`. */
export const nodes = [
  Node.create({ name: "doc", topNode: true, content: "block+" }),
  Node.create({
    name: "paragraph",
    group: "block",
    content: "inline*",
    addAttributes: () => ({
      id: attribute(),
      style: dataAttribute("style", "data-style"),
      align: {
        default: null,
        renderHTML: (attrs: Record<string, unknown>) =>
          attrs.align
            ? { "data-align": attrs.align, style: `text-align:${cssAlign(String(attrs.align))}` }
            : {},
      },
      numId: dataAttribute("numId", "data-list"),
      ilvl: dataAttribute("ilvl", "data-level"),
      line: {
        default: null,
        renderHTML: (attrs: Record<string, unknown>) => {
          const lines = Number(attrs.line) / 240;
          return lines > 0 ? { style: `line-height:${lines * SINGLE_LINE}` } : {};
        },
      },
      indent: {
        default: null,
        renderHTML: (attrs: Record<string, unknown>) =>
          attrs.indent == null ? {} : { style: `margin-left:${Number(attrs.indent) / 20}pt` },
      },
      props: attribute(),
      pageBreak: { default: false, rendered: false },
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
      span: { default: 1, renderHTML: (attrs) => ({ colspan: attrs.span }) },
      merged: { default: false, rendered: false },
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
      raw: attribute(),
      text: { default: "", rendered: false },
    }),
    parseHTML: () => [{ tag: "div[data-opaque]" }],
    renderHTML: ({ node }) => ["div", { "data-opaque": "" }, node.attrs.text || "…"],
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
      asset: { rendered: false },
      width: { rendered: false },
      height: { rendered: false },
      raw: attribute(),
    }),
    parseHTML: () => [{ tag: "img[data-asset]" }],
    renderHTML: ({ node }) => [
      "img",
      { "data-asset": node.attrs.asset, width: node.attrs.width, height: node.attrs.height },
    ],
  }),
  Node.create({
    name: "inlineOpaque",
    group: "inline",
    inline: true,
    atom: true,
    addAttributes: () => ({
      raw: { rendered: false },
      text: { default: "", rendered: false },
    }),
    parseHTML: () => [{ tag: "span[data-opaque]" }],
    renderHTML: ({ node }) => [
      "span",
      { "data-opaque": "", ...(node.attrs.text.includes("\f") ? { "data-page-break": "" } : {}) },
      node.attrs.text,
    ],
  }),
];

export const marks = [
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
      const parts = [
        color ? `color:#${color}` : "",
        size ? `font-size:${Number(size) / 2}pt` : "",
        font ? `font-family:"${String(font).replace(/["\\]/g, "")}"` : "",
        highlight && HIGHLIGHTS[highlight] ? `background:${HIGHLIGHTS[highlight]}` : "",
      ];
      return ["span", { "data-look": "", style: parts.filter(Boolean).join(";") }, 0];
    },
  }),
  Mark.create({
    name: "link",
    addAttributes: () => ({
      href: { rendered: false },
      relation: attribute(),
    }),
    parseHTML: () => [{ tag: "a[data-link]" }],
    renderHTML: ({ mark }) => ["a", { "data-link": "", title: mark.attrs.href }, 0],
  }),
  Mark.create({
    name: "insertion",
    addAttributes: () => ({
      author: { default: "", rendered: false },
      date: { default: "", rendered: false },
    }),
    parseHTML: () => [{ tag: "ins" }],
    renderHTML: ({ mark }) => ["ins", { title: mark.attrs.author }, 0],
  }),
  Mark.create({
    name: "deletion",
    addAttributes: () => ({
      author: { default: "", rendered: false },
      date: { default: "", rendered: false },
    }),
    parseHTML: () => [{ tag: "del" }],
    renderHTML: ({ mark }) => ["del", { title: mark.attrs.author }, 0],
  }),
];

export const extensions: Extensions = [...nodes, ...marks];
