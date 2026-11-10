import {
  type Changed,
  type ClosedReason,
  CONTRACT,
  type Peer,
  type ToClient,
  type ToDocument,
} from "@documents/contract";
import { decodeFrame, encodeAsset, encodeMessage } from "./frame";

const PLACE_EVERY = 100;

/** The host's end of a view: it carries bytes in any way it likes. */
export interface Pipe {
  send(frame: Uint8Array): void;
  onMessage(handler: (frame: Uint8Array) => void): void;
}

export interface Person {
  id: string;
  name: string;
  canEdit: boolean;
}

/** What an editor does for its client. */
export interface Editor {
  /**
   * Shows the document of a snapshot. `refused` says that the document refused the batch on its
   * way, so this copy loads again without it.
   */
  load(snapshot: unknown, person: Person, refused: boolean): void;
  apply(changed: Changed, own: boolean): void;
  /** The next batch of edits, or null when none waits. */
  batch(): unknown | null;
  peers(peers: readonly Peer[]): void;
  saved(rev: number, waits: boolean): void;
  closed(reason: ClosedReason): void;
  destroy(): void;
}

/**
 * Join, unanswered edits and connecting again. Once the document took a client in, the client
 * sends one batch at a time, and an editor's own edit never waits for it.
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

  /** `joined` runs each time that the document takes this client in. */
  constructor(private readonly joined: () => void = () => {}) {}

  use(editor: Editor): void {
    this.editor = editor;
  }

  /** Takes a new pipe, at the start and after each lost link. */
  attach(pipe: Pipe): void {
    this.pipe = pipe;
    this.waitingSeq = null;
    this.joining = true;
    pipe.onMessage((frame) => {
      this.incoming = this.incoming.then(() => this.receive(frame)).catch(console.error);
    });
    this.send({
      type: "join",
      contract: CONTRACT,
      client: this.id,
      epoch: this.epoch,
      rev: this.rev,
    });
  }

  /** The link is gone. Edits wait here until the next pipe. */
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
        this.epoch = message.epoch;
        this.lastSeq = message.seq;
        if (message.catchup.kind === "snapshot") {
          this.rev = message.rev;
          const person = { id: message.id, name: message.name, canEdit: message.canEdit };
          editor?.load(message.catchup.snapshot, person, this.refused);
        } else {
          for (const changed of message.catchup.changes) this.applied(changed);
        }
        this.refused = false;
        this.joining = false;
        this.peers.clear();
        for (const peer of message.peers) this.peers.set(peer.id, peer);
        editor?.peers([...this.peers.values()]);
        this.joined();
        this.pull();
        break;
      }
      case "changed":
        this.applied(message);
        this.pull();
        break;
      case "refused":
        this.waitingSeq = null;
        if (message.reason === "invalid") this.reload();
        else this.pull();
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

  private applied(changed: Changed): void {
    const own = changed.client === this.id;
    this.rev = changed.rev;
    if (own && changed.seq === this.waitingSeq) this.waitingSeq = null;
    if (own) this.lastSeq = Math.max(this.lastSeq, changed.seq);
    this.editor?.apply(changed, own);
  }

  /** A batch that the document calls invalid means that this copy went wrong: load again. */
  private reload(): void {
    this.rev = 0;
    this.epoch = 0;
    this.refused = true;
    this.joining = true;
    this.send({
      type: "join",
      contract: CONTRACT,
      client: this.id,
      epoch: 0,
      rev: 0,
    });
  }

  /** Sends the next batch, when none waits for its answer. */
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
   * Where the person is, at most ten times a second, as the document is at this client's rev.
   * The document moves the place across the changes that come after that rev.
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

  /** An asset by its hash. It comes once, and a copy stays here. */
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
      this.pipe?.send(encodeAsset(hash));
    });
  }

  /** Gives the document an asset that this view added. */
  addAsset(hash: string, bytes: Uint8Array): void {
    this.assets.set(hash, bytes);
    this.pipe?.send(encodeAsset(hash, bytes));
  }
}

type ToDocumentBody = Extract<ToDocument, { type: "change" }>["body"];
