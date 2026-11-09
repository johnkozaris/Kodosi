import { Bar, gap, type Key, keyOf, keys, readOnly, Zoom } from "@documents/bar";
import {
  type Area,
  areaOf,
  type Client,
  type Person,
  type Pictures,
  Status,
  settled,
  type Editor as ViewEditor,
} from "@documents/client";
import type { Changed, ClosedReason, Peer } from "@documents/contract";
import { peersPlugin, showPeers } from "@documents/peers";
import { say } from "@documents/words";
import { baseKeymap } from "@tiptap/pm/commands";
import { history, redo, redoDepth, undo, undoDepth } from "@tiptap/pm/history";
import { keymap } from "@tiptap/pm/keymap";
import { Node as ProseNode } from "@tiptap/pm/model";
import { type Command, EditorState, type Transaction } from "@tiptap/pm/state";
import { Step } from "@tiptap/pm/transform";
import { EditorView } from "@tiptap/pm/view";
import { Redo2, Undo2 } from "lucide";
import { collab, receiveTransaction, sendableSteps } from "prosemirror-collab";
import { schema } from "./schema";
import { indented, indentOf, newLine, outdented, pastedSlice } from "./text";
import "./plain.css";

interface Snapshot {
  doc: Record<string, unknown>;
}

class PlainEditor implements ViewEditor, Pictures {
  private view: EditorView | undefined;
  private readonly bar: Bar;
  private readonly status: Status;
  private readonly zoom: Zoom;
  private readonly undoKey: Key;
  private readonly redoKey: Key;
  private readonly page = document.createElement("div");
  private readonly clientKey = crypto.randomUUID();
  private canEdit = false;
  private placeWaits = false;
  private areas: Area[] = [];

  constructor(
    root: HTMLElement,
    private readonly client: Client,
  ) {
    this.status = new Status(root);
    this.bar = new Bar(root, say("Text"));
    this.page.className = "document-page plain-page";
    this.zoom = new Zoom({
      area: this.page,
      inset: 0,
      scale: (scale) => this.page.style.setProperty("--zoom", String(scale)),
    });
    this.undoKey = keyOf({
      label: say("Undo"),
      icon: Undo2,
      shortcut: keys("Z"),
      press: () => this.run(undo),
    });
    this.redoKey = keyOf({
      label: say("Redo"),
      icon: Redo2,
      shortcut: keys("Z", { shift: true }),
      press: () => this.run(redo),
    });
    root.append(this.bar.element, this.page);
  }

  load(raw: unknown, person: Person): void {
    const snapshot = raw as Snapshot;
    this.view?.destroy();
    this.canEdit = person.canEdit;
    const doc = ProseNode.fromJSON(schema, snapshot.doc);
    const indent = indentOf(doc);
    const state = EditorState.create({
      doc,
      plugins: [
        collab({ clientID: this.clientKey }),
        history(),
        keymap({
          "Mod-z": undo,
          "Shift-Mod-z": redo,
          "Mod-y": redo,
          Tab: indented(indent),
          "Shift-Tab": outdented(indent),
          Enter: newLine,
        }),
        keymap(baseKeymap),
        peersPlugin(),
      ],
    });
    this.view = new EditorView(this.page, {
      state,
      editable: () => this.canEdit,
      attributes: {
        class: "plain-text",
        spellcheck: "false",
        autocapitalize: "off",
        autocorrect: "off",
        translate: "no",
      },
      clipboardTextSerializer: (slice) => slice.content.textBetween(0, slice.content.size, "\n"),
      clipboardTextParser: (text) => pastedSlice(text),
      handlePaste: (view, event) => {
        const text = event.clipboardData?.getData("text/plain");
        if (!text) return false;
        view.dispatch(view.state.tr.replaceSelection(pastedSlice(text)).scrollIntoView());
        return true;
      },
      dispatchTransaction: (transaction) => this.dispatch(transaction),
    });
    this.numberLines();
    this.showBar();
  }

  private dispatch(transaction: Transaction): void {
    const view = this.view;
    if (!view) return;
    view.updateState(view.state.apply(transaction));
    if (transaction.docChanged) {
      this.client.pull();
      this.numberLines();
    }
    if (transaction.selectionSet || transaction.docChanged) this.sendPlace();
    this.showKeys();
  }

  /** The numbers of the lines take the room of the largest. */
  private numberLines(): void {
    const digits = String(String(this.view?.state.doc.childCount ?? 1).length);
    if (this.page.style.getPropertyValue("--digits") !== digits) {
      this.page.style.setProperty("--digits", digits);
    }
  }

  private run(command: Command): void {
    const view = this.view;
    if (view && this.canEdit) command(view.state, view.dispatch);
  }

  private showBar(): void {
    if (!this.canEdit) {
      this.bar.show(readOnly(), gap(), this.zoom.element, this.status.element);
      return;
    }
    this.bar.show(
      this.undoKey.element,
      this.redoKey.element,
      gap(),
      this.zoom.element,
      this.status.element,
    );
    this.showKeys();
  }

  private showKeys(): void {
    const state = this.view?.state;
    if (!state) return;
    this.undoKey.show({ enabled: undoDepth(state) > 0 });
    this.redoKey.show({ enabled: redoDepth(state) > 0 });
  }

  /** The text in pictures as tall as the window, each of whole lines. */
  async pictures(): Promise<number> {
    await settled();
    const root = this.view?.dom;
    if (!root) return 0;
    const page = areaOf(this.page);
    const end = page.y + page.height;
    const area = (top: number, bottom: number): Area => ({
      x: page.x,
      y: top,
      width: page.width,
      height: bottom - top,
    });
    this.areas = [];
    let top = page.y;
    let bottom = top;
    for (const line of root.children) {
      const { y, height } = areaOf(line);
      if (y + height - top > window.innerHeight && bottom > top) {
        this.areas.push(area(top, bottom));
        top = bottom;
      }
      bottom = y + height;
    }
    this.areas.push(area(top, end));
    return this.areas.length;
  }

  async picture(index: number): Promise<Area> {
    const area = this.areas[index];
    if (!area) throw new Error(`There is no page ${index + 1}.`);
    return area;
  }

  batch(): unknown | null {
    const state = this.view?.state;
    const sendable = state && sendableSteps(state);
    if (!sendable) return null;
    return { kind: "steps", steps: sendable.steps.map((step) => step.toJSON()) };
  }

  apply(changed: Changed, own: boolean): void {
    const view = this.view;
    if (!view) return;
    const body = changed.body as { steps?: unknown[] };
    const steps = (body.steps ?? []).map((json: unknown) => Step.fromJSON(schema, json));
    const ids = steps.map(() => (own ? this.clientKey : changed.client));
    view.dispatch(receiveTransaction(view.state, steps, ids, { mapSelectionBackward: true }));
    if (own && this.placeWaits) this.sendPlace();
  }

  /**
   * The place goes as the text is at the client's rev. While this copy's steps wait for their
   * answer, it differs from that rev, so the place waits too.
   */
  private sendPlace(): void {
    const state = this.view?.state;
    if (!state) return;
    this.placeWaits = sendableSteps(state) !== null;
    if (!this.placeWaits) {
      this.client.place({ from: state.selection.from, to: state.selection.to });
    }
  }

  peers(peers: readonly Peer[]): void {
    this.status.peers(peers);
    if (this.view) showPeers(this.view, peers);
  }

  saved(_rev: number, waits: boolean): void {
    this.status.saved(waits);
  }

  closed(reason: ClosedReason): void {
    this.canEdit = false;
    this.view?.setProps({});
    this.showBar();
    this.status.closed(reason);
  }

  selection(): string {
    const state = this.view?.state;
    if (!state) return "";
    return state.doc.textBetween(state.selection.from, state.selection.to, "\n");
  }

  destroy(): void {
    this.zoom.destroy();
    this.bar.destroy();
    this.view?.destroy();
  }
}

export function createEditor(element: HTMLElement, client: Client): PlainEditor {
  return new PlainEditor(element, client);
}
