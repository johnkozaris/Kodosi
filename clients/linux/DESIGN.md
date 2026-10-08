---
name: Kodosi for Linux
description: A focused native workbench for shared work.
colors:
  accent: "#db8a62"
  accent-light: "#943a10"
  accentForeground: "#160e0a"
  accentForeground-light: "#fcf9f3"
  canvas: "#100d0b"
  canvas-light: "#f9f4eb"
  surface: "#1d1714"
  surface-light: "#efe3d2"
  surfaceRaised: "#241c18"
  surfaceRaised-light: "#ead8c3"
  surfaceSelected: "#3a281f"
  surfaceSelected-light: "#dec9b5"
  terminal: "#0a0807"
  terminal-light: "#e7e3de"
  textPrimary: "#f1e9e3"
  textPrimary-light: "#25160e"
  textSecondary: "#ad9a8e"
  textSecondary-light: "#5b4233"
  seam: "#40332c"
  seam-light: "#d6bea4"
  input: "#17120f"
  input-light: "#fffaf2"
typography:
  title: {fontFamily: "system-ui", fontSize: "16px", fontWeight: 600}
  body: {fontFamily: "system-ui", fontSize: "13px"}
  label: {fontFamily: "system-ui", fontSize: "12px"}
rounded: {radiusSmall: "8px", radiusLarge: "12px", radiusModal: "16px", selection: "9px"}
spacing: {spacing2: "6px", spacing3: "8px", spacing4: "10px", spacing5: "12px"}
components:
  button-primary: {backgroundColor: "{colors.accent}", textColor: "{colors.accentForeground}", rounded: "{rounded.radiusSmall}", height: "34px", padding: "6px 12px"}
  button-secondary: {backgroundColor: "{colors.surfaceRaised}", textColor: "{colors.textPrimary}", rounded: "{rounded.radiusSmall}", height: "34px", padding: "6px 12px"}
  input: {backgroundColor: "{colors.input}", textColor: "{colors.textPrimary}", rounded: "{rounded.radiusSmall}", height: "34px", padding: "7px 11px"}
---
# Design System: Kodosi for Linux

## Overview
A compact native workspace with warm charcoal or cream surfaces, restrained copper actions, and real terminals. Preserve the existing logo and the desktop's UI font.
Opaque surfaces, subtle seams, and compact controls keep attention on the work.
## Colors
Copper `accent` carries actions and focus; warm neutrals separate panels, selection, and terminal surround. Unqualified tokens are dark; `-light` tokens are their light counterparts. Use `KodosiTheme` semantic roles, including hover, focus, and danger, rather than copying literals.
## Typography
Inherit the Qt application font; use the title/body/label hierarchy above and quieter metadata. Terminal text stays fixed-pitch. Vector icons carry actions; initials identify people.
## Layout
The room rail is (184 px), header (56 px), and canvas/conversation headers (52 px). Below a room width of (720 px), show the selected pane; otherwise retain a resizable conversation (300–500 px, preferred 370 px). Keep the composer outside the transcript, growing within (46–146 px).
## Elevation & Depth
Opaque tonal surfaces and one-pixel seams provide depth. Room controls and rows are flat; popovers use the raised surface. Avoid decorative gradients and glass effects.
## Shapes
Use the theme's control, popover, and dialog corners; the composer shares the dialog radius. Avatars are circular. Tasks and issues are compact divided rows that reveal details in place.
## Components
Canvas changes preserve the conversation and drafts; history updates restore a message anchor and relative offset unless following the latest. Earlier history remains reachable and message bodies stay complete.
Terminal switching lives in the terminal toolbar. Task titles disclose description, result note, and actions. Repositories load the selected or first entry on arrival. Loading resembles content; failures expose recovery beside their subject.
Buttons use compact native states and a lower focus indicator. Selection movement uses OutCubic easing; reduced motion makes theme transitions immediate and stops the skeleton pulse. QML remains authoritative.
## Do's and Don'ts
- **Do** retain keyboard focus, accessible labels, selectable message text, and short action labels.
- **Do** keep hidden terminal surfaces inactive.
- **Don't** add decorative cards, explanatory status furniture, or motion without a visible state change.

Sources: `src/qml/Theme/KodosiTheme.qml`, `src/qml/Controls/`, `src/qml/Screens/Room*.qml`, `MissionsView.qml`, and `src/qml/Workbench/TerminalTile.qml`.
