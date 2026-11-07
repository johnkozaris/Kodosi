import type { ClosedReason, Peer } from "@documents/contract";
import { say } from "@documents/words";

/** The colors of people in the apps, which pick one by the same hash of the person's id. */
const PEOPLE_COLORS = ["#3c8db5", "#8573d1", "#2f92a3", "#c2659a", "#b58428", "#b362b6", "#607fcc"];
const HASH_START = 0xcbf29ce484222325n;
const HASH_PRIME = 0x100000001b3n;

/** A person's color: FNV-1a of the UTF-8 bytes of their id, as the apps hash it. */
export function colorOf(person: string): string {
  let hash = HASH_START;
  for (const byte of new TextEncoder().encode(person)) {
    hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * HASH_PRIME);
  }
  return PEOPLE_COLORS[Number(hash % BigInt(PEOPLE_COLORS.length))] ?? "#3c8db5";
}

/** The first letters of up to two words of a name, as the apps show a person. */
export function initialsOf(name: string): string {
  const letters = name
    .trim()
    .split(/\s+/)
    .filter((word) => word.length > 0)
    .slice(0, 2)
    .map((word) => String.fromCodePoint(word.codePointAt(0) ?? 0))
    .join("")
    .toUpperCase();
  return letters || "·";
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
        mark.textContent = initialsOf(peer.name);
        mark.style.background = colorOf(peer.person);
        return mark;
      }),
    );
  }

  saved(waits: boolean): void {
    this.state.textContent = waits ? say("Waiting to save") : say("Saved");
  }

  closed(reason: ClosedReason): void {
    const notice = this.root.appendChild(document.createElement("div"));
    notice.className = "document-closed";
    notice.textContent =
      reason === "removed" ? say("Someone removed this file.") : say("This document closed.");
  }
}
