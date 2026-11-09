---
name: kodosi-documents
description: "Read and edit .docx documents, .xlsx sheets, .pptx presentations, PDFs and text files (.txt, .json, .yaml, .yml, .toml, .xml) while the people of a Kodosi terminal work on them, so each edit shows at once in your name and nobody overwrites anybody. Use it for every .docx, .xlsx, .pptx and .pdf file, in place of a library or a script that writes these files, and for a text file that people have open on the canvas."
---

# Kodosi documents

`kodosi doc` reads and edits a live document. When people have the file open on the canvas,
they see your edit at once in your name, and you see theirs. With nobody there, the command
opens the file and saves it. Each part of the file that you do not change stays as it was. A
text file keeps its line endings and its byte order mark.

A text file of more than 10,000 lines or 512 KB is too long to edit live: `kodosi doc` says
so, and you edit it with your own tools.

Show a file on the canvas with `kodosi show <file>`, and tell the person why they should
look.

## Commands

- `kodosi doc new <file>` makes a .docx, .xlsx or .pptx file. `--from <file>` starts it from
  Markdown for a document or a presentation, and from CSV or JSON for a sheet.
- `kodosi doc read <file>` prints the content with its ids, cell names or line numbers, and
  the rev. `--where <place>` reads one block id, a range such as `Sheet1!A1:C9`, a slide id, a
  page number, or lines of a text file such as `12`, `10-20` or `10-`. A read of a text file
  gives at most 2,000 lines, and cuts a line after 2,000 characters.
- `kodosi doc guide <docx|xlsx|pptx|pdf|text> [op]` prints each op with its fields and the
  shape of its JSON.
- `kodosi doc apply <file> --ops '<json>' --base <rev>` sends a list of ops as one change: all
  of them or none. `--ops` also takes the path of a JSON file. Use the rev of your last read.
- `--track` writes the ops of a .docx file as tracked changes in your name, so that a person
  accepts or rejects them. Use it when the person should decide. The ops `acceptChanges` and
  `rejectChanges` decide tracked changes when a person asks you to.
- `kodosi doc check <file>` lists formula errors, shapes that leave their slide, empty
  placeholders, tracked changes and notes. Run it after each change.
- `kodosi doc selection <file>` says who is here and what each person has selected, as `read`
  names it: a block id and the selected words, a range such as `Sheet1!B2:C4`, a slide and a
  shape, a page and a note, or the lines and the column of a text file and the selected text.
  Use it when a person says "this one" or "here".
- `kodosi doc render <file>` draws each page, slide, sheet or screen of text as people see it,
  and prints the paths of the pictures. `--out <folder>` puts them in that folder.

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
- Text file: `{ "op": "replaceText", "find": "port: 80\n", "with": "port: 8080\n" }` finds
  text in one place, across lines too; give more of the text around it, or `"all": true`.
  `insertLines`, `replaceLines` and `deleteLines` name lines by the numbers of your last read,
  and each op of one change counts in the text as you read it.

## How to work

1. Read, and keep the rev.
2. Apply small changes with `--base <rev>`. People watch each one arrive.
3. When the answer says that someone was faster or that the file opened again, read again and
   build your ops on the new content. Do not force your change. Ops that name a cell, a shape,
   a note or a field never wait for this, and neither do `replaceText` and lines added at the
   end of a text file.
4. Run `kodosi doc check`. After a change of layout, run `kodosi doc render` and look at the
   pictures before you say that the work is done.
