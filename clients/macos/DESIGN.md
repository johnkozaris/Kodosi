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
  glowRose: "#E8707A"
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
  button-primary: {backgroundColor: "{colors.accent}", textColor: "{colors.onAccent}", rounded: "capsule", height: "32pt"}
  button-secondary: {backgroundColor: "{colors.raised}", textColor: "{colors.ink}", rounded: "capsule", height: "32pt"}
  button-tinted: {backgroundColor: "{colors.accentSoft}", textColor: "{colors.accent}", rounded: "capsule"}
  input: {backgroundColor: "{colors.well}", textColor: "{colors.ink}", rounded: "{rounded.md}", height: "34pt"}
  segmented-track: {backgroundColor: "{colors.well}", rounded: "capsule", padding: "3pt"}
  segmented-thumb: {backgroundColor: "{colors.raised}", rounded: "capsule"}
  terminal-tile: {backgroundColor: "{colors.terminal}", rounded: "{rounded.tile}"}
---
# Design System: Kodosi for macOS

## Overview
Kodosi is a warm workbench, lit from the top left. A source list rests on the ground. One framed surface holds the work. Raised objects rest on that surface, and inputs sink into wells. The copper cursor is the sign of life: it is in the wordmark, it is the empty slot for a new terminal, and a new terminal opens from it.

Each screen starts with the objects of its job: terminals, rooms, people, tasks. Text is short. One word has one meaning: terminal, room, friend, agent, device.

## Colors
Copper `accent` means "act here" or "this is selected". The glow colors (`glowAmber`, `glowOrange`, `glowRose`) show only while something works or just changed. Green `ready` means done or verified, and people never use green. Unqualified tokens are dark, and `-light` tokens are the light pair. Use `AppTheme` roles, not literals. A terminal is a dark object in the two themes: its tile, its header and the Settings preview use the dark roles (`terminalScope`).

## Typography
Use `AppTextStyle`. `large` is for page titles, `title` for sheets, `headline` for object names, `body` for conversation and rows, `footnote` and `caption` for facts. Monospace is for code, paths and the wordmark. Labels use sentence case.

## Layout
The window has no title bar. The source list is (252 pt), or a rail of marks (78 pt). The main frame is inset (8 pt) with a (18 pt) radius. Pages are one centered column with (36 pt) margins. A room has a (54 pt) header, a canvas, and the conversation on a raised sheet (300–540 pt) that slides in from the right. Below (720 pt) the room shows one of the two.

When chrome moves, the terminal gets its final size at once. The chrome animates, the terminal does not.

## Elevation & Depth
Three levels: resting, lifted (hover, focused composer, popovers) and floating (notices, the Go to palette, the join card). Each raised shape has a light rim on the top left, a dark rim on the bottom right, and two shadows that fall down and to the right. Wells have an inner shadow.

## Shapes
Actions, tags and pills are capsules. Squircles are agents, rooms and kinds of things. Circles are people. A dashed outline is a slot that you can fill.

## Components
- **Agent mark:** a squircle in the color of the program, with its glyph. A shell shows a prompt and the copper cursor. The mark is pale when the terminal is minimized. A warm rim turns round it while the agent works.
- **Person avatar:** a circle in a color that comes from the person's identifier. Your own circle is copper.
- **Room sigil:** a squircle with four small tiles. The color and the lit tiles come from the room's identifier.
- **Selection pill:** one raised pill with a copper caret slides between rows of the source list.
- **Sliding pills:** segmented controls and terminal tabs use a raised thumb that slides. The room dock shows icons, and the selected section says its name. A section that changed out of view gets a breathing dot.
- **Terminal tile:** a rounded tile with a header. Actions are faint until the pointer is on the tile or the tile is selected. A new terminal opens from a copper veil at the prompt. A person who joins shows as a card for a few seconds.
- **Conversation:** your messages are bubbles on the right. Other people have an avatar and a name. An agent shows its mark, its name, the person it works for, and the terminal it wrote from. Mentions of people and terminals are tinted chips, and a mention of a terminal opens it. A message that mentions you has a tinted row.
- **Composer:** a lifted capsule with a copper halo when it has focus. "@" opens a list of people and terminals.
- **Tasks:** cards in "Up for grabs", "In progress" and "Done". A card that changed washes warm one time.
- **Go to (⌘K):** one field finds terminals, rooms, people and actions.

Working text shimmers. Counts roll. Symbols replace in place. A terminal that rang its bell, or an agent that stopped work while you looked elsewhere, shows a breathing copper dot until you open it. With reduced motion, all changes are instant and nothing loops.

## Do's and Don'ts
- **Do** use the springs in `AppTheme.Motion`, and animate only for an action or a real change of state.
- **Do** keep native focus, labels, stable accessibility identifiers and selectable message text.
- **Do** keep hidden terminal surfaces inactive.
- **Don't** clip or scale the terminal surface. Inset it in its tile.
- **Don't** show glow colors at rest, or use copper as decoration.
- **Don't** add explanatory text where an object, a state or one short line is sufficient.

Sources: `Sources/DesignSystem/`, `Sources/Features/Shell/`, `Sources/Features/Missions/`, and `Sources/Features/Sessions/`.
