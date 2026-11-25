import { cssColor, cssString } from "@documents/client";
import { say } from "@documents/words";
import type { Mark, MarkSpec, NodeSpec } from "@tiptap/pm/model";
import { Node as ProseNode, Schema } from "@tiptap/pm/model";
import type { EditorState } from "@tiptap/pm/state";
import { sendableSteps } from "prosemirror-collab";
import spec from "../../../crates/pptx/schema.json";

const POINTS_PER_SIZE_UNIT = 0.01;
const PIXELS_PER_POINT = 96 / 72;
const EMU_PER_PIXEL = 9525;
const DEFAULT_POINTS = 18;
const LINE_HEIGHT = 1.2;
const INDENT_PIXELS = 24;
const BULLET_GAP = 16;
const DEFAULT_FONT = "Calibri";
const SCHEME_NAMES: Record<string, string> = { bg1: "lt1", tx1: "dk1", bg2: "lt2", tx2: "dk2" };

export type Theme = Record<string, string>;

export interface Level {
  size?: string;
  bold?: boolean;
  italic?: boolean;
  color?: string;
  font?: string;
  align?: string;
  bullet?: boolean;
  margin?: number;
}

export interface TextLook {
  levels: Level[];
  anchor: string | null;
  scale: number | null;
}

export interface Link {
  href?: string | null;
  slide?: string | null;
  jump?: string | null;
}

interface TextStyle {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  color: string | null;
  points: number;
  font: string;
  link: Link | null;
}

function own(names: Record<string, string> | null | undefined, name: string): string | undefined {
  return names && Object.hasOwn(names, name) ? names[name] : undefined;
}

export function colorIn(
  theme: Theme | null | undefined,
  value: string | null | undefined,
): string | undefined {
  if (!value?.startsWith("scheme:")) return cssColor(value);
  const name = value.slice("scheme:".length);
  return cssColor(own(theme, own(SCHEME_NAMES, name) ?? name));
}

/** Resolves OOXML theme font tokens (e.g. +mj-lt, +mn-lt) and builds a CSS font-family list. */
export function familyIn(theme: Theme | null | undefined, font: string): string {
  return `${cssString(own(theme, font) ?? font)}, Carlito, sans-serif`;
}

function styleOf(marks: readonly Mark[], level: Level = {}, scale = 1): TextStyle {
  const style = marks.find((mark) => mark.type.name === "textStyle")?.attrs;
  const has = (name: string) => marks.some((mark) => mark.type.name === name);
  const link = (marks.find((mark) => mark.type.name === "link")?.attrs as Link | undefined) ?? null;
  const size = Number(style?.size ?? level.size ?? 0);
  return {
    bold: has("bold") || level.bold === true,
    italic: has("italic") || level.italic === true,
    underline: has("underline") || link !== null,
    color: (style?.color as string | null) ?? (link ? "scheme:hlink" : null) ?? level.color ?? null,
    points: (size ? size * POINTS_PER_SIZE_UNIT : DEFAULT_POINTS) * scale,
    font: (style?.font as string | null) ?? level.font ?? DEFAULT_FONT,
    link,
  };
}

function cssFont(style: TextStyle): string {
  const pixels = style.points * PIXELS_PER_POINT;
  return `${style.italic ? "italic " : ""}${style.bold ? "bold " : ""}${pixels}px ${style.font}`;
}

function alignOf(align: string): string {
  return { ctr: "center", r: "right", just: "justify" }[align] ?? "left";
}

const NODE_DOM: Record<string, NodeSpec["toDOM"]> = {
  paragraph: (node) => [
    "p",
    {
      "data-level": node.attrs.level,
      ...(node.attrs.align ? { style: `text-align:${alignOf(node.attrs.align)}` } : {}),
    },
    0,
  ],
  hardBreak: () => ["br"],
  inlineOpaque: (node) => ["span", String(node.attrs.text)],
};

const MARK_DOM: Record<string, MarkSpec["toDOM"]> = {
  run: () => ["span", 0],
  bold: () => ["strong", 0],
  italic: () => ["em", 0],
  underline: () => ["u", 0],
  link: (mark) => ["a", { "data-link": "", title: linkTitle(mark.attrs as Link) }, 0],
  textStyle: (mark) => {
    const look = styleOf([mark]);
    const color = colorIn(null, look.color);
    const parts = [
      color ? `color:${color}` : "",
      `font-size:${look.points * PIXELS_PER_POINT}px`,
      // Theme font tokens (+mj-lt/+mn-lt) require the slide theme, so let levelRules supply them.
      look.font.startsWith("+") ? "" : `font-family:${familyIn(null, look.font)}`,
    ];
    return ["span", { style: parts.filter(Boolean).join(";") }, 0];
  },
};

function specOf<Spec extends object>(
  entries: readonly ({ name: string } & Spec)[],
  dom: Record<string, unknown>,
): Record<string, Spec> {
  return Object.fromEntries(
    entries.map(({ name, ...rest }) => [name, { ...rest, toDOM: dom[name] } as unknown as Spec]),
  );
}

export const schema = new Schema({
  nodes: specOf<NodeSpec>(spec.nodes as ({ name: string } & NodeSpec)[], NODE_DOM),
  marks: specOf<MarkSpec>(spec.marks as ({ name: string } & MarkSpec)[], MARK_DOM),
});

interface Piece {
  text: string;
  x: number;
  width: number;
  font: string;
  color: string | null;
  underline: boolean;
  link: Link | null;
}

interface Line {
  baseline: number;
  height: number;
  pieces: Piece[];
}

export function linkTitle(link: Link): string {
  if (link.href) return link.href;
  if (link.slide) return say("Another slide");
  return (
    {
      nextslide: say("Next slide"),
      previousslide: say("Previous slide"),
      firstslide: say("First slide"),
      lastslide: say("Last slide"),
    }[link.jump ?? ""] ?? ""
  );
}

interface TextWord extends Piece {
  height: number;
}

export interface Measure {
  width(font: string, text: string): number;
  face(font: string): string;
}

const IDEOGRAPHIC = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/u;
const NEVER_STARTS =
  "、。，．・：；？！ー々〻ゝゞヽヾぁぃぅぇぉっゃゅょゎゕゖァィゥェォッャュョヮヵヶ」』）〕】〉》〟’”…‥";
const NEVER_ENDS = "「『（〔【〈《〝‘“";

/**
 * Splits text into line-break opportunities (spaces plus CJK character boundaries with kinsoku
 * rules) so canvas wrapping matches browser line breaking for Chinese and Japanese.
 */
function breakable(text: string): string[] {
  const pieces: string[] = [];
  for (const word of text.match(/\S+\s*|\s+/g) ?? []) {
    let piece = "";
    let before = "";
    for (const character of word) {
      if (
        piece !== "" &&
        (IDEOGRAPHIC.test(character) || IDEOGRAPHIC.test(before)) &&
        !/\s/u.test(character) &&
        !NEVER_STARTS.includes(character) &&
        !NEVER_ENDS.includes(before)
      ) {
        pieces.push(piece);
        piece = "";
      }
      piece += character;
      before = character;
    }
    pieces.push(piece);
  }
  return pieces;
}

function wordsOf(
  paragraph: ProseNode,
  level: Level,
  scale: number,
  measure: Measure,
): TextWord[][] {
  const lines: TextWord[][] = [[]];
  const push = (text: string, style: TextStyle) => {
    const font = cssFont({ ...style, font: measure.face(style.font) });
    for (const word of breakable(text)) {
      lines.at(-1)?.push({
        text: word,
        x: 0,
        width: measure.width(font, word),
        font,
        color: style.color,
        underline: style.underline,
        link: style.link,
        height: style.points * PIXELS_PER_POINT,
      });
    }
  };
  paragraph.forEach((node) => {
    if (node.type.name === "hardBreak") lines.push([]);
    else if (node.isText) push(node.text ?? "", styleOf(node.marks, level, scale));
    else push(String(node.attrs.text ?? ""), styleOf(node.marks, level, scale));
  });
  return lines;
}

function wrapped(words: TextWord[], width: number): TextWord[][] {
  const lines: TextWord[][] = [[]];
  let used = 0;
  for (const word of words) {
    if (used + word.width > width && (lines.at(-1)?.length ?? 0) > 0) {
      lines.push([]);
      used = 0;
    }
    lines.at(-1)?.push(word);
    used += word.width;
  }
  return lines;
}

export function levelRules(look: TextLook | null, theme: Theme | null): string {
  const scale = look?.scale ?? 1;
  return (look?.levels ?? [])
    .map((level, index) => {
      const style = styleOf([], level, scale);
      const margin =
        level.margin === undefined ? index * INDENT_PIXELS : level.margin / EMU_PER_PIXEL;
      const rules = [
        `font:${cssFont({ ...style, font: familyIn(theme, style.font) })}`,
        `line-height:${LINE_HEIGHT}`,
        `margin:0 0 0 ${margin}px`,
        `text-align:${alignOf(level.align ?? "l")}`,
        `color:${colorIn(theme, style.color) ?? "inherit"}`,
      ];
      const selector = `.slides-text p[data-level="${index}"]`;
      const bullet = level.bullet
        ? `${selector}::before{content:"•";position:absolute;margin-left:-${BULLET_GAP}px;}`
        : "";
      return `${selector}{${rules.join(";")}}${bullet}`;
    })
    .join("\n");
}

export function layOut(
  tree: ProseNode,
  width: number,
  height: number,
  look: TextLook | null,
  measure: Measure,
): Line[] {
  const out: Line[] = [];
  const scale = look?.scale ?? 1;
  let top = 0;
  tree.forEach((paragraph) => {
    const number = Number(paragraph.attrs.level);
    const level = look?.levels[number] ?? {};
    const indent =
      level.margin === undefined ? number * INDENT_PIXELS : level.margin / EMU_PER_PIXEL;
    const bullet = paragraph.attrs.bullet === true || level.bullet === true;
    const start = indent + (bullet && level.margin === undefined ? BULLET_GAP : 0);
    const room = Math.max(1, width - start);
    let first = true;
    for (const part of wordsOf(paragraph, level, scale, measure)) {
      for (const words of wrapped(part, room)) {
        const lineHeight = Math.max(
          (level.size ? Number(level.size) * POINTS_PER_SIZE_UNIT * scale : DEFAULT_POINTS) *
            PIXELS_PER_POINT,
          ...words.map((word) => word.height),
        );
        const used = words.reduce((sum, word) => sum + word.width, 0);
        const free = room - used;
        const align = paragraph.attrs.align ?? level.align;
        let x = start + (align === "ctr" ? free / 2 : align === "r" ? free : 0);
        const pieces: Piece[] = [];
        if (first && bullet) {
          pieces.push({
            text: "•",
            x: Math.max(0, indent - BULLET_GAP),
            width: BULLET_GAP,
            font: words[0]?.font ?? `${DEFAULT_POINTS}px ${measure.face(DEFAULT_FONT)}`,
            color: words[0]?.color ?? null,
            underline: false,
            link: null,
          });
        }
        for (const word of words) {
          pieces.push({ ...word, x });
          x += word.width;
        }
        out.push({ baseline: top + lineHeight, height: lineHeight, pieces });
        top += lineHeight * LINE_HEIGHT;
        first = false;
      }
    }
  });
  const free = Math.max(0, height - top);
  const shift = look?.anchor === "ctr" ? free / 2 : look?.anchor === "b" ? free : 0;
  return out.map((line) => ({ ...line, baseline: line.baseline + shift }));
}

/**
 * Collects visible text from shape editors with unacknowledged steps that differ from the reloaded
 * snapshot, so the user can recover edits that are about to be replaced.
 */
export function unsentText(
  texts: ReadonlyMap<string, EditorState>,
  shown: (key: string) => unknown,
): string {
  const words: string[] = [];
  for (const [key, state] of texts) {
    if (!sendableSteps(state)) continue;
    const json = shown(key);
    if (json && ProseNode.fromJSON(schema, json).eq(state.doc)) continue;
    const text = state.doc.textBetween(0, state.doc.content.size, "\n").trim();
    if (text) words.push(text);
  }
  return words.join("\n\n");
}
