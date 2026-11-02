import { Bar, gap, type Key, keyOf, keys, readOnly, rule } from "@documents/bar";
import {
  type Area,
  areaOf,
  batchOf,
  type Client,
  colorOf,
  type Editor,
  Outbox,
  type Person,
  type Pictures,
  Status,
  settled,
} from "@documents/client";
import type { Changed, ClosedReason, Peer } from "@documents/contract";
import { history, redo, undo } from "@tiptap/pm/history";
import { keymap } from "@tiptap/pm/keymap";
import { Node as ProseNode } from "@tiptap/pm/model";
import { EditorState } from "@tiptap/pm/state";
import { Step } from "@tiptap/pm/transform";
import { EditorView } from "@tiptap/pm/view";
import Konva from "konva";
import { Plus, TextCursorInput, Trash2, Undo2 } from "lucide";
import { collab, receiveTransaction, sendableSteps } from "prosemirror-collab";
import { layOut, levelRules, type Measure, schema, type TextLook } from "./text";
import "./powerpoint.css";

interface Shape {
  id: string;
  name: string;
  kind: string;
  placeholder: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  geometry: string | null;
  fill: string | null;
  line: string | null;
  asset: string | null;
  text: unknown;
  look: TextLook | null;
  order: number;
}

interface Slide {
  id: string;
  position: number;
  shapes: Shape[];
  behind: Shape[];
  background: string | null;
}

interface Snapshot {
  width: number;
  height: number;
  slides: Slide[];
  theme: Record<string, string> | null;
}

type Fields = Partial<
  Pick<Shape, "x" | "y" | "width" | "height" | "rotation" | "fill" | "line" | "order">
>;

type Change =
  | { kind: "shape"; slide: string; shape: string; fields: Fields }
  | { kind: "text"; slide: string; shape: string; steps: unknown[] }
  | { kind: "moveSlide"; slide: string; position: number }
  | { kind: "addSlide"; slide: string; position: number }
  | { kind: "removeSlide"; slide: string }
  | { kind: "addShape"; slide: string; shape: Shape }
  | { kind: "removeShape"; slide: string; shape: string }
  | { kind: "group"; changes: Change[] };

interface Place {
  slide?: string;
  shape?: string;
}

const EMU_PER_PIXEL = 9525;
const ROTATION_UNITS = 60000;
const TEXT_INSET = 9;
const SCHEME_NAMES: Record<string, string> = { bg1: "lt1", tx1: "dk1", bg2: "lt2", tx2: "dk2" };
const TEXT_BOX_WIDTH = 3_000_000;
const TEXT_BOX_HEIGHT = 900_000;
const TEXT_BOX_AT = 1_000_000;

const STAGE_INSET = 16;

const measurer = document.createElement("canvas").getContext("2d");

function textWidth(font: string, text: string): number {
  if (!measurer) return 0;
  measurer.font = font;
  return measurer.measureText(text).width;
}

function pixels(emu: number): number {
  return emu / EMU_PER_PIXEL;
}

/** Shape ids are unique only within a slide. */
function textKey(slide: string, shape: string): string {
  return `${slide}/${shape}`;
}

class PowerPointEditor implements Editor, Pictures {
  private readonly list: HTMLElement;
  private readonly stageHolder: HTMLElement;
  private readonly stageRoot: HTMLDivElement;
  private readonly bar: Bar;
  private readonly status: Status;
  private undoKey: Key | undefined;
  private readonly outbox = new Outbox<Change>();
  private readonly clientKey = crypto.randomUUID();
  private readonly texts = new Map<string, EditorState>();
  private readonly images = new Map<string, HTMLImageElement>();
  private readonly undoes: Change[] = [];
  private readonly textLook = document.createElement("style");
  private stage: Konva.Stage | undefined;
  private layer: Konva.Layer | undefined;
  private transformer: Konva.Transformer | undefined;
  private resized: ResizeObserver | undefined;
  private deck: Snapshot | undefined;
  private current = "";
  private selected: string | null = null;
  private people: readonly Peer[] = [];
  private textView: EditorView | undefined;
  private textShape: string | null = null;
  private canEdit = true;
  private holding = false;

  constructor(
    root: HTMLElement,
    private readonly client: Client,
  ) {
    this.status = new Status(root);
    this.bar = new Bar(root, "Slides");
    root.append(this.bar.element);
    const body = root.appendChild(document.createElement("div"));
    body.className = "slides-body";
    this.list = body.appendChild(document.createElement("div"));
    this.list.className = "slides-list";
    this.stageHolder = body.appendChild(document.createElement("div"));
    this.stageHolder.className = "slides-stage";
    this.stageHolder.tabIndex = 0;
    this.stageRoot = this.stageHolder.appendChild(document.createElement("div"));
    root.appendChild(this.textLook);
    this.stageHolder.addEventListener("keydown", (event) => this.key(event));
  }

  private showBar(): void {
    if (!this.canEdit) {
      this.undoKey = undefined;
      this.bar.show(readOnly(), gap(), this.status.element);
      return;
    }
    this.undoKey = keyOf({
      label: "Undo",
      icon: Undo2,
      shortcut: keys("Z"),
      press: () => this.undoLast(),
    });
    const key = (label: string, icon: typeof Plus, press: () => void) =>
      keyOf({ label, icon, press }).element;
    this.bar.show(
      this.undoKey.element,
      rule(),
      key("New slide", Plus, () => this.addSlide()),
      key("Text box", TextCursorInput, () => this.addTextBox()),
      key("Delete", Trash2, () => this.removeSelected()),
      gap(),
      this.status.element,
    );
    this.undoKey.show({ enabled: this.undoes.length > 0 });
  }

  private key(event: KeyboardEvent): void {
    if (this.textView) return;
    if ((event.metaKey || event.ctrlKey) && event.key === "z") {
      event.preventDefault();
      this.undoLast();
    } else if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      this.removeSelected();
    }
  }

  load(raw: unknown, person: Person): void {
    this.closeText();
    const deck = raw as Snapshot;
    this.deck = deck;
    this.canEdit = person.canEdit;
    this.showBar();
    this.texts.clear();
    for (const slide of deck.slides) {
      for (const shape of slide.shapes) this.addText(slide.id, shape);
    }
    if (!deck.slides.some((slide) => slide.id === this.current)) {
      this.current = deck.slides[0]?.id ?? "";
    }
    this.drawList();
    this.drawSlide();
    this.resized?.disconnect();
    this.resized = new ResizeObserver(() => this.redraw());
    this.resized.observe(this.stageHolder);
  }

  private addText(slide: string, shape: Shape): void {
    if (!shape.text) return;
    this.texts.set(
      textKey(slide, shape.id),
      EditorState.create({
        doc: ProseNode.fromJSON(schema, shape.text),
        plugins: [
          collab({ clientID: this.clientKey }),
          history(),
          keymap({ "Mod-z": undo, "Mod-y": redo, "Shift-Mod-z": redo }),
        ],
      }),
    );
  }

  private slide(): Slide | undefined {
    return this.deck?.slides.find((slide) => slide.id === this.current);
  }

  private color(value: string | null | undefined): string | undefined {
    if (!value) return undefined;
    if (!value.startsWith("scheme:")) return `#${value}`;
    const name = value.slice("scheme:".length);
    const color = this.deck?.theme?.[SCHEME_NAMES[name] ?? name];
    return color ? `#${color}` : undefined;
  }

  private drawList(): void {
    const slides = this.deck?.slides ?? [];
    const taken = new Set(this.people.flatMap((peer) => (peer.place as Place | null)?.slide ?? []));
    this.list.replaceChildren(
      ...slides.map((slide, index) => {
        const item = document.createElement("button");
        item.type = "button";
        item.className = "slides-item";
        item.textContent = String(index + 1);
        item.ariaPressed = String(slide.id === this.current);
        item.dataset.peer = String(taken.has(slide.id));
        item.draggable = this.canEdit;
        item.addEventListener("click", () => this.open(slide.id));
        item.addEventListener("dragstart", (event) => {
          event.dataTransfer?.setData("text/plain", slide.id);
        });
        item.addEventListener("dragover", (event) => event.preventDefault());
        item.addEventListener("drop", (event) => {
          const moved = event.dataTransfer?.getData("text/plain");
          if (moved) this.moveSlide(moved, index);
        });
        return item;
      }),
    );
  }

  private open(slide: string): void {
    this.closeText();
    this.current = slide;
    this.selected = null;
    this.client.place({ slide });
    this.drawList();
    this.drawSlide();
  }

  private scale(): number {
    const deck = this.deck;
    if (!deck) return 1;
    const width = (this.stageHolder.clientWidth - 2 * STAGE_INSET) / pixels(deck.width);
    const height = (this.stageHolder.clientHeight - 2 * STAGE_INSET) / pixels(deck.height);
    return Math.min(width, height) || 1;
  }

  /** A drag keeps its stage, so what comes in while it runs draws when it ends. */
  private redraw(): void {
    if (this.holding) return;
    this.drawSlide();
    if (this.textView) this.placeText();
  }

  private drawSlide(): void {
    this.undoKey?.show({ enabled: this.undoes.length > 0 });
    const deck = this.deck;
    const slide = this.slide();
    this.stage?.destroy();
    if (!deck || !slide) return;
    const scale = this.scale();
    const stage = new Konva.Stage({
      container: this.stageRoot,
      width: pixels(deck.width) * scale,
      height: pixels(deck.height) * scale,
      scaleX: scale,
      scaleY: scale,
    });
    const layer = new Konva.Layer();
    stage.add(layer);
    this.stage = stage;
    this.layer = layer;
    layer.add(
      new Konva.Rect({
        width: pixels(deck.width),
        height: pixels(deck.height),
        fill: this.color(slide.background) ?? "#ffffff",
        listening: false,
      }),
    );
    for (const shape of slide.behind) this.drawShape(shape, false);
    for (const shape of [...slide.shapes].sort((left, right) => left.order - right.order)) {
      this.drawShape(shape, this.canEdit);
    }
    this.transformer = new Konva.Transformer({ keepRatio: false, ignoreStroke: true });
    layer.add(this.transformer);
    const chosen = this.selected ? this.nodeOf(this.selected) : undefined;
    if (chosen && !this.textView) this.transformer.nodes([chosen]);
    stage.on("click tap", (event) => {
      if (event.target === stage || !event.target.findAncestor(".editable", true)) {
        this.select(null);
      }
    });
    this.drawPeers();
  }

  private nodeOf(shape: string): Konva.Node | undefined {
    return this.layer?.findOne((node: Konva.Node) => node.id() === shape);
  }

  private drawPeers(): void {
    const layer = this.layer;
    const slide = this.slide();
    if (!layer || !slide) return;
    for (const old of layer.find(".peer")) old.destroy();
    for (const peer of this.people) {
      const place = peer.place as Place | null;
      const shape = slide.shapes.find((known) => known.id === place?.shape);
      if (place?.slide !== slide.id || !shape) continue;
      const color = colorOf(peer.id);
      const mark = new Konva.Group({
        name: "peer",
        x: pixels(shape.x),
        y: pixels(shape.y),
        rotation: shape.rotation / ROTATION_UNITS,
        listening: false,
      });
      mark.add(
        new Konva.Rect({
          width: pixels(shape.width),
          height: pixels(shape.height),
          stroke: color,
          strokeWidth: 2,
        }),
      );
      mark.add(new Konva.Text({ y: -16, text: peer.name, fontSize: 12, fill: color }));
      layer.add(mark);
    }
    layer.batchDraw();
  }

  private drawShape(shape: Shape, editable: boolean): void {
    const layer = this.layer;
    if (!layer) return;
    const group = new Konva.Group({
      id: shape.id,
      name: editable ? "editable" : "behind",
      x: pixels(shape.x),
      y: pixels(shape.y),
      rotation: shape.rotation / ROTATION_UNITS,
      draggable: editable,
    });
    const width = pixels(shape.width);
    const height = pixels(shape.height);
    const fill = this.color(shape.fill);
    const stroke = this.color(shape.line);
    const body =
      shape.geometry === "ellipse"
        ? new Konva.Ellipse({
            x: width / 2,
            y: height / 2,
            radiusX: width / 2,
            radiusY: height / 2,
          })
        : new Konva.Rect({
            width,
            height,
            cornerRadius: shape.geometry === "roundRect" ? Math.min(width, height) / 6 : 0,
          });
    body.fill(fill ?? "transparent");
    if (stroke) body.stroke(stroke);
    group.add(body);
    if (shape.asset) this.drawPicture(group, shape.asset, width, height);
    if (shape.text && shape.id !== this.textShape) group.add(this.textNode(shape, width, height));
    if (editable) this.bindShape(group, shape);
    layer.add(group);
  }

  private fontMeasure(): Measure {
    return {
      width: textWidth,
      face: (font) => `"${this.deck?.theme?.[font] ?? font}", Carlito, sans-serif`,
    };
  }

  private textNode(shape: Shape, width: number, height: number): Konva.Shape {
    const state = this.texts.get(textKey(this.current, shape.id));
    const tree = state?.doc ?? ProseNode.fromJSON(schema, shape.text);
    const lines = layOut(
      tree,
      width - 2 * TEXT_INSET,
      height - 2 * TEXT_INSET,
      shape.look,
      this.fontMeasure(),
    );
    const fallback = this.color("scheme:tx1") ?? "#1a1a1a";
    return new Konva.Shape({
      x: TEXT_INSET,
      y: TEXT_INSET,
      listening: false,
      sceneFunc: (context) => {
        for (const line of lines) {
          for (const piece of line.pieces) {
            context.setAttr("font", piece.font);
            context.setAttr("fillStyle", this.color(piece.color) ?? fallback);
            context.fillText(piece.text, piece.x, line.baseline);
            if (piece.underline) context.fillRect(piece.x, line.baseline + 2, piece.width, 1);
          }
        }
      },
    });
  }

  private drawPicture(group: Konva.Group, asset: string, width: number, height: number): void {
    const known = this.images.get(asset);
    if (known) {
      group.add(new Konva.Image({ image: known, width, height, listening: false }));
      return;
    }
    this.client.asset(asset).then((bytes) => {
      const image = new Image();
      image.onload = () => {
        this.images.set(asset, image);
        this.redraw();
      };
      image.src = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>]));
    }, console.error);
  }

  private bindShape(group: Konva.Group, shape: Shape): void {
    group.on("click tap", () => this.select(shape.id));
    group.on("dragstart transformstart", () => {
      this.select(shape.id);
      this.holding = true;
    });
    group.on("dragend transformend", () => this.moved(group, shape));
    group.on("dblclick dbltap", () => this.editText(shape));
  }

  private select(shape: string | null): void {
    this.selected = shape;
    this.stageHolder.focus({ preventScroll: true });
    const node = shape ? this.nodeOf(shape) : undefined;
    this.transformer?.nodes(node ? [node] : []);
    this.client.place(shape ? { slide: this.current, shape } : { slide: this.current });
  }

  private moved(group: Konva.Group, shape: Shape): void {
    this.holding = false;
    const fields: Fields = {
      x: Math.round(group.x() * EMU_PER_PIXEL),
      y: Math.round(group.y() * EMU_PER_PIXEL),
      width: Math.round(shape.width * group.scaleX()),
      height: Math.round(shape.height * group.scaleY()),
      rotation: Math.round(group.rotation() * ROTATION_UNITS),
    };
    const before: Fields = {
      x: shape.x,
      y: shape.y,
      width: shape.width,
      height: shape.height,
      rotation: shape.rotation,
    };
    this.undoes.push({ kind: "shape", slide: this.current, shape: shape.id, fields: before });
    Object.assign(shape, fields);
    this.send({ kind: "shape", slide: this.current, shape: shape.id, fields });
    this.drawSlide();
  }

  private editText(shape: Shape): void {
    if (!shape.text || this.textView) return;
    const key = textKey(this.current, shape.id);
    const state = this.texts.get(key);
    if (!state) return;
    this.textShape = shape.id;
    this.select(shape.id);
    this.transformer?.nodes([]);
    const holder = this.stageHolder.appendChild(document.createElement("div"));
    holder.className = "slides-text";
    this.textView = new EditorView(holder, {
      state,
      editable: () => this.canEdit,
      dispatchTransaction: (transaction) => {
        const view = this.textView;
        if (!view) return;
        const next = view.state.apply(transaction);
        view.updateState(next);
        this.texts.set(key, next);
        if (transaction.docChanged) this.client.pull();
      },
      handleDOMEvents: {
        blur: () => {
          this.closeText();
          this.drawSlide();
          return false;
        },
      },
    });
    this.drawSlide();
    this.placeText();
    this.textView.focus();
  }

  /** Lays the text editor over its shape, at the slide's scale. */
  private placeText(): void {
    const view = this.textView;
    const shape = this.slide()?.shapes.find((known) => known.id === this.textShape);
    const holder = view?.dom.parentElement;
    const canvas = this.stage?.container().querySelector("canvas");
    if (!shape || !holder || !canvas) return;
    const box = canvas.getBoundingClientRect();
    const frame = this.stageHolder.getBoundingClientRect();
    const scale = this.scale();
    holder.style.left = `${box.left - frame.left + pixels(shape.x) * scale}px`;
    holder.style.top = `${box.top - frame.top + pixels(shape.y) * scale}px`;
    holder.style.width = `${pixels(shape.width)}px`;
    holder.style.minHeight = `${pixels(shape.height)}px`;
    holder.style.padding = `${TEXT_INSET}px`;
    holder.style.transform = `scale(${scale}) rotate(${shape.rotation / ROTATION_UNITS}deg)`;
    holder.style.color = this.color("scheme:tx1") ?? "#1a1a1a";
    this.textLook.textContent = levelRules(shape.look, this.fontMeasure(), (color) =>
      this.color(color),
    );
  }

  private closeText(): void {
    const view = this.textView;
    this.textView = undefined;
    this.textShape = null;
    view?.dom.parentElement?.remove();
    view?.destroy();
  }

  private addTextBox(): void {
    const slide = this.slide();
    if (!slide) return;
    const shape: Shape = {
      id: crypto.randomUUID(),
      name: "Text box",
      kind: "shape",
      placeholder: null,
      x: TEXT_BOX_AT,
      y: TEXT_BOX_AT,
      width: TEXT_BOX_WIDTH,
      height: TEXT_BOX_HEIGHT,
      rotation: 0,
      geometry: "rect",
      fill: null,
      line: null,
      asset: null,
      text: { type: "doc", content: [{ type: "paragraph" }] },
      look: null,
      order: slide.shapes.length,
    };
    slide.shapes.push(shape);
    this.addText(slide.id, shape);
    this.undoes.push({ kind: "removeShape", slide: slide.id, shape: shape.id });
    this.send({ kind: "addShape", slide: slide.id, shape });
    this.selected = shape.id;
    this.drawSlide();
    this.editText(shape);
  }

  private addSlide(): void {
    const deck = this.deck;
    if (!deck) return;
    const index = deck.slides.findIndex((slide) => slide.id === this.current);
    const change: Change = {
      kind: "addSlide",
      slide: crypto.randomUUID(),
      position: this.positionAt(index + 1),
    };
    this.showChange(change);
    this.send(change);
    this.open(change.slide);
  }

  private positionAt(index: number): number {
    const slides = this.deck?.slides ?? [];
    const before = slides[index - 1]?.position;
    const after = slides[index]?.position;
    if (before === undefined) return (after ?? 1) - 1;
    return after === undefined ? before + 1 : (before + after) / 2;
  }

  private moveSlide(id: string, index: number): void {
    const change: Change = { kind: "moveSlide", slide: id, position: this.positionAt(index) };
    this.showChange(change);
    this.send(change);
    this.drawList();
  }

  private removeSelected(): void {
    const slide = this.slide();
    if (!slide) return;
    if (this.selected) {
      const shape = slide.shapes.find((known) => known.id === this.selected);
      if (shape) this.undoes.push({ kind: "addShape", slide: slide.id, shape });
      const change: Change = { kind: "removeShape", slide: slide.id, shape: this.selected };
      this.selected = null;
      this.showChange(change);
      this.send(change);
      this.drawSlide();
    } else if ((this.deck?.slides.length ?? 0) > 1) {
      const change: Change = { kind: "removeSlide", slide: slide.id };
      this.showChange(change);
      this.send(change);
      this.open(this.deck?.slides[0]?.id ?? "");
    }
  }

  private undoLast(): void {
    const change = this.undoes.pop();
    if (!change) return;
    this.showChange(change);
    this.send(change);
    this.drawSlide();
  }

  private send(change: Change): void {
    this.outbox.add(change);
    this.client.pull();
  }

  private textChanges(): Change[] {
    const changes: Change[] = [];
    for (const slide of this.deck?.slides ?? []) {
      for (const shape of slide.shapes) {
        const state = this.texts.get(textKey(slide.id, shape.id));
        const sendable = state && sendableSteps(state);
        if (sendable) {
          changes.push({
            kind: "text",
            slide: slide.id,
            shape: shape.id,
            steps: sendable.steps.map((step) => step.toJSON()),
          });
        }
      }
    }
    return changes;
  }

  batch(): unknown | null {
    return batchOf([...this.outbox.next(), ...this.textChanges()]);
  }

  private covered(shape: string): Set<string> {
    const names = new Set<string>();
    const visit = (change: Change) => {
      if (change.kind === "group") change.changes.forEach(visit);
      else if (change.kind === "shape" && change.shape === shape) {
        for (const name of Object.keys(change.fields)) names.add(name);
      }
    };
    this.outbox.unanswered().forEach(visit);
    return names;
  }

  private receiveText(slide: string, shape: string, steps: unknown[], author: string): void {
    const key = textKey(slide, shape);
    const state = this.texts.get(key);
    if (!state) return;
    const parsed = steps.map((json) => Step.fromJSON(schema, json));
    const transaction = receiveTransaction(
      state,
      parsed,
      parsed.map(() => author),
      { mapSelectionBackward: true },
    );
    const next = state.apply(transaction);
    this.texts.set(key, next);
    if (this.current === slide && this.textShape === shape) this.textView?.updateState(next);
  }

  /** Shows a change on this copy. A view's own change is here already, but for its text. */
  private showChange(change: Change, own = false, author: string = this.clientKey): void {
    const deck = this.deck;
    if (!deck) return;
    if (change.kind === "group") {
      for (const inner of change.changes) this.showChange(inner, own, author);
      return;
    }
    if (change.kind === "text") {
      this.receiveText(change.slide, change.shape, change.steps, author);
      return;
    }
    if (own) return;
    const slide = deck.slides.find((known) => known.id === change.slide);
    switch (change.kind) {
      case "shape": {
        const shape = slide?.shapes.find((known) => known.id === change.shape);
        if (!shape) break;
        const covered = this.covered(shape.id);
        Object.assign(
          shape,
          Object.fromEntries(Object.entries(change.fields).filter(([name]) => !covered.has(name))),
        );
        break;
      }
      case "moveSlide":
        if (slide) slide.position = change.position;
        deck.slides.sort((left, right) => left.position - right.position);
        break;
      case "addSlide":
        deck.slides.push({
          id: change.slide,
          position: change.position,
          shapes: [],
          behind: [],
          background: null,
        });
        deck.slides.sort((left, right) => left.position - right.position);
        break;
      case "removeSlide":
        deck.slides = deck.slides.filter((known) => known.id !== change.slide);
        if (change.slide === this.current) this.current = deck.slides[0]?.id ?? "";
        break;
      case "addShape":
        if (slide && !slide.shapes.some((known) => known.id === change.shape.id)) {
          slide.shapes.push(change.shape);
          this.addText(slide.id, change.shape);
        }
        break;
      case "removeShape":
        if (slide) slide.shapes = slide.shapes.filter((known) => known.id !== change.shape);
        this.texts.delete(textKey(change.slide, change.shape));
        if (this.current === change.slide && this.textShape === change.shape) this.closeText();
        break;
    }
  }

  apply(changed: Changed, own: boolean): void {
    if (own) this.outbox.answered();
    this.showChange(changed.body as Change, own, own ? this.clientKey : changed.client);
    if (own) return;
    this.drawList();
    this.redraw();
  }

  peers(peers: readonly Peer[]): void {
    this.people = peers;
    this.status.peers(peers);
    this.drawList();
    this.drawPeers();
  }

  saved(_rev: number, waits: boolean): void {
    this.status.saved(waits);
  }

  closed(reason: ClosedReason): void {
    this.canEdit = false;
    this.closeText();
    this.redraw();
    this.showBar();
    this.status.closed(reason);
  }

  async pictures(): Promise<number> {
    return this.deck?.slides.length ?? 0;
  }

  /** Each slide in turn, as the stage draws it. */
  async picture(index: number): Promise<Area> {
    const slide = this.deck?.slides[index];
    if (!slide) throw new Error(`There is no slide ${index + 1}.`);
    this.open(slide.id);
    await settled();
    return areaOf(this.stageRoot.querySelector(".konvajs-content") ?? this.stageRoot);
  }

  selection(): string {
    const key = this.textShape ? textKey(this.current, this.textShape) : "";
    const state = this.texts.get(key);
    if (!state) return "";
    return state.doc.textBetween(state.selection.from, state.selection.to, "\n");
  }

  destroy(): void {
    this.bar.destroy();
    this.resized?.disconnect();
    this.closeText();
    this.stage?.destroy();
  }
}

export function createEditor(element: HTMLElement, client: Client): PowerPointEditor {
  return new PowerPointEditor(element, client);
}
