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
  Status,
  settled,
} from "@documents/client";
import type { Changed, ClosedReason, Peer } from "@documents/contract";
import { SheetInterceptorService, UniverSheetsCorePreset } from "@univerjs/preset-sheets-core";
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
  type LinkTarget,
  linkAt,
  linksOf,
  type Merge,
  moveChange,
  moveLine,
  moveLink,
  movePlace,
  movesLines,
  type Place,
  rangeOf,
  type SheetLink,
  type Snapshot,
  touches,
} from "./change";
import { SheetControl } from "./control";
import { fromUniver, toUniver, type UniverStyle } from "./look";
import "./xlsx.css";

interface UniverCell {
  v?: number | string | boolean | null;
  f?: string | null;
  s?: string | UniverStyle | null;
  t?: number | null;
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

interface Selections {
  subUnitId: string;
  selections: { range: Range }[];
}

interface EditedCell {
  sheet: string;
  row: number;
  column: number;
}

/** A change of the document's order and its rev. */
interface Ordered {
  rev: number;
  change: Change;
}

/** Another person's place in this copy, and the rev at which it came. */
interface Marked {
  peer: Peer;
  place: Place | null;
  rev: number;
}

// Univer has no Greek words.
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
/** Univer draws a sheet a moment after it becomes the active one. */
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
const SET_SELECTIONS = "sheet.operation.set-selections";
const SET_SCROLL = "sheet.operation.set-scroll";
const SET_ACTIVE_SHEET = "sheet.operation.set-worksheet-active";

const STRUCTURE = {
  rows: {
    insert: INSERT_ROW,
    remove: REMOVE_ROW,
    insertCommand: "sheet.command.insert-row",
    removeCommand: "sheet.command.remove-row",
    direction: DIRECTION_UP,
  },
  columns: {
    insert: INSERT_COLUMN,
    remove: REMOVE_COLUMN,
    insertCommand: "sheet.command.insert-col",
    removeCommand: "sheet.command.remove-col",
    direction: DIRECTION_LEFT,
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
      columnData: Object.fromEntries(
        Object.entries(sheet.widths).map(([column, width]) => [column, { w: pixelWidth(width) }]),
      ),
      rowData: Object.fromEntries(
        Object.entries(sheet.heights).map(([row, height]) => [
          row,
          { h: height * PIXELS_PER_POINT },
        ]),
      ),
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

/** A cell's change. Univer changes only part of a style, so the look is the cell's whole look. */
function cellChange(
  row: number,
  column: number,
  cell: UniverCell | null,
  lookOf: (row: number, column: number) => CellChange,
): CellChange {
  const change: CellChange = cell && "s" in cell ? lookOf(row, column) : { row, column };
  if (cell === null || "v" in cell) change.value = valueOf(cell) ?? null;
  if (cell === null || "f" in cell) {
    change.formula = cell?.f ? cell.f.replace(/^=/, "") : null;
  }
  if (cell === null) change.style = null;
  return change;
}

function cellsOf(
  rows: Record<string, Record<string, UniverCell | null>>,
  lookOf: (row: number, column: number) => CellChange,
): CellChange[] {
  return Object.entries(rows).flatMap(([row, columns]) =>
    Object.entries(columns).map(([column, cell]) =>
      cellChange(Number(row), Number(column), cell, lookOf),
    ),
  );
}

function sizes(
  ranges: Range[],
  size: number | Record<string, number | null | undefined>,
  start: (range: Range) => number,
  end: (range: Range) => number,
  convert: (pixels: number) => number,
): Record<string, number> {
  const out: Record<string, number> = {};
  for (const range of ranges) {
    for (let line = start(range); line <= end(range); line++) {
      const pixels = typeof size === "number" ? size : size[line];
      if (typeof pixels === "number") out[line] = convert(pixels);
    }
  }
  return out;
}

class XlsxEditor implements Editor, Pictures {
  private readonly canvas: HTMLElement;
  private readonly marks: HTMLElement;
  private readonly status: Status;
  private readonly bar: XlsxBar;
  private readonly outbox = new Outbox<Change>();
  private readonly scheme: MutationObserver;
  private univer: ReturnType<typeof createUniver> | undefined;
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
  /** The keys that the person held at the last press, which Univer's click does not carry. */
  private pressed: Pick<MouseEvent, "metaKey" | "ctrlKey"> = { metaKey: false, ctrlKey: false };

  constructor(
    private readonly root: HTMLElement,
    private readonly client: Client,
  ) {
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
    this.scheme = new MutationObserver(() => this.univer?.univerAPI.toggleDarkMode(this.dark()));
    this.scheme.observe(root, { attributeFilter: ["data-scheme", "data-dark-pages"] });
  }

  /** The host's scheme, or the computer's when the host names none, when the person wants dark cells. */
  private dark(): boolean {
    if (!this.root.hasAttribute("data-dark-pages")) return false;
    const scheme = this.root.dataset.scheme;
    return scheme ? scheme === "dark" : window.matchMedia("(prefers-color-scheme: dark)").matches;
  }

  load(raw: unknown, person: Person): void {
    this.univer?.univer.dispose();
    const worker = new Worker(new URL("./formula-worker.ts", import.meta.url), { type: "module" });
    const made = createUniver({
      darkMode: this.dark(),
      locale: UNIVER_LOCALE,
      locales: { [UNIVER_LOCALE]: mergeLocales(UNIVER_WORDS) },
      presets: [
        UniverSheetsCorePreset({
          container: this.canvas,
          workerURL: worker,
          toolbar: false,
          footer: false,
        }),
      ],
    });
    this.univer = made;
    this.editing = null;
    this.held = [];
    this.marked = [];
    this.fileLooks = (raw as Snapshot).looks.length;
    this.links = (raw as Snapshot).sheets.flatMap(linksOf);
    this.canEdit = person.canEdit;
    made.univerAPI.createWorkbook(workbookData(raw as Snapshot) as never);
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
    // Univer writes the cell before it ends the edit, so the edit is in the outbox here.
    made.univerAPI.addEvent(made.univerAPI.Event.SheetEditEnded, () => {
      this.editing = null;
      for (const ordered of this.held.splice(0)) this.showOrdered(ordered);
      this.sendPlace();
      this.client.pull();
    });
    if (!person.canEdit) {
      made.univerAPI.getActiveWorkbook()?.setEditable(false);
    }
    this.bar.show(this.control, person.canEdit);
  }

  /**
   * A cell with its whole look as it is now. A look of the file keeps its index, so that what
   * the file has and a look does not, such as borders, stays.
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

  /** The cells of these ranges with their whole looks, after a change of a part of them. */
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

  /**
   * The place goes as the document is at the client's rev. While changes wait, or while this
   * side's lines wait for their answer, this copy differs from that rev, so the place waits too.
   */
  private sendPlace(): void {
    if (!this.here) return;
    this.placeWaits = this.held.length > 0 || this.outbox.unanswered().some(movesLines);
    if (!this.placeWaits)
      this.client.place(this.editing ? { ...this.here, editing: true } : this.here);
  }

  private changeOf(id: string, params: Record<string, unknown>): Change | null {
    const sheet = params.subUnitId as string;
    const range = params.range as Range;
    const lookOf = (row: number, column: number) => this.lookOf(sheet, row, column);
    switch (id) {
      case SET_VALUES: {
        const rows = (params.cellValue ?? {}) as Record<string, Record<string, UniverCell | null>>;
        const cells = cellsOf(rows, lookOf);
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
        return this.lines("rows", sheet, range.startRow, range.endRow - range.startRow + 1);
      case REMOVE_ROW:
        return this.lines("rows", sheet, range.startRow, range.startRow - range.endRow - 1);
      case INSERT_COLUMN:
        return this.lines(
          "columns",
          sheet,
          range.startColumn,
          range.endColumn - range.startColumn + 1,
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
        const cells = cellsOf(made.cellData ?? {}, (row, column) =>
          this.lookOf(made.id, row, column),
        );
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
          widths: sizes(
            params.ranges as Range[],
            params.colWidth as number,
            (line) => line.startColumn,
            (line) => line.endColumn,
            characterWidth,
          ),
        };
      case SET_HEIGHT:
        return {
          kind: "size",
          sheet,
          heights: sizes(
            params.ranges as Range[],
            params.rowHeight as number,
            (line) => line.startRow,
            (line) => line.endRow,
            (pixels) => pixels / PIXELS_PER_POINT,
          ),
        };
      case ADD_MERGE:
        return { kind: "merges", sheet, add: (params.ranges as Range[]).map(mergeOf) };
      case REMOVE_MERGE:
        return { kind: "merges", sheet, remove: (params.ranges as Range[]).map(mergeOf) };
      default:
        return null;
    }
  }

  private lines(kind: "rows" | "columns", sheet: string, at: number, count: number): Change {
    return { kind, sheet, at, count };
  }

  /** Nothing goes while changes wait, because the document is past them and the outbox is not. */
  batch(): unknown | null {
    return this.held.length > 0 ? null : batchOf(this.outbox.next());
  }

  private run(mutations: Mutation[]): void {
    this.remote = true;
    for (const { id, params } of mutations) this.commands?.syncExecuteCommand(id, params, REMOTE);
    this.remote = false;
  }

  private showCells(sheet: string, cells: CellChange[]): void {
    const cellValue: Record<number, Record<number, UniverCell>> = {};
    const unanswered = this.outbox.unanswered();
    for (const cell of cells) {
      if (unanswered.some((change) => covers(change, sheet, cell.row, cell.column))) continue;
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
   * A row or a column changes through the mutations that Univer's own command makes, on whole
   * lines as that command names them, so this side rewrites its formulas and merges, and the
   * person's undo steps go.
   */
  private showLines(kind: "rows" | "columns", sheet: string, at: number, count: number): void {
    this.outbox.move((change) => moveChange(change, sheet, kind, at, count));
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
    const intercepted = this.interceptor?.onCommandExecute({
      id: inserts ? names.insertCommand : names.removeCommand,
      params,
    });
    this.run([
      ...(intercepted?.preRedos ?? []),
      {
        id: inserts ? names.insert : names.remove,
        params: { unitId: WORKBOOK, subUnitId: sheet, range },
      },
      ...(intercepted?.redos ?? []),
    ]);
    this.undo?.clearUndoRedo(WORKBOOK);
    this.follow(kind, sheet, at, count);
  }

  /** The person's selection moves with its cells, as an edit does. */
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

  private show(change: Change): void {
    const unit = { unitId: WORKBOOK };
    switch (change.kind) {
      case "cells":
        this.showCells(change.sheet, change.cells);
        break;
      case "rows":
      case "columns":
        this.showLines(change.kind, change.sheet, change.at, change.count);
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
      case "group":
        for (const inner of change.changes) this.show(inner);
        break;
    }
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
            ranges: [{ startRow: 0, endRow: 0, startColumn: line, endColumn: line }],
            colWidth: pixelWidth(width),
          },
        },
      ]);
    }
    for (const [row, height] of Object.entries(change.heights ?? {})) {
      const line = Number(row);
      this.run([
        {
          id: SET_HEIGHT,
          params: {
            ...unit,
            ranges: [{ startRow: line, endRow: line, startColumn: 0, endColumn: 0 }],
            rowHeight: height * PIXELS_PER_POINT,
          },
        },
      ]);
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
   * While the person's cell editor is open, a change that writes that cell or moves its line
   * waits with each change after it, so the editor keeps what the person typed, its value wins,
   * and it moves with its line when the editor closes.
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

  /** An address goes to the host. A place such as `'Data'!A3` is in this workbook. */
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

  /** Moves the places that came before `rev` across the lines of a change. */
  private moveMarks(change: Change, rev: number): void {
    this.marked = this.marked.map((marked) =>
      marked.place && marked.rev < rev
        ? { ...marked, place: movePlace(marked.place, change) }
        : marked,
    );
    this.drawMarks();
  }

  /**
   * A place that is new comes at the client's rev, so it moves across this side's unanswered
   * lines. Each other place stays as this copy moved it.
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

  /** Draws where each person is. Univer has no such layer, so the marks lie over its canvas. */
  private drawMarks(): void {
    const sheet = this.univer?.univerAPI.getActiveWorkbook()?.getActiveSheet();
    const grid = [...this.canvas.querySelectorAll("canvas")]
      .map((canvas) => canvas.getBoundingClientRect())
      .sort((left, right) => right.height - left.height)[0];
    this.marks.replaceChildren();
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

  /** The sheets, once Univer has all its services: it takes the last ones when it is steady. */
  async pictures(): Promise<number> {
    for (let pause = 0; pause < MOST_PAUSES; pause++) {
      const stage = this.univer?.univerAPI.getCurrentLifecycleStage();
      if (stage === undefined || stage >= LifecycleStages.Steady) break;
      await settled(PAUSE_MS);
    }
    return this.univer?.univerAPI.getActiveWorkbook()?.getNumSheets() ?? 0;
  }

  /** Each sheet in turn, from its first cells. */
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

  destroy(): void {
    this.scheme.disconnect();
    this.bar.destroy();
    this.univer?.univer.dispose();
  }
}

export function createEditor(element: HTMLElement, client: Client): XlsxEditor {
  return new XlsxEditor(element, client);
}
