---
name: kodosi-canvas
description: "Display images, files, websites, or running apps on the canvas beside this Kodosi terminal, and drive the shared browser there. Use when the user asks to see or preview something, when a table, plan with options, comparison, or diagram reads better visually than in a terminal, when you need a browser to open, test, interact with, or screenshot a web page, or when the user wants to see or use this computer's screen or an app window from another device."
---

# Kodosi canvas

Every Kodosi terminal has a canvas beside it containing a file viewer and a shared browser. The
user (and anyone sharing the terminal) sees it in real time on each device where they watch the
terminal, including your browser cursor and live document edits under your name. People can click,
type, and edit alongside you.

## Showing files and URLs

- `kodosi show <file>`: adds one or more files to the canvas and opens the canvas for the user
  (even if they previously closed it). Subsequent edits to that file refresh live, so only run
  `show` once per file.
- `kodosi show <address>`: opens a website or local dev server (such as
  `kodosi show http://localhost:5173`) in the canvas's shared browser.
- `kodosi show browser`: brings up the shared browser as-is without reloading the page. Use this
  after finishing browser work when you want the user to look.
- `kodosi show screen`: shows this computer's screen on the canvas to its owner's other devices,
  where they watch and control it. `kodosi show window <words>` shows the window whose app or title
  holds the words (such as `kodosi show window Simulator`), or the whole screen when none does.
  Use it when the user wants to see or use something outside the terminal, such as an app, a
  simulator, or a system dialog. It works once the person at the computer turns on screen sharing
  in Kodosi's settings there, and friends who share the terminal never see the screen.
- After calling `show`, summarize what you opened in one line. Never ask the user to open a file
  or URL themselves.
- Supported canvas formats: HTML, Markdown, and plain text (Markdown and text use Kodosi's
  styling), images (PNG, JPEG, GIF, WebP, SVG), and live-editable Office/PDF files (`.docx`,
  `.xlsx`, `.pptx`, `.pdf`; see `kodosi skill documents`).
- Local HTML pages can run inline JavaScript, but have no network access and can't load external
  files. Inline all CSS, JS, and images (using inline SVG or `data:` URLs). A page that loads from
  the network opens in the shared browser instead.
- `kodosi skill page` prints a Kodosi-styled light/dark HTML starter template. Use it for plans
  with options, comparisons, or reports, and clearly mark your recommendation.
- `kodosi canvas` lists current canvas items. `kodosi canvas remove <name>` removes one item, and
  `kodosi canvas remove --all` clears all files. Removing a file from the canvas does not delete
  the file on disk.
- Users can select text on the canvas and send it to you along with its source file or URL.

## Shared browser

`kodosi browser <command>` runs an `agent-browser` command inside this terminal's shared browser.
The browser launches automatically on the first command. The user sees your actions while the
canvas is open; call `kodosi show browser` to bring it into view.

- `kodosi browser open https://example.com`: navigates to a URL.
- `kodosi browser snapshot -i`: lists interactive page elements with handles like `@e2`. Handles
  invalidate whenever the page changes, so take a fresh snapshot after updates or whenever Kodosi
  reports that a user interacted with the browser. In PowerShell, drop the `@` prefix (`e2`) so
  PowerShell doesn't parse it as splatting.
- `kodosi browser click @e2`, `kodosi browser fill @e3 "text"`, `kodosi browser press Enter`, and
  `kodosi browser scroll down`: interact with the page.
- `kodosi browser get text @e1` and `kodosi browser get url`: inspect page content and URL.
- `kodosi browser screenshot shot.png`: saves a screenshot.
- `kodosi browser upload @e4 report.pdf`: attaches a file to a file input. Downloads save into this
  terminal's working directory and appear on the canvas automatically.
- `kodosi browser wait --load networkidle`, `kodosi browser back`,
  `kodosi browser tab new https://example.com`, and `kodosi browser tab close`: control waiting,
  history, and tabs. Run `kodosi browser --help` for the full command list.
- The browser sleeps after 10 minutes idle and wakes up with existing logins intact.
- If a page requires authentication, ask the user to sign in on the canvas. Never ask for a
  password or type credentials you discovered.
