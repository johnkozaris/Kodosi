import type { ToClient, ToDocument } from "@documents/contract";

const JSON_FRAME = 0;
const GZIP_FRAME = 1;
const ASSET_FRAME = 2;
const HASH_BYTES = 32;
const GZIP_FROM = 16 * 1024;

export type Frame =
  | { kind: "message"; message: ToClient }
  | { kind: "asset"; hash: string; bytes: Uint8Array };

function hexOf(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function bytesOfHex(text: string): Uint8Array {
  const bytes = new Uint8Array(text.length / 2);
  for (let index = 0; index < bytes.length; index++) {
    bytes[index] = Number.parseInt(text.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

async function transformed(bytes: Uint8Array, stream: GenericTransformStream): Promise<Uint8Array> {
  const writer = stream.writable.getWriter();
  writer.write(bytes as Uint8Array<ArrayBuffer>);
  writer.close();
  return new Uint8Array(await new Response(stream.readable).arrayBuffer());
}

function prefixed(kind: number, body: Uint8Array): Uint8Array {
  const frame = new Uint8Array(body.length + 1);
  frame[0] = kind;
  frame.set(body, 1);
  return frame;
}

export async function encodeMessage(message: ToDocument): Promise<Uint8Array> {
  const json = new TextEncoder().encode(JSON.stringify(message));
  if (json.length < GZIP_FROM) return prefixed(JSON_FRAME, json);
  return prefixed(GZIP_FRAME, await transformed(json, new CompressionStream("gzip")));
}

/** Names an asset with no bytes to ask for it, or with its bytes to give it. */
export function encodeAsset(hash: string, bytes: Uint8Array = new Uint8Array()): Uint8Array {
  const frame = new Uint8Array(1 + HASH_BYTES + bytes.length);
  frame[0] = ASSET_FRAME;
  frame.set(bytesOfHex(hash), 1);
  frame.set(bytes, 1 + HASH_BYTES);
  return frame;
}

export async function decodeFrame(frame: Uint8Array): Promise<Frame> {
  const body = frame.subarray(1);
  switch (frame[0]) {
    case JSON_FRAME:
      return { kind: "message", message: JSON.parse(new TextDecoder().decode(body)) };
    case GZIP_FRAME: {
      const json = await transformed(body, new DecompressionStream("gzip"));
      return { kind: "message", message: JSON.parse(new TextDecoder().decode(json)) };
    }
    case ASSET_FRAME:
      return {
        kind: "asset",
        hash: hexOf(body.subarray(0, HASH_BYTES)),
        bytes: body.subarray(HASH_BYTES),
      };
    default:
      throw new Error("The frame is not valid.");
  }
}
