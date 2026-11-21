import { expect, spyOn, test } from "bun:test";
import type { Changed, ToClient, ToDocument } from "@documents/contract";
import { Client, type Editor, type Pipe, type Sent } from "./client";
import { batchOf, Outbox } from "./outbox";

class TestPipe implements Pipe {
  readonly sent: ToDocument[] = [];
  private handler: (frame: Uint8Array) => void = () => {};

  send(frame: Uint8Array): void {
    this.sent.push(JSON.parse(new TextDecoder().decode(frame.subarray(1))));
  }

  onMessage(handler: (frame: Uint8Array) => void): void {
    this.handler = handler;
  }

  async deliver(message: ToClient): Promise<void> {
    const json = new TextEncoder().encode(JSON.stringify(message));
    const frame = new Uint8Array(json.length + 1);
    frame.set(json, 1);
    this.handler(frame);
    await settled();
  }
}

function settled(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

class TestEditor implements Editor {
  readonly loads: Sent[] = [];
  edits = 0;

  load(_snapshot: unknown, _person: unknown, sent: Sent): void {
    this.loads.push(sent);
  }

  apply(): void {}

  batch(): unknown | null {
    return this.edits > 0 ? { edits: this.edits } : null;
  }

  peers(): void {}
  saved(): void {}
  closed(): void {}
  destroy(): void {}
}

class OutboxEditor implements Editor {
  readonly outbox = new Outbox<string>();
  readonly others: unknown[] = [];
  dropped = false;
  breaks = false;

  load(_snapshot: unknown, _person: unknown, sent: Sent): void {
    if (this.breaks) throw new Error("The view broke.");
    this.dropped = this.outbox.loaded(sent) || this.dropped;
  }

  apply(changed: Changed, own: boolean): void {
    if (this.breaks) throw new Error("The view broke.");
    if (own) this.outbox.answered();
    else this.others.push(changed.body);
  }

  batch(): unknown | null {
    return batchOf(this.outbox.next());
  }

  peers(): void {}
  saved(): void {}
  closed(): void {}
  destroy(): void {}
}

function joined(rev: number, seq: number | null = 0, epoch = 7): ToClient {
  return {
    type: "joined",
    id: "ana",
    name: "Ana",
    canEdit: true,
    epoch,
    rev,
    seq,
    peers: [],
    catchup: { kind: "snapshot", snapshot: {} },
  };
}

function kinds(pipe: TestPipe): string[] {
  return pipe.sent.map((message) => message.type);
}

function bodies(pipe: TestPipe): unknown[] {
  return pipe.sent.map((message) => (message.type === "change" ? message.body : message.type));
}

async function quietly(run: () => Promise<void>): Promise<number> {
  const logged = spyOn(console, "error").mockImplementation(() => {});
  try {
    await run();
    await settled();
    return logged.mock.calls.length;
  } finally {
    logged.mockRestore();
  }
}

test("an edit made while the client joins goes once the document took it in", async () => {
  const editor = new TestEditor();
  const client = new Client();
  client.use(editor);
  const pipe = new TestPipe();
  client.attach(pipe);
  editor.edits = 1;
  client.pull();
  await settled();
  expect(kinds(pipe)).toEqual(["join"]);
  await pipe.deliver(joined(3));
  await settled();
  expect(kinds(pipe)).toEqual(["join", "change"]);
  expect(pipe.sent[1]).toMatchObject({ seq: 1, base: 3 });
});

test("a view that fails to load shows but sends nothing until a join loads it again", async () => {
  const editor = new OutboxEditor();
  let joins = 0;
  const client = new Client(() => joins++);
  client.use(editor);
  const pipe = new TestPipe();
  client.attach(pipe);
  await pipe.deliver(joined(0));
  editor.outbox.add("taken");
  client.pull();
  await settled();
  client.detach();
  const again = new TestPipe();
  client.attach(again);
  editor.breaks = true;
  const errors = await quietly(async () => {
    await again.deliver(joined(1, 1, 8));
    await again.deliver({
      type: "changed",
      rev: 2,
      author: "Ben",
      client: "ben",
      seq: 1,
      body: "b",
    });
    editor.outbox.add("later");
    client.pull();
  });
  expect(errors).toBe(1);
  expect(joins).toBe(2);
  expect(bodies(again)).toEqual(["join"]);
  expect(editor.others).toEqual([]);
  editor.breaks = false;
  client.detach();
  const third = new TestPipe();
  client.attach(third);
  await settled();
  expect(third.sent[0]).toMatchObject({ type: "join", epoch: 0, rev: 0 });
  await third.deliver(joined(2, 1, 8));
  await settled();
  expect(bodies(third)).toEqual(["join", "later"]);
  expect(third.sent[1]).toMatchObject({ seq: 2 });
});

test("an answer that the view fails to take keeps its batch from going again", async () => {
  const editor = new OutboxEditor();
  const client = new Client();
  client.use(editor);
  const pipe = new TestPipe();
  client.attach(pipe);
  await pipe.deliver(joined(0));
  editor.outbox.add("taken");
  client.pull();
  await settled();
  const id = (pipe.sent[0] as { client: string }).client;
  editor.breaks = true;
  await quietly(async () => {
    await pipe.deliver({
      type: "changed",
      rev: 1,
      author: "Ana",
      client: id,
      seq: 1,
      body: "taken",
    });
    editor.outbox.add("later");
    client.pull();
  });
  expect(bodies(pipe)).toEqual(["join", "taken", "join"]);
  editor.breaks = false;
  client.detach();
  const again = new TestPipe();
  client.attach(again);
  await again.deliver(joined(1, 1, 8));
  await settled();
  expect(bodies(again)).toEqual(["join", "later"]);
});

test("a client is fresh to the document only while it sent nothing and has nothing on its way", async () => {
  const editor = new OutboxEditor();
  const client = new Client();
  client.use(editor);
  const pipe = new TestPipe();
  client.attach(pipe);
  await settled();
  expect(pipe.sent[0]).toMatchObject({ type: "join", fresh: true });
  await pipe.deliver(joined(0, null));
  editor.outbox.add("first");
  client.pull();
  await settled();
  client.detach();
  const again = new TestPipe();
  client.attach(again);
  await settled();
  expect(again.sent[0]).toMatchObject({ type: "join", fresh: false });
});

test("a snapshot after a refused batch says so to the editor", async () => {
  const editor = new TestEditor();
  const client = new Client();
  client.use(editor);
  const pipe = new TestPipe();
  client.attach(pipe);
  await pipe.deliver(joined(0));
  editor.edits = 1;
  client.pull();
  await settled();
  await pipe.deliver({ type: "refused", seq: 1, reason: "invalid" });
  await settled();
  expect(pipe.sent.at(-1)).toMatchObject({ type: "join", epoch: 0, rev: 0 });
  await pipe.deliver(joined(2));
  expect(editor.loads).toEqual(["none", "refused"]);
});

test("a first batch goes again only when the document knows the client", async () => {
  for (const [seq, sent] of [
    [0, "lost"],
    [null, "unknown"],
  ] as const) {
    const editor = new TestEditor();
    const client = new Client();
    client.use(editor);
    const pipe = new TestPipe();
    client.attach(pipe);
    await pipe.deliver(joined(0));
    editor.edits = 1;
    client.pull();
    await settled();
    client.detach();
    const again = new TestPipe();
    client.attach(again);
    await again.deliver(joined(0, seq, 8));
    expect(editor.loads).toEqual(["none", sent]);
  }
});

test("a snapshot says what became of the batch on its way, as far as the document knows", async () => {
  for (const [seq, sent] of [
    [2, "taken"],
    [1, "lost"],
    [0, "unknown"],
    [null, "unknown"],
  ] as const) {
    const editor = new TestEditor();
    const client = new Client();
    client.use(editor);
    const pipe = new TestPipe();
    client.attach(pipe);
    await pipe.deliver(joined(0));
    editor.edits = 1;
    client.pull();
    await settled();
    const id = (pipe.sent[0] as { client: string }).client;
    await pipe.deliver({ type: "changed", rev: 1, author: "Ana", client: id, seq: 1, body: {} });
    await settled();
    expect(pipe.sent.at(-1)).toMatchObject({ type: "change", seq: 2 });
    client.detach();
    const again = new TestPipe();
    client.attach(again);
    await again.deliver(joined(1, seq, 8));
    expect(editor.loads).toEqual(["none", sent]);
  }
});

test("a batch that the document refused goes no more, and what came after it goes on", async () => {
  const editor = new OutboxEditor();
  const client = new Client();
  client.use(editor);
  const pipe = new TestPipe();
  client.attach(pipe);
  await pipe.deliver(joined(0));
  editor.outbox.add("refused");
  client.pull();
  editor.outbox.add("after");
  await settled();
  await pipe.deliver({ type: "refused", seq: 1, reason: "invalid" });
  await pipe.deliver(joined(0));
  await settled();
  expect(
    pipe.sent.map((message) => (message.type === "change" ? message.body : message.type)),
  ).toEqual(["join", "refused", "join", "after"]);
  expect(editor.dropped).toBe(true);
});

test("a batch that the document may have taken before it lost what it knew goes no more", async () => {
  const editor = new OutboxEditor();
  const client = new Client();
  client.use(editor);
  const pipe = new TestPipe();
  client.attach(pipe);
  await pipe.deliver(joined(0));
  editor.outbox.add("first");
  client.pull();
  await settled();
  const id = (pipe.sent[0] as { client: string }).client;
  await pipe.deliver({ type: "changed", rev: 1, author: "Ana", client: id, seq: 1, body: "first" });
  editor.outbox.add("doubtful");
  client.pull();
  editor.outbox.add("after");
  await settled();
  client.detach();
  const again = new TestPipe();
  client.attach(again);
  await again.deliver(joined(1, 0, 8));
  await settled();
  expect(
    again.sent.map((message) => (message.type === "change" ? message.body : message.type)),
  ).toEqual(["join", "after"]);
  expect(editor.dropped).toBe(true);
});

test("a batch that the document took goes once, and what came after it goes on the snapshot", async () => {
  const editor = new OutboxEditor();
  const client = new Client();
  client.use(editor);
  const pipe = new TestPipe();
  client.attach(pipe);
  await pipe.deliver(joined(0));
  editor.outbox.add("taken");
  client.pull();
  editor.outbox.add("after");
  await settled();
  client.detach();
  const again = new TestPipe();
  client.attach(again);
  await again.deliver(joined(1, 1, 8));
  await settled();
  expect(
    again.sent.map((message) => (message.type === "change" ? message.body : message.type)),
  ).toEqual(["join", "after"]);
  expect(editor.dropped).toBe(false);
});

class FramePipe implements Pipe {
  readonly frames: Uint8Array[] = [];
  private handler: (frame: Uint8Array) => void = () => {};

  send(frame: Uint8Array): void {
    this.frames.push(frame);
  }

  onMessage(handler: (frame: Uint8Array) => void): void {
    this.handler = handler;
  }

  deliver(frame: Uint8Array): void {
    this.handler(frame);
  }
}

test("an asset that a lost pipe was bringing is asked for again on the next pipe", async () => {
  const client = new Client();
  client.use(new TestEditor());
  const hash = "ab".repeat(32);
  const asked = (pipe: FramePipe) =>
    pipe.frames.filter(
      (frame) => frame[0] === 2 && Buffer.from(frame.subarray(1)).toString("hex") === hash,
    ).length;
  const lost = new FramePipe();
  client.attach(lost);
  const asset = client.asset(hash);
  expect(asked(lost)).toBe(1);
  client.detach();
  const again = new FramePipe();
  client.attach(again);
  expect(asked(again)).toBe(1);
  again.deliver(new Uint8Array([2, ...Buffer.from(hash, "hex"), 7, 8]));
  expect(await asset).toEqual(new Uint8Array([7, 8]));
});
