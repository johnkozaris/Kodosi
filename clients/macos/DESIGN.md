---
name: Kodosi for macOS
description: A warm, living workbench for people, agents and terminals.
colors:
  ground: "#110E0C"
  ground-light: "#F0E9DC"
  surface: "#181411"
  surface-light: "#F7F2E8"
  raised: "#251D19"
  raised-light: "#FEFBF5"
  lifted: "#2E241F"
  lifted-light: "#FFFDF9"
  well: "#0C0A08"
  well-light: "#E8E0D2"
  terminal: "#0D0B09"
  hairline: "#3B2F29"
  hairline-light: "#D9CCBA"
  ink: "#F1E9E3"
  ink-light: "#25160E"
  inkMuted: "#B3A094"
  inkMuted-light: "#5F4838"
  inkFaint: "#8C7A6F"
  inkFaint-light: "#8A7262"
  accent: "#DB8A62"
  accent-light: "#B85A26"
  accentSoft: "#3E281C"
  accentSoft-light: "#F4DEC8"
  onAccent: "#160E0A"
  onAccent-light: "#FFFAF3"
  glowAmber: "#F2BC55"
  glowOrange: "#F08A4B"
  ready: "#7DBB99"
  ready-light: "#2F7D54"
  caution: "#DDB866"
  caution-light: "#8A6A1C"
  danger: "#E77A75"
  danger-light: "#B23A32"
typography:
  large: {fontFamily: "system-ui", fontSize: "28pt", fontWeight: 600, letterSpacing: "-0.6pt"}
  title: {fontFamily: "system-ui", fontSize: "20pt", fontWeight: 600, letterSpacing: "-0.4pt"}
  headline: {fontFamily: "system-ui", fontSize: "15pt", fontWeight: 600}
  subhead: {fontFamily: "system-ui", fontSize: "13pt", fontWeight: 600}
  callout: {fontFamily: "system-ui", fontSize: "14pt"}
  body: {fontFamily: "system-ui", fontSize: "13pt"}
  footnote: {fontFamily: "system-ui", fontSize: "12pt"}
  caption: {fontFamily: "system-ui", fontSize: "11pt", fontWeight: 500}
  mono: {fontFamily: "ui-monospace", fontSize: "12.5pt"}
rounded: {xs: "6pt", sm: "8pt", md: "10pt", lg: "14pt", xl: "18pt", sheet: "22pt", tile: "12pt"}
spacing: {sidebar: "252pt", rail: "78pt", frame-inset: "8pt", header: "54pt", page: "36pt"}
motion:
  spring: {stiffness: 420, damping: 34, mass: 0.9}
  soft: {stiffness: 260, damping: 30}
  snappy: {stiffness: 620, damping: 40}
  fade: "200ms ease-out"
components:
  button-primary: {backgroundColor: "{colors.accent}", textColor: "{colors.onAccent}", rounded: "{rounded.md}", height: "32pt"}
  button-secondary: {backgroundColor: "{colors.lifted}", textColor: "{colors.ink}", rounded: "{rounded.md}", height: "32pt"}
  button-tinted: {backgroundColor: "{colors.accentSoft}", textColor: "{colors.accent}", rounded: "{rounded.md}"}
  input: {backgroundColor: "{colors.well}", textColor: "{colors.ink}", rounded: "{rounded.md}", height: "34pt"}
  segmented-track: {backgroundColor: "{colors.well}", rounded: "capsule", padding: "3pt"}
  segmented-thumb: {backgroundColor: "{colors.raised}", rounded: "capsule"}
  terminal-tile: {backgroundColor: "{colors.terminal}", rounded: "{rounded.tile}"}
---
# Design System: Kodosi for macOS

## Overview
A warm workbench lit from the top left. A source list rests on the ground, one framed surface holds the work, raised objects rest on it and inputs sink into wells. The copper cursor is the sign of life: it ends the wordmark, marks the slot for a new terminal, and new terminals open from it. Each screen starts with the objects of its job, and text is short.

## Colors
Copper `accent` means "act here" or "selected". `glowAmber` flashes behind a fresh change and `glowOrange` is a breathing dot for work or for a change out of view. Green `ready` means done or verified and is never a person's color. Plain tokens are dark and `-light` tokens their light pair. Use `AppTheme` roles, not literals. Terminals stay dark in both themes (`terminalScope`).

## Typography
Use `AppTextStyle`: `large` for page titles, `title` for sheets, `headline` for object names, `body` for conversation and rows, `footnote` and `caption` for facts, monospace for code and paths. Labels use sentence case. The wordmark is a picture.

## Layout
No title bar. The source list is 252 pt, or a 78 pt rail of marks. The main frame is inset 8 pt with an 18 pt radius, and pages are one centered column with 36 pt margins. A room has a 54 pt header, a canvas, and the conversation on a 300–540 pt sheet from the right; below 720 pt it shows one of the two. When chrome moves, the terminal takes its final size at once and does not animate.

## Elevation & Depth
Three levels: resting, lifted (hover, focused composer, popovers) and floating (sheets, notices, Go to, the join card). Raised shapes have a light top-left rim, a dark bottom-right rim and two shadows falling down and right; wells have an inner shadow. Sheets and popovers are the app's own surfaces: a sheet rises in the middle of the dimmed window, a popover rises from its source, and a click outside or Escape dismisses either, unless the sheet waits for a decision.

## Shapes
Capsules for tags and pills, rounded rectangles (8, 10 and 12 pt) for buttons, squircles for agents, rooms and kinds of things, circles for people, and a dashed outline for a slot you can fill.

## Components
- **Button:** a flat face lit from the top left, with a fine light top and left edge and a tight shadow; a dark hairline in the light appearance and a darker bottom-right edge in the dark one. Primary is copper; secondary and tinted are quiet. Hover shifts the face; a press darkens and shrinks it. Ghost buttons are words with a hover fill, and symbol-only buttons are round.
- **Agent mark:** a squircle in the program's color with its glyph (a shell shows a prompt and the copper cursor), pale when minimized. A ring of cells shows only work: the cursor types round it while the program works, and a percent fills it from the top. The status sign shows why the program stopped. Solid colors, no glow; the mark squashes a little at each change.
- **Status sign:** one sign at the end of a row, card, tab or tile header, and the only sign that a program waits: a copper hand, question or key while waiting (the hand waves once), a self-drawing green check for done and a `caution` triangle for failed until opened, and a breathing copper dot for a bell or an agent that stopped without a report.
- **Person avatar:** a circle colored from the person's identifier; your own is copper.
- **Room sigil:** a squircle with four tiles, colored and lit from the room's identifier.
- **Selection pill:** a raised pill with a copper caret that slides between source list rows.
- **Sliding pills:** segmented controls and terminal tabs use a sliding raised thumb. The room dock shows icons and names the selected section; a section that changed out of view gets a breathing dot.
- **Terminal tile:** a rounded tile with a header whose actions stay faint until hover or selection. At the prompt, the start-command marks come first. A new terminal opens from a copper veil, and a person who joins shows as a card for a few seconds.
- **Launcher:** the terminal about to open, dark in both appearances. Its band has a tab for each kind of terminal (plain terminal first, missing agents behind a plus, a new agent named in copper) and names the computer at its right end. Its line reads `[folder] ❯ program [account] [model]` and then the first prompt: the folder, account and model are small pop-up buttons with an icon and a chevron, and each opens a popover aligned to its button. An account is a person mark in the color of its sign-in with its name and e-mail, dashed until someone signs in; effort is a row of rising bars in the model popover and on the model button. The first prompt has a block cursor and a hint ("Type a task") that types itself once. Start and Resume sit under the terminal.
- **Folder header:** a faint label above a folder's terminals; closed, it shows the number of its terminals and the sign of a terminal in it that waits; on hover, a plus for a new terminal and, for a Git repository, a branch button that opens a one-field panel.
- **Conversation:** your messages are bubbles on the right; others show an avatar and a name; agents show their mark, name, owner and terminal. Mentions are tinted chips, a terminal mention opens the terminal, and a message that mentions you has a tinted row.
- **Composer:** a lifted capsule with a copper halo on focus; "@" lists people and terminals.
- **Tasks:** cards in "Up for grabs", "In progress" and "Done"; a changed card washes warm once.
- **Go to (⌘K):** one field in a well finds terminals, rooms, people and actions; the chosen row is a raised pill with a copper notch.

Working text shimmers, counts roll and symbols replace in place. With reduced motion, from the system or from Settings, every change is instant and nothing loops.

## Do's and Don'ts
- **Do** use the springs in `AppTheme.Motion`, and animate only for an action or a real change of state.
- **Do** keep native focus, labels, stable accessibility identifiers and selectable message text.
- **Do** keep hidden terminal surfaces inactive.
- **Don't** clip or scale the terminal surface; inset it in its tile.
- **Don't** put a glow, blur, colored shadow, gradient, gloss or bright rim on a mark, cursor or button, or use copper as decoration.
- **Don't** add explanatory text where an object, a state or one short line is enough.
