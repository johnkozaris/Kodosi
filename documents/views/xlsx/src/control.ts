import type { createUniver, ICommandService, IUndoRedoService } from "@univerjs/presets";
import { ALIGN_TO_UNIVER, type UniverStyle, VERTICAL_TO_UNIVER } from "./look";

type Api = ReturnType<typeof createUniver>["univerAPI"];
type Sheet = ReturnType<NonNullable<ReturnType<Api["getActiveWorkbook"]>>["getActiveSheet"]>;

export type Align = "left" | "center" | "right";
export type Vertical = "top" | "center" | "bottom";
export type MarkName = "bold" | "italic" | "underline" | "strike";
export type Merge = "all" | "rows" | "columns";

/** The look of the cell where the selection starts. */
export interface CellLook {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  size: number;
  font: string;
  color: string | null;
  fill: string | null;
  align: Align | null;
  vertical: Vertical | null;
  wrap: boolean;
  numberFormat: string;
  merged: boolean;
}

export interface SheetName {
  id: string;
  name: string;
  active: boolean;
}

const SET_STYLE = "sheet.command.set-style";
const SET_NUMBER_FORMAT = "sheet.command.numfmt.set.numfmt";
const CLEAR_LOOK = "sheet.command.clear-selection-format";
const COPY_LOOK = "sheet.command.set-once-format-painter";
const UNMERGE = "sheet.command.remove-worksheet-merge";
const MERGES: Record<Merge, string> = {
  all: "sheet.command.add-worksheet-merge-all",
  rows: "sheet.command.add-worksheet-merge-horizontal",
  columns: "sheet.command.add-worksheet-merge-vertical",
};
const MARK_STYLES: Record<MarkName, keyof UniverStyle> = {
  bold: "bl",
  italic: "it",
  underline: "ul",
  strike: "st",
};
const WRAP = 3;
const UNSPECIFIED = 0;
const DEFAULT_SIZE = 11;
const DEFAULT_FONT = "Arial";

function nameOf<V extends string>(
  table: Record<string, number>,
  value: number | undefined,
): V | null {
  return (Object.entries(table).find(([, known]) => known === value)?.[0] as V | undefined) ?? null;
}

/** What the bar can do to a workbook, through the commands of Univer's own controls. */
export class SheetControl {
  constructor(
    private readonly api: Api,
    private readonly commands: ICommandService,
    private readonly history: IUndoRedoService,
  ) {}

  private sheet(): Sheet | undefined {
    return this.api.getActiveWorkbook()?.getActiveSheet();
  }

  private ranges() {
    return this.sheet()?.getSelection()?.getActiveRangeList() ?? [];
  }

  look(): CellLook | null {
    const range = this.api.getActiveWorkbook()?.getActiveRange();
    if (!range) return null;
    const style = (range.getCellStyleData() ?? {}) as UniverStyle;
    return {
      bold: style.bl === 1,
      italic: style.it === 1,
      underline: style.ul?.s === 1,
      strike: style.st?.s === 1,
      size: style.fs ?? DEFAULT_SIZE,
      font: style.ff ?? DEFAULT_FONT,
      color: style.cl?.rgb ?? null,
      fill: style.bg?.rgb ?? null,
      align: nameOf<Align>(ALIGN_TO_UNIVER, style.ht),
      vertical: nameOf<Vertical>(VERTICAL_TO_UNIVER, style.vt),
      wrap: style.tb === WRAP,
      numberFormat: style.n?.pattern ?? "",
      merged: range.isMerged(),
    };
  }

  canUndo(): boolean {
    const unit = this.api.getActiveWorkbook()?.getId();
    return unit !== undefined && this.history.getUndoRedoStatus(unit).undos > 0;
  }

  canRedo(): boolean {
    const unit = this.api.getActiveWorkbook()?.getId();
    return unit !== undefined && this.history.getUndoRedoStatus(unit).redos > 0;
  }

  undo(): void {
    void this.api.undo();
  }

  redo(): void {
    void this.api.redo();
  }

  private setStyle(type: keyof UniverStyle, value: unknown): void {
    this.commands.syncExecuteCommand(SET_STYLE, { style: { type, value } });
  }

  toggle(mark: MarkName): void {
    const on = !this.look()?.[mark];
    const value = mark === "bold" || mark === "italic" ? Number(on) : { s: Number(on) };
    this.setStyle(MARK_STYLES[mark], value);
  }

  setSize(points: number): void {
    this.setStyle("fs", points);
  }

  setFont(font: string): void {
    this.setStyle("ff", font);
  }

  /** A color as `#RRGGBB`, or null for the automatic one. */
  setColor(color: string | null): void {
    this.setStyle("cl", { rgb: color });
  }

  setFill(color: string | null): void {
    this.setStyle("bg", { rgb: color });
  }

  setAlign(align: Align): void {
    this.setStyle("ht", ALIGN_TO_UNIVER[align]);
  }

  setVertical(vertical: Vertical): void {
    this.setStyle("vt", VERTICAL_TO_UNIVER[vertical]);
  }

  setWrap(wrap: boolean): void {
    this.setStyle("tb", wrap ? WRAP : UNSPECIFIED);
  }

  /** Gives the selected cells a number format, or takes it away with an empty one. */
  setNumberFormat(pattern: string): void {
    const values = this.ranges().flatMap((range) => {
      const cells = [];
      for (let row = range.getRow(); row <= range.getLastRow(); row++) {
        for (let col = range.getColumn(); col <= range.getLastColumn(); col++) {
          cells.push({ row, col, ...(pattern ? { pattern } : {}) });
        }
      }
      return cells;
    });
    if (values.length > 0) this.commands.syncExecuteCommand(SET_NUMBER_FORMAT, { values });
  }

  merge(kind: Merge): void {
    void this.commands.executeCommand(MERGES[kind]);
  }

  unmerge(): void {
    void this.commands.executeCommand(UNMERGE);
  }

  clearLook(): void {
    void this.commands.executeCommand(CLEAR_LOOK);
  }

  /** The next selection takes the look of this one. */
  copyLook(): void {
    void this.commands.executeCommand(COPY_LOOK);
  }

  /**
   * Where a function goes and what it counts: under each column of a block, after a row, or in
   * one cell with the numbers right above it or right before it.
   */
  private functionPlaces(): { row: number; column: number; source: string }[] {
    const sheet = this.sheet();
    const range = this.api.getActiveWorkbook()?.getActiveRange();
    if (!sheet || !range) return [];
    const [top, left] = [range.getRow(), range.getColumn()];
    const [bottom, right] = [range.getLastRow(), range.getLastColumn()];
    const a1 = (fromRow: number, fromColumn: number, toRow: number, toColumn: number) =>
      sheet
        .getRange(fromRow, fromColumn, toRow - fromRow + 1, toColumn - fromColumn + 1)
        .getA1Notation();
    if (bottom > top) {
      const places = [];
      for (let column = left; column <= right; column++) {
        places.push({ row: bottom + 1, column, source: a1(top, column, bottom, column) });
      }
      return places;
    }
    if (right > left) return [{ row: top, column: right + 1, source: a1(top, left, top, right) }];
    const isNumber = (row: number, column: number) =>
      row >= 0 && column >= 0 && typeof sheet.getRange(row, column).getRawValue() === "number";
    let first = top;
    while (isNumber(first - 1, left)) first--;
    if (first < top) return [{ row: top, column: left, source: a1(first, left, top - 1, left) }];
    let start = left;
    while (isNumber(top, start - 1)) start--;
    if (start < left) return [{ row: top, column: left, source: a1(top, start, top, left - 1) }];
    return [];
  }

  canInsertFunction(): boolean {
    return this.functionPlaces().length > 0;
  }

  insertFunction(name: string): void {
    const sheet = this.sheet();
    for (const place of this.functionPlaces()) {
      sheet?.getRange(place.row, place.column).setFormula(`=${name}(${place.source})`);
    }
  }

  sheets(): SheetName[] {
    const workbook = this.api.getActiveWorkbook();
    const active = workbook?.getActiveSheet().getSheetId();
    return (workbook?.getSheets() ?? []).map((sheet) => ({
      id: sheet.getSheetId(),
      name: sheet.getSheetName(),
      active: sheet.getSheetId() === active,
    }));
  }

  showSheet(id: string): void {
    this.api.getActiveWorkbook()?.setActiveSheet(id);
  }

  addSheet(): void {
    const workbook = this.api.getActiveWorkbook();
    const sheet = workbook?.insertSheet();
    if (sheet) workbook?.setActiveSheet(sheet);
  }

  renameSheet(name: string): void {
    this.sheet()?.setName(name);
  }

  removeSheet(): void {
    const workbook = this.api.getActiveWorkbook();
    const sheet = workbook?.getActiveSheet();
    if (workbook && sheet && workbook.getNumSheets() > 1) workbook.deleteSheet(sheet);
  }
}
