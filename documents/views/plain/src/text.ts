import { Fragment, type Node as ProseNode, Slice } from "@tiptap/pm/model";
import type { Command, EditorState } from "@tiptap/pm/state";
import { schema } from "./schema";

const MOST_GUESSED = 2000;
const WIDEST_STEP = 8;

function lineOf(text: string): ProseNode {
  return schema.node("line", null, text ? [schema.text(text)] : []);
}

export function pastedSlice(text: string): Slice {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const [only] = lines;
  if (lines.length > 1) return new Slice(Fragment.from(lines.map(lineOf)), 1, 1);
  return only ? new Slice(Fragment.from(schema.text(only)), 0, 0) : Slice.empty;
}

function leadOf(text: string): string {
  return /^[\t ]*/.exec(text)?.[0] ?? "";
}

/**
 * The text that one press of Tab adds: a tab where lines step in with tabs, else the spaces by
 * which lines step in most often, and two spaces where no line steps in.
 */
export function indentOf(doc: ProseNode): string {
  const steps = new Map<number, number>();
  let tabs = 0;
  let spaces = 0;
  let before = 0;
  for (let index = 0; index < Math.min(doc.childCount, MOST_GUESSED); index++) {
    const text = doc.child(index).textContent;
    const lead = leadOf(text);
    if (lead.length === text.length) continue;
    if (lead.startsWith("\t")) {
      tabs++;
      continue;
    }
    if (lead) spaces++;
    const step = lead.length - before;
    if (step > 0 && step <= WIDEST_STEP) steps.set(step, (steps.get(step) ?? 0) + 1);
    before = lead.length;
  }
  if (tabs > spaces) return "\t";
  let width = 2;
  let seen = 0;
  for (const [step, count] of steps) {
    if (count > seen || (count === seen && step < width)) {
      width = step;
      seen = count;
    }
  }
  return " ".repeat(width);
}

function touchedLines(state: EditorState): number[] {
  const { doc, selection } = state;
  const { $from, $to } = selection;
  const first = $from.index(0);
  let last = Math.min($to.index(0), doc.childCount - 1);
  if (last > first && $to.depth > 0 && $to.parentOffset === 0) last--;
  let position = $from.depth > 0 ? $from.before(1) : $from.pos;
  const starts: number[] = [];
  for (let index = first; index <= last; index++) {
    starts.push(position);
    position += doc.child(index).nodeSize;
  }
  return starts;
}

export function indented(indent: string): Command {
  return (state, dispatch) => {
    const lines = touchedLines(state);
    const transaction = state.tr;
    if (lines.length === 1) transaction.insertText(indent);
    else for (const start of lines.reverse()) transaction.insertText(indent, start + 1);
    dispatch?.(transaction.scrollIntoView());
    return true;
  };
}

export function outdented(indent: string): Command {
  return (state, dispatch) => {
    const transaction = state.tr;
    for (const start of touchedLines(state).reverse()) {
      const lead = leadOf(state.doc.nodeAt(start)?.textContent ?? "");
      const width = lead.startsWith("\t") ? 1 : (/^ */.exec(lead)?.[0].length ?? 0);
      const step = Math.min(width, indent === "\t" ? 1 : indent.length);
      if (step > 0) transaction.delete(start + 1, start + 1 + step);
    }
    if (transaction.docChanged) dispatch?.(transaction.scrollIntoView());
    return true;
  };
}

export const newLine: Command = (state, dispatch) => {
  const { $from } = state.selection;
  if ($from.depth === 0) return false;
  const lead = leadOf($from.parent.textContent).slice(0, $from.parentOffset);
  const transaction = state.tr.deleteSelection();
  transaction.split(transaction.selection.from);
  if (lead) transaction.insertText(lead);
  dispatch?.(transaction.scrollIntoView());
  return true;
};
