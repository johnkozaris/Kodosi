import { expect, test } from "bun:test";
import { Schema } from "@tiptap/pm/model";
import { EditorState } from "@tiptap/pm/state";
import { collab } from "prosemirror-collab";
import { shownPlace } from "./index";

const schema = new Schema({
  nodes: { doc: { content: "paragraph+" }, paragraph: { content: "text*" }, text: {} },
});

test("a peer's place comes across the typing that waits in this copy", () => {
  const doc = schema.node("doc", null, [
    schema.node("paragraph", null, [schema.text("Some words")]),
  ]);
  let state = EditorState.create({ doc, plugins: [collab()] });
  state = state.apply(state.tr.insertText("New ", 1));
  const end = 1 + "Some words".length;
  const moved = end + "New ".length;
  expect(shownPlace(state, { from: end, to: end })).toEqual({ from: moved, to: moved });
  expect(shownPlace(state, { from: 1, to: 1 })).toEqual({ from: 1 + "New ".length, to: 5 });
});
