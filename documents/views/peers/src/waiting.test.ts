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

/** A copy that loaded `doc`, and its state after the person typed on it. */
function copyOf(doc: ProseNode, type: (state: EditorState) => EditorState) {
  const waiting = new Waiting();
  waiting.loaded(doc, undefined, false);
  const state = type(EditorState.create({ doc, plugins: [collab({ clientID: "me" })] }));
  return { waiting, state };
}

/** The snapshot with the steps that came onto it, as the ids and text of its paragraphs. */
function shown(snapshot: ProseNode, steps: readonly Step[]): [string, string][] {
  const transform = new Transform(snapshot);
  for (const step of steps) transform.step(step);
  const paragraphs: [string, string][] = [];
  transform.doc.forEach((paragraph) => {
    paragraphs.push([paragraph.attrs.id as string, paragraph.textContent]);
  });
  return paragraphs;
}

test("typing that waits comes onto a snapshot of the file as it was, with its new ids", () => {
  const doc = docOf(["a", "Some words"], ["b", "More"]);
  const { waiting, state } = copyOf(doc, (state) => typed(state, "New ", 1));
  const snapshot = docOf(["a2", "Some words"], ["b2", "More"]);
  const carried = waiting.loaded(snapshot, state, false);
  expect(shown(snapshot, carried.steps)).toEqual([
    ["a2", "New Some words"],
    ["b2", "More"],
  ]);
  expect(carried.lost).toBe("");
});

test("a batch that the document took before the snapshot does not come twice", () => {
  const doc = docOf(["a", "ab"]);
  const type = (state: EditorState) => typed(typed(state, "x", 2), "y", 3);
  const taken = copyOf(doc, type);
  taken.waiting.sending(1);
  const withX = docOf(["a", "axb"]);
  expect(shown(withX, taken.waiting.loaded(withX, taken.state, false).steps)).toEqual([
    ["a", "axyb"],
  ]);
  const lost = copyOf(doc, type);
  lost.waiting.sending(1);
  expect(shown(doc, lost.waiting.loaded(doc, lost.state, false).steps)).toEqual([["a", "axyb"]]);
});

test("typing goes around what others changed while the copy was away", () => {
  const doc = docOf(["a", "one"], ["b", "two"]);
  const { waiting, state } = copyOf(doc, (state) => typed(state, "!", 9));
  const snapshot = docOf(["a", "one more"], ["c", "two"]);
  expect(shown(snapshot, waiting.loaded(snapshot, state, false).steps)).toEqual([
    ["a", "one more"],
    ["c", "two!"],
  ]);
});

test("typing in a part that others removed comes back as text", () => {
  const doc = docOf(["a", "one"], ["b", "two"]);
  const { waiting, state } = copyOf(doc, (state) => typed(state, " three", 9));
  const snapshot = docOf(["a", "one"]);
  const carried = waiting.loaded(snapshot, state, false);
  expect(carried.steps).toEqual([]);
  expect(carried.lost).toBe(" three");
});

test("a refused batch does not come again, and its text shows", () => {
  const doc = docOf(["a", "ab"]);
  const { waiting, state } = copyOf(doc, (state) => typed(typed(state, "x", 2), "y", 1));
  waiting.sending(1);
  const carried = waiting.loaded(doc, state, true);
  expect(shown(doc, carried.steps)).toEqual([["a", "yab"]]);
  expect(carried.lost).toBe("x");
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
  expect(shown(snapshot, waiting.loaded(snapshot, state, false).steps)).toEqual([["a2", "axyb"]]);
});
