---
name: Kodosi for macOS
description: A warm workbench for people, agents and terminals, lit from the top left.
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
spacing: {sidebar: "252pt", dosi: "420pt", rail: "78pt", frame-inset: "8pt", header: "54pt", page: "36pt"}
motion:
  spring: {stiffness: 420, damping: 34, mass: 0.9}
  soft: {stiffness: 260, damping: 30}
  snappy: {stiffness: 620, damping: 40}
  fade: "200ms cubic-bezier(0.23, 1, 0.32, 1)"
  hover: "120ms ease"
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
The app is a warm workbench lit from the top left. A source list rests on the ground, one framed surface holds the work, raised objects rest on it and inputs sink into wells. The copper cursor appears in three places: it ends the wordmark, marks the slot for a new terminal, and new terminals open from it. Each screen starts with the objects of its job, and text is short.

## Colors
Copper `accent` means "act here" or "selected". `glowAmber` flashes behind a fresh change and `glowOrange` is a breathing dot for work or for a change out of view. Green `ready` means done or verified and is never a person's color. Plain tokens are dark and `-light` tokens their light pair. Use `AppTheme` roles, not literals. Terminals stay dark in both appearances.

## Typography
Use `AppTextStyle`: `large` for page titles, `title` for sheets, `headline` for object names, `body` for conversation and rows, `footnote` and `caption` for facts, monospace for code and paths. Labels use sentence case. The wordmark is a picture.

## Layout
The window has no title bar. The source list can shrink to a rail of marks or widen for Dosi, and its edge resizes it; one inset frame holds the work, and pages are one centered column. A room has a header, a work area, and the conversation on a 300 to 540 pt sheet from the right; below 720 pt it shows one of the two. When chrome moves, the terminal takes its final size at once.

## Elevation & Depth
There are three levels: resting, lifted (hover, focused composer, popovers) and floating (sheets, notices, Go to). Raised shapes have a light top-left rim, a dark bottom-right rim and two shadows falling down and right; wells have an inner shadow. A sheet rises in the middle of the dimmed window and a popover from its source; a click outside or Escape dismisses either, unless the sheet waits for a decision.

## Shapes
Tags and pills are capsules, buttons are rounded rectangles, agents, rooms and kinds of things are squircles, and people are circles. A slot you can fill has a dashed outline.

## Components
- **Agent mark.** The agent mark is a squircle in the program's color with its glyph, pale when minimized. While its program works, a warm light circles the mark's edge, in step on every mark, and with reduced motion the whole edge glows; a percent fills the edge clockwise from the top. Nothing glows outside the mark.
- **Status sign.** Each row, card, tab and tile header ends with one status sign, and it is the only sign that a program waits: a copper hand, question or key while it waits, a green check for done and a `caution` triangle for failed until you open the terminal, and a breathing copper dot for a bell or for an agent that stopped without a report.
- **People and rooms.** A person is a circle colored from their identifier, and yours is copper. A room is a squircle of four tiles colored from its identifier; a copper dot beside it says that someone else wrote in it since you last looked.
- **Buttons.** A button is a rounded rectangle with a flat face lit from the top left. Primary is copper, secondary and tinted are quiet, ghost buttons are words with a hover fill, and a symbol-only button shows its quiet face on hover, the raised face while its popover is open, and the raised face with a copper symbol while it is on. A disabled button keeps a quiet face with faint words. Every fill is solid.
- **Selection.** The selection is a raised pill with a copper caret, and segmented controls and terminal tabs slide a raised thumb. A section dock, such as a room's work area or Settings, shows symbols and names only the chosen section. A section with news shows a breathing copper dot, and its symbol hops once when the news comes.
- **Folders.** A folder row is a folder symbol and the folder's name. When terminals run on more than one computer, a faint caption names each computer above its folders, with the friend's face for a friend's computer. A terminal in a room has a small square in the room's color on the corner of its mark, and a terminal that is shared with people has a grey one. The square is in a ring of the row's ground.
- **Drop.** A place that takes a dragged terminal shows a dashed copper outline, and a grid tile shows a copper edge. On a room's work area, a copper circle with an arrow and one line also springs in. The dragged terminal is a lifted chip with its mark and name.
- **Terminal tile.** A terminal is a rounded tile whose header actions stay faint until hover or selection, and the selected tile has a thin copper edge. The launcher is the terminal about to open, dark in both appearances, and a ghost key at the end of its band names the computer that it opens on.
- **Canvas.** The canvas key is a symbol button after the faces in a terminal's header, and it shows only while the canvas has something. News gives it a breathing copper dot and one hop, and while the canvas is open it is on. The canvas tile has a terminal tile's header with the copper canvas symbol, "Canvas", the terminal's name and a chip for each item: a capsule with the kind's symbol and the name. The chosen chip is a raised pill, a chip with news has a breathing copper dot, and a missing file has faint words. A page rests on the surface; the shared browser's bar has its address in a well and ends with the mark of the agent and its last action.
- **Conversation key.** A hidden conversation folds into a raised capsule at the end of the room's header: the last writer's face, or an agent's mark with its working light, and the count of new messages in copper. The sheet shrinks into it and grows back from it.
- **Conversation.** Your messages are bubbles on the right; others show an avatar and a name, and agents their mark, owner and terminal. Mentions are tinted chips.
- **Dosi.** Its face is the copper cursor doubled into two eyes on a dark squircle. It is still at rest; its eyes move to each new look, follow the pointer in the sidebar and blink once when an answer arrives. Its line is on the ground under a hairline at the bottom of the list, above the account, with no box, and a hover lights it like a row. The face and a monospace line say what Dosi does: Ask Dosi, working with the marks of what it touches, answered with a copper dot, or a question with the question sign. Nothing pops up for Dosi. Opening Dosi folds the rows into named chips on a raised tray and grows the line into the composer; closing reverses it, and a mark that moves is the mark that it lands as. Chips that do not fit become one chip with their count. A turn's steps are pills with the target's mark, a still "thinking" line holds the place of the next step, and a question is a raised card with a button for each answer and a well for your own words, which folds into a line after the answer. When Dosi types in a terminal, the tile's rim lights and a card on the tile says what it typed.
- **Account.** At the bottom of the list, your avatar and name open Settings › Account. The settings button beside them, two sliders, opens the menu: Settings, Friends, Provider Usage, Resume with the mark of each agent whose saved conversations it opens, About Kodosi, Check for Updates and Sign out. The rail keeps the avatar and the settings button.
- **Provider usage.** The used part of a plan limit is a ring that fills clockwise from the top over a faint track: `inkMuted` below 80%, `caution` from 80% or when the agent warns, `danger` at the limit, and a dashed track while the number is not known. Around the account's mark in the launcher, the ring shows the limit nearest to stopping the account: the worst color, then the most use. Provider Usage is a panel beside the menu with a row for each signed-in account and a dial, the ring with the percent inside, for each limit. A number older than ten minutes is pale.
- **Tips.** A symbol-only control names itself in a small lifted label after a short pause; while one shows, the next shows at once.

A state has one moving sign: the light circling a working terminal's mark, or Dosi's face at its line, which reads, types and thinks while Dosi works; the faces in the conversation keep still. Its text and the wordmark stay still. Go to and keyboard moves through a list change at once. With reduced motion, from the system or from Settings, or while the window does not show, every change is instant and nothing loops.

## Do's and Don'ts
- **Do** use the springs in `AppTheme.Motion`, and animate only for an action or a real change of state.
- **Do** keep native focus, labels, stable accessibility identifiers, localizable text and selectable message text.
- **Do** keep hidden terminal surfaces inactive, and let Core Animation run anything that loops.
- **Don't** clip or scale the terminal surface; inset it in its tile.
- **Don't** put a glow, blur, colored shadow, gradient, gloss or bright rim on a mark, cursor or button, or use copper as decoration.
- **Don't** add explanatory text where an object, a state or one short line is enough.
