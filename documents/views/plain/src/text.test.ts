import { expect, test } from "bun:test";
import type { Node as ProseNode } from "@tiptap/pm/model";
import { EditorState, TextSelection } from "@tiptap/pm/state";
import { schema } from "./schema";
import { indented, indentOf, newLine, outdented, pastedSlice } from "./text";

function docOf(...lines: string[]): ProseNode {
  return schema.node(
    "doc",
    null,
    lines.map((line) => schema.node("line", null, line ? [schema.text(line)] : [])),
  );
}

function linesOf(state: EditorState): string[] {
  const lines: string[] = [];
  state.doc.forEach((line) => {
    lines.push(line.textContent);
  });
  return lines;
}

function stateOf(lines: string[], from: [number, number], to = from): EditorState {
  const doc = docOf(...lines);
  const position = ([line, column]: [number, number]) => {
    let at = 1;
    for (let index = 0; index < line; index++) at += doc.child(index).nodeSize;
    return at + column;
  };
  return EditorState.create({
    doc,
    selection: TextSelection.create(doc, position(from), position(to)),
  });
}

function run(state: EditorState, command: typeof newLine): EditorState {
  let next = state;
  command(state, (transaction) => {
    next = state.apply(transaction);
  });
  return next;
}

test("a tab indents where lines start with tabs, else by the step that lines take most", () => {
  expect(indentOf(docOf("a:", "    b: 1", "    c:", "        d: 2", "e: 3"))).toBe("    ");
  expect(indentOf(docOf("{", '  "a": [', "    1", "  ]", "}"))).toBe("  ");
  expect(indentOf(docOf("<a>", "\t<b/>", "\t<c>", "\t\t<d/>"))).toBe("\t");
  expect(indentOf(docOf("plain words", "", "more words"))).toBe("  ");
});

test("pasted lines split the line where they go, with any line ending", () => {
  let state = stateOf(["start end"], [0, 6]);
  state = state.apply(state.tr.replaceSelection(pastedSlice("one\r\ntwo\nthree ")));
  expect(linesOf(state)).toEqual(["start one", "two", "three end"]);
  state = state.apply(state.tr.replaceSelection(pastedSlice("\n")));
  expect(linesOf(state)).toEqual(["start one", "two", "three ", "end"]);
  expect(pastedSlice("").size).toBe(0);
});

test("a tab steps in each line of a selection, and shift-tab steps them out once", () => {
  const lines = ["a:", "  b: 1", "  c: 2", "d: 3"];
  const indent = indentOf(docOf(...lines));
  const indentedState = run(stateOf(lines, [1, 1], [2, 2]), indented(indent));
  expect(linesOf(indentedState)).toEqual(["a:", "    b: 1", "    c: 2", "d: 3"]);
  const outdentedState = run(indentedState, outdented(indent));
  expect(linesOf(outdentedState)).toEqual(lines);
  expect(linesOf(run(outdentedState, outdented(indent)))).toEqual(["a:", "b: 1", "c: 2", "d: 3"]);
  expect(linesOf(run(stateOf(lines, [3, 0]), indented(indent)))).toEqual([
    "a:",
    "  b: 1",
    "  c: 2",
    "  d: 3",
  ]);
});

test("a new line starts as far in as the line that it splits", () => {
  const state = run(stateOf(["list:", "  - one"], [1, 7]), newLine);
  expect(linesOf(state)).toEqual(["list:", "  - one", "  "]);
  expect(state.selection.from).toBe(state.doc.content.size - 1);
});
