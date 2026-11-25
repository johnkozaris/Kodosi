import { expect, test } from "bun:test";
import { getSchema } from "@tiptap/core";
import type { Attrs, Mark, ParseRule, Node as ProseNode, TagParseRule } from "@tiptap/pm/model";
import spec from "../../../crates/docx/schema.json";
import { extensions } from "./schema";
import { styleSheet } from "./styles";

const schema = getSchema(extensions);

/** Minimal HTMLElement stub built from a ProseMirror DOMOutputSpec for parseDOM tests. */
function element(rendered: unknown): HTMLElement {
  const [, attrs = {}, ...content] = rendered as [string, Record<string, unknown>?, ...unknown[]];
  return {
    getAttribute: (name: string) => (attrs[name] == null ? null : String(attrs[name])),
    textContent: content.filter((part) => typeof part === "string").join(""),
  } as unknown as HTMLElement;
}

function parsed(rules: readonly ParseRule[] | undefined, from: HTMLElement): Attrs | null {
  return (rules?.[0] as TagParseRule | undefined)?.getAttrs?.(from) || null;
}

function pastedNode(node: ProseNode): Attrs {
  return node.type.create(parsed(node.type.spec.parseDOM, element(node.type.spec.toDOM?.(node))))
    .attrs;
}

function pastedMark(mark: Mark): Attrs {
  const rendered = element(mark.type.spec.toDOM?.(mark, true));
  return mark.type.create(parsed(mark.type.spec.parseDOM, rendered)).attrs;
}

function runStyle(attrs: Record<string, unknown>): string {
  const look = schema.marks.textStyle;
  const [, shown] = look?.spec.toDOM?.(look.create(attrs), true) as [string, { style: string }];
  return shown.style;
}

test("the editor has the server's nodes in the server's order", () => {
  expect(Object.keys(schema.nodes)).toEqual(spec.nodes.map((node) => node.name));
});

test("the editor has the server's marks in the server's order", () => {
  expect(Object.keys(schema.marks)).toEqual(spec.marks.map((mark) => mark.name));
});

test("each node has the same content rule and attributes on both sides", () => {
  for (const node of spec.nodes) {
    const type = schema.nodes[node.name];
    expect(type?.spec.content ?? "", node.name).toBe(node.content ?? "");
    expect(Object.keys(type?.spec.attrs ?? {}).sort(), node.name).toEqual(
      Object.keys(node.attrs ?? {}).sort(),
    );
    expect(type?.spec.group ?? "", node.name).toBe(node.group ?? "");
    expect(Boolean(type?.spec.inline), node.name).toBe(Boolean(node.inline));
  }
});

test("each mark has the same attributes on both sides", () => {
  for (const mark of spec.marks) {
    expect(Object.keys(schema.marks[mark.name]?.spec.attrs ?? {}).sort(), mark.name).toEqual(
      Object.keys(mark.attrs ?? {}).sort(),
    );
  }
});

test("the colour and font of a run cannot add CSS of their own", () => {
  expect(runStyle({ color: "C00000", size: "24", font: "Times New Roman" })).toBe(
    'color:#C00000;font-size:12pt;font-family:"Times New Roman"',
  );
  expect(
    runStyle({
      color: "red;display:none",
      font: 'Arial\n;display:none;x:"',
    }),
  ).toBe('font-family:"Arial ;display:none;x:\\""');
});

test("the colour and font of a style cannot add CSS rules", () => {
  const sheet = styleSheet({
    styles: {
      defaults: { color: "000}body{display:none", font: 'x"}body{display:none}' },
      defaultParagraph: null,
      styles: {},
    },
    numbering: null,
    page: null,
  });
  expect(sheet.split("\n").slice(1)).toEqual([
    '.docx-page .tiptap{font-family:"x\\"}body{display:none}",sans-serif}',
    '.docx-page p{font-family:"x\\"}body{display:none}",sans-serif}',
  ]);
});

test("a copy keeps what a paragraph shows and leaves what names this file alone", () => {
  const paragraph = schema.nodes.paragraph?.create({
    id: "b1",
    style: "Heading1",
    align: "center",
    numId: "3",
    ilvl: "1",
    line: "360",
    indent: "720",
    props: "p1",
  });
  if (!paragraph) throw new Error("no paragraph");
  expect(pastedNode(paragraph)).toEqual({ ...paragraph.attrs, id: null, props: null });
});

test("a copy keeps the address of a link and what names a picture or a piece of the file", () => {
  const link = schema.marks.link?.create({ href: "https://example.com/", relation: "rId5" });
  if (!link) throw new Error("no link");
  expect(pastedMark(link)).toEqual({ href: "https://example.com/", relation: null });
  const image = schema.nodes.image?.create({
    asset: "ab".repeat(32),
    width: 96,
    height: 48,
    raw: "r1",
  });
  const piece = schema.nodes.inlineOpaque?.create({ raw: "r2", text: "PAGE 3" });
  const block = schema.nodes.opaque?.create({ id: "b2", raw: "r3", text: "" });
  if (!image || !piece || !block) throw new Error("no pieces");
  expect(pastedNode(image)).toEqual(image.attrs);
  expect(pastedNode(piece)).toEqual(piece.attrs);
  expect(pastedNode(block)).toEqual({ ...block.attrs, id: null });
});

test("pasted HTML gives the file only values in the forms that it allows", () => {
  const paragraph = schema.nodes.paragraph;
  const look = schema.marks.textStyle;
  if (!paragraph || !look) throw new Error("no paragraph");
  const fromPage = element([
    "p",
    {
      id: "b1",
      props: "p1",
      style: "text-align:center",
      "data-align": "middle",
      "data-list": "1;2",
      "data-line": "1.5",
      "data-indent": "wide",
    },
  ]);
  expect(paragraph.create(parsed(paragraph.spec.parseDOM, fromPage)).attrs).toEqual(
    paragraph.create().attrs,
  );
  const styled = element([
    "span",
    { "data-look": "", color: "red", size: "big", font: "x", highlight: "y" },
  ]);
  expect(look.create(parsed(look.spec.parseDOM, styled)).attrs).toEqual(look.create().attrs);
});
