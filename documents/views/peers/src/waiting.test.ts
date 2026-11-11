import { expect, test } from "bun:test";
import { type Node as ProseNode, Schema } from "@tiptap/pm/model";
import { EditorState } from "@tiptap/pm/state";
import { type Step, Transform } from "@tiptap/pm/transform";
import { collab, receiveTransaction, sendableSteps } from "prosemirror-collab";
import { Waiting } from "./waiting";

const schema = new Schema({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: { content: "text*", attrs: { id: { default: null } } },
    text: {},
  },
});

function docOf(...paragraphs: [string, string][]): ProseNode {
  return schema.node(
    "doc",
    null,
    paragraphs.map(([id, text]) =>
      schema.node("paragraph", { id }, text ? [schema.text(text)] : []),
    ),
  );
}

function typed(state: EditorState, text: string, at: number): EditorState {
  return state.apply(state.tr.insertText(text, at));
}

function copyOf(doc: ProseNode, type: (state: EditorState) => EditorState) {
  const waiting = new Waiting();
  waiting.loaded(doc, undefined, "none");
  const state = type(EditorState.create({ doc, plugins: [collab({ clientID: "me" })] }));
  return { waiting, state };
}

function shown(snapshot: ProseNode, steps: readonly Step[]): [string, string][] {
  const transform = new Transform(snapshot);
  for (const step of steps) transform.step(step);
  const paragraphs: [string, string][] = [];
  transform.doc.forEach((paragraph) => {
    paragraphs.push([paragraph.attrs.id as string, paragraph.textContent]);
  });
  return paragraphs;
}

const plan = docOf(["i", "Intro"], ["p", "Plan"], ["e", "End"]);
const planEnd = 1 + "Intro".length + 2 + "Plan".length;

function anaTyped() {
  const copy = copyOf(plan, (state) => typed(typed(state, " A", planEnd), " B", planEnd + 2));
  copy.waiting.sending(1);
  return copy;
}

test("typing comes onto a snapshot in which others changed the blocks around it", () => {
  const { waiting, state } = anaTyped();
  const snapshot = docOf(["i2", "Intro!"], ["p2", "Plan A"], ["e2", "The end"]);
  const carried = waiting.loaded(snapshot, state, "unknown");
  expect(shown(snapshot, carried.steps)).toEqual([
    ["i2", "Intro!"],
    ["p2", "Plan A B"],
    ["e2", "The end"],
  ]);
  expect(carried.lost).toBeUndefined();
  const without = docOf(["i2", "Intro!"], ["p2", "Plan"], ["e2", "The end"]);
  for (const sent of ["unknown", "lost"] as const) {
    const lost = anaTyped();
    expect(shown(without, lost.waiting.loaded(without, lost.state, sent).steps)).toEqual([
      ["i2", "Intro!"],
      ["p2", "Plan A B"],
      ["e2", "The end"],
    ]);
  }
});

test("typing in a block that others changed too comes back as text and never twice", () => {
  const snapshot = docOf(["i2", "Intro!"], ["p2", "Plan A and C"], ["e", "End"]);
  const unknown = anaTyped();
  const carried = unknown.waiting.loaded(snapshot, unknown.state, "unknown");
  expect(carried.steps).toEqual([]);
  expect(carried.lost).toBe(" A B");
  const taken = anaTyped();
  expect(taken.waiting.loaded(snapshot, taken.state, "taken")).toEqual({ steps: [], lost: " B" });
});

test("a batch on its way that the snapshot does not show is never dropped without its text", () => {
  const endEnd = planEnd + " A".length + 2 + "End".length;
  const copy = copyOf(plan, (state) => typed(typed(state, " A", planEnd), " B", endEnd));
  copy.waiting.sending(1);
  const snapshot = docOf(["i", "Intro"], ["p2", "Plan C"], ["e", "End"]);
  expect(copy.waiting.loaded(snapshot, copy.state, "unknown")).toEqual({
    steps: [],
    lost: " A\n B",
  });
});

test("typing that waits comes onto a snapshot of the file as it was, with its new ids", () => {
  const doc = docOf(["a", "Some words"], ["b", "More"]);
  const { waiting, state } = copyOf(doc, (state) => typed(state, "New ", 1));
  const snapshot = docOf(["a2", "Some words"], ["b2", "More"]);
  const carried = waiting.loaded(snapshot, state, "none");
  expect(shown(snapshot, carried.steps)).toEqual([
    ["a2", "New Some words"],
    ["b2", "More"],
  ]);
  expect(carried.lost).toBeUndefined();
});

test("a batch that the document took before the snapshot does not come twice", () => {
  const doc = docOf(["a", "ab"]);
  const withX = docOf(["a", "axb"]);
  const type = (state: EditorState) => typed(typed(state, "x", 2), "y", 3);
  for (const [snapshot, sent] of [
    [withX, "unknown"],
    [withX, "taken"],
    [doc, "unknown"],
    [doc, "lost"],
  ] as const) {
    const { waiting, state } = copyOf(doc, type);
    waiting.sending(1);
    expect(shown(snapshot, waiting.loaded(snapshot, state, sent).steps)).toEqual([["a", "axyb"]]);
  }
});

test("typing goes around what others changed while the copy was away", () => {
  const doc = docOf(["a", "one"], ["b", "two"]);
  const { waiting, state } = copyOf(doc, (state) => typed(state, "!", 9));
  const snapshot = docOf(["a", "one more"], ["c", "two"]);
  expect(shown(snapshot, waiting.loaded(snapshot, state, "none").steps)).toEqual([
    ["a", "one more"],
    ["c", "two!"],
  ]);
});

test("typing in a part that others removed comes back as text", () => {
  const doc = docOf(["a", "one"], ["b", "two"]);
  const { waiting, state } = copyOf(doc, (state) => typed(state, " three", 9));
  const snapshot = docOf(["a", "one"]);
  expect(waiting.loaded(snapshot, state, "none")).toEqual({ steps: [], lost: " three" });
});

test("typing in a block that shows more than once in a changed snapshot comes back as text", () => {
  const doc = docOf(["a", ""], ["b", "x"], ["c", ""]);
  const { waiting, state } = copyOf(doc, (state) => typed(state, "y", 1));
  const snapshot = docOf(["a", ""], ["b", "x!"], ["c", ""]);
  expect(waiting.loaded(snapshot, state, "none")).toEqual({ steps: [], lost: "y" });
});

test("typing in a line that others filled or removed never lands in another line like it", () => {
  const doc = docOf(["p", "Plan"], ["a", ""], ["r", "Risks"], ["b", ""]);
  const lastLine = doc.content.size - 1;
  for (const snapshot of [
    docOf(["p", "Plan"], ["a", ""], ["r", "Risks"], ["b", "Budget is fine"]),
    docOf(["p", "Plan"], ["a", ""], ["r", "Risks"]),
  ]) {
    const { waiting, state } = copyOf(doc, (state) => typed(state, "Late supplier", lastLine));
    expect(waiting.loaded(snapshot, state, "none")).toEqual({ steps: [], lost: "Late supplier" });
  }
});

test("a refused batch does not come again, and the text that waited shows", () => {
  const doc = docOf(["a", "ab"]);
  const { waiting, state } = copyOf(doc, (state) => typed(typed(state, "x", 2), "y", 1));
  waiting.sending(1);
  expect(waiting.loaded(doc, state, "refused")).toEqual({ steps: [], lost: "y\nx" });
});

test("a batch that the order confirmed stays out of what comes again", () => {
  const doc = docOf(["a", "ab"]);
  const { waiting, state: sending } = copyOf(doc, (state) => typed(state, "x", 2));
  const sent = sendableSteps(sending)?.steps ?? [];
  waiting.sending(sent.length);
  waiting.changed(sent, true);
  const confirmed = sending.apply(receiveTransaction(sending, sent, ["me"]));
  const state = typed(confirmed, "y", 3);
  const snapshot = docOf(["a2", "axb"]);
  expect(shown(snapshot, waiting.loaded(snapshot, state, "none").steps)).toEqual([["a2", "axyb"]]);
});
