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
Copper `accent` means "act here" or "selected". `glowAmber` flashes behind a fresh change and `glowOrange` is a breathing dot for work or for a change out of view. Green `ready` means done or verified and is never a person's color. Plain tokens are dark and `-light` tokens their light pair. Use `AppTheme` roles, not literals. Terminals stay dark in both appearances.

## Typography
Use `AppTextStyle`: `large` for page titles, `title` for sheets, `headline` for object names, `body` for conversation and rows, `footnote` and `caption` for facts, monospace for code and paths. Labels use sentence case. The wordmark is a picture.

## Layout
No title bar. The source list can shrink to a rail of marks, one inset frame holds the work, and pages are one centered column. A room has a header, a canvas, and the conversation on a 300–540 pt sheet from the right; below 720 pt it shows one of the two. When chrome moves, the terminal takes its final size at once.

## Elevation & Depth
Three levels: resting, lifted (hover, focused composer, popovers) and floating (sheets, notices, Go to). Raised shapes have a light top-left rim, a dark bottom-right rim and two shadows falling down and right; wells have an inner shadow. A sheet rises in the middle of the dimmed window and a popover from its source; a click outside or Escape dismisses either, unless the sheet waits for a decision.

## Shapes
Capsules for tags and pills, rounded rectangles for buttons, squircles for agents, rooms and kinds of things, circles for people, and a dashed outline for a slot you can fill.

## Components
- **Agent mark:** a squircle in the program's color with its glyph, pale when minimized. A ring of cells shows only work: the cursor types round it while the program works, and a percent fills it from the top. Solid colors, no glow; the mark squashes a little at each change.
- **Status sign:** one sign at the end of a row, card, tab or tile header, and the only sign that a program waits: a copper hand, question or key while waiting, a green check for done and a `caution` triangle for failed until opened, and a breathing copper dot for a bell or an agent that stopped without a report.
- **People and rooms:** a person is a circle colored from their identifier, and yours is copper. A room is a squircle of four tiles colored from its identifier; a copper dot beside it says that someone else wrote in it since you last looked.
- **Buttons:** a flat face lit from the top left. Primary is copper, secondary and tinted are quiet, ghost buttons are words with a hover fill, and symbol-only buttons are round.
- **Selection:** a raised pill with a copper caret; segmented controls and terminal tabs slide a raised thumb.
- **Terminal tile:** a rounded tile whose header actions stay faint until hover or selection. The launcher is the terminal about to open, dark in both appearances.
- **Conversation:** your messages are bubbles on the right; others show an avatar and a name, and agents their mark, owner and terminal. Mentions are tinted chips.

A state has one moving sign: a working terminal shows its work on its rim, and its text and the wordmark stay still. With reduced motion, from the system or from Settings, or while the window does not show, every change is instant and nothing loops.

## Do's and Don'ts
- **Do** use the springs in `AppTheme.Motion`, and animate only for an action or a real change of state.
- **Do** keep native focus, labels, stable accessibility identifiers and selectable message text.
- **Do** keep hidden terminal surfaces inactive, and let Core Animation run anything that loops.
- **Don't** clip or scale the terminal surface; inset it in its tile.
- **Don't** put a glow, blur, colored shadow, gradient, gloss or bright rim on a mark, cursor or button, or use copper as decoration.
- **Don't** add explanatory text where an object, a state or one short line is enough.
