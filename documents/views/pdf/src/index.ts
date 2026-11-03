import { Bar, gap, type Key, keyOf, readOnly, rule, type Toggles, wellOf } from "@documents/bar";
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
import { ArrowDown, ArrowUp, Eraser, RotateCw, StickyNote, Trash2 } from "lucide";
// The legacy build fills in what the hosts' engines lack, such as Map.getOrInsertComputed in
// QtWebEngine 6.14.
import * as pdfjs from "pdfjs-dist/legacy/build/pdf.mjs";
import workerUrl from "pdfjs-dist/legacy/build/pdf.worker.min.mjs?url";
import "./pdf.css";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

interface Page {
  id: string;
  width: number;
  height: number;
  rotation: number;
}

type Rect = [number, number, number, number];

interface Note {
  page: string;
  kind: string;
  rect: Rect;
  text: string;
  color?: string;
  author: string;
}

interface Snapshot {
  asset: string;
  pages: Page[];
  notes: Record<string, Note>;
  fields: Record<string, string>;
}

type PageWork =
  | { kind: "order"; pages: string[] }
  | { kind: "rotate"; page: string; degrees: number }
  | { kind: "remove"; page: string }
  | { kind: "redact"; page: string; rects: Rect[] };

type Change =
  | { kind: "note"; id: string; note: Note | null }
  | { kind: "field"; name: string; value: string | null }
  | { kind: "work"; work: PageWork }
  | { kind: "worked"; work: PageWork; asset: string; pages: Page[] }
  | { kind: "base"; asset: string }
  | { kind: "group"; changes: Change[] };

interface Widget {
  subtype: string;
  fieldName?: string;
  fieldType?: string;
  fieldValue?: unknown;
  rect: Rect;
  checkBox?: boolean;
  radioButton?: boolean;
  exportValue?: string;
  options?: { exportValue: string; displayValue: string }[];
}

type Tool = "read" | "note" | "redact";

interface Holder {
  element: HTMLElement;
  layer: HTMLElement;
  viewport: pdfjs.PageViewport | undefined;
  drawn: Promise<void> | undefined;
}

const SCALE = 1.4;
const NOTE_POINTS = 24;
const REDACT_LEAST_POINTS = 3;
const RIGHT_ANGLE = 90;
const NOTE_COLOR = "FFD400";

class PdfEditor implements Editor, Pictures {
  private readonly pagesRoot: HTMLElement;
  private readonly bar: Bar;
  private readonly status: Status;
  private readonly outbox = new Outbox<Change>();
  private readonly holders = new Map<string, Holder>();
  private tools: Toggles | undefined;
  private pageKeys: Key[] = [];
  private snapshot: Snapshot | undefined;
  private loading: pdfjs.PDFDocumentLoadingTask | undefined;
  private document: pdfjs.PDFDocumentProxy | undefined;
  private visible: IntersectionObserver | undefined;
  private people: readonly Peer[] = [];
  private canEdit = true;
  private tool: Tool = "read";
  private page: string | null = null;
  private opened: Promise<void> = Promise.resolve();

  constructor(
    root: HTMLElement,
    private readonly client: Client,
  ) {
    this.status = new Status(root);
    this.bar = new Bar(root, "Pages");
    root.append(this.bar.element);
    this.pagesRoot = root.appendChild(document.createElement("div"));
    this.pagesRoot.className = "document-page pdf-pages";
  }

  private showBar(): void {
    if (!this.canEdit) {
      this.tools = undefined;
      this.pageKeys = [];
      this.bar.show(readOnly(), gap(), this.status.element);
      this.pick("read");
      return;
    }
    const toggle = (tool: Tool) => () => this.pick(this.tool === tool ? "read" : tool);
    this.tools = wellOf("Tools", [
      { label: "Note", icon: StickyNote, press: toggle("note") },
      { label: "Redact", icon: Eraser, press: toggle("redact") },
    ]);
    this.pageKeys = [
      keyOf({
        label: "Rotate",
        icon: RotateCw,
        press: () => this.work({ kind: "rotate", degrees: RIGHT_ANGLE }),
      }),
      keyOf({ label: "Move up", icon: ArrowUp, press: () => this.movePage(-1) }),
      keyOf({ label: "Move down", icon: ArrowDown, press: () => this.movePage(1) }),
      keyOf({ label: "Remove page", icon: Trash2, press: () => this.work({ kind: "remove" }) }),
    ];
    this.bar.show(
      this.tools.element,
      rule(),
      ...this.pageKeys.map((key) => key.element),
      gap(),
      this.status.element,
    );
    this.pick(this.tool);
    this.showPicked();
  }

  private pick(tool: Tool): void {
    this.tool = tool;
    this.tools?.show([{ active: tool === "note" }, { active: tool === "redact" }]);
    this.pagesRoot.dataset.tool = tool;
  }

  /** The keys of a page work on the page that the person picked last. */
  private showPicked(): void {
    for (const key of this.pageKeys) key.show({ enabled: this.page !== null });
  }

  private work(work: { kind: "rotate"; degrees: number } | { kind: "remove" }): void {
    if (this.page) this.change({ kind: "work", work: { ...work, page: this.page } });
  }

  private movePage(step: number): void {
    const pages = this.snapshot?.pages.map((page) => page.id) ?? [];
    const from = this.page ? pages.indexOf(this.page) : -1;
    const to = from + step;
    if (from < 0 || to < 0 || to >= pages.length) return;
    const [moved] = pages.splice(from, 1);
    if (moved) pages.splice(to, 0, moved);
    this.change({ kind: "work", work: { kind: "order", pages } });
  }

  load(raw: unknown, person: Person): void {
    this.snapshot = raw as Snapshot;
    this.canEdit = person.canEdit;
    this.showBar();
    this.opened = this.openFile();
    this.opened.catch(console.error);
  }

  private async openFile(): Promise<void> {
    const snapshot = this.snapshot;
    if (!snapshot) return;
    const bytes = await this.client.asset(snapshot.asset);
    await this.loading?.destroy();
    this.loading = pdfjs.getDocument({ data: bytes.slice() });
    this.document = await this.loading.promise;
    this.buildPages(snapshot);
  }

  private buildPages(snapshot: Snapshot): void {
    this.visible?.disconnect();
    this.holders.clear();
    this.pagesRoot.replaceChildren();
    this.visible = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const id = (entry.target as HTMLElement).dataset.page;
          if (id) this.showPage(id).catch(console.error);
        }
      },
      { root: this.pagesRoot, rootMargin: "100% 0px" },
    );
    for (const page of snapshot.pages) {
      const sideways = page.rotation % 180 !== 0;
      const element = this.pagesRoot.appendChild(document.createElement("div"));
      element.className = "pdf-page";
      element.dataset.page = page.id;
      element.style.width = `${(sideways ? page.height : page.width) * SCALE}px`;
      element.style.height = `${(sideways ? page.width : page.height) * SCALE}px`;
      element.addEventListener("pointerdown", () => {
        this.page = page.id;
        this.showPicked();
        this.client.place({ page: page.id });
      });
      const layer = element.appendChild(document.createElement("div"));
      layer.className = "pdf-layer";
      this.holders.set(page.id, { element, layer, viewport: undefined, drawn: undefined });
      this.visible.observe(element);
    }
    this.page ??= snapshot.pages[0]?.id ?? null;
    this.showPicked();
  }

  private showPage(id: string): Promise<void> {
    const holder = this.holders.get(id);
    if (!holder) return Promise.resolve();
    holder.drawn ??= this.drawPage(id, holder);
    return holder.drawn;
  }

  private async drawPage(id: string, holder: Holder): Promise<void> {
    const pdf = this.document;
    const index = this.snapshot?.pages.findIndex((page) => page.id === id) ?? -1;
    if (!pdf || index < 0) return;
    const page = await pdf.getPage(index + 1);
    const viewport = page.getViewport({ scale: SCALE });
    holder.viewport = viewport;
    const ratio = window.devicePixelRatio || 1;
    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(viewport.width * ratio);
    canvas.height = Math.floor(viewport.height * ratio);
    canvas.style.width = "100%";
    canvas.style.height = "100%";
    holder.element.prepend(canvas);
    await page.render({ canvas, viewport, transform: [ratio, 0, 0, ratio, 0, 0] }).promise;
    const text = document.createElement("div");
    text.className = "textLayer";
    text.style.setProperty("--total-scale-factor", String(SCALE));
    holder.element.insertBefore(text, holder.layer);
    await new pdfjs.TextLayer({
      textContentSource: page.streamTextContent(),
      container: text,
      viewport,
    }).render();
    this.bindPointer(id, holder);
    this.drawLayer(id, holder, (await page.getAnnotations()) as Widget[]);
  }

  private bindPointer(id: string, holder: Holder): void {
    let start: [number, number] | null = null;
    let box: HTMLElement | null = null;
    const pointOf = (event: PointerEvent): [number, number] => {
      const frame = holder.element.getBoundingClientRect();
      return [event.clientX - frame.left, event.clientY - frame.top];
    };
    holder.element.addEventListener("pointerdown", (event) => {
      if (this.tool === "note") {
        this.addNote(id, holder, pointOf(event));
        this.pick("read");
      } else if (this.tool === "redact") {
        event.preventDefault();
        start = pointOf(event);
        holder.element.setPointerCapture(event.pointerId);
        box = holder.element.appendChild(document.createElement("div"));
        box.className = "pdf-redaction";
      }
    });
    holder.element.addEventListener("pointermove", (event) => {
      if (!start || !box) return;
      const point = pointOf(event);
      box.style.left = `${Math.min(start[0], point[0])}px`;
      box.style.top = `${Math.min(start[1], point[1])}px`;
      box.style.width = `${Math.abs(point[0] - start[0])}px`;
      box.style.height = `${Math.abs(point[1] - start[1])}px`;
    });
    holder.element.addEventListener("pointerup", (event) => {
      if (!start) return;
      const rect = this.pdfRect(holder, start, pointOf(event));
      start = null;
      box?.remove();
      box = null;
      this.pick("read");
      if (rect) this.change({ kind: "work", work: { kind: "redact", page: id, rects: [rect] } });
    });
  }

  private pdfRect(holder: Holder, from: [number, number], to: [number, number]): Rect | null {
    const viewport = holder.viewport;
    if (!viewport) return null;
    const [left, top] = viewport.convertToPdfPoint(from[0], from[1]) as [number, number];
    const [right, bottom] = viewport.convertToPdfPoint(to[0], to[1]) as [number, number];
    const rect: Rect = [
      Math.min(left, right),
      Math.min(top, bottom),
      Math.max(left, right),
      Math.max(top, bottom),
    ];
    const large =
      rect[2] - rect[0] > REDACT_LEAST_POINTS && rect[3] - rect[1] > REDACT_LEAST_POINTS;
    return large ? rect : null;
  }

  private drawLayer(id: string, holder: Holder, widgets: Widget[]): void {
    const viewport = holder.viewport;
    if (!viewport) return;
    holder.layer.replaceChildren();
    for (const [noteId, note] of Object.entries(this.snapshot?.notes ?? {})) {
      if (note.page === id) this.drawNote(holder, viewport, noteId, note);
    }
    for (const widget of widgets) this.drawField(holder, viewport, widget);
    for (const peer of this.people) {
      if ((peer.place as { page?: string } | null)?.page !== id) continue;
      const mark = holder.layer.appendChild(document.createElement("span"));
      mark.className = "pdf-peer";
      mark.textContent = peer.name;
      mark.style.background = colorOf(peer.id);
    }
  }

  private place(element: HTMLElement, viewport: pdfjs.PageViewport, rect: Rect): void {
    const [left, top] = viewport.convertToViewportPoint(rect[0], rect[1]) as [number, number];
    const [right, bottom] = viewport.convertToViewportPoint(rect[2], rect[3]) as [number, number];
    element.style.left = `${Math.min(left, right)}px`;
    element.style.top = `${Math.min(top, bottom)}px`;
    element.style.width = `${Math.abs(right - left)}px`;
    element.style.height = `${Math.abs(bottom - top)}px`;
  }

  private drawNote(holder: Holder, viewport: pdfjs.PageViewport, id: string, note: Note): void {
    const mark = holder.layer.appendChild(document.createElement("button"));
    mark.type = "button";
    mark.className = "pdf-note";
    mark.title = note.author ? `${note.author}: ${note.text}` : note.text;
    this.place(mark, viewport, note.rect);
    if (note.color) mark.style.background = `#${note.color}66`;
    mark.addEventListener("click", (event) => {
      event.stopPropagation();
      if (!this.canEdit) return;
      const text = window.prompt("Note", note.text);
      if (text !== null) this.change({ kind: "note", id, note: text ? { ...note, text } : null });
    });
  }

  private drawField(holder: Holder, viewport: pdfjs.PageViewport, widget: Widget): void {
    const name = widget.fieldName;
    if (widget.subtype !== "Widget" || !name) return;
    const known = this.snapshot?.fields[name] ?? String(widget.fieldValue ?? "");
    const set = (value: string) => this.change({ kind: "field", name, value });
    let input: HTMLInputElement | HTMLSelectElement;
    if (widget.fieldType === "Ch") {
      const select = document.createElement("select");
      for (const option of widget.options ?? []) {
        select.add(new Option(option.displayValue, option.exportValue));
      }
      select.value = known;
      select.addEventListener("change", () => set(select.value));
      input = select;
    } else if (widget.fieldType === "Btn") {
      if (!widget.checkBox && !widget.radioButton) return;
      const box = document.createElement("input");
      box.type = widget.radioButton ? "radio" : "checkbox";
      box.checked = known === widget.exportValue;
      box.addEventListener("change", () =>
        set(box.checked ? (widget.exportValue ?? "Yes") : "Off"),
      );
      input = box;
    } else {
      const field = document.createElement("input");
      field.value = known;
      field.addEventListener("change", () => set(field.value));
      input = field;
    }
    input.className = "pdf-field";
    input.disabled = !this.canEdit;
    input.ariaLabel = name;
    this.place(input, viewport, widget.rect);
    holder.layer.appendChild(input);
  }

  private redrawLayers(): void {
    const pdf = this.document;
    const pages = this.snapshot?.pages ?? [];
    if (!pdf) return;
    for (const [id, holder] of this.holders) {
      const index = pages.findIndex((page) => page.id === id);
      if (!holder.viewport || index < 0) continue;
      pdf.getPage(index + 1).then(async (page) => {
        this.drawLayer(id, holder, (await page.getAnnotations()) as Widget[]);
      }, console.error);
    }
  }

  private addNote(page: string, holder: Holder, at: [number, number]): void {
    const viewport = holder.viewport;
    const text = window.prompt("Note");
    if (!text || !viewport) return;
    const [x, y] = viewport.convertToPdfPoint(at[0], at[1]) as [number, number];
    this.change({
      kind: "note",
      id: crypto.randomUUID(),
      note: {
        page,
        kind: "Text",
        rect: [x, y - NOTE_POINTS, x + NOTE_POINTS, y],
        text,
        color: NOTE_COLOR,
        author: "",
      },
    });
  }

  private change(change: Change): void {
    this.showChange(change);
    this.outbox.add(change);
    this.client.pull();
    this.redrawLayers();
  }

  private covered(change: Change): boolean {
    const same = (mine: Change): boolean => {
      if (mine.kind === "group") return mine.changes.some(same);
      if (mine.kind === "note") return change.kind === "note" && change.id === mine.id;
      if (mine.kind === "field") return change.kind === "field" && change.name === mine.name;
      return false;
    };
    return this.outbox.unanswered().some(same);
  }

  private showChange(change: Change): void {
    const snapshot = this.snapshot;
    if (!snapshot) return;
    switch (change.kind) {
      case "note":
        if (change.note) snapshot.notes[change.id] = change.note;
        else delete snapshot.notes[change.id];
        break;
      case "field":
        if (change.value === null) delete snapshot.fields[change.name];
        else snapshot.fields[change.name] = change.value;
        break;
      case "worked":
        snapshot.asset = change.asset;
        snapshot.pages = change.pages;
        break;
      case "base":
        snapshot.asset = change.asset;
        break;
      case "group":
        for (const inner of change.changes) this.showChange(inner);
        break;
      case "work":
        break;
    }
  }

  private reloads(change: Change): boolean {
    if (change.kind === "group") return change.changes.some((inner) => this.reloads(inner));
    return change.kind === "worked" || change.kind === "base";
  }

  batch(): unknown | null {
    return batchOf(this.outbox.next());
  }

  apply(changed: Changed, own: boolean): void {
    if (own) this.outbox.answered();
    const change = changed.body as Change;
    if (!own && change.kind !== "group" && this.covered(change)) return;
    if (!own || this.reloads(change)) this.showChange(change);
    if (this.reloads(change)) this.openFile().catch(console.error);
    else if (!own) this.redrawLayers();
  }

  peers(peers: readonly Peer[]): void {
    this.people = peers;
    this.status.peers(peers);
    this.redrawLayers();
  }

  saved(_rev: number, waits: boolean): void {
    this.status.saved(waits);
  }

  closed(reason: ClosedReason): void {
    this.canEdit = false;
    this.showBar();
    this.status.closed(reason);
  }

  selection(): string {
    return window.getSelection()?.toString() ?? "";
  }

  /** Each page, drawn. */
  async pictures(): Promise<number> {
    await this.opened;
    const pages = this.snapshot?.pages ?? [];
    await Promise.all(pages.map((page) => this.showPage(page.id)));
    await settled();
    return pages.length;
  }

  async picture(index: number): Promise<Area> {
    const id = this.snapshot?.pages[index]?.id;
    const holder = id ? this.holders.get(id) : undefined;
    if (!holder) throw new Error(`There is no page ${index + 1}.`);
    return areaOf(holder.element);
  }

  destroy(): void {
    this.bar.destroy();
    this.visible?.disconnect();
    this.loading?.destroy().catch(console.error);
  }
}

export function createEditor(element: HTMLElement, client: Client): PdfEditor {
  return new PdfEditor(element, client);
}
