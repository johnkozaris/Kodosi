import { expect, test } from "bun:test";
import type { ToClient, ToDocument } from "@documents/contract";
import { Client, type Editor, type Pipe, type Sent } from "./client";

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

function joined(rev: number, seq = 0, epoch = 7): ToClient {
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

test("a snapshot says what became of the batch on its way, as far as the document knows", async () => {
  for (const [seq, sent] of [
    [2, "taken"],
    [1, "lost"],
    [0, "unknown"],
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
