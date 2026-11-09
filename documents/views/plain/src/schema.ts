import { Schema } from "@tiptap/pm/model";

/** The server's tree of a text file: its lines, each with its text and nothing else. */
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
