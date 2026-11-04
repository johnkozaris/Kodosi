import { expect, test } from "bun:test";
import { getSchema } from "@tiptap/core";
import spec from "../../../crates/docx/schema.json";
import { extensions } from "./schema";

const schema = getSchema(extensions);

test("the editor has the server's nodes in the server's order", () => {
  expect(Object.keys(schema.nodes)).toEqual(spec.nodes.map((node) => node.name));
});

test("the editor has the server's marks in the server's order", () => {
  expect(Object.keys(schema.marks)).toEqual(spec.marks.map((mark) => mark.name));
});

test("each node has the same content rule and attributes on both sides", () => {
  for (const node of spec.nodes) {
    const type = schema.nodes[node.name];
    expect(type?.spec.content ?? "", node.name).toBe(node.content ?? "");
    expect(Object.keys(type?.spec.attrs ?? {}).sort(), node.name).toEqual(
      Object.keys(node.attrs ?? {}).sort(),
    );
    expect(type?.spec.group ?? "", node.name).toBe(node.group ?? "");
    expect(Boolean(type?.spec.inline), node.name).toBe(Boolean(node.inline));
  }
});

test("each mark has the same attributes on both sides", () => {
  for (const mark of spec.marks) {
    expect(Object.keys(schema.marks[mark.name]?.spec.attrs ?? {}).sort(), mark.name).toEqual(
      Object.keys(mark.attrs ?? {}).sort(),
    );
  }
});
