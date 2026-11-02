import { Client, type Editor, type Pipe } from "@documents/client";
import "./base.css";

export type Kind = "word" | "excel" | "powerpoint" | "pdf";

/** The host's look, as CSS variables without their leading dashes. */
export type Theme = Record<string, string>;

export interface Options {
  kind: Kind;
  theme?: Theme;
  /** Runs when the file shows, and each time that the view joins it again. */
  ready?(): void;
}

export interface View {
  /** Takes a new pipe, at the start and after each lost link. */
  attach(pipe: Pipe): void;
  /** The link is gone. The person can go on, and the edits wait for the next pipe. */
  detach(): void;
  /** The words that the person selected, for a host that passes them on. */
  selection(): string;
  destroy(): void;
}

type EditorFactory = (element: HTMLElement, client: Client) => Editor & { selection(): string };

/** Each format's code loads only when a file of that format opens. */
async function factoryOf(kind: Kind): Promise<EditorFactory> {
  switch (kind) {
    case "word":
      return (await import("@documents/word")).createEditor;
    case "excel":
      return (await import("@documents/excel")).createEditor;
    case "powerpoint":
      return (await import("@documents/powerpoint")).createEditor;
    case "pdf":
      return (await import("@documents/pdf")).createEditor;
  }
}

function setTheme(element: HTMLElement, theme: Theme | undefined): void {
  for (const [name, value] of Object.entries(theme ?? {})) {
    element.style.setProperty(`--${name}`, value);
  }
}

export function mount(element: HTMLElement, options: Options): View {
  const away = document.createElement("div");
  away.className = "document-away";
  away.textContent = "The computer of this file is away. Your edits wait here.";
  const client = new Client(() => {
    away.remove();
    options.ready?.();
  });
  let editor: (Editor & { selection(): string }) | undefined;
  let waitingPipe: Pipe | undefined;
  element.classList.add("document-view");
  setTheme(element, options.theme);
  factoryOf(options.kind).then((factory) => {
    editor = factory(element, client);
    client.use(editor);
    if (waitingPipe) client.attach(waitingPipe);
  }, console.error);
  return {
    attach(pipe) {
      if (editor) client.attach(pipe);
      else waitingPipe = pipe;
    },
    detach() {
      client.detach();
      element.append(away);
    },
    selection: () => editor?.selection() ?? "",
    destroy() {
      editor?.destroy();
      element.replaceChildren();
      element.classList.remove("document-view");
    },
  };
}

const BASE64_STEP = 0x8000;

function base64Of(frame: Uint8Array): string {
  let text = "";
  for (let at = 0; at < frame.length; at += BASE64_STEP) {
    text += String.fromCharCode(...frame.subarray(at, at + BASE64_STEP));
  }
  return btoa(text);
}

function bytesOf(data: string): Uint8Array {
  const text = atob(data);
  const bytes = new Uint8Array(text.length);
  for (let at = 0; at < text.length; at++) bytes[at] = text.charCodeAt(at);
  return bytes;
}

/** A view in a native app, whose pipe carries each frame as Base64 text. */
function hosted(options: Options, post: (frame: string) => void) {
  const element = document.body.appendChild(document.createElement("div"));
  element.style.height = "100vh";
  const view = mount(element, options);
  let receive: (frame: Uint8Array) => void = () => {};
  return {
    deliver: (data: string) => receive(bytesOf(data)),
    attach() {
      view.attach({
        send: (frame) => post(base64Of(frame)),
        onMessage: (handler) => {
          receive = handler;
        },
      });
    },
    detach: view.detach,
    selection: view.selection,
    restyle: (theme: Theme) => setTheme(element, theme),
  };
}

declare global {
  interface Window {
    documentView?: { mount: typeof mount; hosted: typeof hosted };
  }
}

window.documentView = { mount, hosted };
