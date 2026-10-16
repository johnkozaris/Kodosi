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
The Linux and Windows app looks and behaves exactly like the [Mac app](../macos/DESIGN.md): its overview, colors, typography, layout, shapes, components and rules apply here. A program in a terminal gets every plain Ctrl key, so each shortcut is Ctrl+Shift with the Mac's key: ⌘K and ⇧⌘[ become Ctrl+Shift+K and Ctrl+Shift+[. Resume a conversation (⇧⌘N) is Ctrl+Shift+R, because Ctrl+Shift+N opens a new terminal. Ctrl+K and Ctrl+, also work when no terminal has focus. On Linux the desktop draws the window frame; on Windows Kodosi draws a 36 px title zone with its own window buttons.

## Colors
Use `KodosiTheme` roles, not literals. Terminals use the `terminal*` roles in both themes.
