import { type Editor, getMarkRange } from "@tiptap/core";
import type { Mark, Node as ProseNode, ResolvedPos } from "@tiptap/pm/model";
import { type EditorState, TextSelection, type Transaction } from "@tiptap/pm/state";
import type { DocxView, StyleLook } from "./styles";
import { inTable } from "./tables";

export type Align = "left" | "center" | "right" | "both";
export type MarkName = "bold" | "italic" | "underline" | "strike" | "superscript" | "subscript";

/** The look that a textStyle mark gives: a color as hex, a size in half points. */
export interface TextLook {
  color?: string | null;
  size?: string | null;
  font?: string | null;
  highlight?: string | null;
}

type Attrs = Record<string, unknown>;

const LOOK_MARKS = [
  "bold",
  "italic",
  "underline",
  "strike",
  "superscript",
  "subscript",
  "textStyle",
];
const MOST_LEVEL = 8;
/** A step of indent: half an inch, in the twentieths of a point that a file counts. */
const INDENT_STEP = 720;
/** The page of a file that does not say, in points: A4 with margins of an inch. */
const PAGE = { width: 595, left: 72, right: 72 };
const EMAIL = /^[^\s@/:]+@[^\s@/]+\.[^\s@/]+$/;
const HOST = /^[^\s@/:]+\.[^\s@/:]+([:/?#]|$)/;

/** The marks where the selection starts, or the ones that the next letters get. */
function marksAt(state: EditorState): readonly Mark[] {
  const { empty, from, to, $from } = state.selection;
  if (empty) return state.storedMarks ?? $from.marks();
  let found: readonly Mark[] | undefined;
  state.doc.nodesBetween(from, to, (node) => {
    if (!found && node.isText) found = node.marks;
    return !found;
  });
  return found ?? [];
}

export function textLook(state: EditorState): TextLook {
  return (marksAt(state).find((mark) => mark.type.name === "textStyle")?.attrs ?? {}) as TextLook;
}

export function toggleMark(editor: Editor, name: MarkName): void {
  editor.chain().focus().toggleMark(name).run();
}

/** Changes some of the text look and keeps the rest; a look with nothing left goes. */
export function setTextLook(editor: Editor, patch: TextLook): void {
  editor
    .chain()
    .focus()
    .command(({ tr, state }) => {
      const type = state.schema.marks.textStyle;
      if (!type) return false;
      const next = (attrs: Attrs) => {
        const merged = { ...attrs, ...patch };
        return Object.values(merged).some((value) => value != null) ? type.create(merged) : null;
      };
      if (tr.selection.empty) {
        const mark = next(marksAt(state).find((known) => known.type === type)?.attrs ?? {});
        tr.removeStoredMark(type);
        if (mark) tr.addStoredMark(mark);
        return true;
      }
      for (const range of tr.selection.ranges) {
        const from = range.$from.pos;
        const to = range.$to.pos;
        state.doc.nodesBetween(from, to, (node, position) => {
          if (!node.isText) return;
          const start = Math.max(position, from);
          const end = Math.min(position + node.nodeSize, to);
          const mark = next(node.marks.find((known) => known.type === type)?.attrs ?? {});
          tr.removeMark(start, end, type);
          if (mark) tr.addMark(start, end, mark);
        });
      }
      return true;
    })
    .run();
}

/** Takes away the marks of the look; links and tracked changes stay. */
export function clearLook(editor: Editor): void {
  editor
    .chain()
    .focus()
    .command(({ tr, state }) => {
      const { empty, from, to } = tr.selection;
      for (const name of LOOK_MARKS) {
        const type = state.schema.marks[name];
        if (!type) continue;
        if (empty) tr.removeStoredMark(type);
        else tr.removeMark(from, to, type);
      }
      return true;
    })
    .run();
}

/** The paragraph that holds the start of the selection. */
export function paragraphAt(state: EditorState): ProseNode | null {
  const parent = state.selection.$from.parent;
  return parent.type.name === "paragraph" ? parent : null;
}

/** Changes each paragraph that the selection touches. */
function eachParagraph(editor: Editor, change: (paragraph: ProseNode) => Attrs | null): void {
  editor
    .chain()
    .focus()
    .command(({ tr, state }) => {
      const { from, to } = state.selection;
      state.doc.nodesBetween(from, to, (node, position) => {
        if (node.type.name !== "paragraph") return true;
        const next = change(node);
        if (next) tr.setNodeMarkup(position, undefined, { ...node.attrs, ...next });
        return false;
      });
      return true;
    })
    .run();
}

/** Sends a change, and keeps the caret in the page where the person goes on. */
export function dispatch(editor: Editor, transaction: Transaction | null): void {
  if (!transaction) return;
  editor.view.dispatch(transaction.scrollIntoView());
  editor.view.focus();
}

export function setStyle(editor: Editor, style: string): void {
  eachParagraph(editor, () => ({ style }));
}

export function setAlign(editor: Editor, align: Align): void {
  eachParagraph(editor, () => ({ align }));
}

/** Sets the spacing of the lines, in 240ths of a line. */
export function setLine(editor: Editor, line: string): void {
  eachParagraph(editor, () => ({ line }));
}

/** The spacing of the lines that a paragraph sets itself, in 240ths of a line. */
export function lineOf(state: EditorState): string | null {
  return (paragraphAt(state)?.attrs.line as string | null | undefined) ?? null;
}

function styleOf(view: DocxView, paragraph: ProseNode | null): StyleLook | undefined {
  const styles = view.styles;
  const id = (paragraph?.attrs.style as string | null) ?? styles?.defaultParagraph;
  return id ? styles?.styles[id] : undefined;
}

/** The alignment that a paragraph shows, from its own setting or from its style. */
export function alignOf(view: DocxView, paragraph: ProseNode | null): Align {
  const align = (paragraph?.attrs.align as string | null) ?? styleOf(view, paragraph)?.align;
  if (align === "center" || align === "right" || align === "both") return align;
  if (align === "end") return "right";
  if (align === "distribute") return "both";
  return "left";
}

/** The size of the letters at the selection in points, from the mark, the style or the file. */
export function sizeOf(view: DocxView, state: EditorState): number {
  const own = textLook(state).size;
  if (own) return Number(own) / 2;
  return styleOf(view, paragraphAt(state))?.size ?? view.styles?.defaults.size ?? 11;
}

export function fontOf(view: DocxView, state: EditorState): string {
  return (
    textLook(state).font ??
    styleOf(view, paragraphAt(state))?.font ??
    view.styles?.defaults.font ??
    "Calibri"
  );
}

/** The file's own lists: the first that has bullets and the first that counts. */
export function listsOf(view: DocxView): { bullets: string | null; numbers: string | null } {
  const lists = Object.entries(view.numbering ?? {});
  const first = (bullets: boolean) =>
    lists.find(([, levels]) => {
      const format = levels[0]?.format;
      return bullets ? format === "bullet" : format !== undefined && format !== "bullet";
    })?.[0] ?? null;
  return { bullets: first(true), numbers: first(false) };
}

export function listFormatOf(view: DocxView, paragraph: ProseNode | null): string | null {
  const list = paragraph?.attrs.numId as string | null | undefined;
  if (!list || list === "0") return null;
  const level = Number(paragraph?.attrs.ilvl ?? 0);
  return view.numbering?.[list]?.[level]?.format ?? null;
}

/** Puts the paragraphs in a list, or takes them out when they are in it. */
export function toggleList(editor: Editor, list: string): void {
  const inList = paragraphAt(editor.state)?.attrs.numId === list;
  eachParagraph(editor, (paragraph) =>
    inList ? { numId: null, ilvl: null } : { numId: list, ilvl: paragraph.attrs.ilvl ?? "0" },
  );
}

function inList(paragraph: ProseNode): boolean {
  const list = paragraph.attrs.numId as string | null | undefined;
  return Boolean(list) && list !== "0";
}

/** A list item one level in or out, or nothing at the first or the last level. */
function levelled(paragraph: ProseNode, step: 1 | -1): Attrs | null {
  const level = Number(paragraph.attrs.ilvl ?? 0);
  const next = Math.min(MOST_LEVEL, Math.max(0, level + step));
  return next === level ? null : { ilvl: String(next) };
}

/** The indent where a paragraph's lines start, its own or its style's, in twentieths of a point. */
function indentOf(view: DocxView, paragraph: ProseNode): number {
  const own = paragraph.attrs.indent as string | null | undefined;
  if (own != null) return Number(own) || 0;
  return Math.round((styleOf(view, paragraph)?.indent ?? 0) * 20);
}

/** The deepest indent that leaves the lines a step of the page. */
function mostIndent(view: DocxView): number {
  const page = view.page;
  const width =
    (page?.width ?? PAGE.width) - (page?.left ?? PAGE.left) - (page?.right ?? PAGE.right);
  return Math.round(width * 20) - INDENT_STEP;
}

/**
 * What a step of indent makes of a paragraph: a list item moves a level, and another paragraph
 * moves to the next half inch, between the margin and the far side of the page.
 */
export function indented(view: DocxView, paragraph: ProseNode, step: 1 | -1): Attrs | null {
  if (inList(paragraph)) return levelled(paragraph, step);
  const now = indentOf(view, paragraph);
  if (step > 0) {
    const next = (Math.floor(now / INDENT_STEP) + 1) * INDENT_STEP;
    return next > mostIndent(view) ? null : { indent: String(next) };
  }
  if (now <= 0) return null;
  return { indent: String(Math.max(0, (Math.ceil(now / INDENT_STEP) - 1) * INDENT_STEP)) };
}

export function indent(editor: Editor, view: DocxView, step: 1 | -1): void {
  eachParagraph(editor, (paragraph) => indented(view, paragraph, step));
}

export function canIndent(state: EditorState, view: DocxView, step: 1 | -1): boolean {
  const paragraph = paragraphAt(state);
  return paragraph !== null && indented(view, paragraph, step) !== null;
}

/** Moves the items of a list one level in or out, as Tab and Shift-Tab do. */
export function shiftLevel(editor: Editor, step: 1 | -1): void {
  eachParagraph(editor, (paragraph) => (inList(paragraph) ? levelled(paragraph, step) : null));
}

/** Whether the caret is in an item of a list. */
export function inListAt(state: EditorState): boolean {
  const paragraph = paragraphAt(state);
  return paragraph !== null && inList(paragraph);
}

interface Tracked {
  from: number;
  to: number;
  mark: Mark;
}

/** The attributes of a paragraph that say whether the mark that ends it is a tracked change. */
const ENDS = ["inserted", "deleted"] as const;

/** A paragraph whose ending mark is a tracked change, with the attribute that says so. */
interface TrackedEnd {
  pos: number;
  name: (typeof ENDS)[number];
}

function isChange(mark: Mark): boolean {
  return mark.type.name === "insertion" || mark.type.name === "deletion";
}

function endsOf(node: ProseNode, pos: number): TrackedEnd[] {
  return ENDS.filter((name) => node.attrs[name] != null).map((name) => ({ pos, name }));
}

function hasChangedEnd(node: ProseNode): boolean {
  return ENDS.some((name) => node.attrs[name] != null);
}

/**
 * The runs of changed text of a textblock whose content starts at `start`, with neighbors
 * joined, so a replacement (a deletion next to an insertion) is one change. XML of its own
 * between them, such as a bookmark, does not part them. A changed mark at the end of the block
 * is a run of no length there.
 */
function changeRunsIn(block: ProseNode, start: number): { from: number; to: number }[] {
  const runs: { from: number; to: number }[] = [];
  let joins = -1;
  const add = (from: number, to: number) => {
    const last = runs.at(-1);
    if (last && joins === from) last.to = to;
    else runs.push({ from, to });
    joins = to;
  };
  block.forEach((node, offset) => {
    const from = start + offset;
    if (node.marks.some(isChange)) add(from, from + node.nodeSize);
    else if (node.type.name === "inlineOpaque" && joins === from) joins += node.nodeSize;
  });
  const end = start + block.content.size;
  if (hasChangedEnd(block)) add(end, end);
  return runs;
}

/** The changed text that touches a caret, from the first to the last change of the run. */
function changeRunAt(at: ResolvedPos): { from: number; to: number } | undefined {
  return changeRunsIn(at.parent, at.start()).find((run) => run.from <= at.pos && at.pos <= run.to);
}

/** The first tracked change after the selection, or the last one before it. */
export function changeNear(
  state: EditorState,
  forward: boolean,
): { from: number; to: number } | undefined {
  const runs: { from: number; to: number }[] = [];
  state.doc.descendants((node, position) => {
    if (!node.isTextblock) return true;
    runs.push(...changeRunsIn(node, position + 1));
    return false;
  });
  const { from, to } = state.selection;
  return forward
    ? runs.find((run) => run.from > to || (run.from === to && run.to > to))
    : runs.findLast((run) => run.to < from || (run.to === from && run.from < from));
}

/** Selects the next or the previous tracked change, and shows it. */
export function goToChange(editor: Editor, forward: boolean): void {
  const run = changeNear(editor.state, forward);
  if (run) editor.chain().focus().setTextSelection(run).scrollIntoView().run();
}

/**
 * The tracked changes in the selection, or the changes that touch the caret: the changed text,
 * and the paragraphs whose changed ending mark lies in it.
 */
function changesAt(state: EditorState, all: boolean): { words: Tracked[]; ends: TrackedEnd[] } {
  let { from, to } = all ? { from: 0, to: state.doc.content.size } : state.selection;
  const words: Tracked[] = [];
  const ends: TrackedEnd[] = [];
  if (!all && from === to) {
    const run = changeRunAt(state.doc.resolve(from));
    if (!run) return { words, ends };
    ({ from, to } = run);
  }
  state.doc.nodesBetween(from, to, (node, position) => {
    if (node.isTextblock) {
      const end = position + 1 + node.content.size;
      if (from <= end && end <= to) ends.push(...endsOf(node, position));
      return true;
    }
    if (!node.isInline) return;
    for (const mark of node.marks.filter(isChange)) {
      words.push({
        from: Math.max(position, from),
        to: Math.min(position + node.nodeSize, to),
        mark,
      });
    }
  });
  return { words, ends };
}

export function hasChanges(doc: ProseNode): boolean {
  let found = false;
  doc.descendants((node) => {
    if (!found && (node.marks.some(isChange) || hasChangedEnd(node))) found = true;
    return !found;
  });
  return found;
}

export function canDecide(state: EditorState): boolean {
  const { words, ends } = changesAt(state, false);
  return words.length > 0 || ends.length > 0;
}

/**
 * Takes away the mark that ends the paragraph at `pos`. The paragraph goes when nothing but XML of
 * its own is left of it, as a deleted block does, and else its content goes to the start of the
 * paragraph after it, whose mark stays and keeps its look. The last block of the body or of a
 * cell, and a paragraph before a table, keep their mark and lose only its change.
 */
function removeEnd(transaction: Transaction, pos: number, name: TrackedEnd["name"]): void {
  const node = transaction.doc.nodeAt(pos);
  if (!node) return;
  const end = pos + node.nodeSize;
  const alone = transaction.doc.resolve(pos).parent.childCount === 1;
  const next = transaction.doc.resolve(end).nodeAfter;
  const words = node.content.content.some((child) => child.type.name !== "inlineOpaque");
  if (!words && !alone) {
    transaction.delete(pos, end);
    return;
  }
  if (words && next?.type.name === "paragraph") {
    transaction.delete(pos, end).insert(pos + 1, node.content);
    return;
  }
  transaction.setNodeAttribute(pos, name, null);
}

/**
 * The one step that accepts or rejects tracked changes, which undo takes back. An accepted
 * insertion and a rejected deletion lose their mark. The text of the others goes, and then the
 * marks that end paragraphs.
 */
export function decision(state: EditorState, accept: boolean, all: boolean): Transaction | null {
  const { words, ends } = changesAt(state, all);
  if (words.length === 0 && ends.length === 0) return null;
  const stays = (name: string) => (name === "insertion" || name === "inserted") === accept;
  const transaction = state.tr;
  for (const change of words.filter((change) => stays(change.mark.type.name))) {
    transaction.removeMark(change.from, change.to, change.mark);
  }
  for (const end of ends.filter((end) => stays(end.name))) {
    transaction.setNodeAttribute(end.pos, end.name, null);
  }
  for (const change of words.filter((change) => !stays(change.mark.type.name)).reverse()) {
    transaction.delete(transaction.mapping.map(change.from), transaction.mapping.map(change.to));
  }
  for (const end of ends.filter((end) => !stays(end.name)).reverse()) {
    removeEnd(transaction, transaction.mapping.map(end.pos), end.name);
  }
  return transaction;
}

export function decide(editor: Editor, accept: boolean, all: boolean): void {
  const transaction = decision(editor.state, accept, all);
  if (transaction) editor.view.dispatch(transaction);
}

/**
 * The address of a link that a person typed: a web or e-mail address as they wrote it, with
 * the scheme that it lacks. Other schemes make no link.
 */
export function linkAddress(typed: string): string | null {
  const text = typed.trim();
  let address = text;
  if (!/^(https?|mailto):/i.test(text)) {
    if (EMAIL.test(text)) address = `mailto:${text}`;
    else if (HOST.test(text)) address = `https://${text}`;
    else return null;
  }
  if (!URL.canParse(address)) return null;
  const url = new URL(address);
  if (url.protocol === "mailto:") return url.pathname.includes("@") ? address : null;
  return url.hostname ? address : null;
}

/** The link that the caret is in, or that holds the whole selection. */
export function linkAt(state: EditorState): { from: number; to: number; href: string } | null {
  const type = state.schema.marks.link;
  if (!type) return null;
  const { $from, from, to } = state.selection;
  const range = getMarkRange($from, type);
  if (!range || range.from > from || range.to < to) return null;
  const href = state.doc.nodeAt(range.from)?.marks.find((mark) => mark.type === type)?.attrs.href as
    | string
    | undefined;
  return { ...range, href: href ?? "" };
}

/**
 * Gives the link that the caret is in a new address, or links the selected words. With
 * neither, the words go in at the caret as a new link, and the next letters are plain.
 */
export function linked(state: EditorState, address: string, words: string): Transaction | null {
  const type = state.schema.marks.link;
  if (!type) return null;
  const mark = type.create({ href: address });
  const tr = state.tr;
  const link = linkAt(state);
  const { from, to, empty } = state.selection;
  if (link) return tr.removeMark(link.from, link.to, type).addMark(link.from, link.to, mark);
  if (!empty) return tr.addMark(from, to, mark);
  const end = from + words.length;
  tr.insertText(words, from).addMark(from, end, mark);
  tr.setSelection(TextSelection.create(tr.doc, end));
  return tr.setStoredMarks(
    tr.doc
      .resolve(end)
      .marks()
      .filter((known) => known.type !== type),
  );
}

/** Takes the link away from the words that the caret is in. */
export function unlinked(state: EditorState): Transaction | null {
  const type = state.schema.marks.link;
  const link = linkAt(state);
  return type && link ? state.tr.removeMark(link.from, link.to, type) : null;
}

/** Ends the caret's paragraph with a page break, and the caret goes on at the top of the next page. */
export function pageBroken(state: EditorState): Transaction | null {
  const type = state.schema.nodes.pageBreak;
  if (!type || inTable(state) || !(state.selection instanceof TextSelection)) return null;
  const tr = state.tr.deleteSelection();
  const $at = tr.selection.$from;
  const paragraph = $at.parent;
  if (paragraph.type.name !== "paragraph") return null;
  tr.insert($at.pos, type.create());
  tr.split($at.pos + 1, 1, [{ type: paragraph.type, attrs: { ...paragraph.attrs, id: null } }]);
  return tr.setSelection(TextSelection.create(tr.doc, $at.pos + 3));
}
