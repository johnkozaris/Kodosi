import { expect, test } from "bun:test";
import { getSchema } from "@tiptap/core";
import { EditorState, TextSelection } from "@tiptap/pm/state";
import { changeNear, decision, hasChanges, indented, linkAddress, pageBroken } from "./commands";
import { extensions } from "./schema";
import type { DocxView } from "./styles";

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

test("the next and the previous change step over the change at the caret", () => {
  const state = replaced();
  const second = changeNear(state, true);
  expect(second && state.doc.textBetween(second.from, second.to)).toBe("secondlast");
  expect(changeNear(state, false)).toBeUndefined();
  if (!second) return;
  const there = state.apply(
    state.tr.setSelection(TextSelection.create(state.doc, second.from, second.to)),
  );
  const first = changeNear(there, false);
  expect(first && there.doc.textBetween(first.from, first.to)).toBe("weekmonth");
  expect(changeNear(there, true)).toBeUndefined();
});

const A4: DocxView = {
  styles: {
    defaults: {},
    defaultParagraph: null,
    styles: { Quote: { indent: 36 } },
  },
  numbering: null,
  page: { width: 595, height: 842, top: 72, right: 72, bottom: 72, left: 72 },
};

test("an indent goes by half inches between the margin and the far side of the page", () => {
  const indent = (attrs: Record<string, unknown>, step: 1 | -1) =>
    indented(A4, schema.node("paragraph", attrs), step);
  expect(indent({}, 1)).toEqual({ indent: "720" });
  expect(indent({ indent: "1000" }, 1)).toEqual({ indent: "1440" });
  expect(indent({ indent: "1000" }, -1)).toEqual({ indent: "720" });
  expect(indent({ indent: "0" }, -1)).toBeNull();
  expect(indent({ style: "Quote" }, -1)).toEqual({ indent: "0" });
  expect(indent({ indent: "7920" }, 1)).toBeNull();
  expect(indent({ numId: "1", ilvl: "2" }, 1)).toEqual({ ilvl: "3" });
  expect(indent({ numId: "1", ilvl: "8" }, 1)).toBeNull();
  expect(indent({ numId: "1", ilvl: "0" }, -1)).toBeNull();
});

test("a link goes to a web or an e-mail address and nowhere else", () => {
  expect(linkAddress(" kodosi.com/docs ")).toBe("https://kodosi.com/docs");
  expect(linkAddress("http://localhost:3000")).toBe("http://localhost:3000");
  expect(linkAddress("ana@example.com")).toBe("mailto:ana@example.com");
  expect(linkAddress("MAILTO:ana@example.com")).toBe("MAILTO:ana@example.com");
  for (const address of ["javascript:alert(1)", "ftp://kodosi.com", "file:///etc", "plan", ""]) {
    expect(linkAddress(address)).toBeNull();
  }
});

test("a page break ends the caret's paragraph, and the caret goes on in the next", () => {
  const doc = schema.node("doc", null, [
    schema.node("paragraph", { id: "a" }, [schema.text("OneTwo")]),
  ]);
  const state = EditorState.create({ doc, selection: TextSelection.create(doc, 1 + 3) });
  const broken = state.apply(pageBroken(state) ?? state.tr);
  expect(broken.doc.child(0).lastChild?.type.name).toBe("pageBreak");
  expect(broken.doc.child(0).textContent).toBe("One");
  expect(broken.doc.child(1).textContent).toBe("Two");
  expect(broken.doc.child(1).attrs.id).toBeNull();
  expect(broken.selection.$from.parentOffset).toBe(0);
  expect(broken.selection.$from.index(0)).toBe(1);
});
