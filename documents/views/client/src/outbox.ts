/**
 * The edits that a view made and the document did not answer. The batch on its way stays here
 * until its answer comes, so a view can move it or send it again.
 */
export class Outbox<Change> {
  private sent: Change[] = [];
  private queued: Change[] = [];

  add(change: Change): void {
    this.queued.push(change);
  }

  /** The unanswered edits, oldest first. */
  unanswered(): Change[] {
    return [...this.sent, ...this.queued];
  }

  /** Puts the next batch on its way, or gives the one that a refusal returned. */
  next(): Change[] {
    if (this.sent.length === 0) {
      this.sent = this.queued;
      this.queued = [];
    }
    return this.sent;
  }

  answered(): void {
    this.sent = [];
  }

  /** Replaces each unanswered edit with what `move` gives. */
  move(move: (change: Change) => Change | null): void {
    const moved = (changes: Change[]) => changes.flatMap((change) => move(change) ?? []);
    this.sent = moved(this.sent);
    this.queued = moved(this.queued);
  }
}

/** One change for a batch, which a group holds when it has many. */
export function batchOf<Change>(changes: readonly Change[]): unknown | null {
  if (changes.length === 0) return null;
  return changes.length === 1 ? changes[0] : { kind: "group", changes };
}
