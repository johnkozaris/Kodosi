import { expect, test } from "bun:test";
import { Node as ProseNode } from "@tiptap/pm/model";
import { EditorState } from "@tiptap/pm/state";
import { collab } from "prosemirror-collab";
import { layOut, levelRules, schema, unsentText } from "./text";

function runStyle(attrs: Record<string, unknown>): string {
  const look = schema.marks.textStyle;
  const [, shown] = look?.spec.toDOM?.(look.create(attrs), true) as [string, { style: string }];
  return shown.style;
}

test("the colour and font of a run cannot add CSS of their own", () => {
  expect(runStyle({ color: "C00000", size: "2400", font: "Segoe UI" })).toBe(
    'color:#C00000;font-size:32px;font-family:"Segoe UI", Carlito, sans-serif',
  );
  expect(runStyle({ font: "+mj-lt" })).toBe("font-size:24px");
  expect(
    runStyle({
      color: "red;display:none",
      font: 'Arial\n;display:none;x:"',
    }),
  ).toBe('font-size:24px;font-family:"Arial ;display:none;x:\\"", Carlito, sans-serif');
});

test("the colour and font of a text level cannot add CSS rules", () => {
  const rules = levelRules(
    {
      levels: [
        { color: "scheme:tx1", font: "+mn-lt" },
        { color: "000}body{display:none", font: 'x"}body{display:none}' },
        { font: "constructor" },
      ],
      anchor: null,
      scale: null,
    },
    { dk1: "1F1F1F", "+mn-lt": "Aptos" },
  );
  expect(rules.split("\n")).toEqual([
    '.slides-text p[data-level="0"]{font:24px "Aptos", Carlito, sans-serif;line-height:1.2;' +
      "margin:0 0 0 0px;text-align:left;color:#1F1F1F}",
    '.slides-text p[data-level="1"]{font:24px "x\\"}body{display:none}", Carlito, sans-serif;' +
      "line-height:1.2;margin:0 0 0 24px;text-align:left;color:inherit}",
    '.slides-text p[data-level="2"]{font:24px "constructor", Carlito, sans-serif;' +
      "line-height:1.2;margin:0 0 0 48px;text-align:left;color:inherit}",
  ]);
});

test("Chinese and Japanese text wraps between its characters, and a line starts with no closing mark", () => {
  const tree = ProseNode.fromJSON(schema, {
    type: "doc",
    content: [
      { type: "paragraph", content: [{ type: "text", text: "Plan 東京の会、議は「明日」です。" }] },
    ],
  });
  const measure = { width: (_: string, text: string) => [...text].length * 10, face: String };
  const lines = layOut(tree, 50, 200, null, measure).map((line) =>
    line.pieces.map((piece) => piece.text).join(""),
  );
  expect(lines).toEqual(["Plan ", "東京の会、", "議は「明", "日」です。"]);
});

test("words that wait and that a snapshot does not show come back to write again", () => {
  const typed = (words: string) => ({
    type: "doc",
    content: [{ type: "paragraph", content: [{ type: "text", text: words }] }],
  });
  const start = EditorState.create({
    doc: ProseNode.fromJSON(schema, typed("Plan")),
    plugins: [collab()],
  });
  const waiting = start.apply(start.tr.insertText(" for 2027", 5));
  const texts = new Map([
    ["256/2", waiting],
    ["256/3", start],
  ]);
  expect(unsentText(texts, () => typed("Plan"))).toBe("Plan for 2027");
  expect(unsentText(texts, () => undefined)).toBe("Plan for 2027");
  expect(unsentText(texts, () => typed("Plan for 2027"))).toBe("");
});
