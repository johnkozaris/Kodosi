/**
 * The batch on its way stays here until its answer comes, so a view can move it or send it
 * again.
 */
export class Outbox<Change> {
  private sent: Change[] = [];
  private queued: Change[] = [];

  add(change: Change): void {
    this.queued.push(change);
  }

  unanswered(): Change[] {
    return [...this.sent, ...this.queued];
  }

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

  move(move: (change: Change) => Change | null): void {
    const moved = (changes: Change[]) => changes.flatMap((change) => move(change) ?? []);
    this.sent = moved(this.sent);
    this.queued = moved(this.queued);
  }
}

export function batchOf<Change>(changes: readonly Change[]): unknown | null {
  if (changes.length === 0) return null;
  return changes.length === 1 ? changes[0] : { kind: "group", changes };
}
