import { type Fragment, Mark, type Node as ProseNode, Slice } from "@tiptap/pm/model";
import type { EditorState } from "@tiptap/pm/state";
import { AttrStep, Mapping, ReplaceStep, type Step, Transform } from "@tiptap/pm/transform";
import { sendableSteps } from "prosemirror-collab";

/** This copy's steps on a snapshot, and the text of those that do not fit it. */
export interface Carried {
  steps: Step[];
  lost: string;
}

/** Whether two nodes differ in their type or marks. Their attributes do not count. */
function differs(a: ProseNode, b: ProseNode): boolean {
  return a.type !== b.type || !Mark.sameSet(a.marks, b.marks);
}

function diffStart(a: Fragment, b: Fragment, pos: number): number | null {
  for (let index = 0; ; index++) {
    if (index === a.childCount || index === b.childCount) {
      return a.childCount === b.childCount ? null : pos;
    }
    const childA = a.child(index);
    const childB = b.child(index);
    if (differs(childA, childB)) return pos;
    if (childA.isText && childA.text !== childB.text) {
      const textA = childA.text ?? "";
      const textB = childB.text ?? "";
      let same = 0;
      while (same < textA.length && textA[same] === textB[same]) same++;
      return pos + same;
    }
    const inner = diffStart(childA.content, childB.content, pos + 1);
    if (inner !== null) return inner;
    pos += childA.nodeSize;
  }
}

function diffEnd(a: Fragment, b: Fragment, posA: number, posB: number): [number, number] | null {
  for (let indexA = a.childCount, indexB = b.childCount; ; ) {
    if (indexA === 0 || indexB === 0) return indexA === indexB ? null : [posA, posB];
    const childA = a.child(--indexA);
    const childB = b.child(--indexB);
    if (differs(childA, childB)) return [posA, posB];
    if (childA.isText && childA.text !== childB.text) {
      const textA = childA.text ?? "";
      const textB = childB.text ?? "";
      const most = Math.min(textA.length, textB.length);
      let same = 0;
      while (same < most && textA[textA.length - same - 1] === textB[textB.length - same - 1]) {
        same++;
      }
      return [posA - same, posB - same];
    }
    const inner = diffEnd(childA.content, childB.content, posA - 1, posB - 1);
    if (inner) return inner;
    posA -= childA.nodeSize;
    posB -= childB.nodeSize;
  }
}

function same(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const keys = Object.keys(a);
  if (keys.length !== Object.keys(b).length) return false;
  return keys.every((key) =>
    same((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]),
  );
}

/** Gives each node of `doc` the attributes of the node at its place in `to`, of the same shape. */
function setAttributes(doc: ProseNode, to: ProseNode, pos: number, transform: Transform): void {
  doc.forEach((child, offset, index) => {
    const other = to.child(index);
    for (const [name, value] of Object.entries(other.attrs)) {
      if (!same(child.attrs[name], value)) transform.step(new AttrStep(pos + offset, name, value));
    }
    if (!child.isLeaf) setAttributes(child, other, pos + offset + 1, transform);
  });
}

/**
 * Turns one document into another: one replacement of the part that differs, then the attributes
 * of the nodes, such as the ids that a file gets when it opens again. A step made on the first
 * document moves across this as long as its place stays.
 */
function between(from: ProseNode, to: ProseNode): Transform {
  const transform = new Transform(from);
  const start = diffStart(from.content, to.content, 0);
  if (start !== null) {
    const ends = diffEnd(from.content, to.content, from.content.size, to.content.size);
    let [endA, endB] = ends ?? [start, start];
    const overlap = start - Math.min(endA, endB);
    if (overlap > 0) {
      endA += overlap;
      endB += overlap;
    }
    transform.maybeStep(new ReplaceStep(start, endA, to.slice(start, endB)));
  }
  if (diffStart(transform.doc.content, to.content, 0) === null) {
    setAttributes(transform.doc, to, 0, transform);
  }
  if (transform.doc.content.eq(to.content)) return transform;
  const whole = new Transform(from);
  whole.step(new ReplaceStep(0, from.content.size, new Slice(to.content, 0, 0)));
  return whole;
}

/** How much a transform changes: the size of what it replaces and puts in, and its steps. */
function distance(transform: Transform): number {
  let size = transform.steps.length;
  for (const map of transform.mapping.maps) {
    map.forEach((fromA, toA, fromB, toB) => {
      size += toA - fromA + toB - fromB;
    });
  }
  return size;
}

/**
 * Moves steps across others that came first, as prosemirror-collab does: each step maps back
 * across the steps before it, across `over`, and forward across those that moved. `docs` holds
 * the document before each step. It gives the steps that still apply, and the places of the others.
 */
function rebase(
  steps: readonly Step[],
  docs: readonly ProseNode[],
  over: Transform,
): { kept: Step[]; lost: number[] } {
  const mapping = new Mapping();
  for (let index = steps.length - 1; index >= 0; index--) {
    const step = steps[index] as Step;
    mapping.appendMap(step.invert(docs[index] as ProseNode).getMap());
  }
  mapping.appendMapping(over.mapping);
  const moved = new Transform(over.doc);
  const kept: Step[] = [];
  const lost: number[] = [];
  steps.forEach((step, index) => {
    const mirror = steps.length - 1 - index;
    const mapped = step.map(mapping.slice(mirror + 1));
    if (mapped && !moved.maybeStep(mapped).failed) {
      mapping.appendMap(mapped.getMap(), mirror);
      kept.push(mapped);
    } else {
      lost.push(index);
    }
  });
  return { kept, lost };
}

/** The text that some steps of `typed` put in, as it shows when all its steps are made. */
function textOf(typed: Transform, steps: readonly number[]): string {
  const ranges: [number, number][] = [];
  for (const index of steps) {
    const later = typed.mapping.slice(index + 1);
    typed.mapping.maps[index]?.forEach((_fromA, _toA, fromB, toB) => {
      const from = later.map(fromB, 1);
      const to = later.map(toB, -1);
      if (to > from) ranges.push([from, to]);
    });
  }
  ranges.sort(([a], [b]) => a - b);
  const joined: [number, number][] = [];
  for (const [from, to] of ranges) {
    const last = joined.at(-1);
    if (last && from <= last[1]) last[1] = Math.max(last[1], to);
    else joined.push([from, to]);
  }
  return joined
    .map(([from, to]) => typed.doc.textBetween(from, to, "\n"))
    .filter((text) => text.trim().length > 0)
    .join("\n");
}

/**
 * Moves the steps that wait, made on `confirmed`, onto a snapshot. The first `sent` of them went
 * to the document, which may have taken them before the snapshot, so they come again only when
 * the snapshot lies nearer to the document without them. When the document refused them, they
 * never come again.
 */
function carry(
  confirmed: ProseNode,
  waiting: readonly Step[],
  sent: number,
  refused: boolean,
  snapshot: ProseNode,
): Carried {
  const typed = new Transform(confirmed);
  for (const step of waiting) {
    if (typed.maybeStep(step).failed) break;
  }
  const went = Math.min(sent, typed.steps.length);
  const starts = refused || went === 0 ? [went] : [0, went];
  const nearest = starts
    .map((start) => {
      const over = between(typed.docs[start] ?? typed.doc, snapshot);
      return { start, over, distance: distance(over) };
    })
    .reduce((best, way) => (way.distance < best.distance ? way : best));
  const { start, over } = nearest;
  const { kept, lost } = rebase(typed.steps.slice(start), typed.docs.slice(start), over);
  const refusedSteps = refused ? Array.from({ length: start }, (_, index) => index) : [];
  const gone = [...refusedSteps, ...lost.map((index) => start + index)];
  return { steps: kept, lost: textOf(typed, gone) };
}

/**
 * The steps of this copy that wait for the document, kept across a snapshot. A snapshot comes
 * when the document opened again, such as after it closed or its computer started again, and
 * what the person did meanwhile moves onto it.
 */
export class Waiting {
  /** The document at the client's rev, as the changes in the order made it. */
  private confirmed: ProseNode | undefined;
  /** How many of the steps that wait are in the batch on its way. */
  private sent = 0;

  /** A batch with this many steps goes to the document. */
  sending(steps: number): void {
    this.sent = steps;
  }

  /** A change came in the order. */
  changed(steps: readonly Step[], own: boolean): void {
    if (own) this.sent = 0;
    for (const step of steps) {
      if (this.confirmed) this.confirmed = step.apply(this.confirmed).doc ?? this.confirmed;
    }
  }

  /** The copy shows `snapshot` now, in place of `before`. It gives the steps that go on it. */
  loaded(snapshot: ProseNode, before: EditorState | undefined, refused: boolean): Carried {
    const waiting = before ? (sendableSteps(before)?.steps ?? []) : [];
    const carried =
      this.confirmed && waiting.length > 0
        ? carry(this.confirmed, waiting, this.sent, refused, snapshot)
        : { steps: [], lost: "" };
    this.confirmed = snapshot;
    this.sent = 0;
    return carried;
  }
}
