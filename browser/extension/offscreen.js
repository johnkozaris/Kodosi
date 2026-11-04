const HEADER_BYTES = 8 + 1 + 8;
const QUEUE_MOST = 2;
const FRAME_RATE = 60;
/** Constrained Baseline, level 5.2: every decoder reads it, and it holds 4K at 60 frames. */
const CODEC = "avc1.42E034";
const RECONNECT_FIRST_MS = 250;
const RECONNECT_LAST_MS = 5000;

/** @type {WebSocket} */
let socket;
const captures = new Map();

function toHost(message) {
  if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message));
}

function failed(stream, error) {
  toHost({ type: "failed", stream, problem: String(error) });
}

function evenPixels(value) {
  return Math.max(2, Math.round(value / 2) * 2);
}

class Capture {
  constructor(stream, tab, width, height, bitrate) {
    this.stream = stream;
    this.tab = tab;
    this.width = evenPixels(width);
    this.height = evenPixels(height);
    this.bitrate = bitrate;
    this.wantKey = true;
    this.last = null;
    this.stopped = false;
    this.encoder = new VideoEncoder({
      output: (chunk) => this.send(chunk),
      error: (error) => failed(stream, error),
    });
    this.configure();
  }

  configure() {
    this.encoder.configure({
      codec: CODEC,
      width: this.width,
      height: this.height,
      bitrate: this.bitrate,
      framerate: FRAME_RATE,
      latencyMode: "realtime",
      hardwareAcceleration: "no-preference",
      avc: { format: "annexb" },
    });
  }

  async start() {
    const id = await chrome.runtime.sendMessage({
      to: "worker",
      command: { type: "streamId", tab: this.tab },
    });
    if (typeof id !== "string") throw new Error(id?.error ?? "no stream id");
    this.media = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: {
        mandatory: {
          chromeMediaSource: "tab",
          chromeMediaSourceId: id,
          minWidth: this.width,
          maxWidth: this.width,
          minHeight: this.height,
          maxHeight: this.height,
          maxFrameRate: FRAME_RATE,
        },
      },
    });
    const [track] = this.media.getVideoTracks();
    this.track = track;
    this.reader = new MediaStreamTrackProcessor({ track }).readable.getReader();
    this.pump().catch((error) => {
      if (!this.stopped) failed(this.stream, error);
    });
  }

  async pump() {
    for (;;) {
      const { value: frame, done } = await this.reader.read();
      if (done) return;
      if (this.stopped) {
        frame.close();
        return;
      }
      this.last?.close();
      this.last = frame.clone();
      this.encode(frame);
      frame.close();
    }
  }

  encode(frame) {
    if (this.encoder.encodeQueueSize >= QUEUE_MOST && !this.wantKey) return;
    const keyFrame = this.wantKey;
    this.wantKey = false;
    this.encoder.encode(frame, { keyFrame });
  }

  /** A still page sends no frames, so a viewer that joins gets the last picture again. */
  keyframe() {
    this.wantKey = true;
    if (this.last) this.encode(this.last);
  }

  send(chunk) {
    const message = new ArrayBuffer(HEADER_BYTES + chunk.byteLength);
    const view = new DataView(message);
    view.setBigInt64(0, BigInt(this.stream));
    view.setUint8(8, chunk.type === "key" ? 1 : 0);
    view.setBigUint64(9, BigInt(Math.max(0, chunk.timestamp)));
    chunk.copyTo(new Uint8Array(message, HEADER_BYTES));
    if (socket?.readyState === WebSocket.OPEN) socket.send(message);
  }

  setBitrate(bitrate) {
    this.bitrate = bitrate;
    this.configure();
  }

  async resize(width, height) {
    this.width = evenPixels(width);
    this.height = evenPixels(height);
    await this.track.applyConstraints({
      width: { min: this.width, max: this.width },
      height: { min: this.height, max: this.height },
    });
    this.wantKey = true;
    this.configure();
  }

  stop() {
    this.stopped = true;
    this.media?.getTracks().forEach((track) => {
      track.stop();
    });
    this.last?.close();
    this.last = null;
    if (this.encoder.state !== "closed") this.encoder.close();
  }
}

async function capture({ stream, tab, width, height, bitrate }) {
  const current = captures.get(stream);
  if (current?.tab === tab) {
    await current.resize(width, height);
    return;
  }
  current?.stop();
  const next = new Capture(stream, tab, width, height, bitrate);
  captures.set(stream, next);
  await next.start();
}

const commands = {
  capture: (command) => capture(command).catch((error) => failed(command.stream, error)),
  stop({ stream }) {
    captures.get(stream)?.stop();
    captures.delete(stream);
  },
  keyframe({ stream }) {
    captures.get(stream)?.keyframe();
  },
  bitrate({ stream, bitrate }) {
    captures.get(stream)?.setBitrate(bitrate);
  },
};

function fromHost(event) {
  const command = JSON.parse(event.data);
  const local = commands[command.type];
  if (local) local(command);
  else chrome.runtime.sendMessage({ to: "worker", command });
}

/** A host that lost the connection takes the next one, so this page connects again. */
async function connect(wait = RECONNECT_FIRST_MS) {
  const again = () => setTimeout(() => connect(Math.min(wait * 2, RECONNECT_LAST_MS)), wait);
  let address;
  try {
    const { port, token } = await (await fetch(chrome.runtime.getURL("connection.json"))).json();
    address = `ws://127.0.0.1:${port}/${token}`;
  } catch {
    again();
    return;
  }
  socket = new WebSocket(address);
  socket.binaryType = "arraybuffer";
  socket.onmessage = fromHost;
  socket.onopen = () => {
    wait = RECONNECT_FIRST_MS;
    chrome.runtime.sendMessage({ to: "worker", command: { type: "report" } });
  };
  socket.onclose = () => {
    for (const capture of captures.values()) capture.stop();
    captures.clear();
    again();
  };
}

chrome.runtime.onMessage.addListener((request) => {
  if (request.to === "offscreen") toHost(request.message);
});

connect();
