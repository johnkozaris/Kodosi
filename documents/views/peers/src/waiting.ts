import type { Sent } from "@documents/client";
import { Mark, type Node as ProseNode } from "@tiptap/pm/model";
import type { EditorState } from "@tiptap/pm/state";
import { Mapping, ReplaceStep, type Step, Transform } from "@tiptap/pm/transform";
import { sendableSteps } from "prosemirror-collab";

/** This copy's steps on a snapshot, and the text of the edits that stay off it. */
export interface Carried {
  steps: Step[];
  /** The text of the edits that stay off the snapshot, when they change what shows. */
  lost: string | undefined;
}

type Range = [number, number];

/**
 * Whether two nodes show the same: their type, marks, text and children. Attributes, such as the
 * ids that a file gets when it opens again, do not count.
 */
function alike(a: ProseNode, b: ProseNode): boolean {
  if (a === b) return true;
  if (a.type !== b.type || a.text !== b.text || a.childCount !== b.childCount) return false;
  if (!Mark.sameSet(a.marks, b.marks)) return false;
  for (let index = 0; index < a.childCount; index++) {
    if (!alike(a.child(index), b.child(index))) return false;
  }
  return true;
}

/**
 * The top-level blocks of `doc` that differ from `other`: the blocks before and after them are
 * the same in both. Where blocks only went in or out, the range takes in a block on each side, so
 * it has a place.
 */
function changed(doc: ProseNode, other: ProseNode): Range | null {
  const most = Math.min(doc.childCount, other.childCount);
  let first = 0;
  while (first < most && doc.child(first).eq(other.child(first))) first++;
  let last = 0;
  while (
    last < most - first &&
    doc.child(doc.childCount - 1 - last).eq(other.child(other.childCount - 1 - last))
  ) {
    last++;
  }
  const end = doc.childCount - last;
  if (first < end) return [first, end];
  if (doc.childCount === other.childCount) return null;
  return [Math.max(0, first - 1), Math.min(doc.childCount, end + 1)];
}

function around(a: Range | null, b: Range | null): Range | null {
  if (!a || !b) return a ?? b;
  return [Math.min(a[0], b[0]), Math.max(a[1], b[1])];
}

/** Where the blocks of `range` of `doc` show in `snapshot`, when they show there once. */
function placeOf(doc: ProseNode, [from, to]: Range, snapshot: ProseNode): number | null {
  let place: number | null = null;
  for (let at = 0; at + to - from <= snapshot.childCount; at++) {
    let same = true;
    for (let index = from; same && index < to; index++) {
      same = alike(doc.child(index), snapshot.child(at + index - from));
    }
    if (same && place !== null) return null;
    if (same) place = at;
  }
  return place;
}

function startOf(doc: ProseNode, block: number): number {
  let pos = 0;
  for (let index = 0; index < block; index++) pos += doc.child(index).nodeSize;
  return pos;
}

/**
 * Moves steps across others that came first, as prosemirror-collab does: each step maps back
 * across the steps before it, across `over`, and forward across those that moved. `docs` holds
 * the document before each step.
 */
function rebase(steps: readonly Step[], docs: readonly ProseNode[], over: Transform): Step[] {
  const mapping = new Mapping();
  for (let index = steps.length - 1; index >= 0; index--) {
    const step = steps[index] as Step;
    mapping.appendMap(step.invert(docs[index] as ProseNode).getMap());
  }
  mapping.appendMapping(over.mapping);
  const moved = new Transform(over.doc);
  const kept: Step[] = [];
  steps.forEach((step, index) => {
    const mirror = steps.length - 1 - index;
    const mapped = step.map(mapping.slice(mirror + 1));
    if (mapped && !moved.maybeStep(mapped).failed) {
      mapping.appendMap(mapped.getMap(), mirror);
      kept.push(mapped);
    }
  });
  return kept;
}

/**
 * Moves the steps `from` on of `typed` onto `snapshot`, where the blocks `range` of the document
 * before them show unchanged once. The blocks around them may hold changes of others. It gives the
 * moved steps only when the snapshot ends with the person's blocks in place of those blocks and
 * all else as it was.
 */
function onto(typed: Transform, from: number, [first, end]: Range, snapshot: ProseNode) {
  const base = typed.docs[from] ?? typed.doc;
  const at = placeOf(base, [first, end], snapshot);
  if (at === null) return null;
  const blocks = end - first;
  const over = new Transform(base);
  const after = snapshot.slice(startOf(snapshot, at + blocks));
  const before = snapshot.slice(0, startOf(snapshot, at));
  if (
    over.maybeStep(new ReplaceStep(startOf(base, end), base.content.size, after)).failed ||
    over.maybeStep(new ReplaceStep(0, startOf(base, first), before)).failed
  ) {
    return null;
  }
  const steps = rebase(typed.steps.slice(from), typed.docs.slice(from), over);
  const moved = new Transform(snapshot);
  if (steps.some((step) => moved.maybeStep(step).failed)) return null;
  const person = typed.doc;
  const last = person.childCount - (base.childCount - end);
  const expected: ProseNode[] = [];
  for (let index = 0; index < at; index++) expected.push(snapshot.child(index));
  for (let index = first; index < last; index++) expected.push(person.child(index));
  for (let index = at + blocks; index < snapshot.childCount; index++) {
    expected.push(snapshot.child(index));
  }
  const result = moved.doc;
  const same =
    result.childCount === expected.length &&
    expected.every((block, index) => alike(result.child(index), block));
  return same ? steps : null;
}

/**
 * The steps from `from` on, moved onto the snapshot, or null when they do not fit it. With
 * `proving`, the steps before `from` must show in the snapshot too.
 */
function way(typed: Transform, from: number, proving: boolean, snapshot: ProseNode) {
  const base = typed.docs[from] ?? typed.doc;
  if (alike(base, snapshot)) return onto(typed, from, [0, base.childCount], snapshot);
  const batch = proving ? changed(base, typed.docs[0] ?? typed.doc) : null;
  const range = around(changed(base, typed.doc), batch);
  return range ? onto(typed, from, range, snapshot) : [];
}

/** The text that some steps of `typed` put in, as it shows when all its steps are made. */
function textOf(typed: Transform, steps: readonly number[]): string {
  const ranges: Range[] = [];
  for (const index of steps) {
    const later = typed.mapping.slice(index + 1);
    typed.mapping.maps[index]?.forEach((_fromA, _toA, fromB, toB) => {
      const from = later.map(fromB, 1);
      const to = later.map(toB, -1);
      if (to > from) ranges.push([from, to]);
    });
  }
  ranges.sort(([a], [b]) => a - b);
  const joined: Range[] = [];
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
 * Moves the steps that wait, made on `confirmed`, onto a snapshot. The first `count` of them are
 * the batch on its way, and `sent` says what became of it. They come only onto a snapshot that
 * shows as the document they were made on, at least in the blocks that they change, so a step
 * never comes twice and never comes in part. Else the text of the edits shows, to write again.
 */
function carry(
  confirmed: ProseNode,
  waiting: readonly Step[],
  count: number,
  sent: Sent,
  snapshot: ProseNode,
): Carried {
  const typed = new Transform(confirmed);
  for (const step of waiting) {
    if (typed.maybeStep(step).failed) break;
  }
  const batch = Math.min(count, typed.steps.length);
  const complete = typed.steps.length === waiting.length;
  const fits = (from: number, proving: boolean) =>
    complete ? way(typed, from, proving, snapshot) : null;
  let steps: Step[] | null = null;
  if (sent === "none" || sent === "lost") steps = fits(0, false);
  else if (sent === "taken") steps = fits(batch, false);
  else if (sent === "unknown") {
    const all = fits(0, false);
    const rest = batch > 0 ? fits(batch, true) : null;
    const unchanged = alike(confirmed, typed.docs[batch] ?? typed.doc);
    steps = all && rest && !unchanged ? null : (all ?? rest);
  }
  if (steps) return { steps, lost: undefined };
  const shown = sent === "taken" ? batch : 0;
  const before = typed.docs[shown] ?? typed.doc;
  const gone = Array.from({ length: typed.steps.length - shown }, (_, index) => shown + index);
  return { steps: [], lost: alike(before, typed.doc) ? undefined : textOf(typed, gone) };
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
  loaded(snapshot: ProseNode, before: EditorState | undefined, sent: Sent): Carried {
    const waiting = before ? (sendableSteps(before)?.steps ?? []) : [];
    const confirmed = this.confirmed;
    const count = sent === "none" ? 0 : this.sent;
    this.confirmed = snapshot;
    this.sent = 0;
    if (!confirmed || waiting.length === 0) return { steps: [], lost: undefined };
    return carry(confirmed, waiting, count, sent, snapshot);
  }
}
