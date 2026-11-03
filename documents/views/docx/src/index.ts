import {
  type Area,
  areaOf,
  type Client,
  colorOf,
  type Person,
  type Pictures,
  Status,
  settled,
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
import { DocxBar } from "./bar";
import * as commands from "./commands";
import { PAGE_GAP, pages } from "./pages";
import { type Place, placeOf, shownPlace } from "./places";
import { extensions } from "./schema";
import { type DocxView, pageSize, styleSheet } from "./styles";
import "./docx.css";

interface Snapshot {
  doc: Record<string, unknown>;
  view: DocxView;
}

/** Another person's place in this copy. */
interface Shown extends Place {
  peer: Peer;
}

interface Peers {
  shown: readonly Shown[];
  marks: DecorationSet;
}

const peerKey = new PluginKey<Peers>("peers");
const PAGE_INSET = 16;
/** How long the pages of a rendering may take to stop moving, in pauses. */
const MOST_PAUSES = 30;
const PAUSE_MS = 100;

function peerMarks(doc: ProseNode, shown: readonly Shown[]): DecorationSet {
  const decorations: Decoration[] = [];
  for (const { peer, from, to } of shown) {
    const color = colorOf(peer.person);
    if (to > from) {
      decorations.push(Decoration.inline(from, to, { style: `background:${color}33` }));
    }
    const caret = document.createElement("span");
    caret.className = "peer-caret";
    caret.style.borderColor = color;
    const name = caret.appendChild(document.createElement("span"));
    name.textContent = peer.name;
    name.style.background = color;
    decorations.push(Decoration.widget(to, caret, { side: 1, key: `${peer.id} ${peer.name}` }));
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

/**
 * Where the other people are. Each place moves with every change of this copy, and a peer
 * whose place is new comes in from the document's order.
 */
function peersPlugin(): Plugin<Peers> {
  return new Plugin<Peers>({
    key: peerKey,
    state: {
      init: () => ({ shown: [], marks: DecorationSet.empty }),
      apply(transaction, current, _old, state) {
        const peers = transaction.getMeta(peerKey) as readonly Peer[] | undefined;
        if (!peers && !transaction.docChanged) return current;
        const moved = transaction.docChanged
          ? current.shown.map((place) => ({
              ...place,
              from: transaction.mapping.map(place.from, 1),
              to: transaction.mapping.map(place.to, 1),
            }))
          : current.shown;
        const shown = peers
          ? peers.flatMap((peer) => {
              const known = moved.find((place) => place.peer === peer);
              if (known) return [known];
              const place = placeOf(peer.place);
              return place ? [{ peer, ...shownPlace(state, place) }] : [];
            })
          : moved;
        return { shown, marks: peerMarks(state.doc, shown) };
      },
    },
    props: {
      decorations(state) {
        return peerKey.getState(state)?.marks;
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

class DocxEditor implements ViewEditor, Pictures {
  private editor: Editor | undefined;
  private readonly bar: DocxBar;
  private readonly status: Status;
  private readonly page: HTMLElement;
  private readonly look = document.createElement("style");
  private readonly clientKey = crypto.randomUUID();
  private fitting: ResizeObserver | undefined;
  private choosing: MutationObserver | undefined;
  private view: DocxView = { styles: null, numbering: null, page: null };
  private pageAreas: Area[] = [];
  private placeWaits = false;

  constructor(
    private readonly root: HTMLElement,
    private readonly client: Client,
  ) {
    this.status = new Status(root);
    this.bar = new DocxBar(root, this.status.element);
    root.append(this.bar.element);
    this.page = root.appendChild(document.createElement("div"));
    this.page.className = "document-page docx-page";
    root.appendChild(this.look);
  }

  /** A key that changes the file, which does nothing while the file is read only. */
  private keyed(run: (editor: Editor) => void): () => boolean {
    return () => {
      const editor = this.editor;
      if (!editor?.isEditable) return false;
      run(editor);
      return true;
    };
  }

  private keys(): Record<string, () => boolean> {
    const lists = commands.listsOf(this.view);
    const list = (id: string | null) =>
      this.keyed((editor) => {
        if (id) commands.toggleList(editor, id);
      });
    return {
      "Mod-b": this.keyed((editor) => commands.toggleMark(editor, "bold")),
      "Mod-i": this.keyed((editor) => commands.toggleMark(editor, "italic")),
      "Mod-u": this.keyed((editor) => commands.toggleMark(editor, "underline")),
      "Shift-Mod-x": this.keyed((editor) => commands.toggleMark(editor, "strike")),
      "Shift-Mod-l": this.keyed((editor) => commands.setAlign(editor, "left")),
      "Shift-Mod-e": this.keyed((editor) => commands.setAlign(editor, "center")),
      "Shift-Mod-r": this.keyed((editor) => commands.setAlign(editor, "right")),
      "Shift-Mod-j": this.keyed((editor) => commands.setAlign(editor, "both")),
      "Shift-Mod-8": list(lists.bullets),
      "Shift-Mod-7": list(lists.numbers),
      Tab: this.keyed((editor) => {
        if (commands.canIndent(editor.state, 1)) commands.indent(editor, 1);
        else if (!commands.paragraphAt(editor.state)?.attrs.numId) {
          editor.chain().focus().insertContent({ type: "tab" }).run();
        }
      }),
      "Shift-Tab": this.keyed((editor) => commands.indent(editor, -1)),
    };
  }

  load(raw: unknown, person: Person): void {
    const snapshot = raw as Snapshot;
    this.editor?.destroy();
    this.view = snapshot.view;
    this.look.textContent = styleSheet(snapshot.view);
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
            keymap({ "Mod-z": undo, "Mod-y": redo, "Shift-Mod-z": redo, ...this.keys() }),
            blockIds(),
            peersPlugin(),
            imagesPlugin(client),
            followsLinks(),
            pages(
              shape,
              {
                keepNext: (block) => lookOf(block)?.keepNext === true,
                startsPage: (block) =>
                  block.attrs.pageBreak === true || lookOf(block)?.pageBreak === true,
              },
              () => !this.fitted(),
            ),
          ],
        }),
      ],
      content: snapshot.doc,
      editable: person.canEdit,
      enableInputRules: false,
      enablePasteRules: false,
      onTransaction: ({ transaction }) => {
        if (transaction.docChanged) client.pull();
        if (transaction.selectionSet || transaction.docChanged) this.sendPlace();
        this.bar.update();
      },
    });
    this.bar.show(this.editor, snapshot.view, person.canEdit);
    this.fitting?.disconnect();
    this.fitting = new ResizeObserver(() => this.fit(shape.width));
    this.fitting.observe(this.page);
    this.choosing?.disconnect();
    this.choosing = new MutationObserver(() => this.fit(shape.width));
    this.choosing.observe(this.root, { attributeFilter: ["data-fit"] });
  }

  /** The person chose text that fits the view over pages. */
  private fitted(): boolean {
    return this.root.hasAttribute("data-fit");
  }

  /** A page keeps its lines, so a narrow view shows it smaller. Text that fits needs no scale. */
  private fit(pageWidth: number): void {
    const root = this.editor?.view.dom;
    const room = this.page.clientWidth - 2 * PAGE_INSET;
    if (root) root.style.zoom = this.fitted() ? "1" : String(Math.min(1, room / pageWidth));
  }

  /** The pages, once their breaks stop moving. */
  async pictures(): Promise<number> {
    let last = "";
    for (let pause = 0; pause < MOST_PAUSES; pause++) {
      await settled(PAUSE_MS);
      this.pageAreas = this.pagesNow();
      const now = JSON.stringify(this.pageAreas);
      if (now === last) break;
      last = now;
    }
    return this.pageAreas.length;
  }

  async picture(index: number): Promise<Area> {
    const area = this.pageAreas[index];
    if (!area) throw new Error(`There is no page ${index + 1}.`);
    return area;
  }

  /** Each page of the one flow of text, between the spaces where pages end. */
  private pagesNow(): Area[] {
    const root = this.editor?.view.dom;
    if (!root) return [];
    const zoom = Number(root.style.zoom) || 1;
    const sheet = areaOf(root);
    const tops = [sheet.y];
    const bottoms: number[] = [];
    for (const space of root.querySelectorAll<HTMLElement>(".page-space")) {
      const end = areaOf(space).y + Number(space.dataset.end ?? 0) * zoom;
      bottoms.push(end);
      tops.push(end + PAGE_GAP * zoom);
    }
    bottoms.push(sheet.y + sheet.height);
    return tops.map((top, index) => ({
      x: sheet.x,
      y: top,
      width: sheet.width,
      height: (bottoms[index] ?? top) - top,
    }));
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
    if (own && this.placeWaits) this.sendPlace();
  }

  /**
   * The place goes as the document is at the client's rev. While this copy's steps wait for
   * their answer, it differs from that rev, so the place waits too.
   */
  private sendPlace(): void {
    const state = this.editor?.state;
    if (!state) return;
    this.placeWaits = sendableSteps(state) !== null;
    if (!this.placeWaits) {
      this.client.place({ from: state.selection.from, to: state.selection.to });
    }
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
    if (this.editor) this.bar.show(this.editor, this.view, false);
    this.status.closed(reason);
  }

  selection(): string {
    const state = this.editor?.state;
    if (!state) return "";
    return state.doc.textBetween(state.selection.from, state.selection.to, "\n");
  }

  destroy(): void {
    this.fitting?.disconnect();
    this.choosing?.disconnect();
    this.bar.destroy();
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

export function createEditor(element: HTMLElement, client: Client): DocxEditor {
  return new DocxEditor(element, client);
}
