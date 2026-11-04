import type { Editor } from "@tiptap/core";
import type { Mark, Node as ProseNode, ResolvedPos } from "@tiptap/pm/model";
import type { EditorState, Transaction } from "@tiptap/pm/state";
import type { DocxView, StyleLook } from "./styles";

export type Align = "left" | "center" | "right" | "both";
export type MarkName = "bold" | "italic" | "underline" | "strike";

/** The look that a textStyle mark gives: a color as hex, a size in half points. */
export interface TextLook {
  color?: string | null;
  size?: string | null;
  font?: string | null;
  highlight?: string | null;
}

type Attrs = Record<string, unknown>;

const LOOK_MARKS = ["bold", "italic", "underline", "strike", "textStyle"];
const MOST_LEVEL = 8;

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
function eachParagraph(editor: Editor, change: (attrs: Attrs) => Attrs | null): void {
  editor
    .chain()
    .focus()
    .command(({ tr, state }) => {
      const { from, to } = state.selection;
      state.doc.nodesBetween(from, to, (node, position) => {
        if (node.type.name !== "paragraph") return true;
        const next = change(node.attrs);
        if (next) tr.setNodeMarkup(position, undefined, { ...node.attrs, ...next });
        return false;
      });
      return true;
    })
    .run();
}

export function setStyle(editor: Editor, style: string): void {
  eachParagraph(editor, () => ({ style }));
}

export function setAlign(editor: Editor, align: Align): void {
  eachParagraph(editor, () => ({ align }));
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
  eachParagraph(editor, (attrs) =>
    inList ? { numId: null, ilvl: null } : { numId: list, ilvl: attrs.ilvl ?? "0" },
  );
}

/** Moves the items of a list one level in or out. */
export function indent(editor: Editor, step: 1 | -1): void {
  eachParagraph(editor, (attrs) => {
    if (!attrs.numId || attrs.numId === "0") return null;
    const level = Math.min(MOST_LEVEL, Math.max(0, Number(attrs.ilvl ?? 0) + step));
    return { ilvl: String(level) };
  });
}

interface Tracked {
  from: number;
  to: number;
  mark: Mark;
}

function isChange(mark: Mark): boolean {
  return mark.type.name === "insertion" || mark.type.name === "deletion";
}

/**
 * The changed text that touches a caret, from the first to the last change of the run, so a
 * replacement (a deletion next to an insertion) is one decision.
 */
function changeRunAt(at: ResolvedPos): { from: number; to: number } | undefined {
  const runs: { from: number; to: number }[] = [];
  at.parent.forEach((node, offset) => {
    if (!node.marks.some(isChange)) return;
    const from = at.start() + offset;
    const last = runs.at(-1);
    if (last && last.to === from) last.to = from + node.nodeSize;
    else runs.push({ from, to: from + node.nodeSize });
  });
  return runs.find((run) => run.from <= at.pos && at.pos <= run.to);
}

/** The tracked changes in the selection, or the changes that touch the caret. */
function changesAt(state: EditorState, all: boolean): Tracked[] {
  let { from, to } = all ? { from: 0, to: state.doc.content.size } : state.selection;
  if (!all && from === to) {
    const run = changeRunAt(state.doc.resolve(from));
    if (!run) return [];
    ({ from, to } = run);
  }
  const changes: Tracked[] = [];
  state.doc.nodesBetween(from, to, (node, position) => {
    if (!node.isInline) return;
    for (const mark of node.marks.filter(isChange)) {
      changes.push({
        from: Math.max(position, from),
        to: Math.min(position + node.nodeSize, to),
        mark,
      });
    }
  });
  return changes;
}

export function hasChanges(doc: ProseNode): boolean {
  let found = false;
  doc.descendants((node) => {
    if (!found && node.isInline && node.marks.some(isChange)) found = true;
    return !found;
  });
  return found;
}

export function canDecide(state: EditorState): boolean {
  return changesAt(state, false).length > 0;
}

/**
 * The one step that accepts or rejects tracked changes, which undo takes back. An accepted
 * insertion and a rejected deletion lose their mark, and the text of the others goes.
 */
export function decision(state: EditorState, accept: boolean, all: boolean): Transaction | null {
  const changes = changesAt(state, all);
  if (changes.length === 0) return null;
  const stays = (change: Tracked) => (change.mark.type.name === "insertion") === accept;
  const transaction = state.tr;
  for (const change of changes.filter(stays)) {
    transaction.removeMark(change.from, change.to, change.mark);
  }
  for (const change of changes.filter((change) => !stays(change)).reverse()) {
    transaction.delete(transaction.mapping.map(change.from), transaction.mapping.map(change.to));
  }
  return transaction;
}

export function decide(editor: Editor, accept: boolean, all: boolean): void {
  const transaction = decision(editor.state, accept, all);
  if (transaction) editor.view.dispatch(transaction);
}

export function canIndent(state: EditorState, step: 1 | -1): boolean {
  const paragraph = paragraphAt(state);
  const list = paragraph?.attrs.numId as string | null | undefined;
  if (!list || list === "0") return false;
  const level = Number(paragraph?.attrs.ilvl ?? 0);
  return step > 0 ? level < MOST_LEVEL : level > 0;
}
