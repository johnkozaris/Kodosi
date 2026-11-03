---
name: kodosi-documents
description: "Read and edit .docx documents, .xlsx sheets, .pptx presentations and PDFs while the people of a Kodosi terminal work on them, so each edit shows at once in your name and nobody overwrites anybody. Use it for every .docx, .xlsx, .pptx and .pdf file, in place of a library or a script that writes these files."
---

# Kodosi documents

`kodosi doc` reads and edits a live document. When people have the file open on the canvas,
they see your edit at once in your name, and you see theirs. With nobody there, the command
opens the file and saves it. Each part of the file that you do not change stays as it was.

Show a file on the canvas with `kodosi show <file>`, and tell the person why they should
look.

## Commands

- `kodosi doc new <file>` makes a .docx, .xlsx or .pptx file. `--from <file>` starts it from
  Markdown for a document or a presentation, and from CSV or JSON for a sheet.
- `kodosi doc read <file>` prints the content with its ids and cell names, and the rev.
  `--where <place>` reads one block id, a range such as `Sheet1!A1:C9`, a slide id or a page
  number.
- `kodosi doc guide <docx|xlsx|pptx|pdf> [op]` prints each op with its fields and the shape
  of its JSON.
- `kodosi doc apply <file> --ops '<json>' --base <rev>` sends a list of ops as one change: all
  of them or none. `--ops` also takes the path of a JSON file. Use the rev of your last read.
- `--track` writes the ops of a .docx file as tracked changes in your name, so that a person
  accepts or rejects them. Use it when the person should decide. The ops `acceptChanges` and
  `rejectChanges` decide tracked changes when a person asks you to.
- `kodosi doc check <file>` lists formula errors, shapes that leave their slide, empty
  placeholders, tracked changes and notes. Run it after each change.
- `kodosi doc selection <file>` says who is here and what each person has selected. Use it
  when a person says "this one" or "here".
- `kodosi doc render <file>` draws each page, slide or sheet as people see it, and prints the
  paths of the pictures. `--out <folder>` puts them in that folder.

## Ops

An op is a JSON object with its name in `op`:

```json
[
  { "op": "replaceText", "find": "Q3", "with": "Q4" },
  { "op": "insertParagraph", "text": "Next steps", "style": "Heading2", "after": "<block id>" }
]
```

- Sheet: `{ "op": "setCell", "sheet": "Sheet1", "cell": "B4", "formula": "=SUM(B2:B3)" }`.
  `read` then prints the value that the formula gives.
- Presentation: `{ "op": "addSlide" }`, then read again for the ids of the new slide and its
  shapes, and `setText` them.
- PDF: `{ "op": "setField", "name": "Name", "value": "Ana" }`. `redact` removes what a page
  shows for good, and the file saves at once.

## How to work

1. Read, and keep the rev.
2. Apply small changes with `--base <rev>`. People watch each one arrive.
3. When the answer says that someone was faster or that the file opened again, read again and
   build your ops on the new content. Do not force your change. Ops that name a cell, a shape,
   a note or a field never wait for this.
4. Run `kodosi doc check`. After a change of layout, run `kodosi doc render` and look at the
   pictures before you say that the work is done.
