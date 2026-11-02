import {
  batchOf,
  type Client,
  colorOf,
  type Editor,
  Outbox,
  type Person,
  Status,
} from "@documents/client";
import type { Changed, ClosedReason, Peer } from "@documents/contract";
import { SheetInterceptorService, UniverSheetsCorePreset } from "@univerjs/preset-sheets-core";
import sheetsCoreEnUS from "@univerjs/preset-sheets-core/locales/en-US";
import {
  createUniver,
  ICommandService,
  IUndoRedoService,
  LocaleType,
  mergeLocales,
} from "@univerjs/presets";
import "@univerjs/preset-sheets-core/lib/index.css";
import {
  type Cell,
  type CellChange,
  type Change,
  covers,
  type Merge,
  moveChange,
  type Snapshot,
} from "./change";
import { fromUniver, toUniver, type UniverStyle } from "./look";
import "./excel.css";

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

interface Place {
  sheet?: string;
  row?: number;
  column?: number;
  endRow?: number;
  endColumn?: number;
}

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

function cellChange(row: number, column: number, cell: UniverCell | null): CellChange {
  const change: CellChange = { row, column };
  if (cell === null || "v" in cell) change.value = valueOf(cell) ?? null;
  if (cell === null || "f" in cell) {
    change.formula = cell?.f ? cell.f.replace(/^=/, "") : null;
  }
  const style = cell?.s;
  if (style === null) change.style = null;
  else if (typeof style === "object") change.look = fromUniver(style);
  else if (typeof style === "string" && /^\d+$/.test(style)) change.style = Number(style);
  return change;
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

class ExcelEditor implements Editor {
  private readonly canvas: HTMLElement;
  private readonly marks: HTMLElement;
  private readonly status: Status;
  private readonly outbox = new Outbox<Change>();
  private univer: ReturnType<typeof createUniver> | undefined;
  private commands: ICommandService | undefined;
  private interceptor: SheetInterceptorService | undefined;
  private undo: IUndoRedoService | undefined;
  private people: readonly Peer[] = [];
  private remote = false;

  constructor(
    root: HTMLElement,
    private readonly client: Client,
  ) {
    this.status = new Status(root);
    const holder = root.appendChild(document.createElement("div"));
    holder.className = "excel-holder";
    this.canvas = holder.appendChild(document.createElement("div"));
    this.canvas.className = "excel-canvas";
    this.marks = holder.appendChild(document.createElement("div"));
    this.marks.className = "excel-marks";
    holder.append(this.status.element);
  }

  load(raw: unknown, person: Person): void {
    this.univer?.univer.dispose();
    const worker = new Worker(new URL("./formula-worker.ts", import.meta.url), { type: "module" });
    const made = createUniver({
      darkMode: window.matchMedia("(prefers-color-scheme: dark)").matches,
      locale: LocaleType.EN_US,
      locales: { [LocaleType.EN_US]: mergeLocales(sheetsCoreEnUS) },
      presets: [
        UniverSheetsCorePreset({ container: this.canvas, workerURL: worker, footer: false }),
      ],
    });
    this.univer = made;
    made.univerAPI.createWorkbook(workbookData(raw as Snapshot) as never);
    const injector = made.univer.__getInjector();
    this.commands = injector.get(ICommandService);
    this.interceptor = injector.get(SheetInterceptorService);
    this.undo = injector.get(IUndoRedoService);
    this.commands.onMutationExecutedForCollab((info, options) => {
      if (this.remote || options?.onlyLocal || options?.fromCollab) return;
      const change = this.changeOf(info.id, (info.params ?? {}) as Record<string, unknown>);
      if (change) {
        this.outbox.add(change);
        this.client.pull();
      }
    });
    this.commands.onCommandExecuted((info) => {
      if (info.id === SET_SELECTIONS) this.placed(info.params as Selections);
      else if (info.id === SET_SCROLL || info.id === SET_ACTIVE_SHEET) this.drawMarks();
    });
    if (!person.canEdit) {
      made.univerAPI.getActiveWorkbook()?.setEditable(false);
    }
  }

  private placed({ subUnitId, selections }: Selections): void {
    const range = selections[0]?.range;
    if (range) {
      this.client.place({
        sheet: subUnitId,
        row: range.startRow,
        column: range.startColumn,
        endRow: range.endRow,
        endColumn: range.endColumn,
      });
    }
  }

  private changeOf(id: string, params: Record<string, unknown>): Change | null {
    const sheet = params.subUnitId as string;
    const range = params.range as Range;
    switch (id) {
      case SET_VALUES: {
        const rows = (params.cellValue ?? {}) as Record<string, Record<string, UniverCell | null>>;
        const cells = Object.entries(rows).flatMap(([row, columns]) =>
          Object.entries(columns).map(([column, cell]) =>
            cellChange(Number(row), Number(column), cell),
          ),
        );
        return cells.length > 0 ? { kind: "cells", sheet, cells } : null;
      }
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
        const made = params.sheet as { id: string; name: string };
        return { kind: "addSheet", id: made.id, name: made.name };
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

  batch(): unknown | null {
    return batchOf(this.outbox.next());
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
    this.run([{ id: SET_VALUES, params: { unitId: WORKBOOK, subUnitId: sheet, cellValue } }]);
  }

  /**
   * A row or a column changes through the mutations that Univer's own command makes, so this
   * side rewrites its formulas and merges, and the person's undo steps go.
   */
  private showLines(kind: "rows" | "columns", sheet: string, at: number, count: number): void {
    this.outbox.move((change) => moveChange(change, sheet, kind, at, count));
    const last = at + Math.abs(count) - 1;
    const range =
      kind === "rows"
        ? { startRow: at, endRow: last, startColumn: 0, endColumn: 0 }
        : { startRow: 0, endRow: 0, startColumn: at, endColumn: last };
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
    if (own) this.outbox.answered();
    else this.show(changed.body as Change);
  }

  peers(peers: readonly Peer[]): void {
    this.people = peers;
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
    for (const peer of this.people) {
      const place = peer.place as Place | null;
      if (place?.sheet !== sheet.getSheetId() || place.row === undefined) continue;
      const first = sheet.getRange(place.row, place.column ?? 0).getCellRect();
      const last = sheet
        .getRange(place.endRow ?? place.row, place.endColumn ?? place.column ?? 0)
        .getCellRect();
      const mark = this.marks.appendChild(document.createElement("div"));
      mark.className = "excel-mark";
      mark.style.borderColor = colorOf(peer.id);
      mark.style.left = `${first.left + shiftX}px`;
      mark.style.top = `${first.top + shiftY}px`;
      mark.style.width = `${last.right - first.left}px`;
      mark.style.height = `${last.bottom - first.top}px`;
      const name = mark.appendChild(document.createElement("span"));
      name.textContent = peer.name;
      name.style.background = colorOf(peer.id);
    }
  }

  saved(_rev: number, waits: boolean): void {
    this.status.saved(waits);
  }

  closed(reason: ClosedReason): void {
    this.univer?.univerAPI.getActiveWorkbook()?.setEditable(false);
    this.status.closed(reason);
  }

  selection(): string {
    const range = this.univer?.univerAPI.getActiveWorkbook()?.getActiveSheet()?.getSelection();
    return range?.getActiveRange()?.getValues().flat().join("\t") ?? "";
  }

  destroy(): void {
    this.univer?.univer.dispose();
  }
}

export function createEditor(element: HTMLElement, client: Client): ExcelEditor {
  return new ExcelEditor(element, client);
}
