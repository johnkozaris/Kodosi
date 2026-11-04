import { expect, test } from "bun:test";
import { getSchema } from "@tiptap/core";
import { EditorState, TextSelection } from "@tiptap/pm/state";
import { decision, hasChanges } from "./commands";
import { extensions } from "./schema";

const schema = getSchema(extensions);

/** Two replacements of an agent, each a deletion next to an insertion, with the caret in "month". */
function replaced(): EditorState {
  const deleted = schema.mark("deletion", { author: "Agent" });
  const inserted = schema.mark("insertion", { author: "Agent" });
  const doc = schema.node("doc", null, [
    schema.node("paragraph", null, [
      schema.text("Plan for the "),
      schema.text("week", [deleted]),
      schema.text("month", [inserted]),
      schema.text(" ahead"),
    ]),
    schema.node("paragraph", null, [
      schema.text("second", [deleted]),
      schema.text("last", [inserted]),
    ]),
  ]);
  const caret = 1 + "Plan for the weekmo".length;
  return EditorState.create({ doc, selection: TextSelection.create(doc, caret) });
}

test("a decision at the caret takes the whole replacement there and nothing else", () => {
  const state = replaced();
  const accepted = state.apply(decision(state, true, false) ?? state.tr).doc;
  expect(accepted.child(0).textContent).toBe("Plan for the month ahead");
  expect(hasChanges(accepted.child(0))).toBe(false);
  expect(hasChanges(accepted.child(1))).toBe(true);
  const rejected = state.apply(decision(state, false, false) ?? state.tr).doc;
  expect(rejected.child(0).textContent).toBe("Plan for the week ahead");
  expect(hasChanges(rejected.child(0))).toBe(false);
});
