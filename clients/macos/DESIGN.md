---
name: Kodosi for macOS
description: A focused native workbench for shared work.
colors:
  primary: "hsl(22, 65%, 60%)"
  primary-light: "hsl(20, 75%, 39%)"
  primaryForeground: "hsl(20, 15%, 6%)"
  primaryForeground-light: "hsl(40, 60%, 97%)"
  background: "hsl(20, 15%, 6%)"
  background-light: "hsl(38, 55%, 95%)"
  foreground: "hsl(33, 22%, 90%)"
  foreground-light: "hsl(22, 45%, 10%)"
  mutedForeground: "hsl(28, 14%, 56%)"
  mutedForeground-light: "hsl(22, 28%, 28%)"
  card: "hsl(20, 12%, 14%)"
  card-light: "hsl(36, 50%, 90%)"
  secondary: "hsl(20, 12%, 17%)"
  secondary-light: "hsl(32, 48%, 84%)"
  surfacePanel: "hsl(20, 13%, 11%)"
  surfacePanel-light: "hsl(35, 48%, 88%)"
  surfaceStage: "hsl(20, 14%, 6%)"
  surfaceStage-light: "hsl(38, 52%, 93%)"
  surfaceTerminal: "hsl(18, 15%, 3%)"
  surfaceTerminal-light: "hsl(28, 16%, 89%)"
  seam: "hsl(20, 10%, 18%)"
  seam-light: "hsl(30, 32%, 72%)"
typography:
  title: {fontFamily: "system-ui", fontSize: "16pt", fontWeight: 600}
  body: {fontFamily: "system-ui"}
  label: {fontFamily: "system-ui", fontSize: "12pt", fontWeight: 500}
  mono: {fontFamily: "ui-monospace"}
rounded: {sm: "3pt", selection: "9pt", composer: "16pt"}
spacing: {control-x: "12pt", control-y: "8pt", content: "16pt"}
components:
  button-primary: {backgroundColor: "{colors.primary}", textColor: "{colors.primaryForeground}", rounded: "{rounded.sm}", padding: "8pt 12pt"}
  button-secondary: {backgroundColor: "{colors.card}", textColor: "{colors.foreground}", rounded: "{rounded.sm}", padding: "8pt 12pt"}
  input: {backgroundColor: "{colors.surfaceStage}", textColor: "{colors.foreground}", rounded: "{rounded.sm}", padding: "8pt 10pt"}
---
# Design System: Kodosi for macOS

## Overview
A compact native workspace with warm charcoal or cream surfaces, restrained copper actions, and real terminals. Preserve the existing logo and platform typography.
Opaque surfaces, subtle seams, and compact controls keep attention on the work.
## Colors
Copper `primary` carries actions and selection; warm neutrals separate panels, workspace, and terminal surround. Unqualified tokens are dark; `-light` tokens are their light counterparts. Use `AppTheme` semantic roles, including its status colors, rather than copying literals.
## Typography
Use `AppTextStyle`: native title3/headline for headings, body for conversation, callout for controls, caption for metadata, and monospaced styles for code. Explicit sizes above are native points. SF Symbols carry actions; initials identify people.
## Layout
The room rail is (184 pt), header (56 pt), and canvas dock sits beside the conversation header. At room widths below (720 pt), show the selected pane; wider rooms use a resizable split with conversation (300–500 pt). Keep the composer outside the scrolling transcript.
## Elevation & Depth
Opaque tonal layers and hairline seams define the workspace. `ElevatedSurface` adds a faint rim and soft contact/ambient shadows to controls; pressing reduces the shadows and lowers the control slightly.
## Shapes
Small control corners, broader selection/composer corners, circular avatars, and continuous terminal surfaces. Tasks and issues use compact divided rows with details revealed in place.
## Components
Canvas changes preserve the conversation, drafts, and reading position; new messages follow the bottom only while the reader is already there. Earlier history remains reachable and message bodies stay complete.
Terminal switching lives in the terminal toolbar. Task titles disclose description, result note, and actions. Repositories load the selected or first entry on arrival. Loading resembles content; failures expose a nearby retry and optional detail.
Selection uses short springs; disclosures use smooth reveals. Respect reduced motion by suppressing these animations. Native source remains authoritative.
## Do's and Don'ts
- **Do** use native focus, accessible labels, selectable message text, and short action labels.
- **Do** keep hidden terminal surfaces inactive.
- **Don't** add decorative cards, explanatory status furniture, or motion without a visible state change.

Sources: `Sources/DesignSystem/AppTheme.swift`, `AppTextStyle.swift`, shared controls, `Sources/Features/Missions/`, and `Sources/Features/Sessions/SessionTileView.swift`.
