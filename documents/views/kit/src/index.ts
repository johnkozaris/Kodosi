import { Client, type Editor, type Pipe } from "@documents/client";
import "./base.css";

export type Kind = "word" | "excel" | "powerpoint" | "pdf";

/** The host's look, as CSS variables without their leading dashes. */
export type Theme = Record<string, string>;

export interface Options {
  kind: Kind;
  theme?: Theme;
}

export interface View {
  /** Takes a new pipe, at the start and after each lost link. */
  attach(pipe: Pipe): void;
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
  const client = new Client();
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
    selection: () => editor?.selection() ?? "",
    destroy() {
      editor?.destroy();
      element.replaceChildren();
      element.classList.remove("document-view");
    },
  };
}

declare global {
  interface Window {
    documentView?: { mount: typeof mount };
  }
}

window.documentView = { mount };
