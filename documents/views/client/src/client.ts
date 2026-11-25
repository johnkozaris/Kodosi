import {
  type Changed,
  type ClosedReason,
  CONTRACT,
  type Peer,
  type ToClient,
  type ToDocument,
} from "@documents/contract";
import { decodeFrame, encodeAssetRequest, encodeMessage } from "./frame";

const PLACE_EVERY = 100;

export interface Pipe {
  send(frame: Uint8Array): void;
  onMessage(handler: (frame: Uint8Array) => void): void;
}

export interface Person {
  id: string;
  name: string;
  canEdit: boolean;
}

/**
 * Outcome of an in-flight change batch after rejoining: no batch was in flight, the document
 * accepted it, it never arrived, the document refused it, or the document lost client state and
 * cannot tell.
 */
export type Sent = "none" | "taken" | "lost" | "refused" | "unknown";

export interface Editor {
  load(snapshot: unknown, person: Person, sent: Sent): void;
  apply(changed: Changed, own: boolean): void;
  batch(): unknown | null;
  peers(peers: readonly Peer[]): void;
  saved(rev: number, waits: boolean): void;
  closed(reason: ClosedReason): void;
}

/**
 * Sends at most one change batch in flight at a time after joining, while local edits proceed
 * without waiting for acknowledgement.
 */
export class Client {
  private readonly id = crypto.randomUUID();
  private editor: Editor | undefined;
  private pipe: Pipe | undefined;
  private epoch = 0;
  private rev = 0;
  private lastSeq = 0;
  private waitingSeq: number | null = null;
  private joining = false;
  private refused = false;
  private readonly peers = new Map<string, Peer>();
  private readonly assets = new Map<string, Uint8Array>();
  private readonly assetWaiters = new Map<string, ((bytes: Uint8Array) => void)[]>();
  private incoming: Promise<void> = Promise.resolve();
  private placeTimer: ReturnType<typeof setTimeout> | undefined;
  private nextPlace: { rev: number; body: ToDocumentBody } | undefined;
  private placeSentAt = 0;
  private readonly joined: () => void;

  constructor(joined: () => void = () => {}) {
    this.joined = joined;
  }

  use(editor: Editor): void {
    this.editor = editor;
  }

  attach(pipe: Pipe): void {
    this.pipe = pipe;
    this.joining = true;
    pipe.onMessage((frame) => {
      this.incoming = this.incoming.then(() => this.receive(frame)).catch(console.error);
    });
    this.join();
    // Re-request pending assets because responses from the previous pipe will not arrive here.
    for (const hash of this.assetWaiters.keys()) pipe.send(encodeAssetRequest(hash));
  }

  private join(): void {
    this.send({
      type: "join",
      contract: CONTRACT,
      client: this.id,
      epoch: this.epoch,
      rev: this.rev,
      fresh: this.lastSeq === 0 && this.waitingSeq === null,
    });
  }

  detach(): void {
    this.pipe = undefined;
  }

  private send(message: ToDocument): void {
    encodeMessage(message).then((frame) => this.pipe?.send(frame), console.error);
  }

  private async receive(frame: Uint8Array): Promise<void> {
    const decoded = await decodeFrame(frame);
    if (decoded.kind === "asset") {
      this.assets.set(decoded.hash, decoded.bytes);
      for (const waiter of this.assetWaiters.get(decoded.hash) ?? []) waiter(decoded.bytes);
      this.assetWaiters.delete(decoded.hash);
      return;
    }
    this.handle(decoded.message);
  }

  private handle(message: ToClient): void {
    const editor = this.editor;
    switch (message.type) {
      case "joined": {
        const sent = this.sentOf(message.seq);
        this.epoch = message.epoch;
        this.peers.clear();
        for (const peer of message.peers) this.peers.set(peer.id, peer);
        try {
          if (message.catchup.kind === "snapshot") {
            this.rev = message.rev;
            const person = { id: message.id, name: message.name, canEdit: message.canEdit };
            editor?.load(message.catchup.snapshot, person, sent);
          } else {
            for (const changed of message.catchup.changes) this.applied(changed);
          }
        } catch (error) {
          this.outOfStep();
          this.joined();
          throw error;
        }
        this.waitingSeq = null;
        this.lastSeq = message.seq ?? 0;
        this.refused = false;
        this.joining = false;
        this.joined();
        this.pull();
        editor?.peers([...this.peers.values()]);
        break;
      }
      case "changed":
        if (this.joining) break;
        try {
          this.applied(message);
        } catch (error) {
          this.reload();
          throw error;
        }
        this.pull();
        break;
      case "refused":
        this.waitingSeq = null;
        if (message.reason === "invalid") {
          // Local state diverged from the document; force a full snapshot reload.
          this.refused = true;
          this.reload();
        } else this.pull();
        break;
      case "saved":
        editor?.saved(message.rev, message.waits);
        break;
      case "peer":
        this.peers.set(message.id, message);
        editor?.peers([...this.peers.values()]);
        break;
      case "left":
        this.peers.delete(message.id);
        editor?.peers([...this.peers.values()]);
        break;
      case "closed":
        editor?.closed(message.reason);
        break;
      case "answer":
        break;
    }
  }

  // Clear waitingSeq only after editor.apply succeeds; if apply throws, reload still needs
  // waitingSeq intact so the next join reports that the batch was already taken.
  private applied(changed: Changed): void {
    const own = changed.client === this.id;
    this.rev = changed.rev;
    this.editor?.apply(changed, own);
    if (own && changed.seq === this.waitingSeq) this.waitingSeq = null;
    if (own) this.lastSeq = Math.max(this.lastSeq, changed.seq);
  }

  /**
   * The document persists each client's last accepted seq across reopens and returns null if it
   * has no record of this client. A batch is treated as "lost" (safe to resend) only when the
   * document confirms it accepted the immediately preceding seq.
   */
  private sentOf(seq: number | null): Sent {
    const waiting = this.waitingSeq;
    if (this.refused) return "refused";
    if (waiting === null) return "none";
    if (seq === null) return "unknown";
    if (seq >= waiting) return "taken";
    return seq === waiting - 1 ? "lost" : "unknown";
  }

  /**
   * Resets sync state so the next join fetches a full snapshot and blocks outgoing batches until
   * then, while preserving waitingSeq so the rejoin still reports the in-flight batch's status.
   */
  private outOfStep(): void {
    this.joining = true;
    this.epoch = 0;
    this.rev = 0;
  }

  // Sending a second join makes the document drop this pipe so the host reconnects and loads a
  // fresh snapshot.
  private reload(): void {
    this.outOfStep();
    this.join();
  }

  pull(): void {
    if (this.joining || this.waitingSeq !== null || !this.pipe) return;
    const body = this.editor?.batch();
    if (body === null || body === undefined) return;
    this.waitingSeq = this.lastSeq + 1;
    this.lastSeq = this.waitingSeq;
    this.send({
      type: "change",
      seq: this.waitingSeq,
      base: this.rev,
      body: body as ToDocumentBody,
    });
  }

  /**
   * Reports the local selection relative to this client's current rev; the document transforms it
   * across any later revisions.
   */
  place(body: unknown): void {
    this.nextPlace = { rev: this.rev, body: body as ToDocumentBody };
    if (this.placeTimer !== undefined) return;
    const wait = Math.max(0, this.placeSentAt + PLACE_EVERY - Date.now());
    this.placeTimer = setTimeout(() => {
      this.placeTimer = undefined;
      this.placeSentAt = Date.now();
      if (this.nextPlace) this.send({ type: "place", ...this.nextPlace });
    }, wait);
  }

  asset(hash: string): Promise<Uint8Array> {
    const known = this.assets.get(hash);
    if (known) return Promise.resolve(known);
    return new Promise((resolve) => {
      const waiters = this.assetWaiters.get(hash);
      if (waiters) {
        waiters.push(resolve);
        return;
      }
      this.assetWaiters.set(hash, [resolve]);
      this.pipe?.send(encodeAssetRequest(hash));
    });
  }

  heldAsset(hash: string): Uint8Array | undefined {
    return this.assets.get(hash);
  }
}

type ToDocumentBody = Extract<ToDocument, { type: "change" }>["body"];
