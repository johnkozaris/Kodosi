---
name: Kodosi for Linux and Windows
description: A warm, living workbench for people, agents and terminals.
colors:
  ground: "#110e0c"
  ground-light: "#f0e9dc"
  surface: "#181411"
  surface-light: "#f7f2e8"
  raised: "#251d19"
  raised-light: "#fefbf5"
  lifted: "#2e241f"
  lifted-light: "#fffdf9"
  well: "#0c0a08"
  well-light: "#e8e0d2"
  terminal: "#0d0b09"
  hairline: "#3b2f29"
  hairline-light: "#d9ccba"
  ink: "#f1e9e3"
  ink-light: "#25160e"
  inkMuted: "#b3a094"
  inkMuted-light: "#5f4838"
  inkFaint: "#8c7a6f"
  inkFaint-light: "#8a7262"
  accent: "#db8a62"
  accent-light: "#b85a26"
  accentSoft: "#3e281c"
  accentSoft-light: "#f4dec8"
  accentInk: "#160e0a"
  accentInk-light: "#fffaf3"
  ready: "#7dbb99"
  ready-light: "#2f7d54"
  caution: "#ddb866"
  caution-light: "#8a6a1c"
  danger: "#e77a75"
  danger-light: "#b23a32"
typography:
  large: {fontFamily: "system-ui", fontSize: "28px", fontWeight: 600}
  title: {fontFamily: "system-ui", fontSize: "20px", fontWeight: 600}
  headline: {fontFamily: "system-ui", fontSize: "15px", fontWeight: 600}
  callout: {fontFamily: "system-ui", fontSize: "14px"}
  body: {fontFamily: "system-ui", fontSize: "13px"}
  footnote: {fontFamily: "system-ui", fontSize: "12px"}
  caption: {fontFamily: "system-ui", fontSize: "11px", fontWeight: 500}
rounded: {xs: "6px", sm: "8px", md: "10px", lg: "14px", xl: "18px", sheet: "22px", tile: "12px"}
spacing: {sidebar: "252px", rail: "78px", frame-inset: "8px", header: "54px", page: "36px"}
motion:
  hover: "120ms ease-out"
  fade: "200ms ease-out"
  snappy: "240ms ease-out"
  spring: "360ms ease-out with a small overshoot"
components:
  button-primary: {backgroundColor: "{colors.accent}", textColor: "{colors.accentInk}", rounded: "{rounded.md}", height: "32px"}
  button-secondary: {backgroundColor: "{colors.lifted}", textColor: "{colors.ink}", rounded: "{rounded.md}", height: "32px"}
  button-tinted: {backgroundColor: "{colors.accentSoft}", textColor: "{colors.accent}", rounded: "{rounded.md}"}
  input: {backgroundColor: "{colors.well}", textColor: "{colors.ink}", rounded: "{rounded.md}", height: "34px"}
  segmented-track: {backgroundColor: "{colors.well}", rounded: "capsule", padding: "3px"}
  segmented-thumb: {backgroundColor: "{colors.raised}", rounded: "capsule"}
  terminal-tile: {backgroundColor: "{colors.terminal}", rounded: "{rounded.tile}"}
---
# Design System: Kodosi for Linux and Windows

## Overview
Kodosi is a warm workbench, lit from above. A source list rests on the ground. One framed surface holds the work. Raised objects rest on that surface, and inputs sink into wells. The copper cursor is the sign of life: it stands at the end of the wordmark, it is the empty slot for a new terminal, and a new terminal opens from it.

Each screen starts with the objects of its job: terminals, rooms, people, tasks. Text is short. One word has one meaning: terminal, room, friend, agent, device. The Mac client uses the same system.

## Colors
Copper `accent` means "act here" or "this is selected". Green `ready` means done or verified, and people never use green. Unqualified tokens are dark, and `-light` tokens are the light pair. Use `KodosiTheme` roles, not literals. A terminal is a dark object in the two themes: its tile and header use the `terminal*` roles.

## Typography
Inherit the Qt application font. `large` is for page titles, `title` for sheets, `headline` for object names, `body` for conversation and rows, `footnote` and `caption` for facts. Monospace is for code and paths. The wordmark is a picture: the K with "odosi" in the same copper planks. Labels use sentence case.

## Layout
The desktop draws the window frame. The source list is (252 px), or a rail of marks (78 px). The main frame is inset (8 px) with a (18 px) radius. Pages have (36 px) margins. A room has a (54 px) header, a canvas, and the conversation on a raised sheet (300–540 px) that slides in from the right. Below (720 px) the room shows one of the two.

When chrome moves, the terminal gets its final size at once. The chrome animates, the terminal does not.

## Elevation & Depth
Three levels: resting, lifted (hover, focused composer, menus) and floating (sheets, notices, the Go to palette). `Raised` draws a light rim, a dark rim and soft shadows that fall down and to the right. It uses plain shapes, so it renders the same with and without a GPU. Do not add shader effects.

## Shapes
Tags and pills are capsules. Buttons are rounded rectangles, with a radius of (8 px), (10 px) or (12 px) for the three sizes. Rounded squares are agents, rooms and kinds of things. Circles are people. A thin outline is a slot that you can fill.

## Components
- **Button:** a rounded rectangle with a flat face in one color. The light comes from the top left, as on every raised shape: a fine light line on the top and the left edge, and a tight shadow below. In the light appearance a dark hairline goes round the edge. Primary buttons have a copper face. Secondary, tinted and danger buttons have a quiet face with the same lines. In the dark appearance there is no hairline: the bottom and the right edge have a darker line, the shadow is a little deeper, and the face is lighter than the surface behind it. With the pointer on it, the face changes a little. A press makes the face darker and the button a little smaller. A ghost button is words only, with a faint fill under the pointer. A button with only a symbol is round. `Bezel` draws the face with plain shapes.
- **Agent mark:** a rounded square in the color of the program, with its glyph. A shell shows a prompt and the copper cursor. The mark is pale when the terminal is minimized. The mark shows the state of its program in a ring of small cells round it. While the program works, the copper cursor types its way round the ring in quick bursts with pauses: each cell lights as the cursor passes and cools behind it in steps, and the cursor blinks in a pause. A percent fills the cells from the top, with the cursor blinking at the end. The ring shows only work; the status sign shows why the program stopped. The cells are solid colors, with no glow. The mark squashes a little at each change of state.
- **Status sign:** one small sign at the end of a terminal's row, card, tab or tile header, in the color of its state. It is the only sign that a program waits. A copper hand (approval), question or key (sign-in) stays while the program waits for you, and the hand waves one time. A green check that draws itself means done, and a `caution` triangle means failed; these two show until you open the terminal. A terminal that rang its bell, or an agent with no report that stopped work, keeps the breathing copper dot.
- **Person avatar:** a circle in a color that comes from the person's identifier. Your own circle is copper.
- **Room sigil:** a rounded square with four small tiles. The color and the lit tiles come from the room's identifier.
- **Selection pill:** one raised pill with a copper caret slides between rows of the source list.
- **Sliding pills:** segmented controls use a raised thumb that slides. The room dock shows icons, and the selected section says its name.
- **Terminal tile:** a rounded tile with a header. Actions are faint until the pointer is on the tile or the tile is selected. While the terminal is at its prompt, the marks of the start commands are the first actions, and a second command of the same program shows a letter. A new terminal opens from a copper veil. A person who joins shows as a card for a few seconds.
- **Launcher:** the terminal about to open, drawn as a tile and dark in both looks. The first time in a run of the app, the home shows only the copper cursor; a line grows from it, then the window around the line, as in the launch film. The band holds a tab for each kind of terminal, the plain terminal first: the chosen tab is the body's color and flares into the body, with the agent's mark (it squashes and springs back when chosen) and name; the other tabs are marks, and agents that are not here wait behind a plus. An agent that the computer just got shows its name in its tab, in copper, with a ring that leaves the tab twice. The computer is named at the right end of the band. Below it, the prompt `[folder] ❯ [account] [···]` and the first prompt. Each part of the command is a key of one solid color, lit from the top left as the buttons are, with no tinted or see-through fills; the folder key shows the name of the folder and a teal folder glyph that bounces when the folder changes, and the account words take the agent's color. The dots open the model and the effort keys; a model or an effort that the person chose always shows. A key opens its list in the terminal under the line, as a shell shows completions, and the terminal grows to hold it: `#` comment lines name the groups, the copper cursor marks the line in use, and the line under the keys or the pointer shows in reverse; the lines print one after another, typing narrows the list, Return takes a line and Escape closes it. The account list has a comment for each sign-in, with its e-mail and its folder, and under it the commands that use that sign-in, with the options that they add; `+ new account` takes the name of a new sign-in on the line itself and shows the folder that it will make. The first prompt takes the rest of the row, or a row of its own; it has a block cursor that stays lit while the person types, then blinks, and shows the character under it in the terminal's color, hollow without focus. While it is empty, a suggestion types itself once at an uneven pace and stays faint beside the cursor; Tab or → takes it. A comment line under the prompt names the signed-in account. Under the terminal: Start, and Resume as an icon.
- **Folder header:** a faint label above the terminals of one folder. Closed, it shows the sign of a terminal in it that waits for you. With the pointer on it, it shows a plus for a new terminal there and, for a Git repository, a branch button. The branch button opens a small panel with one name field, filled with a name you can replace, and one Start button.
- **Conversation:** your messages are bubbles on the right. Other people have an avatar and a name. An agent shows its mark, its name, the person it works for, and the terminal it wrote from. Mentions of people and terminals are tinted, and a mention of a terminal opens it. A message that mentions you has a tinted row.
- **Composer:** a lifted capsule with a copper halo when it has focus. "@" opens a list of people and terminals.
- **Tasks:** cards in "Up for grabs", "In progress" and "Done".
- **Go to (Ctrl+K):** one field in a well finds terminals, rooms, people and actions. The chosen row is a raised pill with a copper notch and a return key, and each action has a colored tile.

Working text shimmers. New words of a program rise into place. A terminal that rang its bell, or where an agent stopped work while you looked elsewhere, shows a breathing copper dot until you look at it. With reduced motion, all changes are instant and nothing loops. QML remains authoritative.

## Do's and Don'ts
- **Do** animate only for an action or a real change of state.
- **Do** keep keyboard focus, accessible names, stable object names and selectable message text.
- **Do** keep hidden terminal surfaces inactive.
- **Don't** clip or scale the terminal surface. Inset it in its tile.
- **Don't** put a glow, a blur or a colored shadow round a mark, a cursor or a rim, or use copper as decoration.
- **Don't** put a gradient, a gloss or a bright rim on a button.
- **Don't** add explanatory text where an object, a state or one short line is sufficient.

Sources: `src/qml/Theme/`, `src/qml/Controls/`, `src/qml/Shell/`, `src/qml/Screens/`, and `src/qml/Workbench/`.
