import {
  Bar,
  type Fitting,
  gap,
  type Key,
  keyOf,
  readOnly,
  rule,
  type Toggles,
  wellOf,
  Zoom,
} from "@documents/bar";
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
import { say } from "@documents/words";

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

/** Where a person is: the page in the middle of their view, and the note that they chose. */
interface Place {
  page?: string;
  note?: string;
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

/** A form field or a link of a page, as pdf.js reads it. */
interface Annotation {
  subtype: string;
  fieldName?: string;
  fieldType?: string;
  fieldValue?: unknown;
  rect: Rect;
  checkBox?: boolean;
  radioButton?: boolean;
  exportValue?: string;
  options?: { exportValue: string; displayValue: string }[];
  /** A web or mail address that pdf.js found safe to open. */
  url?: string;
  /** A place in the file: a name, or the page and the position. */
  dest?: string | unknown[] | null;
}

type Tool = "read" | "note" | "redact";

/** A picture of a page at a scale, and the drawing that makes it. */
interface Picture {
  scale: number;
  shown: Promise<void>;
  task: pdfjs.RenderTask | undefined;
}

interface Holder {
  element: HTMLElement;
  layer: HTMLElement;
  /** The page at its actual size. */
  viewport: pdfjs.PageViewport | undefined;
  drawn: Promise<void> | undefined;
  /** The newest picture of the page. An older one stays until it shows. */
  picture: Picture | undefined;
  /** Whether the page is in the view or near it. Only then does it keep a picture. */
  near: boolean;
}

/** CSS pixels for each point of a page at its actual size. */
const ACTUAL = pdfjs.PixelsPerInch.PDF_TO_CSS_UNITS;
/** The space around and between the pages at their actual size, in CSS pixels. */
const SPACE = 12;
/** The most pixels of one picture. Beyond them, a zoom enlarges the picture. */
const MOST_PIXELS = 4096 * 4096;
/** How long a zoom rests before the pages get pictures at its scale. */
const REDRAW_MS = 150;
const NOTE_POINTS = 24;
const REDACT_LEAST_POINTS = 3;
const RIGHT_ANGLE = 90;
const NOTE_COLOR = "FFD400";

/** The size of a page in points, as it lies after its rotation. */
function sizeOf(page: Page): [number, number] {
  return page.rotation % 180 === 0 ? [page.width, page.height] : [page.height, page.width];
}

/**
 * Lays an element over a box of a page at its actual size, in shares of the page, so that it
 * keeps its place at each zoom.
 */
function cover(element: HTMLElement, viewport: pdfjs.PageViewport, box: Rect): void {
  const [left, top, right, bottom] = box;
  element.style.left = `${(left / viewport.width) * 100}%`;
  element.style.top = `${(top / viewport.height) * 100}%`;
  element.style.width = `${((right - left) / viewport.width) * 100}%`;
  element.style.height = `${((bottom - top) / viewport.height) * 100}%`;
}

function boxOf(from: [number, number], to: [number, number]): Rect {
  return [
    Math.min(from[0], to[0]),
    Math.min(from[1], to[1]),
    Math.max(from[0], to[0]),
    Math.max(from[1], to[1]),
  ];
}

class PdfEditor implements Editor, Pictures {
  private readonly pagesRoot: HTMLElement;
  private readonly bar: Bar;
  private readonly status: Status;
  private readonly zoom: Zoom;
  private readonly outbox = new Outbox<Change>();
  private readonly holders = new Map<string, Holder>();
  private scale = 1;
  private resting = 0;
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
  private here: Place = {};
  private opened: Promise<void> = Promise.resolve();

  constructor(
    root: HTMLElement,
    private readonly client: Client,
  ) {
    this.status = new Status(root);
    this.bar = new Bar(root, say("Pages"));
    root.append(this.bar.element);
    this.pagesRoot = root.appendChild(document.createElement("div"));
    this.pagesRoot.className = "document-page pdf-pages";
    this.pagesRoot.style.setProperty("--space", `${SPACE}px`);
    this.pagesRoot.addEventListener("scroll", () => this.placeInView(), { passive: true });
    this.zoom = new Zoom({
      area: this.pagesRoot,
      inset: 0,
      fits: () => this.fits(),
      scale: (scale) => this.zoomTo(scale),
    });
  }

  /** The scales at which the widest page fills the view, and at which the largest page fits. */
  private fits(): Record<Fitting, number> {
    let width = 0;
    let height = 0;
    for (const page of this.snapshot?.pages ?? []) {
      const [across, down] = sizeOf(page);
      width = Math.max(width, across);
      height = Math.max(height, down);
    }
    const across = (this.pagesRoot.clientWidth - 1) / (width * ACTUAL + 2 * SPACE);
    const down = (this.pagesRoot.clientHeight - 1) / (height * ACTUAL + 2 * SPACE);
    return { width: across, page: Math.min(across, down) };
  }

  /**
   * The pages take their size at once. The pages near the view get a sharp picture once the
   * zoom rests.
   */
  private zoomTo(scale: number): void {
    this.scale = scale;
    this.pagesRoot.style.setProperty("--zoom", String(scale));
    clearTimeout(this.resting);
    this.resting = window.setTimeout(() => {
      for (const [id, holder] of this.holders) {
        if (holder.near) this.showPage(id).catch(console.error);
      }
    }, REDRAW_MS);
  }

  /** Where the person is: a page, and the note that they chose on it. */
  private placeAt(page: string, note?: string): void {
    if (this.here.page === page && this.here.note === note) return;
    this.here = note ? { page, note } : { page };
    this.client.place(this.here);
  }

  /** The page in the middle of the view is the person's page. */
  private placeInView(): void {
    const view = this.pagesRoot.getBoundingClientRect();
    const middle = view.top + view.height / 2;
    let nearest: string | undefined;
    let distance = Number.POSITIVE_INFINITY;
    for (const [id, holder] of this.holders) {
      const box = holder.element.getBoundingClientRect();
      const away = Math.max(0, box.top - middle, middle - box.bottom);
      if (away < distance) {
        distance = away;
        nearest = id;
      }
    }
    if (nearest && nearest !== this.here.page) this.placeAt(nearest);
  }

  private showBar(): void {
    if (!this.canEdit) {
      this.tools = undefined;
      this.pageKeys = [];
      this.bar.show(readOnly(), gap(), this.zoom.element, this.status.element);
      this.pick("read");
      return;
    }
    const toggle = (tool: Tool) => () => this.pick(this.tool === tool ? "read" : tool);
    this.tools = wellOf(say("Tools"), [
      { label: say("Note"), icon: StickyNote, press: toggle("note") },
      { label: say("Redact"), icon: Eraser, press: toggle("redact") },
    ]);
    this.pageKeys = [
      keyOf({
        label: say("Rotate"),
        icon: RotateCw,
        press: () => this.work({ kind: "rotate", degrees: RIGHT_ANGLE }),
      }),
      keyOf({ label: say("Move up"), icon: ArrowUp, press: () => this.movePage(-1) }),
      keyOf({ label: say("Move down"), icon: ArrowDown, press: () => this.movePage(1) }),
      keyOf({
        label: say("Remove page"),
        icon: Trash2,
        press: () => this.work({ kind: "remove" }),
      }),
    ];
    this.bar.show(
      this.tools.element,
      rule(),
      ...this.pageKeys.map((key) => key.element),
      gap(),
      this.zoom.element,
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
    for (const holder of this.holders.values()) holder.picture?.task?.cancel();
    this.holders.clear();
    this.pagesRoot.replaceChildren();
    this.visible = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const id = (entry.target as HTMLElement).dataset.page;
          const holder = id ? this.holders.get(id) : undefined;
          if (!id || !holder) continue;
          holder.near = entry.isIntersecting;
          if (holder.near) this.showPage(id).catch(console.error);
          else this.dropPicture(holder);
        }
      },
      { root: this.pagesRoot, rootMargin: "100% 0px" },
    );
    for (const page of snapshot.pages) {
      const [width, height] = sizeOf(page);
      const element = this.pagesRoot.appendChild(document.createElement("div"));
      element.className = "pdf-page";
      element.dataset.page = page.id;
      element.style.width = `calc(var(--zoom) * ${width * ACTUAL}px)`;
      element.style.height = `calc(var(--zoom) * ${height * ACTUAL}px)`;
      element.addEventListener("pointerdown", () => {
        this.page = page.id;
        this.showPicked();
        this.placeAt(page.id);
      });
      const layer = element.appendChild(document.createElement("div"));
      layer.className = "pdf-layer";
      this.holders.set(page.id, {
        element,
        layer,
        viewport: undefined,
        drawn: undefined,
        picture: undefined,
        near: true,
      });
      this.visible.observe(element);
    }
    this.zoom.refit();
    this.page ??= snapshot.pages[0]?.id ?? null;
    this.showPicked();
    this.here = {};
    this.placeInView();
  }

  /** Draws a page once, and gives it a picture at the zoom. */
  private showPage(id: string): Promise<void> {
    const holder = this.holders.get(id);
    if (!holder) return Promise.resolve();
    holder.drawn ??= this.drawPage(id, holder);
    return holder.drawn.then(() => this.paint(id, holder));
  }

  private async drawPage(id: string, holder: Holder): Promise<void> {
    const pdf = this.document;
    const index = this.snapshot?.pages.findIndex((page) => page.id === id) ?? -1;
    if (!pdf || index < 0) return;
    const page = await pdf.getPage(index + 1);
    const viewport = page.getViewport({ scale: ACTUAL });
    holder.viewport = viewport;
    const painted = this.paint(id, holder);
    const text = document.createElement("div");
    text.className = "textLayer";
    text.style.setProperty("--total-scale-factor", `calc(var(--zoom) * ${ACTUAL})`);
    holder.element.insertBefore(text, holder.layer);
    await new pdfjs.TextLayer({
      textContentSource: page.streamTextContent(),
      container: text,
      viewport,
    }).render();
    this.bindPointer(id, holder);
    this.drawLayer(id, holder, (await page.getAnnotations()) as Annotation[]);
    await painted;
  }

  /** Draws the picture of a page near the view at the zoom, unless one is there or on its way. */
  private paint(id: string, holder: Holder): Promise<void> {
    const scale = this.scale;
    if (!holder.near) return Promise.resolve();
    if (holder.picture?.scale === scale) return holder.picture.shown;
    holder.picture?.task?.cancel();
    const picture: Picture = { scale, shown: Promise.resolve(), task: undefined };
    holder.picture = picture;
    picture.shown = this.drawPicture(id, holder, picture);
    return picture.shown;
  }

  private async drawPicture(id: string, holder: Holder, picture: Picture): Promise<void> {
    const pdf = this.document;
    const index = this.snapshot?.pages.findIndex((page) => page.id === id) ?? -1;
    if (!pdf || index < 0) return;
    const page = await pdf.getPage(index + 1);
    if (holder.picture !== picture) return;
    const viewport = page.getViewport({ scale: ACTUAL * picture.scale });
    const area = viewport.width * viewport.height;
    const ratio = Math.min(window.devicePixelRatio || 1, Math.sqrt(MOST_PIXELS / area));
    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(viewport.width * ratio);
    canvas.height = Math.floor(viewport.height * ratio);
    picture.task = page.render({ canvas, viewport, transform: [ratio, 0, 0, ratio, 0, 0] });
    try {
      await picture.task.promise;
    } catch (error) {
      if (error instanceof pdfjs.RenderingCancelledException) return;
      throw error;
    }
    if (holder.picture !== picture) return;
    picture.task = undefined;
    holder.element.querySelector(":scope > canvas")?.remove();
    holder.element.prepend(canvas);
  }

  /** A page far from the view lets its picture go, and draws it again when it comes near. */
  private dropPicture(holder: Holder): void {
    holder.picture?.task?.cancel();
    holder.picture = undefined;
    holder.element.querySelector(":scope > canvas")?.remove();
  }

  private bindPointer(id: string, holder: Holder): void {
    let start: [number, number] | null = null;
    let box: HTMLElement | null = null;
    /** Where the pointer is on the page at its actual size. */
    const pointOf = (event: PointerEvent): [number, number] => {
      const frame = holder.element.getBoundingClientRect();
      const scale = frame.width / (holder.viewport?.width || frame.width);
      return [(event.clientX - frame.left) / scale, (event.clientY - frame.top) / scale];
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
      if (!start || !box || !holder.viewport) return;
      cover(box, holder.viewport, boxOf(start, pointOf(event)));
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
    const rect = boxOf(
      viewport.convertToPdfPoint(from[0], from[1]) as [number, number],
      viewport.convertToPdfPoint(to[0], to[1]) as [number, number],
    );
    const large =
      rect[2] - rect[0] > REDACT_LEAST_POINTS && rect[3] - rect[1] > REDACT_LEAST_POINTS;
    return large ? rect : null;
  }

  private drawLayer(id: string, holder: Holder, annotations: Annotation[]): void {
    const viewport = holder.viewport;
    if (!viewport) return;
    holder.layer.replaceChildren();
    for (const link of annotations) {
      if (link.subtype === "Link") this.drawLink(holder, viewport, link);
    }
    for (const [noteId, note] of Object.entries(this.snapshot?.notes ?? {})) {
      if (note.page === id) this.drawNote(holder, viewport, noteId, note);
    }
    for (const widget of annotations) this.drawField(holder, viewport, widget);
    for (const peer of this.people) {
      if ((peer.place as Place | null)?.page !== id) continue;
      const mark = holder.layer.appendChild(document.createElement("span"));
      mark.className = "pdf-peer";
      mark.textContent = peer.name;
      mark.style.background = colorOf(peer.person);
    }
  }

  private place(element: HTMLElement, viewport: pdfjs.PageViewport, rect: Rect): void {
    cover(
      element,
      viewport,
      boxOf(
        viewport.convertToViewportPoint(rect[0], rect[1]) as [number, number],
        viewport.convertToViewportPoint(rect[2], rect[3]) as [number, number],
      ),
    );
  }

  private drawNote(holder: Holder, viewport: pdfjs.PageViewport, id: string, note: Note): void {
    const mark = holder.layer.appendChild(document.createElement("button"));
    mark.type = "button";
    mark.className = "pdf-note";
    mark.title = note.author ? `${note.author}: ${note.text}` : note.text;
    this.place(mark, viewport, note.rect);
    if (note.color) mark.style.background = `#${note.color}66`;
    const chosen = this.people.find((peer) => (peer.place as Place | null)?.note === id);
    if (chosen) mark.style.outline = `2px solid ${colorOf(chosen.person)}`;
    mark.addEventListener("click", (event) => {
      event.stopPropagation();
      this.placeAt(note.page, id);
      if (!this.canEdit) return;
      const text = window.prompt(say("Note"), note.text);
      if (text !== null) this.change({ kind: "note", id, note: text ? { ...note, text } : null });
    });
  }

  /**
   * A web or mail link opens as the host opens links, which is in the browser of the device. A
   * link to a place in the file scrolls there.
   */
  private drawLink(holder: Holder, viewport: pdfjs.PageViewport, link: Annotation): void {
    if (!link.url && !link.dest) return;
    const mark = holder.layer.appendChild(document.createElement("a"));
    mark.className = "pdf-link";
    this.place(mark, viewport, link.rect);
    if (link.url) {
      mark.href = link.url;
      mark.target = "_blank";
      mark.rel = "noopener";
      mark.title = link.url;
      return;
    }
    mark.href = "#";
    mark.addEventListener("click", (event) => {
      event.preventDefault();
      this.goTo(link.dest).catch(console.error);
    });
  }

  /** Scrolls to a place in the file: the top of its page, or the height that it names there. */
  private async goTo(dest: Annotation["dest"]): Promise<void> {
    const pdf = this.document;
    const explicit = typeof dest === "string" ? await pdf?.getDestination(dest) : dest;
    const target = explicit?.[0];
    if (!pdf || !explicit || target === undefined || target === null) return;
    const index =
      typeof target === "number"
        ? target
        : await pdf.getPageIndex(target as Parameters<typeof pdf.getPageIndex>[0]);
    const id = this.snapshot?.pages[index]?.id;
    const holder = id ? this.holders.get(id) : undefined;
    if (!holder) return;
    const kind = (explicit[1] as { name?: string } | null | undefined)?.name;
    const top =
      kind === "XYZ" ? explicit[3] : kind === "FitH" || kind === "FitBH" ? explicit[2] : null;
    const page = await pdf.getPage(index + 1);
    const [, down] =
      typeof top === "number"
        ? page.getViewport({ scale: ACTUAL }).convertToViewportPoint(0, top)
        : [0, 0];
    const area = this.pagesRoot.getBoundingClientRect();
    const box = holder.element.getBoundingClientRect();
    this.pagesRoot.scrollTop += box.top - area.top + Math.max(0, down ?? 0) * this.scale;
  }

  private drawField(holder: Holder, viewport: pdfjs.PageViewport, widget: Annotation): void {
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
        this.drawLayer(id, holder, (await page.getAnnotations()) as Annotation[]);
      }, console.error);
    }
  }

  private addNote(page: string, holder: Holder, at: [number, number]): void {
    const viewport = holder.viewport;
    const text = window.prompt(say("Note"));
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
    clearTimeout(this.resting);
    this.zoom.destroy();
    this.bar.destroy();
    this.visible?.disconnect();
    this.loading?.destroy().catch(console.error);
  }
}

export function createEditor(element: HTMLElement, client: Client): PdfEditor {
  return new PdfEditor(element, client);
}
