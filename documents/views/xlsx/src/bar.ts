import {
  Bar,
  type Choice,
  closePopover,
  fieldOf,
  fits,
  footOf,
  gap,
  keyOf,
  keys,
  type MenuEntry,
  menuKey,
  openPanel,
  type PanelRow,
  panelKey,
  panelOf,
  pickerOf,
  readOnly,
  rule,
  type Swatch,
  segmentsOf,
  stepperOf,
  swatchesOf,
  wellOf,
} from "@documents/bar";
import { say } from "@documents/words";
import {
  ALargeSmall,
  AlignVerticalJustifyCenter,
  AlignVerticalJustifyEnd,
  AlignVerticalJustifyStart,
  Ban,
  Baseline,
  Bold,
  type IconNode,
  Italic,
  Paintbrush,
  PencilLine,
  Plus,
  Redo2,
  RemoveFormatting,
  Sigma,
  Strikethrough,
  TableCellsMerge,
  TableCellsSplit,
  TextAlignCenter,
  TextAlignEnd,
  TextAlignStart,
  Trash2,
  Underline,
  Undo2,
  WrapText,
} from "lucide";
import type { Align, CellLook, MarkName, SheetControl, Vertical } from "./control";

const SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72];
const FONTS = [
  "Aptos",
  "Arial",
  "Calibri",
  "Cambria",
  "Courier New",
  "Georgia",
  "Helvetica",
  "Times New Roman",
  "Verdana",
];
const FORMATS: readonly { label: string; pattern: string; hint: string }[] = [
  { label: say("General"), pattern: "", hint: "1234.5" },
  { label: say("Number"), pattern: "#,##0.00", hint: "1,234.50" },
  { label: say("Whole number"), pattern: "#,##0", hint: "1,235" },
  { label: say("Percent"), pattern: "0.00%", hint: "12.34%" },
  { label: say("Dollars"), pattern: "$#,##0.00", hint: "$1,234.50" },
  { label: say("Euros"), pattern: '#,##0.00 "€"', hint: "1,234.50 €" },
  { label: say("Date"), pattern: "yyyy-mm-dd", hint: "2026-11-02" },
  { label: say("Time"), pattern: "hh:mm", hint: "13:45" },
  { label: say("Scientific"), pattern: "0.00E+00", hint: "1.23E+03" },
  { label: say("Text"), pattern: "@", hint: "abc" },
];
const FUNCTIONS: readonly { label: string; name: string }[] = [
  { label: say("Sum"), name: "SUM" },
  { label: say("Average"), name: "AVERAGE" },
  { label: say("Count"), name: "COUNT" },
  { label: say("Largest"), name: "MAX" },
  { label: say("Smallest"), name: "MIN" },
];
const COLORS: readonly Swatch[] = [
  { value: null, label: say("Automatic"), fill: "var(--well)", icon: Baseline },
  { value: "#7F7F7F", label: say("Gray"), fill: "#7F7F7F" },
  { value: "#C00000", label: say("Dark red"), fill: "#C00000" },
  { value: "#E97132", label: say("Orange"), fill: "#E97132" },
  { value: "#00B050", label: say("Green"), fill: "#00B050" },
  { value: "#0070C0", label: say("Blue"), fill: "#0070C0" },
  { value: "#002060", label: say("Dark blue"), fill: "#002060" },
  { value: "#7030A0", label: say("Purple"), fill: "#7030A0" },
];
const FILLS: readonly Swatch[] = [
  { value: null, label: say("No fill"), fill: "var(--well)", icon: Ban },
  { value: "#FFFF00", label: say("Yellow"), fill: "#FFFF00" },
  { value: "#FFF2CC", label: say("Light yellow"), fill: "#FFF2CC" },
  { value: "#E2EFDA", label: say("Light green"), fill: "#E2EFDA" },
  { value: "#DDEBF7", label: say("Light blue"), fill: "#DDEBF7" },
  { value: "#FCE4D6", label: say("Light orange"), fill: "#FCE4D6" },
  { value: "#D9D2E9", label: say("Light purple"), fill: "#D9D2E9" },
  { value: "#EDEDED", label: say("Light gray"), fill: "#EDEDED" },
];
const ALIGNS: readonly Choice<Align>[] = [
  { value: "left", label: say("Align left"), icon: TextAlignStart },
  { value: "center", label: say("Center"), icon: TextAlignCenter },
  { value: "right", label: say("Align right"), icon: TextAlignEnd },
];
const VERTICALS: readonly Choice<Vertical>[] = [
  { value: "top", label: say("Top"), icon: AlignVerticalJustifyStart },
  { value: "center", label: say("Middle"), icon: AlignVerticalJustifyCenter },
  { value: "bottom", label: say("Bottom"), icon: AlignVerticalJustifyEnd },
];
const MARKS: readonly { name: MarkName; label: string; icon: IconNode; key?: string }[] = [
  { name: "bold", label: say("Bold"), icon: Bold, key: keys("B") },
  { name: "italic", label: say("Italic"), icon: Italic, key: keys("I") },
  { name: "underline", label: say("Underline"), icon: Underline, key: keys("U") },
  { name: "strike", label: say("Strikethrough"), icon: Strikethrough },
];

function formatName(pattern: string): string {
  return FORMATS.find((format) => format.pattern === pattern)?.label ?? say("Custom");
}

function sizeText(size: number): string {
  return String(Math.round(size * 2) / 2);
}

/** The bar of a sheet: what the person changes most, and the rest in the Text panel. */
export class XlsxBar {
  private readonly bar: Bar;
  private refresh: (() => void) | undefined;
  private pending = 0;

  constructor(
    root: HTMLElement,
    private readonly status: HTMLElement,
  ) {
    this.bar = new Bar(root, say("Format"));
  }

  get element(): HTMLElement {
    return this.bar.element;
  }

  show(control: SheetControl, canEdit: boolean): void {
    const sheets = this.sheetPicker(control, canEdit);
    if (!canEdit) {
      this.refresh = sheets.refresh;
      this.bar.show(readOnly(), gap(), sheets.element, this.status);
      return;
    }
    const undo = keyOf({
      label: say("Undo"),
      icon: Undo2,
      shortcut: keys("Z"),
      press: () => control.undo(),
    });
    const redo = keyOf({
      label: say("Redo"),
      icon: Redo2,
      shortcut: keys("Z", { shift: true }),
      fits: "narrow",
      press: () => control.redo(),
    });
    const format = this.formatPicker(control, "mid");
    const size = this.sizeStepper(control, "wide");
    const marks = wellOf(
      say("Marks"),
      MARKS.map((mark) => ({
        label: mark.label,
        icon: mark.icon,
        ...(mark.key ? { shortcut: mark.key } : {}),
        press: () => control.toggle(mark.name),
      })),
    );
    const align = segmentsOf(say("Alignment"), ALIGNS, (value) => control.setAlign(value), "wide");
    const text = panelKey({
      label: say("Text"),
      icon: ALargeSmall,
      build: () => this.textPanel(control),
    });
    const copy = keyOf({
      label: say("Copy look"),
      icon: Paintbrush,
      fits: "wide",
      press: () => control.copyLook(),
    });
    const merge = menuKey({
      label: say("Merge"),
      icon: TableCellsMerge,
      fits: "mid",
      entries: () => {
        const merged = control.look()?.merged ?? false;
        return [
          {
            label: say("Merge and center"),
            icon: TableCellsMerge,
            run: () => {
              control.merge("all");
              control.setAlign("center");
            },
          },
          { label: say("Merge across"), run: () => control.merge("rows") },
          { label: say("Merge down"), run: () => control.merge("columns") },
          { label: say("Merge cells"), run: () => control.merge("all") },
          "separator",
          {
            label: say("Unmerge"),
            icon: TableCellsSplit,
            disabled: !merged,
            run: () => control.unmerge(),
          },
        ];
      },
    });
    const functions = menuKey({
      label: say("Functions"),
      icon: Sigma,
      fits: "narrow",
      entries: () => {
        const ready = control.canInsertFunction();
        return FUNCTIONS.map((entry) => ({
          label: entry.label,
          hint: entry.name,
          disabled: !ready,
          run: () => control.insertFunction(entry.name),
        }));
      },
    });
    this.refresh = () => {
      const look = control.look();
      undo.show({ enabled: control.canUndo() });
      redo.show({ enabled: control.canRedo() });
      format.refresh(look);
      size.refresh(look);
      marks.show(MARKS.map((mark) => ({ active: look?.[mark.name] ?? false })));
      align.show(look?.align ?? null);
      sheets.refresh();
    };
    this.bar.show(
      undo.element,
      redo.element,
      rule(),
      format.element,
      size.element,
      marks.element,
      align.element,
      text.element,
      copy.element,
      rule("mid"),
      merge.element,
      functions.element,
      gap(),
      sheets.element,
      this.status,
    );
    this.refresh();
  }

  /** Shows what is true at the selection, once a frame however many changes come. */
  update(): void {
    if (this.pending || !this.refresh) return;
    this.pending = requestAnimationFrame(() => {
      this.pending = 0;
      this.refresh?.();
    });
  }

  destroy(): void {
    cancelAnimationFrame(this.pending);
    this.bar.destroy();
  }

  private formatPicker(control: SheetControl, fits?: "mid") {
    const picker = pickerOf({
      label: say("Number format"),
      width: 118,
      ...(fits ? { fits } : {}),
      entries: () => {
        const current = control.look()?.numberFormat ?? "";
        return FORMATS.map((format) => ({
          label: format.label,
          hint: format.hint,
          checked: format.pattern === current,
          run: () => {
            control.setNumberFormat(format.pattern);
            refresh(control.look());
          },
        }));
      },
    });
    const refresh = (look: CellLook | null) => picker.show(formatName(look?.numberFormat ?? ""));
    refresh(control.look());
    return { element: picker.element, refresh };
  }

  private sizeStepper(control: SheetControl, fits?: "wide") {
    const stepper = stepperOf(
      say("Size"),
      (larger) => {
        const now = control.look()?.size ?? 11;
        const next = larger
          ? (SIZES.find((step) => step > now) ?? now)
          : (SIZES.findLast((step) => step < now) ?? now);
        control.setSize(next);
        refresh(control.look());
      },
      fits,
    );
    const refresh = (look: CellLook | null) => {
      const now = look?.size ?? 11;
      stepper.show(sizeText(now), now > (SIZES[0] ?? 0), now < (SIZES.at(-1) ?? 0));
    };
    refresh(control.look());
    return { element: stepper.element, refresh };
  }

  private sheetPicker(control: SheetControl, canEdit: boolean) {
    const picker = pickerOf({
      label: say("Sheet"),
      width: 132,
      entries: () => {
        const sheets = control.sheets();
        const entries: MenuEntry[] = sheets.map((sheet) => ({
          label: sheet.name,
          checked: sheet.active,
          run: () => control.showSheet(sheet.id),
        }));
        if (!canEdit) return entries;
        const name = sheets.find((sheet) => sheet.active)?.name ?? "";
        return [
          ...entries,
          "separator",
          { label: say("Add sheet"), icon: Plus, run: () => control.addSheet() },
          {
            label: say("Rename sheet…"),
            icon: PencilLine,
            run: () => openPanel(picker.element, this.renamePanel(control, name)),
          },
          {
            label: say("Delete sheet…"),
            icon: Trash2,
            disabled: sheets.length < 2,
            run: () => openPanel(picker.element, this.deletePanel(control, name)),
          },
        ];
      },
    });
    const refresh = () =>
      picker.show(control.sheets().find((sheet) => sheet.active)?.name ?? say("Sheet"));
    refresh();
    return { element: picker.element, refresh };
  }

  private renamePanel(control: SheetControl, name: string): HTMLElement {
    return panelOf([
      [
        say("Name"),
        fieldOf(say("Name"), name, (text) => {
          control.renameSheet(text);
          closePopover();
          this.update();
        }),
      ],
    ]);
  }

  private deletePanel(control: SheetControl, name: string): HTMLElement {
    const line = document.createElement("span");
    line.textContent = say("{name} and its cells go. Undo brings them back.", { name });
    return panelOf(
      [[say("Delete"), line]],
      footOf([{ label: say("Delete sheet"), icon: Trash2, press: () => control.removeSheet() }]),
    );
  }

  private textPanel(control: SheetControl): HTMLElement {
    const look = control.look();
    const anchor = this.bar.element;
    const rows: PanelRow[] = [];
    const font = pickerOf({
      label: say("Font"),
      width: 200,
      entries: () => {
        const current = control.look()?.font ?? "Arial";
        const names = FONTS.includes(current) ? FONTS : [current, ...FONTS];
        return names.map((name) => ({
          label: name,
          checked: name === current,
          look: { fontFamily: `"${name}", sans-serif` },
          run: () => {
            control.setFont(name);
            font.show(name);
          },
        }));
      },
    });
    font.show(look?.font ?? "Arial");
    rows.push([say("Font"), font.element]);
    if (!fits(anchor, "wide")) rows.push([say("Size"), this.sizeStepper(control).element]);
    rows.push([
      say("Colour"),
      swatchesOf(say("Colour"), COLORS, look?.color ?? null, (color) => control.setColor(color)),
    ]);
    rows.push([
      say("Fill"),
      swatchesOf(say("Fill"), FILLS, look?.fill ?? null, (fill) => control.setFill(fill)),
    ]);
    if (!fits(anchor, "wide")) {
      const align = segmentsOf(say("Alignment"), ALIGNS, (value) => {
        control.setAlign(value);
        align.show(value);
      });
      align.show(look?.align ?? null);
      rows.push([say("Across"), align.element]);
    }
    const vertical = segmentsOf(say("Vertical"), VERTICALS, (value) => {
      control.setVertical(value);
      vertical.show(value);
    });
    vertical.show(look?.vertical ?? "bottom");
    rows.push([say("Down"), vertical.element]);
    const wrap = wellOf(say("Wrap"), [
      {
        label: say("Wrap text"),
        icon: WrapText,
        press: () => {
          const on = !(control.look()?.wrap ?? false);
          control.setWrap(on);
          wrap.show([{ active: on }]);
        },
      },
    ]);
    wrap.show([{ active: look?.wrap ?? false }]);
    rows.push([say("Wrap"), wrap.element]);
    if (!fits(anchor, "mid")) rows.push([say("Format"), this.formatPicker(control).element]);
    return panelOf(
      rows,
      footOf([
        { label: say("Copy look"), icon: Paintbrush, press: () => control.copyLook() },
        { label: say("Clear look"), icon: RemoveFormatting, press: () => control.clearLook() },
      ]),
    );
  }
}
