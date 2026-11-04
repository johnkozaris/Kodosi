import type { Node as ProseNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet, type EditorView } from "@tiptap/pm/view";

/** A page in CSS pixels. */
export interface PageShape {
  height: number;
  top: number;
  bottom: number;
}

export interface BlockRules {
  keepNext(block: ProseNode): boolean;
  startsPage(block: ProseNode): boolean;
}

interface Line {
  top: number;
  bottom: number;
  left: number;
  block: number;
  first: boolean;
  last: boolean;
  keepNext: boolean;
  forced: boolean;
  kind: "block" | "line" | "row";
  position: number;
}

interface PageBreak {
  kind: Line["kind"];
  position: number;
  end: number;
  height: number;
  left: number;
}

interface Hole {
  top: number;
  height: number;
}

const pagesKey = new PluginKey<DecorationSet>("pages");
export const PAGE_GAP = 16;
const SETTLE_MS = 60;
const SAME_PIXELS = 0.5;

function rowsOf(table: HTMLTableElement): HTMLTableRowElement[] {
  return [...table.rows].filter((row) => !row.classList.contains("page-break"));
}

function outsideHoles(rect: DOMRect, holes: readonly Hole[]): boolean {
  return !holes.some((hole) => rect.top >= hole.top && rect.bottom <= hole.top + hole.height + 1);
}

/** The visible lines of a paragraph, top to bottom. */
function textLines(element: HTMLElement, holes: readonly Hole[]): DOMRect[] {
  const range = document.createRange();
  range.selectNodeContents(element);
  const lines: DOMRect[] = [];
  for (const rect of range.getClientRects()) {
    if (rect.height === 0 || !outsideHoles(rect, holes)) continue;
    const last = lines.at(-1);
    if (last && rect.top < last.bottom - 1) {
      last.height = Math.max(last.bottom, rect.bottom) - last.top;
    } else {
      lines.push(DOMRect.fromRect(rect));
    }
  }
  return lines.length > 0 ? lines : [element.getBoundingClientRect()];
}

class Measure {
  private next = 0;
  private passed = 0;

  constructor(
    private readonly holes: readonly Hole[],
    private readonly zoom: number,
    private readonly origin: number,
  ) {}

  /** Where a point would lie with no spaces between pages. Points must come in order. */
  natural(y: number): number {
    for (let hole = this.holes[this.next]; hole && hole.top < y; hole = this.holes[this.next]) {
      this.passed += hole.height;
      this.next += 1;
    }
    return (y - this.passed - this.origin) / this.zoom;
  }
}

function linesOf(
  view: EditorView,
  rules: BlockRules,
  holes: readonly Hole[],
  zoom: number,
): Line[] {
  const root = view.dom.getBoundingClientRect();
  const measure = new Measure(holes, zoom, root.top);
  const lines: Line[] = [];
  let forceNext = false;
  view.state.doc.forEach((block, offset, index) => {
    const element = view.nodeDOM(offset);
    if (!(element instanceof HTMLElement)) return;
    const common = { block: index, keepNext: rules.keepNext(block) };
    const leftOf = (left: number) => (left - root.left) / zoom;
    if (element instanceof HTMLTableElement) {
      let at = offset + 1;
      block.forEach((row, _rowOffset, rowIndex) => {
        const rect = rowsOf(element)[rowIndex]?.getBoundingClientRect();
        if (rect) {
          lines.push({
            ...common,
            top: measure.natural(rect.top),
            bottom: measure.natural(rect.bottom),
            left: leftOf(rect.left),
            first: false,
            last: false,
            forced: forceNext,
            kind: "row",
            position: at,
          });
          forceNext = false;
        }
        at += row.nodeSize;
      });
      return;
    }
    const pageBreaks = [...element.querySelectorAll("[data-page-break]")].map(
      (mark) => mark.getBoundingClientRect().bottom,
    );
    const rects = textLines(element, holes);
    for (const [line, rect] of rects.entries()) {
      const first = line === 0;
      lines.push({
        ...common,
        top: measure.natural(rect.top),
        bottom: measure.natural(rect.bottom),
        left: leftOf(element.getBoundingClientRect().left),
        first,
        last: line === rects.length - 1,
        forced: first
          ? forceNext || rules.startsPage(block)
          : pageBreaks.some((bottom) => rect.top >= bottom - 1),
        kind: first ? "block" : "line",
        position: first ? offset : -1,
      });
      forceNext = false;
      if (!first) {
        const position = view.posAtCoords({ left: rect.left + 1, top: rect.top + 1 });
        const line = lines.at(-1);
        if (line) line.position = position?.pos ?? offset + 1;
      }
    }
    forceNext = pageBreaks.some((bottom) => (rects.at(-1)?.bottom ?? 0) <= bottom + 1);
  });
  return lines;
}

/** Moves a break so that no heading stands alone and no line of a paragraph does. */
function settle(lines: readonly Line[], at: number, pageTop: number): number {
  let index = at;
  const line = lines[index];
  const before = lines[index - 1];
  const twoBefore = lines[index - 2];
  if (line && before?.first && line.block === before.block) index -= 1;
  else if (line?.last && before && !before.first && line.block === before.block) index -= 1;
  else if (line?.last && twoBefore?.first && line.block === twoBefore.block) index -= 2;
  for (let previous = lines[index - 1]; previous; previous = lines[index - 1]) {
    if (!previous.keepNext || !previous.first || previous.top <= pageTop) break;
    index -= 1;
  }
  return index > 0 && (lines[index]?.top ?? 0) > pageTop ? index : at;
}

function planBreaks(lines: readonly Line[], page: PageShape, zoomedTop: number): PageBreak[] {
  const room = page.height - page.top - page.bottom;
  const breaks: PageBreak[] = [];
  let pageTop = zoomedTop;
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    if (!line) continue;
    const full = line.bottom - pageTop > room && line.top > pageTop;
    if (!full && !line.forced) continue;
    const moved = line.forced ? index : settle(lines, index, pageTop);
    const start = lines[moved];
    const end = lines[moved - 1];
    if (!start || !end) continue;
    const blank = room - (end.bottom - pageTop) + page.bottom;
    breaks.push({
      kind: start.kind,
      position: start.position,
      end: blank,
      height: blank + PAGE_GAP + page.top - (start.top - end.bottom),
      left: start.left,
    });
    pageTop = start.top;
    index = moved;
  }
  const last = lines.at(-1);
  breaks.push({
    kind: "block",
    position: -1,
    end: 0,
    height: room - ((last?.bottom ?? pageTop) - pageTop) + page.bottom,
    left: 0,
  });
  return breaks;
}

function spaceOf(pageBreak: PageBreak, width: number): HTMLElement {
  const { end, height, left } = pageBreak;
  const space = document.createElement(pageBreak.kind === "block" ? "div" : "span");
  space.className = "page-space";
  space.dataset.end = String(end);
  space.contentEditable = "false";
  space.style.height = `${height}px`;
  space.style.width = `${width}px`;
  space.style.marginLeft = `${-left}px`;
  const gapEnd = end + PAGE_GAP;
  space.style.background =
    `linear-gradient(var(--page-color) ${end}px, var(--surface-behind) ${end}px, ` +
    `var(--surface-behind) ${gapEnd}px, var(--page-color) ${gapEnd}px)`;
  if (pageBreak.kind !== "row") {
    space.classList.add("page-break");
    return space;
  }
  const row = document.createElement("tr");
  row.className = "page-break";
  const cell = row.appendChild(document.createElement("td"));
  cell.colSpan = 99;
  cell.className = "page-break-cell";
  cell.appendChild(space);
  return row;
}

function decorationOf(pageBreak: PageBreak, width: number): Decoration {
  const name = [pageBreak.kind, pageBreak.height.toFixed(1), pageBreak.end.toFixed(1)].join("-");
  return Decoration.widget(pageBreak.position, () => spaceOf(pageBreak, width), {
    key: name,
    side: -1,
    ignoreSelection: true,
    stopEvent: () => true,
  });
}

function sameBreaks(left: readonly PageBreak[], right: readonly PageBreak[]): boolean {
  return (
    left.length === right.length &&
    left.every((one, index) => {
      const other = right[index];
      return (
        other !== undefined &&
        other.position === one.position &&
        other.kind === one.kind &&
        Math.abs(other.height - one.height) < SAME_PIXELS
      );
    })
  );
}

/** Draws pages over the one flow of text, with spaces at the lines where a page ends. */
export function pages(
  page: PageShape,
  rules: BlockRules,
  paged: () => boolean,
): Plugin<DecorationSet> {
  let current: PageBreak[] = [];
  const layOut = (view: EditorView) => {
    if (view.isDestroyed) return;
    const root = view.dom;
    if (!paged()) {
      root.style.paddingBottom = "";
      if (current.length === 0) return;
      current = [];
      view.dispatch(
        view.state.tr.setMeta(pagesKey, DecorationSet.empty).setMeta("addToHistory", false),
      );
      return;
    }
    const zoom = Number(root.dataset.scale) || 1;
    const holes = [...root.querySelectorAll(".page-break")]
      .map((space) => space.getBoundingClientRect())
      .map(({ top, height }) => ({ top, height }));
    const lines = linesOf(view, rules, holes, zoom);
    const planned = planBreaks(lines, page, page.top);
    const tail = planned.pop();
    if (tail) root.style.paddingBottom = `${Math.max(0, tail.height)}px`;
    if (sameBreaks(planned, current)) return;
    current = planned;
    const width = root.getBoundingClientRect().width / zoom;
    const decorations = planned.map((pageBreak) => decorationOf(pageBreak, width));
    const set = DecorationSet.create(view.state.doc, decorations);
    view.dispatch(view.state.tr.setMeta(pagesKey, set).setMeta("addToHistory", false));
  };
  return new Plugin<DecorationSet>({
    key: pagesKey,
    state: {
      init: () => DecorationSet.empty,
      apply(transaction, set) {
        const next = transaction.getMeta(pagesKey) as DecorationSet | undefined;
        return next ?? set.map(transaction.mapping, transaction.doc);
      },
    },
    props: {
      decorations(state) {
        return pagesKey.getState(state);
      },
    },
    view(view) {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const schedule = () => {
        clearTimeout(timer);
        timer = setTimeout(() => requestAnimationFrame(() => layOut(view)), SETTLE_MS);
      };
      const resized = new ResizeObserver(schedule);
      resized.observe(view.dom);
      document.fonts.ready.then(schedule, console.error);
      schedule();
      return {
        update: (_view, previous) => {
          if (previous.doc !== view.state.doc) schedule();
        },
        destroy: () => {
          clearTimeout(timer);
          resized.disconnect();
        },
      };
    },
  });
}
