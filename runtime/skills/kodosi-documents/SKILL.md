---
name: kodosi-documents
description: "Read and edit .docx documents, .xlsx spreadsheets, .pptx presentations, PDFs, and text files (.txt, .json, .yaml, .yml, .toml, .xml) live alongside people in a Kodosi terminal so edits appear immediately under your name without overwriting each other. Always use this instead of scripts or libraries for .docx, .xlsx, .pptx, and .pdf files, and for any text file currently open on the canvas."
---

# Kodosi documents

`kodosi doc` reads and edits live documents. When people have a file open on the canvas, your
edits appear immediately under your name and you see theirs in real time. Even when no one has it
open, `kodosi doc` saves edits in place while leaving untouched parts byte-intact. Run
`kodosi show <file>` to open the file on the canvas, and run it again after edits you want the
user to see so it comes to the front (another item may be focused).

Text files keep their line endings and BOM. If a file exceeds live editing limits (10,000 lines or
512 KB), `kodosi doc` tells you so you can fall back to normal file tools.

## Commands

- `kodosi doc new <file>`: creates a `.docx`, `.xlsx`, or `.pptx` file. Pass `--from <file>` to
  generate a document or presentation from Markdown, or a spreadsheet from CSV or JSON.
- `kodosi doc read <file>`: prints the document with block IDs, cell coordinates, or line numbers,
  plus the current `rev`. Use `--where <place>` to scope to one block ID, a range like
  `Sheet1!A1:C9`, a slide ID, a page number, or text line ranges like `12`, `10-20`, or `10-`.
  Text reads cap at 2,000 lines and truncate lines past 2,000 characters.
- `kodosi doc guide <docx|xlsx|pptx|pdf|text> [op]`: prints available operations and their JSON
  schemas.
- `kodosi doc apply <file> --ops '<json>' --base <rev>`: applies a batch of operations atomically
  (all or nothing). `--ops` also accepts a path to a JSON file. Pass the `rev` from your last
  read.
- `--track`: records text edits in `.docx` files as tracked-change suggestions users can accept or
  reject (other op types apply directly).
- `kodosi doc check <file>`: reports formula errors, overflowing slide shapes, empty placeholders,
  tracked changes, and notes. Run this after every edit.
- `kodosi doc selection <file>`: lists active viewers and their current selections using `read`
  IDs (block ID + text, cell range like `Sheet1!B2:C4`, slide + shape, page + note, or line/column
  + selected text). Useful whenever someone says "this part" or "here".
- `kodosi doc render <file>`: renders pages, slides, sheets, or text screens as images and prints
  their paths (pass `--out <folder>` to choose the output directory).

## Operations

Each operation is a JSON object identified by `"op"`:

```json
[
  { "op": "replaceText", "find": "Q3", "with": "Q4" },
  { "op": "insertParagraph", "text": "Next steps", "style": "Heading2", "after": "<block id>" }
]
```

- Spreadsheets: `{ "op": "setCell", "sheet": "Sheet1", "cell": "B4", "formula": "=SUM(B2:B3)" }`.
  Subsequent `read` calls show the evaluated value.
- Presentations: `{ "op": "addSlide" }`, then run `read` to get the new slide and shape IDs before
  calling `setText`.
- PDFs: `{ "op": "setField", "name": "Name", "value": "Ana" }`. `redact` permanently removes
  content from the page and saves immediately.
- Text files: `{ "op": "replaceText", "find": "port: 80\n", "with": "port: 8080\n" }` matches
  uniquely (including multi-line strings); include more surrounding context or pass `"all": true`.
  `insertLines`, `replaceLines`, and `deleteLines` reference line numbers from your last read,
  with all ops in one batch indexed against that snapshot.

## Workflow

1. Run `kodosi doc read` and note the `rev`.
2. Send small batches with `--base <rev>` so viewers see changes arrive cleanly.
3. If `apply` reports a concurrent edit or reopen, re-read the file and rebuild your ops against
   the new state instead of forcing them. Ops targeting a specific cell, shape, note, or form
   field, plus `replaceText` and appends to the end of a text file, don't conflict on base rev.
4. Run `kodosi doc check`. After layout changes, also run `kodosi doc render` and inspect the
   rendered images before finishing. Call `kodosi show <file>` if the user should review it on the
   canvas.
