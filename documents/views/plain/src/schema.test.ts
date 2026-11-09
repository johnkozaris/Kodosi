import { expect, test } from "bun:test";
import spec from "../../../crates/plain/schema.json";
import { schema } from "./schema";

test("the editor has the server's nodes in the server's order, with their content", () => {
  expect(Object.keys(schema.nodes)).toEqual(spec.nodes.map((node) => node.name));
  for (const node of spec.nodes) {
    expect(schema.nodes[node.name]?.spec.content ?? "", node.name).toBe(node.content ?? "");
  }
  expect(Object.keys(schema.marks)).toEqual(spec.marks);
});
