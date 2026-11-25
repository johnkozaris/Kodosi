import type { Sent } from "./client";

/**
 * Retains the in-flight batch alongside queued edits until acknowledged, allowing views to rebase
 * or resend it.
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

  /**
   * Reconciles the in-flight batch after a snapshot load. Drops it when already applied ("taken"),
   * rejected ("refused"), or uncertain ("unknown", to avoid applying it twice), returning true if
   * uncommitted edits were dropped so the view can warn the user. Queued edits are preserved.
   */
  loaded(sent: Sent): boolean {
    const doubtful = sent === "refused" || sent === "unknown";
    const dropped = doubtful && this.sent.length > 0;
    if (doubtful || sent === "taken") this.sent = [];
    return dropped;
  }

  clear(): void {
    this.sent = [];
    this.queued = [];
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
