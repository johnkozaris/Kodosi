---
name: kodosi-canvas
description: "Show files, images, websites, running apps, or a remote desktop on the canvas beside this Kodosi terminal, and drive the shared browser. Use when the user asks to see or preview something, when a table, plan with options, comparison, or diagram reads better visually, when you need a browser or to use a website, or when the user wants to view or control a local app or screen from another device."
---

# Kodosi canvas

Every Kodosi terminal has a live canvas beside it for files, a shared browser, and remote desktop
viewing of this computer (owner's other devices only). Anyone watching the terminal sees files and
the browser in real time on every device, including your browser cursor and live document edits
under your name, and can click, type, and edit alongside you.

## Showing files, URLs, and screen

- `kodosi show <file>`: adds one or more files and opens the canvas to the first (even if closed).
  Edits refresh live, and re-running `kodosi show <file>` brings it back to the front without
  duplicating it; run it again after edits you want the user to see, because another item may be
  focused. When revising a plan, report, or page, overwrite the existing file in place instead of
  creating extra copies.
- `kodosi show <address>`: opens a website or local dev server (such as
  `kodosi show http://localhost:5173`) in the shared browser.
- `kodosi show browser`: focuses the shared browser as-is without reloading. Use this after
  browser work when you want the user to look.
- `kodosi show screen` and `kodosi show window <words>` (such as `kodosi show window Simulator`):
  stream this computer's screen or a matching app window as a remote desktop so the user can view
  and control local apps, simulators, or system dialogs from another device (falls back to the
  whole screen if none matches). Works once screen sharing is enabled in Kodosi settings here;
  friends who share the terminal never see the screen.
- After showing something, summarize it in one line. Don't ask the user to open a file or URL
  manually unless it can't be avoided.
- Supported formats: HTML, Markdown, plain text, and diffs (Markdown, text, and diffs use Kodosi's
  styling), images (PNG, JPEG, GIF, WebP, SVG), and live-editable Office/PDF files (`.docx`,
  `.xlsx`, `.pptx`, `.pdf`; see `kodosi skill documents`).
- Local HTML and Markdown pages run sandboxed with no network or local file access (`img-src`
  allows only `data:` and `blob:`). Relative or disk paths like `<img src="shot.png">` or
  `![](shot.png)` will not load. Inline CSS, JS, and `<svg>`, or base64-encode images as
  `data:image/png;base64,...` URIs; for screenshots or large images, show the image file directly
  with `kodosi show shot.png` instead. HTML pages that load from the network open in the shared
  browser.
- `kodosi skill page` prints a Kodosi-styled light/dark HTML starter template. Use it for plans
  with options, comparisons, or reports, and clearly mark your recommendation.
- `kodosi canvas` lists current items. `kodosi canvas remove <name>` removes one item (also
  accepts `browser` or `screen`), and `kodosi canvas remove --all` clears all files. Remove stale,
  finished, or unrelated items when moving to another task so the canvas doesn't get crowded and
  the user doesn't get lost. Removing an item from the canvas does not delete the file on disk.

## Shared browser

When you need a browser or to use a website, use `kodosi browser <command>` instead of another
browser tool. It drives this terminal's shared browser via `agent-browser` (starting on first use).
The user sees your cursor and actions live while the canvas is open; call `kodosi show browser` to
bring it into view when they should look.

- `kodosi browser open https://example.com`, `kodosi browser back`,
  `kodosi browser tab new https://example.com`, `kodosi browser tab close`, and
  `kodosi browser wait --load networkidle`: navigate, manage tabs, and wait (`kodosi browser --help`
  for all commands).
- `kodosi browser snapshot -i`: lists interactive elements with handles like `@e2`. Handles
  invalidate whenever the page changes, so take a fresh snapshot after updates or whenever Kodosi
  reports that a user interacted with the browser.
- `kodosi browser click @e2`, `kodosi browser fill @e3 "text"`, `kodosi browser press Enter`,
  `kodosi browser scroll down`, `kodosi browser get text @e1`, `kodosi browser get url`, and
  `kodosi browser screenshot shot.png`: interact with and inspect the page.
- `kodosi browser scrollintoview @e2`, `kodosi browser hover @e2`, and
  `kodosi browser highlight @e2`: bring an element into view, point at it with your live cursor, or
  visually highlight it on the page for the user.
- `kodosi browser upload @e4 report.pdf`: attaches a file to a file input. Downloads save into this
  terminal's working directory and appear on the canvas automatically.
- The browser sleeps after 10 minutes idle and wakes up with existing logins intact.
- If a page requires sign-in, ask the user to sign in on the canvas. Never ask for a password or
  type credentials you discovered.
