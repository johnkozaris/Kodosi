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
  sizeGridOf,
  stepperOf,
  swatchesOf,
  wellOf,
} from "@documents/bar";
import { language, say } from "@documents/words";
import type { Editor } from "@tiptap/core";
import { redo, redoDepth, undo, undoDepth } from "@tiptap/pm/history";
import type { Node as ProseNode } from "@tiptap/pm/model";
import type { EditorState, Transaction } from "@tiptap/pm/state";
import {
  ALargeSmall,
  ArrowDown,
  ArrowDownToLine,
  ArrowLeftToLine,
  ArrowRightToLine,
  ArrowUp,
  ArrowUpToLine,
  Ban,
  Baseline,
  Bold,
  Check,
  FileDiff,
  type IconNode,
  Italic,
  Link,
  List,
  ListIndentDecrease,
  ListIndentIncrease,
  ListOrdered,
  Plus,
  Redo2,
  RemoveFormatting,
  SeparatorHorizontal,
  Strikethrough,
  Subscript,
  Superscript,
  Table2,
  TextAlignCenter,
  TextAlignEnd,
  TextAlignJustify,
  TextAlignStart,
  Trash2,
  Underline,
  Undo2,
  Unlink,
  X,
} from "lucide";
import type { Align, MarkName } from "./commands";
import * as commands from "./commands";
import type { DocxView } from "./styles";
import * as tables from "./tables";

const SIZES = [8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72];
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
const COLORS: readonly Swatch[] = [
  { value: null, label: say("Automatic"), fill: "var(--well)", icon: Baseline },
  { value: "7F7F7F", label: say("Gray"), fill: "#7F7F7F" },
  { value: "C00000", label: say("Dark red"), fill: "#C00000" },
  { value: "E97132", label: say("Orange"), fill: "#E97132" },
  { value: "00B050", label: say("Green"), fill: "#00B050" },
  { value: "0070C0", label: say("Blue"), fill: "#0070C0" },
  { value: "002060", label: say("Dark blue"), fill: "#002060" },
  { value: "7030A0", label: say("Purple"), fill: "#7030A0" },
];
const HIGHLIGHTS: readonly Swatch[] = [
  { value: null, label: say("No highlight"), fill: "var(--well)", icon: Ban },
  { value: "yellow", label: say("Yellow"), fill: "#FFFF00" },
  { value: "green", label: say("Bright green"), fill: "#00FF00" },
  { value: "cyan", label: say("Turquoise"), fill: "#00FFFF" },
  { value: "magenta", label: say("Pink"), fill: "#FF00FF" },
  { value: "lightGray", label: say("Gray"), fill: "#C0C0C0" },
];
const ALIGNS: readonly Choice<Align>[] = [
  {
    value: "left",
    label: say("Align left"),
    icon: TextAlignStart,
    shortcut: keys("L", { shift: true }),
  },
  {
    value: "center",
    label: say("Center"),
    icon: TextAlignCenter,
    shortcut: keys("E", { shift: true }),
  },
  {
    value: "right",
    label: say("Align right"),
    icon: TextAlignEnd,
    shortcut: keys("R", { shift: true }),
  },
  {
    value: "both",
    label: say("Justify"),
    icon: TextAlignJustify,
    shortcut: keys("J", { shift: true }),
  },
];
interface MarkKey {
  name: MarkName;
  label: string;
  icon: IconNode;
  key?: string;
}

const MARKS: readonly MarkKey[] = [
  { name: "bold", label: say("Bold"), icon: Bold, key: keys("B") },
  { name: "italic", label: say("Italic"), icon: Italic, key: keys("I") },
  { name: "underline", label: say("Underline"), icon: Underline, key: keys("U") },
];
const EFFECTS: readonly MarkKey[] = [
  {
    name: "strike",
    label: say("Strikethrough"),
    icon: Strikethrough,
    key: keys("X", { shift: true }),
  },
  { name: "superscript", label: say("Superscript"), icon: Superscript },
  { name: "subscript", label: say("Subscript"), icon: Subscript },
];
/** In 240ths of a line. */
const LINES = [240, 276, 360, 480];
const SPACINGS: readonly Choice<string>[] = LINES.map((line) => ({
  value: String(line),
  label: new Intl.NumberFormat(language()).format(line / 240),
}));
const TABLE_COLUMNS = 8;
const TABLE_ROWS = 6;

function marksWell(editor: Editor, legend: string, marks: readonly MarkKey[]) {
  const well = wellOf(
    legend,
    marks.map((mark) => ({
      label: mark.label,
      icon: mark.icon,
      ...(mark.key ? { shortcut: mark.key } : {}),
      press: () => {
        commands.toggleMark(editor, mark.name);
        refresh();
      },
    })),
  );
  const refresh = () => well.show(marks.map((mark) => ({ active: editor.isActive(mark.name) })));
  refresh();
  return { element: well.element, refresh };
}

/** A file keeps the built-in names of styles in small letters, and apps show them with a capital. */
function nameOf(name: string): string {
  return name.charAt(0).toUpperCase() + name.slice(1);
}

function sizeText(size: number): string {
  return String(Math.round(size * 2) / 2);
}

export class DocxBar {
  private readonly bar: Bar;
  private editor: Editor | undefined;
  private view: DocxView = { styles: null, numbering: null, page: null };
  private refresh: (() => void) | undefined;
  private pending = 0;
  private checked: ProseNode | undefined;
  private tracked = false;

  constructor(
    root: HTMLElement,
    private readonly end: readonly HTMLElement[],
  ) {
    this.bar = new Bar(root, say("Format"));
  }

  get element(): HTMLElement {
    return this.bar.element;
  }

  show(editor: Editor, view: DocxView, canEdit: boolean): void {
    this.editor = editor;
    this.view = view;
    if (!canEdit) {
      this.refresh = undefined;
      this.bar.show(readOnly(), gap(), ...this.end);
      return;
    }
    const undoKey = keyOf({
      label: say("Undo"),
      icon: Undo2,
      shortcut: keys("Z"),
      press: () => this.history(undo),
    });
    const redoKey = keyOf({
      label: say("Redo"),
      icon: Redo2,
      shortcut: keys("Z", { shift: true }),
      fits: "narrow",
      press: () => this.history(redo),
    });
    const style = this.stylePicker("mid");
    const marks = marksWell(editor, say("Font style"), MARKS);
    const align = segmentsOf(
      say("Alignment"),
      ALIGNS,
      (value) => commands.setAlign(editor, value),
      "wide",
    );
    const lists = this.listWell("wide");
    const text = panelKey({ label: say("Text"), icon: ALargeSmall, build: () => this.textPanel() });
    const insert = menuKey({
      label: say("Insert"),
      icon: Plus,
      entries: () => this.insertEntries(insert.element),
    });
    const table = menuKey({
      label: say("Table"),
      icon: Table2,
      entries: () => this.tableEntries(),
    });
    const changes = menuKey({
      label: say("Tracked changes"),
      icon: FileDiff,
      align: "end",
      entries: () => {
        const state = editor.state;
        const here = !commands.canDecide(state);
        return [
          {
            label: say("Previous change"),
            icon: ArrowUp,
            disabled: !commands.changeNear(state, false),
            run: () => commands.goToChange(editor, false),
          },
          {
            label: say("Next change"),
            icon: ArrowDown,
            disabled: !commands.changeNear(state, true),
            run: () => commands.goToChange(editor, true),
          },
          "separator",
          {
            label: say("Accept"),
            icon: Check,
            disabled: here,
            run: () => commands.decide(editor, true, false),
          },
          {
            label: say("Reject"),
            icon: X,
            disabled: here,
            run: () => commands.decide(editor, false, false),
          },
          "separator",
          { label: say("Accept all"), run: () => commands.decide(editor, true, true) },
          { label: say("Reject all"), run: () => commands.decide(editor, false, true) },
        ];
      },
    });
    this.refresh = () => {
      const state = editor.state;
      const paragraph = commands.paragraphAt(state);
      undoKey.show({ enabled: undoDepth(state) > 0 });
      redoKey.show({ enabled: redoDepth(state) > 0 });
      style.refresh();
      marks.refresh();
      align.show(commands.alignOf(this.view, paragraph));
      lists.refresh();
      table.element.hidden = !tables.inTable(state);
      if (state.doc !== this.checked) {
        this.checked = state.doc;
        this.tracked = commands.hasChanges(state.doc);
      }
      changes.element.hidden = !this.tracked;
    };
    this.bar.show(
      undoKey.element,
      redoKey.element,
      rule(),
      style.element,
      marks.element,
      align.element,
      lists.element,
      text.element,
      insert.element,
      table.element,
      changes.element,
      gap(),
      ...this.end,
    );
    this.refresh();
  }

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

  private history(command: typeof undo): void {
    const view = this.editor?.view;
    if (view) command(view.state, view.dispatch);
  }

  private styleEntries(after: () => void): MenuEntry[] {
    const editor = this.editor;
    const styles = this.view.styles;
    if (!editor || !styles) return [];
    const current =
      (commands.paragraphAt(editor.state)?.attrs.style as string | null) ?? styles.defaultParagraph;
    const first = (id: string) => (id === styles.defaultParagraph ? 0 : 1);
    return Object.entries(styles.styles)
      .filter(
        ([id, look]) =>
          (look.kind ?? "paragraph") === "paragraph" && (look.quick || id === current),
      )
      .sort(
        ([left, leftLook], [right, rightLook]) =>
          first(left) - first(right) ||
          nameOf(leftLook.name ?? left).localeCompare(nameOf(rightLook.name ?? right), undefined, {
            numeric: true,
          }),
      )
      .map(([id, look]) => ({
        label: nameOf(look.name ?? id),
        checked: id === current,
        look: {
          fontWeight: look.bold ? "650" : "400",
          fontStyle: look.italic ? "italic" : "normal",
          fontSize: `${Math.min(Math.max(look.size ?? 11, 11), 18)}px`,
          ...(look.color ? { color: `#${look.color}` } : {}),
        },
        run: () => {
          commands.setStyle(editor, id);
          after();
        },
      }));
  }

  private stylePicker(fits?: "mid") {
    const picker = pickerOf({
      label: say("Style"),
      width: 148,
      ...(fits ? { fits } : {}),
      entries: () => this.styleEntries(refresh),
    });
    const refresh = () => {
      const editor = this.editor;
      const styles = this.view.styles;
      if (!editor || !styles) {
        picker.show(say("Style"), false);
        return;
      }
      const id =
        (commands.paragraphAt(editor.state)?.attrs.style as string | null) ??
        styles.defaultParagraph ??
        "";
      picker.show(nameOf(styles.styles[id]?.name ?? id ?? "Normal"));
    };
    refresh();
    return { element: picker.element, refresh };
  }

  private listWell(fits?: "wide") {
    const editor = this.editor;
    const { bullets, numbers } = commands.listsOf(this.view);
    const toggle = (list: string | null) => {
      if (!editor || !list) return;
      commands.toggleList(editor, list);
      refresh();
    };
    const well = wellOf(
      say("Lists"),
      [
        {
          label: say("Bullets"),
          icon: List,
          shortcut: keys("8", { shift: true }),
          press: () => toggle(bullets),
        },
        {
          label: say("Numbers"),
          icon: ListOrdered,
          shortcut: keys("7", { shift: true }),
          press: () => toggle(numbers),
        },
      ],
      fits,
    );
    const refresh = () => {
      const paragraph = editor ? commands.paragraphAt(editor.state) : null;
      const list = paragraph?.attrs.numId as string | null | undefined;
      well.show([
        { active: list != null && list === bullets, enabled: bullets !== null },
        { active: list != null && list === numbers, enabled: numbers !== null },
      ]);
    };
    refresh();
    return { element: well.element, refresh };
  }

  private textPanel(): HTMLElement {
    const editor = this.editor;
    if (!editor) return panelOf([]);
    const anchor = this.bar.element;
    const look = commands.textLook(editor.state);
    const rows: PanelRow[] = [];
    if (!fits(anchor, "mid")) rows.push([say("Style"), this.stylePicker().element]);
    const font = pickerOf({
      label: say("Font"),
      width: 200,
      entries: () => {
        const current = commands.fontOf(this.view, editor.state);
        const names = FONTS.includes(current) ? FONTS : [current, ...FONTS];
        return names.map((name) => ({
          label: name,
          checked: name === current,
          look: { fontFamily: `"${name}", sans-serif` },
          run: () => {
            commands.setTextLook(editor, { font: name });
            font.show(name);
          },
        }));
      },
    });
    font.show(commands.fontOf(this.view, editor.state));
    rows.push([say("Font"), font.element]);
    const size = stepperOf(say("Size"), (larger) => {
      const now = commands.sizeOf(this.view, editor.state);
      const next = larger
        ? (SIZES.find((step) => step > now) ?? now)
        : (SIZES.findLast((step) => step < now) ?? now);
      commands.setTextLook(editor, { size: String(Math.round(next * 2)) });
      showSize();
    });
    const showSize = () => {
      const now = commands.sizeOf(this.view, editor.state);
      size.show(sizeText(now), now > (SIZES[0] ?? 0), now < (SIZES.at(-1) ?? 0));
    };
    showSize();
    rows.push([say("Size"), size.element]);
    rows.push([
      say("Colour"),
      swatchesOf(say("Colour"), COLORS, look.color ?? null, (color) =>
        commands.setTextLook(editor, { color }),
      ),
    ]);
    rows.push([
      say("Highlight"),
      swatchesOf(say("Highlight"), HIGHLIGHTS, look.highlight ?? null, (highlight) =>
        commands.setTextLook(editor, { highlight }),
      ),
    ]);
    rows.push([say("Effects"), marksWell(editor, say("Effects"), EFFECTS).element]);
    if (!fits(anchor, "wide")) {
      const align = segmentsOf(say("Alignment"), ALIGNS, (value) => {
        commands.setAlign(editor, value);
        align.show(value);
      });
      align.show(commands.alignOf(this.view, commands.paragraphAt(editor.state)));
      rows.push([say("Alignment"), align.element]);
      const lists = this.listWell();
      rows.push([say("Lists"), lists.element]);
    }
    const step = (by: 1 | -1) => () => {
      commands.indent(editor, this.view, by);
      showIndent();
    };
    const indent = wellOf(say("Indent"), [
      { label: say("Decrease indent"), icon: ListIndentDecrease, press: step(-1) },
      { label: say("Increase indent"), icon: ListIndentIncrease, press: step(1) },
    ]);
    const showIndent = () =>
      indent.show([
        { enabled: commands.canIndent(editor.state, this.view, -1) },
        { enabled: commands.canIndent(editor.state, this.view, 1) },
      ]);
    showIndent();
    rows.push([say("Indent"), indent.element]);
    const spacing = segmentsOf(say("Spacing"), SPACINGS, (line) => {
      commands.setLine(editor, line);
      spacing.show(line);
    });
    spacing.show(commands.lineOf(editor.state));
    rows.push([say("Spacing"), spacing.element]);
    return panelOf(
      rows,
      footOf([
        {
          label: say("Clear formatting"),
          icon: RemoveFormatting,
          press: () => commands.clearLook(editor),
        },
      ]),
    );
  }

  private insertEntries(anchor: HTMLElement): MenuEntry[] {
    const editor = this.editor;
    if (!editor) return [];
    const link = commands.linkAt(editor.state);
    const linking: MenuEntry[] = link
      ? [
          {
            label: say("Edit link…"),
            icon: Link,
            run: () => openPanel(anchor, this.linkPanel(editor, link.href)),
          },
          {
            label: say("Remove link"),
            icon: Unlink,
            run: () => commands.dispatch(editor, commands.unlinked(editor.state)),
          },
        ]
      : [
          {
            label: say("Link…"),
            icon: Link,
            run: () => openPanel(anchor, this.linkPanel(editor, "")),
          },
        ];
    return [
      {
        label: say("Table"),
        icon: Table2,
        run: () => {
          const grid = sizeGridOf({
            label: say("Table size"),
            columns: TABLE_COLUMNS,
            rows: TABLE_ROWS,
            pick: (columns, rows) =>
              commands.dispatch(editor, tables.tableInserted(editor.state, columns, rows)),
          });
          openPanel(anchor, grid);
          if (!editor.view.hasFocus()) grid.querySelector("button")?.focus();
        },
      },
      ...linking,
      "separator",
      {
        label: say("Page break"),
        icon: SeparatorHorizontal,
        disabled: tables.inTable(editor.state),
        run: () => commands.dispatch(editor, commands.pageBroken(editor.state)),
      },
    ];
  }

  private linkPanel(editor: Editor, href: string): HTMLElement {
    const note = document.createElement("span");
    note.className = "bar-panel-note";
    const field = fieldOf(say("Address"), href, (text) => {
      const address = commands.linkAddress(text);
      if (!address) {
        note.textContent = say("Type a web or e-mail address.");
        if (!note.isConnected)
          panel.firstElementChild?.append(document.createElement("span"), note);
        return;
      }
      closePopover();
      commands.dispatch(editor, commands.linked(editor.state, address, text.trim()));
    });
    const panel = panelOf([[say("Address"), field]]);
    return panel;
  }

  private tableEntries(): MenuEntry[] {
    const editor = this.editor;
    if (!editor) return [];
    const change = (made: (state: EditorState) => Transaction | null) => () =>
      commands.dispatch(editor, made(editor.state));
    return [
      {
        label: say("Insert row above"),
        icon: ArrowUpToLine,
        run: change((state) => tables.rowAdded(state, false)),
      },
      {
        label: say("Insert row below"),
        icon: ArrowDownToLine,
        run: change((state) => tables.rowAdded(state, true)),
      },
      {
        label: say("Insert column left"),
        icon: ArrowLeftToLine,
        run: change((state) => tables.columnAdded(state, false)),
      },
      {
        label: say("Insert column right"),
        icon: ArrowRightToLine,
        run: change((state) => tables.columnAdded(state, true)),
      },
      "separator",
      { label: say("Delete row"), icon: Trash2, run: change(tables.rowRemoved) },
      { label: say("Delete column"), icon: Trash2, run: change(tables.columnRemoved) },
      { label: say("Delete table"), icon: Trash2, run: change(tables.tableRemoved) },
    ];
  }
}
