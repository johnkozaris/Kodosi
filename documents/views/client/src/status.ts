import type { ClosedReason, Peer } from "@documents/contract";

const PEER_COLORS = ["#b5651d", "#2f6db5", "#3d8c5a", "#8c4bb5", "#b5304d", "#2c8c8c"];

export function colorOf(id: string): string {
  let hash = 0;
  for (const letter of id) hash = (hash * 31 + (letter.codePointAt(0) ?? 0)) | 0;
  return PEER_COLORS[Math.abs(hash) % PEER_COLORS.length] ?? "#b5651d";
}

/** What every format shows: who is here, and the state of the save. */
export class Status {
  readonly element = document.createElement("div");
  private readonly state = document.createElement("span");
  private readonly people = document.createElement("div");

  constructor(private readonly root: HTMLElement) {
    this.element.className = "document-status";
    this.state.className = "document-state";
    this.people.className = "document-peers";
    this.element.append(this.state, this.people);
  }

  peers(peers: readonly Peer[]): void {
    this.people.replaceChildren(
      ...peers.map((peer) => {
        const mark = document.createElement("span");
        mark.className = "document-peer";
        mark.title = peer.name;
        mark.textContent = peer.name.slice(0, 1).toUpperCase();
        mark.style.background = colorOf(peer.id);
        return mark;
      }),
    );
  }

  saved(waits: boolean): void {
    this.state.textContent = waits ? "Save waits" : "Saved";
  }

  closed(reason: ClosedReason): void {
    const notice = this.root.appendChild(document.createElement("div"));
    notice.className = "document-closed";
    notice.textContent =
      reason === "removed" ? "Someone removed this file." : "This document closed.";
  }
}
