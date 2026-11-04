import { expect, test } from "bun:test";
import { getSchema } from "@tiptap/core";
import { EditorState } from "@tiptap/pm/state";
import { collab } from "prosemirror-collab";
import { shownPlace } from "./places";
import { extensions } from "./schema";

const schema = getSchema(extensions);

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
