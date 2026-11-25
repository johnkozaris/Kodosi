import { type Area, Client, type Editor, type Pictures, type Pipe } from "@documents/client";
import { say, useLanguage } from "@documents/words";
import "./base.css";
import "./fonts.css";

export type Kind = "docx" | "xlsx" | "pptx" | "pdf" | "text";

export interface Preferences {
  /** When false, reflows document text to fit the viewport width instead of paginating. */
  pages?: boolean;
  dark?: boolean;
}

const DEFAULTS: Record<Kind, Required<Preferences>> = {
  docx: { pages: true, dark: false },
  xlsx: { pages: true, dark: true },
  pptx: { pages: true, dark: false },
  pdf: { pages: true, dark: false },
  text: { pages: false, dark: true },
};

/** CSS custom properties without leading "--", plus optional "scheme" ("light" or "dark"). */
export type Theme = Record<string, string>;

export interface Options {
  kind: Kind;
  theme?: Theme;
  preferences?: Preferences;
  language?: string;
  /** Called after initial load and after each reconnect rejoin. */
  ready?(): void;
  /** Renders the entire document without a toolbar or scroll container for snapshot capture. */
  render?: boolean;
}

export interface View {
  /** Connects a transport pipe on startup and after each reconnect. */
  attach(pipe: Pipe): void;
  /** Marks the transport disconnected; local edits continue and queue until the next attach. */
  detach(): void;
  selection(): string;
  restyle(theme: Theme): void;
  prefer(preferences: Preferences): void;
}

type FileEditor = Editor & Pictures & { selection(): string };
type EditorFactory = (element: HTMLElement, client: Client) => FileEditor;

async function factoryOf(kind: Kind): Promise<EditorFactory> {
  switch (kind) {
    case "docx":
      return (await import("@documents/docx")).createEditor;
    case "xlsx":
      return (await import("@documents/xlsx")).createEditor;
    case "pptx":
      return (await import("@documents/pptx")).createEditor;
    case "pdf":
      return (await import("@documents/pdf")).createEditor;
    case "text":
      return (await import("@documents/plain")).createEditor;
  }
}

// Snapshot rendering ignores user preferences so captured pages stay canonical.
function setPreferences(
  element: HTMLElement,
  options: Options,
  preferences: Preferences | undefined,
): void {
  if (options.render) return;
  const chosen = { ...DEFAULTS[options.kind], ...preferences };
  element.toggleAttribute("data-fit", !chosen.pages);
  element.toggleAttribute("data-dark-pages", chosen.dark);
}

function setTheme(element: HTMLElement, theme: Theme | undefined): void {
  for (const [name, value] of Object.entries(theme ?? {})) {
    if (name === "scheme") element.dataset.scheme = value;
    else element.style.setProperty(`--${name}`, value);
  }
}

export function mount(element: HTMLElement, options: Options): View & Pictures {
  const away = document.createElement("div");
  away.className = "document-away";
  const client = new Client(() => {
    away.remove();
    options.ready?.();
  });
  let editor: FileEditor | undefined;
  let waitingPipe: Pipe | undefined;
  element.classList.add("document-view");
  if (options.render) element.dataset.render = "";
  setTheme(element, options.theme);
  setPreferences(element, options, options.preferences);
  useLanguage(options.language)
    .catch(console.error)
    .then(() => factoryOf(options.kind))
    .then((factory) => {
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
      away.textContent = say("The computer with this file is offline. Your edits wait here.");
      element.append(away);
    },
    selection: () => editor?.selection() ?? "",
    restyle: (theme) => setTheme(element, theme),
    prefer: (preferences) => setPreferences(element, options, preferences),
    pictures: () => editor?.pictures() ?? Promise.resolve(0),
    picture: (index: number): Promise<Area> =>
      editor?.picture(index) ?? Promise.reject(new Error("The file is not open.")),
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

/** Bridge entry point for native app webviews, transporting binary frames as Base64 strings. */
function hosted(options: Options, post: (frame: string) => void) {
  const element = document.body.appendChild(document.createElement("div"));
  if (!options.render) element.style.height = "100vh";
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
    pictures: view.pictures,
    picture: view.picture,
    restyle: view.restyle,
    prefer: view.prefer,
  };
}

declare global {
  interface Window {
    documentView?: { mount: typeof mount; hosted: typeof hosted };
  }
}

window.documentView = { mount, hosted };
