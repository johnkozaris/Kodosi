---
name: kodosi-documents
description: "Read and edit Word, Excel, PowerPoint and PDF files while the people of a Kodosi terminal work on them, so each edit shows at once in your name and nobody overwrites anybody. Use it for every .docx, .xlsx, .pptx and .pdf file, in place of any other tool that writes these files."
---

# Kodosi documents

`kodosi doc` reads and edits a live document. When people have the file open on the canvas,
they see your edit at once in your name, and you see theirs. With nobody there, the command
opens the file and saves it.

Show a file on the canvas with `kodosi show <file>`, and tell the person why they should
look.

## Commands

- `kodosi doc read <file>` prints the content with its ids and cell names, and the rev.
- `kodosi doc read <file> --where <place>` reads a block id, a range such as `Sheet1!A1:C9`,
  a slide id or a page number.
- `kodosi doc apply <file> --ops '<json>' --base <rev>` sends named ops as one change, all or
  none. Use the rev that your last read printed.
- `kodosi doc apply <file> --ops ops.json --base <rev> --track` writes the ops of a Word file
  as tracked changes in your name.
- `kodosi doc check <file>` lists formula errors, shapes that leave the slide and other
  problems. Run it after each edit.
- `kodosi doc selection <file>` says who is here and what each person has selected, so you
  can tell what "this one" means.
- `kodosi doc guide <word|excel|powerpoint|pdf>` prints each op with its fields, and
  `kodosi doc guide excel setCell` prints one op.
- `kodosi doc new <file>` makes a Word, Excel or PowerPoint file, and `--from <file>` starts
  it from Markdown, CSV or JSON.

## How to work

Read, then apply with the rev of that read. When the answer says that the positions moved,
read again and build your ops on the new content: somebody else edited it first. Ops that name
a cell or a shape by its id do not wait for this.

Send small changes. People watch each one arrive.
