import { Schema } from "@tiptap/pm/model";

export const schema = new Schema({
  nodes: {
    doc: { content: "line+" },
    line: {
      content: "text*",
      parseDOM: [{ tag: "div" }, { tag: "p" }],
      toDOM: () => ["div", { class: "plain-line" }, 0],
    },
    text: {},
  },
  marks: {},
});
