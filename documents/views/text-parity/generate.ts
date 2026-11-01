import { Fragment, Node, type NodeType, Schema, Slice } from "prosemirror-model";
import { schema as basic } from "prosemirror-schema-basic";
import { addListNodes } from "prosemirror-schema-list";
import {
  AddMarkStep,
  canJoin,
  canSplit,
  findWrapping,
  liftTarget,
  ReplaceStep,
  type Step,
  Transform,
} from "prosemirror-transform";

const schema = new Schema({
  nodes: addListNodes(basic.spec.nodes, "paragraph block*", "block").update("doc", {
    content: "block+",
    attrs: { version: { default: 1 } },
  }),
  marks: basic.spec.marks.addToEnd("note", { excludes: "", attrs: { id: {} }, group: "notes" }),
});

function specOf(schema: Schema) {
  const nodes: unknown[] = [];
  schema.spec.nodes.forEach((name, spec) => {
    nodes.push({
      name,
      content: spec.content,
      group: spec.group,
      marks: spec.marks,
      inline: spec.inline ?? false,
      atom: spec.atom ?? false,
      attrs: spec.attrs ?? {},
    });
  });
  const marks: unknown[] = [];
  schema.spec.marks.forEach((name, spec) => {
    marks.push({ name, group: spec.group, excludes: spec.excludes, attrs: spec.attrs ?? {} });
  });
  return { nodes, marks };
}

class Random {
  constructor(private state: number) {}

  next(): number {
    this.state = (this.state + 0x6d2b79f5) | 0;
    let value = Math.imul(this.state ^ (this.state >>> 15), 1 | this.state);
    value = (value + Math.imul(value ^ (value >>> 7), 61 | value)) ^ value;
    return (value ^ (value >>> 14)) >>> 0;
  }

  below(limit: number): number {
    return limit > 0 ? this.next() % limit : 0;
  }

  pick<T>(items: readonly T[]): T {
    return items[this.below(items.length)] as T;
  }
}

const WORDS = ["word", "été", "日本語", "😀 smile", "a", " ", "x😀y", "long text here"];

function randomMarks(random: Random) {
  const marks = [];
  if (random.below(3) === 0) marks.push(schema.marks.strong.create());
  if (random.below(4) === 0) marks.push(schema.marks.em.create());
  if (random.below(6) === 0) marks.push(schema.marks.link.create({ href: "https://a.b" }));
  if (random.below(8) === 0) marks.push(schema.marks.note.create({ id: random.below(3) }));
  return marks;
}

function randomParagraph(random: Random): Node {
  const content: Node[] = [];
  for (let count = random.below(4); count > 0; count--) {
    const kind = random.below(10);
    if (kind === 0) content.push(schema.nodes.hard_break.create());
    else if (kind === 1) content.push(schema.nodes.image.create({ src: "a.png" }));
    else content.push(schema.text(random.pick(WORDS), randomMarks(random)));
  }
  return schema.nodes.paragraph.create(null, Fragment.fromArray(content));
}

function randomBlock(random: Random, depth: number): Node {
  const kind = random.below(depth > 2 ? 3 : 7);
  if (kind === 0) return schema.nodes.horizontal_rule.create();
  if (kind === 1)
    return schema.nodes.heading.create(
      { level: 1 + random.below(6) },
      schema.text(random.pick(WORDS)),
    );
  if (kind === 2) return randomParagraph(random);
  if (kind === 3)
    return schema.nodes.code_block.create({ params: "" }, schema.text(random.pick(WORDS)));
  if (kind === 4)
    return schema.nodes.blockquote.create(null, randomBlocks(random, depth + 1, 1 + random.below(2)));
  const items: Node[] = [];
  for (let count = 1 + random.below(3); count > 0; count--) {
    items.push(
      schema.nodes.list_item.create(null, [
        randomParagraph(random),
        ...randomBlocks(random, depth + 1, random.below(2)),
      ]),
    );
  }
  return (kind === 5 ? schema.nodes.bullet_list : schema.nodes.ordered_list).create(null, items);
}

function randomBlocks(random: Random, depth: number, count: number): Node[] {
  const blocks: Node[] = [];
  for (let left = count; left > 0; left--) blocks.push(randomBlock(random, depth));
  return blocks;
}

function randomDoc(random: Random): Node {
  return schema.nodes.doc.create(null, randomBlocks(random, 0, 1 + random.below(6)));
}

/** Moves a position out of a surrogate pair, where no editor puts one. */
function whole(doc: Node, position: number): number {
  const $position = doc.resolve(position);
  const text = $position.parent.maybeChild($position.index())?.text;
  const offset = $position.textOffset;
  const inPair = text && offset > 0 && (text.charCodeAt(offset) & 0xfc00) === 0xdc00;
  return inPair ? position + 1 : position;
}

function randomRange(random: Random, doc: Node, longest: number): [number, number] {
  const size = doc.content.size;
  const from = whole(doc, random.below(size + 1));
  const to = whole(doc, Math.min(size, from + random.below(longest)));
  return [from, Math.max(from, to)];
}

function randomSlice(random: Random, from: Node): Slice {
  return from.slice(...randomRange(random, from, from.content.size + 1));
}

function randomMark(random: Random) {
  return random.pick([
    schema.marks.strong.create(),
    schema.marks.em.create(),
    schema.marks.code.create(),
    schema.marks.link.create({ href: "https://b.c" }),
    schema.marks.note.create({ id: 1 }),
  ]);
}

/** One edit through the same commands that editors use, which can make any kind of step. */
function randomEdit(random: Random, transform: Transform, other: Node) {
  const doc = transform.doc;
  const [from, to] = randomRange(random, doc, 12);
  const $from = doc.resolve(from);
  switch (random.below(14)) {
    case 0:
      if ($from.parent.inlineContent) transform.insert(from, schema.text(random.pick(WORDS)));
      break;
    case 1:
      transform.delete(from, to);
      break;
    case 2:
      transform.replace(from, to, randomSlice(random, other));
      break;
    case 3:
      if (to > from) transform.addMark(from, to, randomMark(random));
      break;
    case 4:
      if (to > from) transform.removeMark(from, to, randomMark(random));
      break;
    case 5:
      if (canSplit(doc, from)) transform.split(from);
      break;
    case 6:
      if (canJoin(doc, from)) transform.join(from);
      break;
    case 7: {
      const range = doc.resolve(from).blockRange(doc.resolve(to));
      const target = range && liftTarget(range);
      if (range && target != null) transform.lift(range, target);
      break;
    }
    case 8: {
      const range = doc.resolve(from).blockRange(doc.resolve(to));
      const kind: NodeType = random.pick([
        schema.nodes.blockquote,
        schema.nodes.bullet_list,
        schema.nodes.ordered_list,
      ]);
      const wrapping = range && findWrapping(range, kind);
      if (range && wrapping) transform.wrap(range, wrapping);
      break;
    }
    case 9:
      transform.setBlockType(from, to, schema.nodes.heading, { level: 2 });
      break;
    case 10: {
      const node = doc.nodeAt(from);
      if (node?.type === schema.nodes.heading) transform.setNodeAttribute(from, "level", 3);
      else if (node?.type === schema.nodes.image) transform.setNodeAttribute(from, "title", "t");
      else transform.setDocAttribute("version", random.below(9));
      break;
    }
    case 11: {
      const node = doc.nodeAt(from);
      if (node?.isInline && !node.isText) {
        if (random.below(2) === 0) transform.addNodeMark(from, randomMark(random));
        else transform.removeNodeMark(from, randomMark(random));
      }
      break;
    }
    case 12:
      transform.replaceRangeWith(from, to, schema.nodes.horizontal_rule.create());
      break;
    default:
      transform.deleteRange(from, to);
      break;
  }
}

/** A step that nothing built with care, as a view that went wrong could send. */
function randomRawStep(random: Random, doc: Node, other: Node): Step {
  const [from, to] = randomRange(random, doc, doc.content.size + 1);
  if (random.below(4) === 0) return new AddMarkStep(from, to, randomMark(random));
  return new ReplaceStep(from, to, randomSlice(random, other), random.below(5) === 0);
}

function outcome(step: Step, doc: Node) {
  try {
    const result = step.apply(doc);
    return result.doc ? { after: result.doc } : { failed: true };
  } catch {
    return { failed: true };
  }
}

function runCase(seed: number) {
  const random = new Random(seed);
  const start = randomDoc(random);
  const other = randomDoc(random);
  let doc = start;
  const steps: unknown[] = [];
  for (let round = 0; round < 60 && steps.length < 200; round++) {
    const transform = new Transform(doc);
    try {
      randomEdit(random, transform, other);
    } catch {
      continue;
    }
    for (let index = 0; index < transform.steps.length; index++) {
      const step = transform.steps[index] as Step;
      steps.push({
        step: step.toJSON(),
        after: (transform.docs[index + 1] ?? transform.doc).toJSON(),
      });
    }
    doc = transform.doc;
    const raw = randomRawStep(random, doc, other);
    const result = outcome(raw, doc);
    steps.push({ step: raw.toJSON(), after: result.after?.toJSON() ?? null });
    if (result.after) doc = result.after;
  }
  return { doc: start.toJSON(), steps };
}

const [firstSeed, count] = process.argv.slice(2).map(Number);
console.log(JSON.stringify(specOf(schema)));
for (let seed = firstSeed ?? 1; seed < (firstSeed ?? 1) + (count ?? 1); seed++) {
  console.log(JSON.stringify(runCase(seed)));
}
