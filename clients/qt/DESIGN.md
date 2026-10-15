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
The same warm workbench as the [Mac app](../macos/DESIGN.md), lit from above. Its overview, colors, shapes, components and rules apply; this file holds the Qt tokens and the differences.

## Colors
Use `KodosiTheme` roles, not literals. Terminal tiles and headers use the `terminal*` roles in both themes.

## Typography
Inherit the Qt application font, with the scale above.

## Layout
The desktop draws the window frame; the rest follows the Mac layout in pixels.

## Elevation & Depth
`Raised` and `Bezel` draw rims, faces and shadows with plain shapes, so they render the same with and without a GPU. Do not add shader effects.

## Shapes
Agents, rooms and kinds of things are rounded squares rather than squircles, and a slot that you can fill has a thin outline.

## Components
As on the Mac, except: Go to is on Ctrl+K; segmented controls and the room dock slide a raised thumb, with no breathing dot for a changed section; mentions are tinted text, not chips; a changed task card does not wash; and the words of a program rise into place, with no rolling counts or blur. QML is authoritative.

## Do's and Don'ts
- **Do** keep keyboard focus, accessible names, stable object names and selectable message text.
- **Don't** break the Mac rules: no glow, blur, gradient or gloss, no scaled terminal surface, no explanatory text where an object will do.
