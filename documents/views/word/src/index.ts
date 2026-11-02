import {
  type Client,
  colorOf,
  type Person,
  Status,
  type Editor as ViewEditor,
} from "@documents/client";
import type { Changed, ClosedReason, Peer } from "@documents/contract";
import { Editor, Extension } from "@tiptap/core";
import { history, redo, undo } from "@tiptap/pm/history";
import { keymap } from "@tiptap/pm/keymap";
import type { Node as ProseNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Step } from "@tiptap/pm/transform";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import { collab, receiveTransaction, sendableSteps } from "prosemirror-collab";
import { pages } from "./pages";
import { extensions } from "./schema";
import { pageSize, styleSheet, type WordView } from "./styles";
import "./word.css";

interface Snapshot {
  doc: Record<string, unknown>;
  view: WordView;
}

interface Place {
  from: number;
  to: number;
}

const peerKey = new PluginKey<DecorationSet>("peers");
const PAGE_INSET = 16;

function placeOf(body: unknown, size: number): Place | null {
  const place = body as Partial<Place> | null;
  if (typeof place?.from !== "number" || typeof place.to !== "number") return null;
  return { from: Math.min(place.from, size), to: Math.min(place.to, size) };
}

function peerDecorations(doc: ProseNode, peers: readonly Peer[]): DecorationSet {
  const decorations: Decoration[] = [];
  for (const peer of peers) {
    const place = placeOf(peer.place, doc.content.size);
    if (!place) continue;
    const color = colorOf(peer.id);
    if (place.to > place.from) {
      decorations.push(Decoration.inline(place.from, place.to, { style: `background:${color}33` }));
    }
    const caret = document.createElement("span");
    caret.className = "peer-caret";
    caret.style.borderColor = color;
    const name = caret.appendChild(document.createElement("span"));
    name.textContent = peer.name;
    name.style.background = color;
    decorations.push(Decoration.widget(place.to, caret, { side: 1 }));
  }
  return DecorationSet.create(doc, decorations);
}

/** Each top-level block keeps one id, so agents and saves can name it. */
function blockIds(): Plugin {
  return new Plugin({
    appendTransaction(transactions, _old, state) {
      if (!transactions.some((transaction) => transaction.docChanged)) return null;
      const seen = new Set<string>();
      let transaction = state.tr;
      let position = 0;
      state.doc.forEach((node) => {
        const id = node.attrs.id as string | null | undefined;
        if (id === undefined) return;
        if (id === null || seen.has(id)) {
          transaction = transaction.setNodeAttribute(position, "id", crypto.randomUUID());
        } else {
          seen.add(id);
        }
        position += node.nodeSize;
      });
      if (!transaction.docChanged) return null;
      return transaction;
    },
  });
}

function peersPlugin(): Plugin<DecorationSet> {
  return new Plugin<DecorationSet>({
    key: peerKey,
    state: {
      init: () => DecorationSet.empty,
      apply(transaction, current) {
        const peers = transaction.getMeta(peerKey) as readonly Peer[] | undefined;
        if (peers) return peerDecorations(transaction.doc, peers);
        return current.map(transaction.mapping, transaction.doc);
      },
    },
    props: {
      decorations(state) {
        return peerKey.getState(state);
      },
    },
  });
}

function imagesPlugin(client: Client): Plugin {
  const load = (root: HTMLElement) => {
    for (const image of root.querySelectorAll<HTMLImageElement>("img[data-asset]:not([src])")) {
      const asset = image.dataset.asset;
      if (!asset) continue;
      image.src = "data:,";
      client.asset(asset).then((bytes) => {
        image.src = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>]));
      }, console.error);
    }
  };
  return new Plugin({
    view(view) {
      load(view.dom);
      return { update: () => load(view.dom) };
    },
  });
}

class WordEditor implements ViewEditor {
  private editor: Editor | undefined;
  private readonly bar: HTMLElement;
  private readonly status: Status;
  private readonly page: HTMLElement;
  private readonly look = document.createElement("style");
  private readonly clientKey = crypto.randomUUID();
  private fitting: ResizeObserver | undefined;

  constructor(
    root: HTMLElement,
    private readonly client: Client,
  ) {
    this.status = new Status(root);
    this.bar = root.appendChild(document.createElement("div"));
    this.bar.className = "document-bar";
    this.page = root.appendChild(document.createElement("div"));
    this.page.className = "document-page word-page";
    root.appendChild(this.look);
  }

  private tool(label: string, name: string, run: () => void): HTMLButtonElement {
    const button = this.bar.appendChild(document.createElement("button"));
    button.type = "button";
    button.textContent = label;
    button.title = name;
    button.ariaLabel = name;
    button.addEventListener("mousedown", (event) => {
      event.preventDefault();
      run();
    });
    return button;
  }

  private buildBar(view: WordView): void {
    this.bar.replaceChildren();
    const style = this.bar.appendChild(document.createElement("select"));
    style.ariaLabel = "Style";
    for (const [id, look] of Object.entries(view.styles?.styles ?? {})) {
      style.add(new Option(look.name ?? id, id));
    }
    style.addEventListener("change", () => {
      this.editor
        ?.chain()
        .focus()
        .command(({ tr, state }) => {
          const { $from } = state.selection;
          for (let depth = $from.depth; depth > 0; depth--) {
            if ($from.node(depth).type.name === "paragraph") {
              tr.setNodeAttribute($from.before(depth), "style", style.value);
              return true;
            }
          }
          return false;
        })
        .run();
    });
    for (const [label, name, mark] of [
      ["B", "Bold", "bold"],
      ["I", "Italic", "italic"],
      ["U", "Underline", "underline"],
      ["S", "Strikethrough", "strike"],
    ] as const) {
      this.tool(label, name, () => this.editor?.chain().focus().toggleMark(mark).run());
    }
    this.tool("↶", "Undo", () => this.run(undo));
    this.tool("↷", "Redo", () => this.run(redo));
    this.bar.appendChild(document.createElement("span")).className = "gap";
    this.bar.append(this.status.element);
  }

  private run(command: typeof undo): void {
    const view = this.editor?.view;
    if (view) command(view.state, view.dispatch);
  }

  load(raw: unknown, person: Person): void {
    const snapshot = raw as Snapshot;
    this.editor?.destroy();
    this.look.textContent = styleSheet(snapshot.view);
    this.buildBar(snapshot.view);
    const client = this.client;
    const clientID = this.clientKey;
    const shape = pageSize(snapshot.view);
    const lookOf = (block: ProseNode) =>
      snapshot.view.styles?.styles[(block.attrs.style as string | null) ?? ""];
    this.editor = new Editor({
      element: this.page,
      extensions: [
        ...extensions,
        Extension.create({
          name: "documentCollab",
          addProseMirrorPlugins: () => [
            collab({ clientID }),
            history(),
            keymap({ "Mod-z": undo, "Mod-y": redo, "Shift-Mod-z": redo }),
            blockIds(),
            peersPlugin(),
            imagesPlugin(client),
            followsLinks(),
            pages(shape, {
              keepNext: (block) => lookOf(block)?.keepNext === true,
              startsPage: (block) =>
                block.attrs.pageBreak === true || lookOf(block)?.pageBreak === true,
            }),
          ],
        }),
      ],
      content: snapshot.doc,
      editable: person.canEdit,
      enableInputRules: false,
      enablePasteRules: false,
      onTransaction: ({ editor, transaction }) => {
        if (transaction.docChanged) client.pull();
        if (transaction.selectionSet || transaction.docChanged) {
          const { from, to } = editor.state.selection;
          client.place({ from, to });
        }
      },
    });
    this.fitting?.disconnect();
    this.fitting = new ResizeObserver(() => this.fit(shape.width));
    this.fitting.observe(this.page);
  }

  /** A page keeps its lines, so a narrow view shows it smaller. */
  private fit(pageWidth: number): void {
    const root = this.editor?.view.dom;
    const room = this.page.clientWidth - 2 * PAGE_INSET;
    if (root) root.style.zoom = String(Math.min(1, room / pageWidth));
  }

  batch(): unknown | null {
    const state = this.editor?.state;
    const sendable = state && sendableSteps(state);
    if (!sendable) return null;
    return { kind: "steps", steps: sendable.steps.map((step) => step.toJSON()) };
  }

  apply(changed: Changed, own: boolean): void {
    const editor = this.editor;
    if (!editor) return;
    const body = changed.body as { steps?: unknown[] };
    const steps = (body.steps ?? []).map((json: unknown) => Step.fromJSON(editor.schema, json));
    const ids = steps.map(() => (own ? this.clientKey : changed.client));
    const transaction = receiveTransaction(editor.state, steps, ids, {
      mapSelectionBackward: true,
    });
    editor.view.dispatch(transaction);
  }

  peers(peers: readonly Peer[]): void {
    this.status.peers(peers);
    const view = this.editor?.view;
    if (view) view.dispatch(view.state.tr.setMeta(peerKey, peers));
  }

  saved(_rev: number, waits: boolean): void {
    this.status.saved(waits);
  }

  closed(reason: ClosedReason): void {
    this.editor?.setEditable(false);
    this.status.closed(reason);
  }

  selection(): string {
    const state = this.editor?.state;
    if (!state) return "";
    return state.doc.textBetween(state.selection.from, state.selection.to, "\n");
  }

  destroy(): void {
    this.fitting?.disconnect();
    this.editor?.destroy();
  }
}

/** A host sees an ordinary link, and opens it by its own rules. */
function follow(address: string): void {
  if (!/^(https?|mailto):/i.test(address)) return;
  const link = document.createElement("a");
  link.href = address;
  link.target = "_blank";
  link.rel = "noopener";
  link.click();
}

/** A click places the caret, so a link opens with ⌘ or Ctrl, as in a terminal. */
function followsLinks(): Plugin {
  return new Plugin({
    props: {
      handleClick(view, position, event) {
        if (!event.metaKey && !event.ctrlKey) return false;
        const link = view.state.doc
          .resolve(position)
          .marks()
          .find((mark) => mark.type.name === "link");
        if (!link) return false;
        follow(link.attrs.href as string);
        return true;
      },
    },
  });
}

export function createEditor(element: HTMLElement, client: Client): WordEditor {
  return new WordEditor(element, client);
}
