import { type Fitting, Zoom } from "@documents/bar";
import {
  type Area,
  areaOf,
  type Client,
  followLink,
  opensLink,
  type Person,
  type Pictures,
  type Sent,
  Status,
  settled,
  type Editor as ViewEditor,
} from "@documents/client";
import type { Changed, ClosedReason, Peer } from "@documents/contract";
import { peersPlugin, showPeers, Waiting } from "@documents/peers";
import { Editor, Extension } from "@tiptap/core";
import { history, redo, undo } from "@tiptap/pm/history";
import { keymap } from "@tiptap/pm/keymap";
import type { Node as ProseNode } from "@tiptap/pm/model";
import { Plugin } from "@tiptap/pm/state";
import { Step } from "@tiptap/pm/transform";
import { collab, receiveTransaction, sendableSteps } from "prosemirror-collab";
import { DocxBar } from "./bar";
import * as commands from "./commands";
import { PAGE_GAP, pages } from "./pages";
import { extensions } from "./schema";
import { type DocxView, pageSize, styleSheet } from "./styles";
import "./docx.css";

interface Snapshot {
  doc: Record<string, unknown>;
  view: DocxView;
}

const PAGE_INSET = 16;
/** How long the pages of a rendering may take to stop moving, in pauses. */
const MOST_PAUSES = 30;
const PAUSE_MS = 100;

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
  private readonly page = document.createElement("div");
  private readonly sheet = document.createElement("div");
  private readonly zoom: Zoom;
  private readonly look = document.createElement("style");
  private readonly clientKey = crypto.randomUUID();
  private readonly waiting = new Waiting();
  private readonly choosing: MutationObserver;
  private sizing: ResizeObserver | undefined;
  private scale = 1;
  private view: DocxView = { styles: null, numbering: null, page: null };
  private shape = pageSize(this.view);
  private pageAreas: Area[] = [];
  private placeWaits = false;

  constructor(
    private readonly root: HTMLElement,
    private readonly client: Client,
  ) {
    this.status = new Status(root);
    this.page.className = "document-page docx-page";
    this.sheet.className = "docx-sheet";
    this.page.append(this.sheet);
    this.zoom = new Zoom({
      area: this.page,
      inset: PAGE_INSET,
      fits: () => this.fits(),
      scale: (scale) => this.scaleTo(scale),
    });
    this.bar = new DocxBar(root, [this.zoom.element, this.status.element]);
    root.append(this.bar.element, this.page, this.look);
    this.choosing = new MutationObserver(() => this.zoom.refit());
    this.choosing.observe(root, { attributeFilter: ["data-fit"] });
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
        if (commands.inListAt(editor.state)) commands.shiftLevel(editor, 1);
        else editor.chain().focus().insertContent({ type: "tab" }).run();
      }),
      "Shift-Tab": this.keyed((editor) => commands.shiftLevel(editor, -1)),
    };
  }

  load(raw: unknown, person: Person, sent: Sent): void {
    const snapshot = raw as Snapshot;
    const before = this.editor?.state;
    this.editor?.destroy();
    this.view = snapshot.view;
    this.look.textContent = styleSheet(snapshot.view);
    const client = this.client;
    const clientID = this.clientKey;
    this.shape = pageSize(snapshot.view);
    const lookOf = (block: ProseNode) =>
      snapshot.view.styles?.styles[(block.attrs.style as string | null) ?? ""];
    this.editor = new Editor({
      element: this.sheet,
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
              this.shape,
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
    this.sizing?.disconnect();
    this.sizing = new ResizeObserver(() => this.sizeSheet());
    this.sizing.observe(this.editor.view.dom);
    this.zoom.refit();
    const carried = this.waiting.loaded(this.editor.state.doc, before, sent);
    if (carried.steps.length > 0) {
      const transaction = this.editor.state.tr.setMeta("addToHistory", false);
      for (const step of carried.steps) transaction.maybeStep(step);
      this.editor.view.dispatch(transaction);
    }
    if (carried.lost !== undefined) this.status.kept(carried.lost);
  }

  /**
   * The pages lay out once at their own size, so a zoom never moves a word: the scale only makes
   * their picture larger or smaller. Text that fits the view lays out at the width that the
   * scale gives it.
   */
  private scaleTo(scale: number): void {
    this.scale = scale;
    const root = this.editor?.view.dom;
    if (!root) return;
    const room = this.page.clientWidth - 2 * PAGE_INSET;
    root.style.width = this.fitted() ? `${room / scale}px` : "";
    root.style.transform = `scale(${scale})`;
    root.dataset.scale = String(scale);
    this.sizeSheet();
  }

  /** A transform leaves the layout as it is, so the sheet holds the place of the scaled pages. */
  private sizeSheet(): void {
    const root = this.editor?.view.dom;
    if (!root) return;
    this.sheet.style.width = `${root.offsetWidth * this.scale}px`;
    this.sheet.style.height = `${root.offsetHeight * this.scale}px`;
  }

  /** The person chose text that fits the view over pages. */
  private fitted(): boolean {
    return this.root.hasAttribute("data-fit");
  }

  /** A page keeps its lines, so a narrow view shows it smaller. Text that fits needs no scale. */
  private fits(): Record<Fitting, number> {
    if (this.fitted()) return { width: 1, page: 1 };
    const width = (this.page.clientWidth - 2 * PAGE_INSET) / this.shape.width;
    const height = (this.page.clientHeight - 2 * PAGE_INSET) / this.shape.height;
    return { width, page: Math.min(width, height) };
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
    const zoom = this.scale;
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
    this.waiting.sending(sendable.steps.length);
    return { kind: "steps", steps: sendable.steps.map((step) => step.toJSON()) };
  }

  apply(changed: Changed, own: boolean): void {
    const editor = this.editor;
    if (!editor) return;
    const body = changed.body as { steps?: unknown[] };
    const steps = (body.steps ?? []).map((json: unknown) => Step.fromJSON(editor.schema, json));
    this.waiting.changed(steps, own);
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
    if (view) showPeers(view, peers);
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
    this.zoom.destroy();
    this.choosing.disconnect();
    this.sizing?.disconnect();
    this.bar.destroy();
    this.editor?.destroy();
  }
}

function followsLinks(): Plugin {
  return new Plugin({
    props: {
      handleClick(view, position, event) {
        if (!opensLink(event, view.editable)) return false;
        const link = view.state.doc
          .resolve(position)
          .marks()
          .find((mark) => mark.type.name === "link");
        if (!link) return false;
        followLink(link.attrs.href as string);
        return true;
      },
    },
  });
}

export function createEditor(element: HTMLElement, client: Client): DocxEditor {
  return new DocxEditor(element, client);
}
