import {
  type Area,
  areaOf,
  batchOf,
  type Client,
  colorOf,
  type Editor,
  followLink,
  Outbox,
  opensLink,
  type Person,
  type Pictures,
  type Sent,
  Status,
  settled,
} from "@documents/client";
import type { Changed, ClosedReason, Peer } from "@documents/contract";
import {
  HeaderMoveRenderController,
  IRenderManagerService,
  SheetInterceptorService,
  UniverSheetsCorePreset,
} from "@univerjs/preset-sheets-core";
import sheetsCoreEnUS from "@univerjs/preset-sheets-core/locales/en-US";
import {
  createUniver,
  ICommandService,
  IUndoRedoService,
  LifecycleStages,
  LocaleType,
  mergeLocales,
  RANGE_TYPE,
} from "@univerjs/presets";
import { createElement, Pencil } from "lucide";
import "@univerjs/preset-sheets-core/lib/index.css";
import { language } from "@documents/words";
import { XlsxBar } from "./bar";
import {
  type Cell,
  type CellChange,
  type Change,
  covers,
  type Frozen,
  freezes,
  hides,
  type LinkTarget,
  linkAt,
  linksOf,
  type Merge,
  moveChange,
  moveFrozen,
  moveLine,
  moveLink,
  movePlace,
  movesLines,
  type Place,
  rangeOf,
  SheetFitting,
  type SheetLink,
  type Snapshot,
  touches,
  withoutSheet,
} from "./change";
import { SheetControl } from "./control";
import { fromUniver, toUniver, type UniverStyle } from "./look";
import "./xlsx.css";

interface UniverCell {
  v?: number | string | boolean | null;
  f?: string | null;
  /** Shared-formula ID; only the anchor cell carries `f`, while followers carry `si`. */
  si?: string | null;
  s?: string | UniverStyle | null;
  t?: number | null;
}

interface CellReader {
  look(row: number, column: number): CellChange;
  formula(row: number, column: number): string | null;
}

interface Range {
  startRow: number;
  endRow: number;
  startColumn: number;
  endColumn: number;
}

interface Mutation {
  id: string;
  params: object;
}

type LineInfo = Record<string, { h?: number; w?: number } | undefined> | undefined;

interface UniverFreeze {
  startRow: number;
  startColumn: number;
  ySplit: number;
  xSplit: number;
}

interface Selections {
  subUnitId: string;
  selections: { range: Range }[];
}

interface EditedCell {
  sheet: string;
  row: number;
  column: number;
}

interface Ordered {
  rev: number;
  change: Change;
}

interface Marked {
  peer: Peer;
  place: Place | null;
  rev: number;
}

// Univer does not ship a Greek locale, so Greek falls back to en-US.
const UNIVER_LOCALES: Readonly<Record<string, [LocaleType, () => Promise<{ default: object }>]>> = {
  de: [LocaleType.DE_DE, () => import("@univerjs/preset-sheets-core/locales/de-DE")],
  es: [LocaleType.ES_ES, () => import("@univerjs/preset-sheets-core/locales/es-ES")],
  fr: [LocaleType.FR_FR, () => import("@univerjs/preset-sheets-core/locales/fr-FR")],
  it: [LocaleType.IT_IT, () => import("@univerjs/preset-sheets-core/locales/it-IT")],
  ja: [LocaleType.JA_JP, () => import("@univerjs/preset-sheets-core/locales/ja-JP")],
  ko: [LocaleType.KO_KR, () => import("@univerjs/preset-sheets-core/locales/ko-KR")],
  "zh-Hans": [LocaleType.ZH_CN, () => import("@univerjs/preset-sheets-core/locales/zh-CN")],
};
const UNIVER_CHOICE = UNIVER_LOCALES[language()];
const UNIVER_LOCALE = UNIVER_CHOICE?.[0] ?? LocaleType.EN_US;
const UNIVER_WORDS = UNIVER_CHOICE ? (await UNIVER_CHOICE[1]()).default : sheetsCoreEnUS;

const WORKBOOK = "workbook";
const REMOTE = { fromCollab: true };
const STRING_TYPE = 1;
const BOOLEAN_TYPE = 3;
const SPARE_ROWS = 100;
const SPARE_COLUMNS = 10;
const MIN_ROWS = 200;
const MIN_COLUMNS = 26;
const PIXELS_PER_CHARACTER = 7;
const WIDTH_PADDING = 5;
const PIXELS_PER_POINT = 4 / 3;
// Univer renders asynchronously after a sheet becomes active.
const PAUSE_MS = 150;
const MOST_PAUSES = 60;
const DIRECTION_UP = 0;
const DIRECTION_LEFT = 2;

const SET_VALUES = "sheet.mutation.set-range-values";
const INSERT_ROW = "sheet.mutation.insert-row";
const REMOVE_ROW = "sheet.mutation.remove-rows";
const INSERT_COLUMN = "sheet.mutation.insert-col";
const REMOVE_COLUMN = "sheet.mutation.remove-col";
const RENAME_SHEET = "sheet.mutation.set-worksheet-name";
const INSERT_SHEET = "sheet.mutation.insert-sheet";
const REMOVE_SHEET = "sheet.mutation.remove-sheet";
const SET_WIDTH = "sheet.mutation.set-worksheet-col-width";
const SET_HEIGHT = "sheet.mutation.set-worksheet-row-height";
const ADD_MERGE = "sheet.mutation.add-worksheet-merge";
const REMOVE_MERGE = "sheet.mutation.remove-worksheet-merge";
const SET_NUMBER_FORMAT = "sheet.mutation.set.numfmt";
const REMOVE_NUMBER_FORMAT = "sheet.mutation.remove.numfmt";
const SET_ROW_HIDDEN = "sheet.mutation.set-row-hidden";
const SET_ROW_VISIBLE = "sheet.mutation.set-row-visible";
const SET_COLUMN_HIDDEN = "sheet.mutation.set-col-hidden";
const SET_COLUMN_VISIBLE = "sheet.mutation.set-col-visible";
const SET_FROZEN = "sheet.mutation.set-frozen";
const SET_AUTO_HEIGHT = "sheet.mutation.set-worksheet-row-is-auto-height";
const MOVE_RANGE = "sheet.mutation.move-range";
const SET_SELECTIONS = "sheet.operation.set-selections";
const SET_SCROLL = "sheet.operation.set-scroll";
const SET_ACTIVE_SHEET = "sheet.operation.set-worksheet-active";

// Hide Univer's range protection menu; shared files have no per-range permissions.
const MENU = { "sheet.contextMenu.permission": { hidden: true } };
const HIDDEN = 1;
const CUSTOM_HEIGHT = 0;
const AUTO_HEIGHT = 1;
const NOTHING_FROZEN: Frozen = { top: 0, left: 0, rows: 0, columns: 0 };

const STRUCTURE = {
  rows: {
    insert: INSERT_ROW,
    remove: REMOVE_ROW,
    insertCommand: "sheet.command.insert-row",
    removeCommand: "sheet.command.remove-row",
    direction: DIRECTION_UP,
    hide: SET_ROW_HIDDEN,
    show: SET_ROW_VISIBLE,
  },
  columns: {
    insert: INSERT_COLUMN,
    remove: REMOVE_COLUMN,
    insertCommand: "sheet.command.insert-col",
    removeCommand: "sheet.command.remove-col",
    direction: DIRECTION_LEFT,
    hide: SET_COLUMN_HIDDEN,
    show: SET_COLUMN_VISIBLE,
  },
} as const;

function cellOf(cell: Cell): UniverCell {
  const shown: UniverCell = {};
  const value = cell.value;
  if (typeof value === "boolean") {
    shown.v = value ? 1 : 0;
    shown.t = BOOLEAN_TYPE;
  } else if (typeof value === "object") {
    shown.v = value.error;
    shown.t = STRING_TYPE;
  } else if (typeof value === "string") {
    shown.v = value;
    shown.t = STRING_TYPE;
  } else if (value !== undefined) {
    shown.v = value;
  }
  if (cell.formula) shown.f = `=${cell.formula}`;
  if (cell.style !== undefined) shown.s = String(cell.style);
  return shown;
}

function pixelWidth(characters: number): number {
  return Math.round(characters * PIXELS_PER_CHARACTER + WIDTH_PADDING);
}

function characterWidth(pixels: number): number {
  return (pixels - WIDTH_PADDING) / PIXELS_PER_CHARACTER;
}

// Univer encodes freeze panes by first scrollable index (-1 when unfrozen) plus split count.
function frozenOf({ startRow, startColumn, ySplit, xSplit }: UniverFreeze): Frozen {
  return {
    top: ySplit > 0 ? Math.max(startRow - ySplit, 0) : 0,
    left: xSplit > 0 ? Math.max(startColumn - xSplit, 0) : 0,
    rows: ySplit,
    columns: xSplit,
  };
}

function univerFreeze({ top, left, rows, columns }: Frozen): UniverFreeze {
  return {
    startRow: rows > 0 ? top + rows : -1,
    startColumn: columns > 0 ? left + columns : -1,
    ySplit: rows,
    xSplit: columns,
  };
}

function isFrozen({ rows, columns }: Frozen): boolean {
  return rows > 0 || columns > 0;
}

function lineData(
  sizes: Record<string, number>,
  size: (value: number) => object,
  hidden: readonly number[],
): Record<string, object> {
  const data: Record<string, object> = Object.fromEntries(
    Object.entries(sizes).map(([line, value]) => [line, size(value)]),
  );
  for (const line of hidden) data[line] = { ...data[line], hd: HIDDEN };
  return data;
}

// Disable row/column drag-reordering because the document protocol has no move-line operation.
function keepLinesInPlace(renders: IRenderManagerService): void {
  const moves = renders.getRenderUnitById(WORKBOOK)?.with(HeaderMoveRenderController);
  moves?.interceptor.intercept(
    moves.interceptor.getInterceptPoints().HEADER_MOVE_PERMISSION_CHECK,
    {
      priority: 1,
      handler: () => false,
    },
  );
}

function lineRange(kind: "rows" | "columns", line: number): Range {
  return kind === "rows"
    ? { startRow: line, endRow: line, startColumn: 0, endColumn: 0 }
    : { startRow: 0, endRow: 0, startColumn: line, endColumn: line };
}

function workbookData(snapshot: Snapshot) {
  const styles: Record<string, UniverStyle> = {};
  for (const [index, look] of snapshot.looks.entries()) styles[String(index)] = toUniver(look);
  const sheets: Record<string, unknown> = {};
  for (const sheet of snapshot.sheets) {
    const cellData: Record<string, Record<string, UniverCell>> = {};
    for (const [row, cells] of Object.entries(sheet.cells)) {
      cellData[row] = Object.fromEntries(
        Object.entries(cells).map(([column, cell]) => [column, cellOf(cell)]),
      );
    }
    sheets[sheet.id] = {
      id: sheet.id,
      name: sheet.name,
      rowCount: Math.max(sheet.rows + SPARE_ROWS, MIN_ROWS),
      columnCount: Math.max(sheet.columns + SPARE_COLUMNS, MIN_COLUMNS),
      cellData,
      mergeData: sheet.merges.map(mergeRange),
      columnData: lineData(
        sheet.widths,
        (width) => ({ w: pixelWidth(width) }),
        sheet.hiddenColumns,
      ),
      rowData: lineData(
        sheet.heights,
        (height) => ({ h: height * PIXELS_PER_POINT, ia: CUSTOM_HEIGHT }),
        sheet.hiddenRows,
      ),
      freeze: univerFreeze(sheet.frozen),
    };
  }
  return {
    id: WORKBOOK,
    name: WORKBOOK,
    appVersion: "1",
    locale: LocaleType.EN_US,
    sheetOrder: snapshot.sheets.map((sheet) => sheet.id),
    styles,
    sheets,
  };
}

function mergeRange([startRow, startColumn, endRow, endColumn]: Merge): Range {
  return { startRow, startColumn, endRow, endColumn };
}

function mergeOf(range: Range): Merge {
  return [range.startRow, range.startColumn, range.endRow, range.endColumn];
}

function valueOf(cell: UniverCell | null | undefined): CellChange["value"] {
  const value = cell?.v;
  if (value === undefined || value === null || value === "") return null;
  return cell?.t === BOOLEAN_TYPE ? Boolean(value) : value;
}

// Range moves replace destination cells completely, clearing omitted value, formula, or style.
function wholeCell(
  row: number,
  column: number,
  cell: UniverCell | null | undefined,
  reader: CellReader,
): CellChange {
  return {
    ...reader.look(row, column),
    value: valueOf(cell) ?? null,
    formula: reader.formula(row, column),
  };
}

// Univer emits partial style patches in `s`, so read the cell's full resolved style instead.
function cellChange(
  row: number,
  column: number,
  cell: UniverCell | null,
  reader: CellReader,
): CellChange {
  const change: CellChange = cell && "s" in cell ? reader.look(row, column) : { row, column };
  if (cell === null || "v" in cell) change.value = valueOf(cell) ?? null;
  if (cell === null || "f" in cell || "si" in cell) change.formula = reader.formula(row, column);
  if (cell === null) change.style = null;
  return change;
}

function cellsOf(
  rows: Record<string, Record<string, UniverCell | null>>,
  reader: CellReader,
): CellChange[] {
  return Object.entries(rows).flatMap(([row, columns]) =>
    Object.entries(columns).map(([column, cell]) =>
      cellChange(Number(row), Number(column), cell, reader),
    ),
  );
}

function perLine<V>(
  ranges: readonly Range[],
  kind: "rows" | "columns",
  value: (line: number) => V | undefined,
): Record<string, V> {
  const [start, end] =
    kind === "rows" ? (["startRow", "endRow"] as const) : (["startColumn", "endColumn"] as const);
  const out: Record<string, V> = {};
  for (const range of ranges) {
    for (let line = range[start]; line <= range[end]; line++) {
      const found = value(line);
      if (found !== undefined) out[line] = found;
    }
  }
  return out;
}

function sized(
  size: number | Record<string, number | null | undefined>,
  convert: (pixels: number) => number,
): (line: number) => number | undefined {
  return (line) => {
    const pixels = typeof size === "number" ? size : size[line];
    return typeof pixels === "number" ? convert(pixels) : undefined;
  };
}

class XlsxEditor implements Editor, Pictures {
  private readonly root: HTMLElement;
  private readonly client: Client;
  private readonly canvas: HTMLElement;
  private readonly marks: HTMLElement;
  private readonly status: Status;
  private readonly bar: XlsxBar;
  private readonly outbox = new Outbox<Change>();
  private univer: ReturnType<typeof createUniver> | undefined;
  // Univer does not terminate a Worker passed in externally, so track it for explicit cleanup.
  private worker: Worker | undefined;
  private commands: ICommandService | undefined;
  private interceptor: SheetInterceptorService | undefined;
  private undo: IUndoRedoService | undefined;
  private control: SheetControl | undefined;
  private marked: readonly Marked[] = [];
  private remote = false;
  private fileLooks = 0;
  private rev = 0;
  private here: Place | null = null;
  private placeWaits = false;
  private editing: EditedCell | null = null;
  private held: Ordered[] = [];
  private links: SheetLink[] = [];
  private canEdit = false;
  // Modifier keys from the last pointerdown, since Univer's CellClicked event omits them.
  private pressed: Pick<MouseEvent, "metaKey" | "ctrlKey"> = { metaKey: false, ctrlKey: false };

  constructor(root: HTMLElement, client: Client) {
    this.root = root;
    this.client = client;
    this.status = new Status(root);
    this.bar = new XlsxBar(root, this.status.element);
    root.append(this.bar.element);
    const holder = root.appendChild(document.createElement("div"));
    holder.className = "xlsx-holder";
    this.canvas = holder.appendChild(document.createElement("div"));
    this.canvas.className = "xlsx-canvas";
    this.canvas.addEventListener(
      "pointerdown",
      ({ metaKey, ctrlKey }) => {
        this.pressed = { metaKey, ctrlKey };
      },
      { capture: true },
    );
    this.marks = holder.appendChild(document.createElement("div"));
    this.marks.className = "xlsx-marks";
    new MutationObserver(() => this.univer?.univerAPI.toggleDarkMode(this.dark())).observe(root, {
      attributeFilter: ["data-scheme", "data-dark-pages"],
    });
  }

  private dark(): boolean {
    if (!this.root.hasAttribute("data-dark-pages")) return false;
    const scheme = this.root.dataset.scheme;
    return scheme ? scheme === "dark" : window.matchMedia("(prefers-color-scheme: dark)").matches;
  }

  load(raw: unknown, person: Person, sent: Sent): void {
    const snapshot = raw as Snapshot;
    const dropped = this.outbox.loaded(sent);
    // Queued edits may use row/column indices shifted by the dropped batch, so drop them too.
    if (dropped) this.outbox.clear();
    const fitting = new SheetFitting(snapshot.sheets.map(({ id }) => id));
    this.outbox.move((change) => fitting.fit(change));
    if (dropped || fitting.lost) this.status.kept("");
    this.univer?.univer.dispose();
    this.worker?.terminate();
    this.worker = new Worker(new URL("./formula-worker.ts", import.meta.url), { type: "module" });
    const made = createUniver({
      darkMode: this.dark(),
      locale: UNIVER_LOCALE,
      locales: { [UNIVER_LOCALE]: mergeLocales(UNIVER_WORDS) },
      presets: [
        UniverSheetsCorePreset({
          container: this.canvas,
          workerURL: this.worker,
          toolbar: false,
          footer: false,
          menu: MENU,
          sheets: { enableFillHandle: person.canEdit },
        }),
      ],
    });
    this.univer = made;
    this.editing = null;
    this.held = [];
    this.marked = [];
    this.fileLooks = snapshot.looks.length;
    this.links = snapshot.sheets.flatMap(linksOf);
    this.canEdit = person.canEdit;
    made.univerAPI.createWorkbook(workbookData(snapshot) as never);
    const injector = made.univer.__getInjector();
    this.commands = injector.get(ICommandService);
    this.interceptor = injector.get(SheetInterceptorService);
    this.undo = injector.get(IUndoRedoService);
    this.control = new SheetControl(made.univerAPI, this.commands, this.undo);
    this.commands.onMutationExecutedForCollab((info, options) => {
      if (this.remote || options?.onlyLocal || options?.fromCollab) return;
      const change = this.changeOf(info.id, (info.params ?? {}) as Record<string, unknown>);
      if (change) {
        this.outbox.add(change);
        this.moveMarks(change, Number.POSITIVE_INFINITY);
        this.moveLinks(change);
        this.client.pull();
      }
    });
    this.commands.onCommandExecuted((info) => {
      if (info.id === SET_SELECTIONS) this.placed(info.params as Selections);
      else if (info.id === SET_SCROLL || info.id === SET_ACTIVE_SHEET) this.drawMarks();
      this.bar.update();
    });
    made.univerAPI.addEvent(made.univerAPI.Event.LifeCycleChanged, ({ stage }) => {
      if (stage !== LifecycleStages.Rendered) return;
      keepLinesInPlace(injector.get(IRenderManagerService));
      this.drawMarks();
    });
    made.univerAPI.addEvent(made.univerAPI.Event.CellClicked, ({ worksheet, row, column }) => {
      const link = linkAt(this.links, worksheet.getSheetId(), row, column);
      if (link && opensLink(this.pressed, this.canEdit)) this.followCell(link.to);
    });
    made.univerAPI.addEvent(made.univerAPI.Event.CellHover, ({ worksheet, row, column }) => {
      const link = linkAt(this.links, worksheet.getSheetId(), row, column);
      this.canvas.title = link ? ("href" in link.to ? link.to.href : link.to.location) : "";
    });
    made.univerAPI.addEvent(made.univerAPI.Event.SheetEditStarted, ({ worksheet, row, column }) => {
      this.editing = { sheet: worksheet.getSheetId(), row, column };
      this.sendPlace();
    });
    // Univer commits the cell mutation before SheetEditEnded, so it is already in the outbox.
    made.univerAPI.addEvent(made.univerAPI.Event.SheetEditEnded, () => {
      this.editing = null;
      for (const ordered of this.held.splice(0)) this.showOrdered(ordered);
      this.sendPlace();
      this.client.pull();
    });
    for (const change of this.outbox.unanswered()) {
      this.show(change, true);
      this.moveLinks(change);
    }
    if (!person.canEdit) {
      made.univerAPI.getActiveWorkbook()?.setEditable(false);
    }
    this.bar.show(this.control, person.canEdit);
  }

  /**
   * Preserves the original style index for unchanged file styles so properties not modeled by Look
   * (such as borders) survive round-tripping.
   */
  private lookOf(sheet: string, row: number, column: number): CellChange {
    const range = this.univer?.univerAPI
      .getActiveWorkbook()
      ?.getSheetBySheetId(sheet)
      ?.getRange(row, column);
    const style = range?.getCellData()?.s;
    if (typeof style === "string" && /^\d+$/.test(style) && Number(style) < this.fileLooks) {
      return { row, column, style: Number(style) };
    }
    const look = fromUniver((range?.getCellStyleData("cell") ?? {}) as UniverStyle);
    return Object.keys(look).length > 0 ? { row, column, look } : { row, column, style: null };
  }

  private reader(sheet: string): CellReader {
    const worksheet = () => this.univer?.univerAPI.getActiveWorkbook()?.getSheetBySheetId(sheet);
    return {
      look: (row, column) => this.lookOf(sheet, row, column),
      // Shared-formula follower cells have no raw `f`; ask Univer for the expanded formula.
      formula: (row, column) =>
        worksheet()?.getRange(row, column).getFormula().replace(/^=/, "") || null,
    };
  }

  private restyled(sheet: string, ranges: readonly Range[]): Change | null {
    const cells: CellChange[] = [];
    for (const range of ranges) {
      for (let row = range.startRow; row <= range.endRow; row++) {
        for (let column = range.startColumn; column <= range.endColumn; column++) {
          cells.push(this.lookOf(sheet, row, column));
        }
      }
    }
    return cells.length > 0 ? { kind: "cells", sheet, cells } : null;
  }

  private placed({ subUnitId, selections }: Selections): void {
    const range = selections[0]?.range;
    if (range) {
      this.here = {
        sheet: subUnitId,
        row: range.startRow,
        column: range.startColumn,
        endRow: range.endRow,
        endColumn: range.endColumn,
      };
      this.sendPlace();
    }
  }

  // Selection coordinates must match the client's confirmed rev, so defer reporting while remote
  // changes are held or local row/column shifts await acknowledgement.
  private sendPlace(): void {
    if (!this.here) return;
    this.placeWaits = this.held.length > 0 || this.outbox.unanswered().some(movesLines);
    if (!this.placeWaits)
      this.client.place(this.editing ? { ...this.here, editing: true } : this.here);
  }

  private changeOf(id: string, params: Record<string, unknown>): Change | null {
    const sheet = params.subUnitId as string;
    const range = params.range as Range;
    const reader = this.reader(sheet);
    switch (id) {
      case SET_VALUES: {
        const rows = (params.cellValue ?? {}) as Record<string, Record<string, UniverCell | null>>;
        const cells = cellsOf(rows, reader);
        return cells.length > 0 ? { kind: "cells", sheet, cells } : null;
      }
      case SET_NUMBER_FORMAT: {
        const values = (params.values ?? {}) as Record<string, { ranges: Range[] }>;
        return this.restyled(
          sheet,
          Object.values(values).flatMap((value) => value.ranges),
        );
      }
      case REMOVE_NUMBER_FORMAT:
        return this.restyled(sheet, params.ranges as Range[]);
      case INSERT_ROW:
        return this.inserted(
          "rows",
          sheet,
          range.startRow,
          range.endRow - range.startRow + 1,
          params.rowInfo as LineInfo,
        );
      case REMOVE_ROW:
        return this.lines("rows", sheet, range.startRow, range.startRow - range.endRow - 1);
      case INSERT_COLUMN:
        return this.inserted(
          "columns",
          sheet,
          range.startColumn,
          range.endColumn - range.startColumn + 1,
          params.colInfo as LineInfo,
        );
      case REMOVE_COLUMN:
        return this.lines(
          "columns",
          sheet,
          range.startColumn,
          range.startColumn - range.endColumn - 1,
        );
      case RENAME_SHEET:
        return { kind: "renameSheet", sheet, name: params.name as string };
      case INSERT_SHEET: {
        const made = params.sheet as {
          id: string;
          name: string;
          cellData?: Record<string, Record<string, UniverCell | null>>;
        };
        const added: Change = { kind: "addSheet", id: made.id, name: made.name };
        const cells = cellsOf(made.cellData ?? {}, this.reader(made.id));
        return cells.length > 0
          ? { kind: "group", changes: [added, { kind: "cells", sheet: made.id, cells }] }
          : added;
      }
      case REMOVE_SHEET:
        return { kind: "removeSheet", sheet };
      case SET_WIDTH:
        return {
          kind: "size",
          sheet,
          widths: perLine(
            params.ranges as Range[],
            "columns",
            sized(params.colWidth as number, characterWidth),
          ),
        };
      case SET_HEIGHT:
        return {
          kind: "size",
          sheet,
          heights: perLine(
            params.ranges as Range[],
            "rows",
            sized(params.rowHeight as number, (pixels) => pixels / PIXELS_PER_POINT),
          ),
        };
      case ADD_MERGE:
        return { kind: "merges", sheet, add: (params.ranges as Range[]).map(mergeOf) };
      case REMOVE_MERGE:
        return { kind: "merges", sheet, remove: (params.ranges as Range[]).map(mergeOf) };
      case SET_ROW_HIDDEN:
      case SET_ROW_VISIBLE:
        return {
          kind: "hidden",
          sheet,
          rows: perLine(params.ranges as Range[], "rows", () => id === SET_ROW_HIDDEN),
        };
      case SET_COLUMN_HIDDEN:
      case SET_COLUMN_VISIBLE:
        return {
          kind: "hidden",
          sheet,
          columns: perLine(params.ranges as Range[], "columns", () => id === SET_COLUMN_HIDDEN),
        };
      case SET_FROZEN:
        return { kind: "freeze", sheet, ...frozenOf(params as unknown as UniverFreeze) };
      case SET_AUTO_HEIGHT: {
        const fits = params.autoHeightInfo as number | Record<string, number | undefined>;
        const worksheet = this.univer?.univerAPI.getActiveWorkbook()?.getSheetBySheetId(sheet);
        return {
          kind: "size",
          sheet,
          heights: perLine(params.ranges as Range[], "rows", (row) =>
            (typeof fits === "number" ? fits : fits[row]) === CUSTOM_HEIGHT
              ? (worksheet?.getRowHeight(row) ?? 0) / PIXELS_PER_POINT
              : null,
          ),
        };
      }
      case MOVE_RANGE: {
        const sides = [params.from, params.to] as {
          subUnitId: string;
          value: Record<string, Record<string, UniverCell | null>>;
        }[];
        const changes = sides.flatMap<Change>(({ subUnitId, value }) => {
          const reader = this.reader(subUnitId);
          const cells = Object.entries(value ?? {}).flatMap(([row, columns]) =>
            Object.entries(columns).map(([column, cell]) =>
              wholeCell(Number(row), Number(column), cell, reader),
            ),
          );
          return cells.length > 0 ? [{ kind: "cells", sheet: subUnitId, cells }] : [];
        });
        return changes.length > 0 ? { kind: "group", changes } : null;
      }
      default:
        return null;
    }
  }

  /**
   * Univer copies the adjacent row/column size onto newly inserted lines. Because the server
   * stores a copied row height as explicit (non-auto), mirror that locally via showSizes.
   */
  private inserted(
    kind: "rows" | "columns",
    sheet: string,
    at: number,
    count: number,
    info: LineInfo,
  ): Change {
    const lines = this.lines(kind, sheet, at, count);
    const sizes: Record<string, number> = {};
    for (const [offset, line] of Object.entries(info ?? {})) {
      const pixels = kind === "rows" ? line?.h : line?.w;
      if (typeof pixels !== "number") continue;
      sizes[at + Number(offset)] =
        kind === "rows" ? pixels / PIXELS_PER_POINT : characterWidth(pixels);
    }
    if (Object.keys(sizes).length === 0) return lines;
    if (kind === "columns") {
      return { kind: "group", changes: [lines, { kind: "size", sheet, widths: sizes }] };
    }
    const heights: Change = { kind: "size", sheet, heights: sizes };
    this.showSizes(heights);
    return { kind: "group", changes: [lines, heights] };
  }

  private lines(kind: "rows" | "columns", sheet: string, at: number, count: number): Change {
    return { kind, sheet, at, count };
  }

  // Pause outgoing batches while remote changes are held so edits are not sent against a stale
  // grid state.
  batch(): unknown | null {
    return this.held.length > 0 ? null : batchOf(this.outbox.next());
  }

  private run(mutations: Mutation[]): void {
    this.remote = true;
    try {
      for (const { id, params } of mutations) this.commands?.syncExecuteCommand(id, params, REMOTE);
    } finally {
      this.remote = false;
    }
  }

  private showCells(sheet: string, cells: CellChange[], own: boolean): void {
    const cellValue: Record<number, Record<number, UniverCell>> = {};
    const unanswered = this.outbox.unanswered();
    for (const cell of cells) {
      if (!own && unanswered.some((change) => covers(change, sheet, cell.row, cell.column))) {
        continue;
      }
      const shown: UniverCell = {};
      if (cell.value === null) Object.assign(shown, { v: null, t: null });
      else if (cell.value !== undefined) Object.assign(shown, cellOf({ value: cell.value }));
      if (cell.formula !== undefined) shown.f = cell.formula ? `=${cell.formula}` : null;
      if (cell.look) shown.s = toUniver(cell.look);
      else if (cell.style !== undefined) {
        shown.s = cell.style === null ? null : String(cell.style);
      }
      cellValue[cell.row] ??= {};
      (cellValue[cell.row] as Record<number, UniverCell>)[cell.column] = shown;
    }
    this.run([
      {
        id: SET_VALUES,
        params: { unitId: WORKBOOK, subUnitId: sheet, cellValue, isOverrideStyle: true },
      },
    ]);
  }

  /**
   * Runs row/column inserts and removals through Univer's command interceptor with full-line
   * ranges so Univer rewrites formulas and merges, then clears local undo history.
   */
  private showLines(
    kind: "rows" | "columns",
    sheet: string,
    at: number,
    count: number,
    own: boolean,
  ): void {
    if (!own) this.outbox.move((change) => moveChange(change, sheet, kind, at, count));
    const worksheet = this.univer?.univerAPI.getActiveWorkbook()?.getSheetBySheetId(sheet);
    const last = at + Math.abs(count) - 1;
    const range =
      kind === "rows"
        ? {
            startRow: at,
            endRow: last,
            startColumn: 0,
            endColumn: Math.max((worksheet?.getMaxColumns() ?? 1) - 1, 0),
            rangeType: RANGE_TYPE.ROW,
          }
        : {
            startRow: 0,
            endRow: Math.max((worksheet?.getMaxRows() ?? 1) - 1, 0),
            startColumn: at,
            endColumn: last,
            rangeType: RANGE_TYPE.COLUMN,
          };
    const names = STRUCTURE[kind];
    const params = {
      unitId: WORKBOOK,
      subUnitId: sheet,
      range,
      direction: names.direction,
    };
    const inserts = count > 0;
    const panes = this.frozenOf(sheet);
    const shown =
      this.univer?.univerAPI.getActiveWorkbook()?.getActiveSheet().getSheetId() ?? sheet;
    const shownPanes = this.frozenOf(shown);
    const intercepted = this.interceptor?.onCommandExecute({
      id: inserts ? names.insertCommand : names.removeCommand,
      params,
    });
    // Univer's interceptor shifts freeze panes on the active sheet even when editing a different
    // sheet, so drop its SET_FROZEN mutations and update panes explicitly below.
    const withoutPanes = (mutations: Mutation[] = []) =>
      mutations.filter(({ id }) => id !== SET_FROZEN);
    this.run([
      ...withoutPanes(intercepted?.preRedos),
      {
        id: inserts ? names.insert : names.remove,
        params: { unitId: WORKBOOK, subUnitId: sheet, range },
      },
      ...withoutPanes(intercepted?.redos),
    ]);
    if (isFrozen(panes)) this.freeze(sheet, moveFrozen(panes, kind, at, count));
    if (shown !== sheet && isFrozen(shownPanes)) this.freeze(shown, shownPanes);
    this.undo?.clearUndoRedo(WORKBOOK);
    this.follow(kind, sheet, at, count);
  }

  private frozenOf(sheet: string): Frozen {
    const freeze = this.univer?.univerAPI
      .getActiveWorkbook()
      ?.getSheetBySheetId(sheet)
      ?.getFreeze();
    return freeze ? frozenOf(freeze) : NOTHING_FROZEN;
  }

  private freeze(sheet: string, frozen: Frozen): void {
    this.run([
      {
        id: SET_FROZEN,
        params: { unitId: WORKBOOK, subUnitId: sheet, ...univerFreeze(frozen), resetScroll: false },
      },
    ]);
  }

  private showHidden(change: Extract<Change, { kind: "hidden" }>, own: boolean): void {
    const unanswered = own ? [] : this.outbox.unanswered();
    const mutations: Mutation[] = [];
    for (const kind of ["rows", "columns"] as const) {
      const lines = Object.entries(change[kind] ?? {}).filter(
        ([line]) => !unanswered.some((waiting) => hides(waiting, change.sheet, kind, Number(line))),
      );
      for (const hide of [true, false]) {
        const ranges = lines
          .filter(([, hidden]) => hidden === hide)
          .map(([line]) => lineRange(kind, Number(line)));
        if (ranges.length === 0) continue;
        mutations.push({
          id: hide ? STRUCTURE[kind].hide : STRUCTURE[kind].show,
          params: { unitId: WORKBOOK, subUnitId: change.sheet, ranges },
        });
      }
    }
    this.run(mutations);
  }

  private follow(kind: "rows" | "columns", sheet: string, at: number, count: number): void {
    const worksheet = this.univer?.univerAPI.getActiveWorkbook()?.getActiveSheet();
    const range = worksheet?.getActiveRange();
    if (!worksheet || !range || worksheet.getSheetId() !== sheet) return;
    const moved = (line: number, lines: "rows" | "columns") =>
      lines === kind ? (moveLine(line, at, count) ?? at) : line;
    const row = moved(range.getRow(), "rows");
    const column = moved(range.getColumn(), "columns");
    if (row !== range.getRow() || column !== range.getColumn()) {
      worksheet.getRange(row, column, range.getHeight(), range.getWidth()).activate();
    }
  }

  // When own is true, replays local unacknowledged edits onto a reloaded snapshot without
  // filtering them against the outbox.
  private show(change: Change, own = false): void {
    if (change.kind === "removeSheet" && !own) {
      this.outbox.move((waiting) => withoutSheet(waiting, change.sheet));
    }
    // Skip changes for a sheet already removed locally by an unacknowledged removeSheet.
    if ("sheet" in change && !this.holds(change.sheet)) return;
    const unit = { unitId: WORKBOOK };
    switch (change.kind) {
      case "cells":
        this.showCells(change.sheet, change.cells, own);
        break;
      case "rows":
      case "columns":
        this.showLines(change.kind, change.sheet, change.at, change.count, own);
        break;
      case "renameSheet":
        this.run([
          { id: RENAME_SHEET, params: { ...unit, subUnitId: change.sheet, name: change.name } },
        ]);
        break;
      case "addSheet": {
        const count = this.univer?.univerAPI.getActiveWorkbook()?.getNumSheets() ?? 0;
        this.run([
          {
            id: INSERT_SHEET,
            params: {
              ...unit,
              index: count,
              sheet: {
                id: change.id,
                name: change.name,
                rowCount: MIN_ROWS,
                columnCount: MIN_COLUMNS,
                cellData: {},
              },
            },
          },
        ]);
        break;
      }
      case "removeSheet":
        this.run([
          {
            id: REMOVE_SHEET,
            params: { ...unit, subUnitId: change.sheet, subUnitName: change.sheet },
          },
        ]);
        break;
      case "size":
        this.showSizes(change);
        break;
      case "merges":
        this.run([
          ...(change.add?.length
            ? [
                {
                  id: ADD_MERGE,
                  params: {
                    ...unit,
                    subUnitId: change.sheet,
                    ranges: change.add.map(mergeRange),
                  },
                },
              ]
            : []),
          ...(change.remove?.length
            ? [
                {
                  id: REMOVE_MERGE,
                  params: {
                    ...unit,
                    subUnitId: change.sheet,
                    ranges: change.remove.map(mergeRange),
                  },
                },
              ]
            : []),
        ]);
        break;
      case "hidden":
        this.showHidden(change, own);
        break;
      case "freeze":
        if (own || !this.outbox.unanswered().some((waiting) => freezes(waiting, change.sheet))) {
          this.freeze(change.sheet, change);
        }
        break;
      case "group":
        for (const inner of change.changes) this.show(inner, own);
        break;
    }
  }

  private holds(sheet: string): boolean {
    return Boolean(this.univer?.univerAPI.getActiveWorkbook()?.getSheetBySheetId(sheet));
  }

  private showSizes(change: Extract<Change, { kind: "size" }>): void {
    const unit = { unitId: WORKBOOK, subUnitId: change.sheet };
    for (const [column, width] of Object.entries(change.widths ?? {})) {
      const line = Number(column);
      this.run([
        {
          id: SET_WIDTH,
          params: {
            ...unit,
            ranges: [lineRange("columns", line)],
            colWidth: pixelWidth(width),
          },
        },
      ]);
    }
    const columns = this.univer?.univerAPI
      .getActiveWorkbook()
      ?.getSheetBySheetId(change.sheet)
      ?.getMaxColumns();
    for (const [row, height] of Object.entries(change.heights ?? {})) {
      const ranges = [
        { ...lineRange("rows", Number(row)), endColumn: Math.max((columns ?? 1) - 1, 0) },
      ];
      if (height === null) {
        this.run([
          { id: SET_AUTO_HEIGHT, params: { ...unit, ranges, autoHeightInfo: AUTO_HEIGHT } },
        ]);
        // Auto-fit rows carry no explicit height from the server, so re-measure locally.
        this.run(this.interceptor?.generateMutationsOfAutoHeight({ ...unit, ranges }).redos ?? []);
      } else {
        this.run([
          { id: SET_HEIGHT, params: { ...unit, ranges, rowHeight: height * PIXELS_PER_POINT } },
          { id: SET_AUTO_HEIGHT, params: { ...unit, ranges, autoHeightInfo: CUSTOM_HEIGHT } },
        ]);
      }
    }
  }

  apply(changed: Changed, own: boolean): void {
    if (own) {
      this.outbox.answered();
      if (this.placeWaits) this.sendPlace();
    } else this.receive({ rev: changed.rev, change: changed.body as Change });
    this.rev = changed.rev;
  }

  /**
   * Buffers incoming changes in order while the inline cell editor is open and a change touches
   * the edited cell (or shifts its row/column), so in-progress typing is not clobbered: once
   * editing ends, the held changes apply in sequence, the typed value wins, and the edit moves
   * with its line.
   */
  private receive(ordered: Ordered): void {
    const editing = this.editing;
    const { change } = ordered;
    if (
      this.held.length > 0 ||
      (editing && touches(change, editing.sheet, editing.row, editing.column))
    ) {
      this.held.push(ordered);
    } else {
      this.showOrdered(ordered);
    }
  }

  private showOrdered({ rev, change }: Ordered): void {
    this.show(change);
    this.moveMarks(change, rev);
    this.moveLinks(change);
  }

  private moveLinks(change: Change): void {
    if (movesLines(change) || change.kind === "removeSheet") {
      this.links = this.links.flatMap((link) => moveLink(link, change) ?? []);
    }
  }

  private followCell(to: LinkTarget): void {
    if ("href" in to) {
      followLink(to.href);
      return;
    }
    const split = to.location.lastIndexOf("!");
    const name =
      split < 0
        ? ""
        : to.location
            .slice(0, split)
            .replace(/^'(.*)'$/, "$1")
            .replaceAll("''", "'");
    const range = rangeOf(split < 0 ? to.location : to.location.slice(split + 1));
    const workbook = this.univer?.univerAPI.getActiveWorkbook();
    const sheet = name ? workbook?.getSheetByName(name) : workbook?.getActiveSheet();
    if (!sheet || !range) return;
    sheet.activate();
    sheet.getRange(range.row, range.column).activate();
  }

  private moveMarks(change: Change, rev: number): void {
    this.marked = this.marked.map((marked) =>
      marked.place && marked.rev < rev
        ? { ...marked, place: movePlace(marked.place, change) }
        : marked,
    );
    this.drawMarks();
  }

  /**
   * Updates peer selections. New peer entries arrive at the client's confirmed rev and are shifted
   * across unacknowledged local edits; existing entries retain their locally tracked positions.
   */
  peers(peers: readonly Peer[]): void {
    this.marked = peers.map(
      (peer) =>
        this.marked.find((marked) => marked.peer === peer) ?? {
          peer,
          place: peer.place
            ? this.outbox
                .unanswered()
                .reduce((moved, change) => movePlace(moved, change), peer.place as Place)
            : null,
          rev: this.rev,
        },
    );
    this.status.peers(peers);
    this.drawMarks();
  }

  /**
   * Renders peer selection overlays above Univer's canvas, since Univer has no built-in
   * collaborator overlay layer and only exposes cell rects once rendered.
   */
  private drawMarks(): void {
    this.marks.replaceChildren();
    const api = this.univer?.univerAPI;
    if (!api || api.getCurrentLifecycleStage() < LifecycleStages.Rendered) return;
    const sheet = api.getActiveWorkbook()?.getActiveSheet();
    const grid = [...this.canvas.querySelectorAll("canvas")]
      .map((canvas) => canvas.getBoundingClientRect())
      .sort((left, right) => right.height - left.height)[0];
    if (!sheet || !grid) return;
    const frame = this.marks.getBoundingClientRect();
    const scroll = sheet.getScrollState();
    const start = sheet
      .getRange(scroll.sheetViewStartRow, scroll.sheetViewStartColumn)
      .getCellRect();
    const home = sheet.getRange(0, 0).getCellRect();
    const shiftX = grid.left - frame.left - (start.left - home.left + scroll.offsetX);
    const shiftY = grid.top - frame.top - (start.top - home.top + scroll.offsetY);
    for (const { peer, place } of this.marked) {
      if (place?.sheet !== sheet.getSheetId() || place.row === undefined) continue;
      const first = sheet.getRange(place.row, place.column ?? 0).getCellRect();
      const last = sheet
        .getRange(place.endRow ?? place.row, place.endColumn ?? place.column ?? 0)
        .getCellRect();
      const mark = this.marks.appendChild(document.createElement("div"));
      mark.className = "xlsx-mark";
      mark.style.borderColor = colorOf(peer.person);
      mark.style.left = `${first.left + shiftX}px`;
      mark.style.top = `${first.top + shiftY}px`;
      mark.style.width = `${last.right - first.left}px`;
      mark.style.height = `${last.bottom - first.top}px`;
      const name = mark.appendChild(document.createElement("span"));
      if (place.editing) name.append(createElement(Pencil, { class: "xlsx-editing" }));
      name.append(peer.name);
      name.style.background = colorOf(peer.person);
    }
  }

  saved(_rev: number, waits: boolean): void {
    this.status.saved(waits);
  }

  closed(reason: ClosedReason): void {
    this.univer?.univerAPI.getActiveWorkbook()?.setEditable(false);
    if (this.control) this.bar.show(this.control, false);
    this.status.closed(reason);
  }

  // Wait for Univer's Steady lifecycle stage so all sheet services are initialized.
  async pictures(): Promise<number> {
    for (let pause = 0; pause < MOST_PAUSES; pause++) {
      const stage = this.univer?.univerAPI.getCurrentLifecycleStage();
      if (stage === undefined || stage >= LifecycleStages.Steady) break;
      await settled(PAUSE_MS);
    }
    return this.univer?.univerAPI.getActiveWorkbook()?.getNumSheets() ?? 0;
  }

  async picture(index: number): Promise<Area> {
    const workbook = this.univer?.univerAPI.getActiveWorkbook();
    const sheet = workbook?.getSheets()[index];
    if (!workbook || !sheet) throw new Error(`There is no sheet ${index + 1}.`);
    workbook.setActiveSheet(sheet);
    await settled(PAUSE_MS);
    return areaOf(this.canvas);
  }

  selection(): string {
    const range = this.univer?.univerAPI.getActiveWorkbook()?.getActiveSheet()?.getSelection();
    return range?.getActiveRange()?.getValues().flat().join("\t") ?? "";
  }
}

export function createEditor(element: HTMLElement, client: Client): XlsxEditor {
  return new XlsxEditor(element, client);
}
