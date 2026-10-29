---
name: kodosi-canvas
description: "Show pages, pictures, PDFs and Office files to the people of a Kodosi terminal on its canvas, and use a browser that they see and can use too. Use it when a result is easier to see than to read in the terminal."
---

# Kodosi canvas

Each Kodosi terminal has a canvas. The people in the terminal open it next to the terminal,
on every computer and phone where they see the terminal. Show them what is easier to see
than to read: a plan with choices, a table, a diagram, a picture, a report, a page of the
app that you build.

The canvas never opens by itself. When you show something, tell the person what it is and
why they should look.

## Show files

- `kodosi show <file>`: show one or more files on the canvas of this terminal. The newest
  comes first, and the canvas keeps 20. When you change a file, the canvas shows the new
  version, so show a file once and then edit it.
- Pages are HTML, Markdown and text files. Markdown and text get the look of Kodosi.
  Pictures (PNG, JPEG, GIF, WebP, SVG), PDFs and Office files show as they are.
- A page runs in a closed frame. It can run its own scripts, but it cannot load anything
  from the network or open other files. Put the styles, scripts and pictures into the page
  itself; use inline SVG or data: addresses for pictures.
- `kodosi skill page` prints a page with the look of Kodosi: its colors, cards, tags and
  tables, in light and dark. Start from it for a page that people decide on.
- Write pages where you can write files, for example the temporary folder or the project.
- `kodosi canvas` lists what the canvas shows. `kodosi canvas remove <name>` takes an item
  off, and `kodosi canvas remove --all` takes all files off.

A good page has one subject: a title, a line that says what the page is for, the facts, and
what the person must decide. Mark your recommendation.

## Use the shared browser

`kodosi browser <command>` runs an agent-browser command on the browser of this terminal.
The browser starts at the first command. It sleeps after 10 minutes with no use and wakes
with the same sign-ins. The people see it on the canvas, and they can click and type in it.

- `kodosi browser open https://example.com`: go to an address.
- `kodosi browser snapshot -i`: the parts of the page that you can use, each with a
  reference such as `@e2`. Take a new snapshot after each change of the page.
- `kodosi browser click @e2`, `kodosi browser fill @e3 "text"`,
  `kodosi browser press Enter`, `kodosi browser scroll down`.
- `kodosi browser get text @e1` and `kodosi browser get url` read the page.
- `kodosi browser screenshot shot.png` saves a picture; show it with `kodosi show shot.png`.
- `kodosi browser wait --load networkidle`, `kodosi browser back`,
  `kodosi browser tab new https://example.com` and `kodosi browser tab close`.
- `kodosi browser --help` lists every command.

When a person used the browser since your last command, Kodosi says so. Take a new
snapshot before you use references again.

When a page asks for a sign-in, ask the person to sign in on the canvas. Never ask for a
password, and never type one that you found.
