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
  terminal-light: "#FAF6EE"
  band: "#221A16"
  band-light: "#F1EADF"
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
  mono: {fontFamily: "ui-monospace", fontSize: "12pt"}
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

The interface is a warm workbench lit from the top left. The sidebar sits on the base ground, a
single inset frame holds the active workspace, raised controls sit above the surface, and text
inputs sink into recessed wells. The copper cursor is the sole brand motif: it closes the
wordmark, marks the placeholder slot for a new terminal, and is the origin when a new
terminal opens. Screens lead with the actual objects of work, keeping copy minimal.

## Colors

Copper (`accent`) signals primary actions or active selection and nothing else. `glowAmber` briefly
flashes behind freshly updated items, while `glowOrange` is a gently pulsing dot for active work
or off-screen activity. Green (`ready`) indicates completion or verification and is never assigned
as a user avatar color.

Base tokens are dark mode; `-light` tokens are their light-mode counterparts. Always reference
`AppTheme` semantic roles rather than hardcoded hex values. All surfaces use solid fills. A
terminal adopts its host computer's light or dark appearance for everyone viewing it, and chrome
belonging to that terminal follows the same theme.

## Typography

Use `AppTextStyle` roles consistently: `large` for page titles, `title` for sheets, `headline` for
object names, `body` for chat messages and list rows, `footnote` and `caption` for metadata, and
`mono` for code and paths. Use sentence case for all labels. The wordmark is rendered as an image
asset.

A person picks the text size in Settings: Small, Normal, Large or Larger, about one point a step at the size of the conversation. It scales the text styles, the other sizes of words and of the symbols beside them (`font(scaled:)`), and the heights of the controls that hold words, so a key keeps its text centered. Marks, avatars, pictures, icon-only keys and spacing keep their size. The terminal has its own font size, and a document on the canvas its own zoom.

## Layout

The window has no standard title bar. The sidebar collapses to an icon rail or widens for Dosi,
one inset frame holds the workspace, and settings/detail pages use a single centered column. When
chrome resizes or tiles appear and disappear, terminals snap straight to their final grid
dimensions without intermediate resizes.

## Elevation and depth

UI sits on three planes: resting, lifted (hover states, focused composers, popovers), and floating
(sheets, alerts, Go to). Raised elements carry a light top-left rim, a dark bottom-right rim, and
two subtle downward-right shadows; recessed wells carry an inner shadow.

Sheets rise centered over a dimmed window and popovers anchor to their trigger. Clicking outside
or pressing Escape dismisses either, unless the sheet requires an explicit decision.

## Shapes

Tags and pills are capsules, buttons are rounded rectangles, agents, projects, and category icon
tiles are squircles, and people are circles. Empty fillable slots use a dashed outline.

## Components

### Icons and avatars
Agents use a squircle in the provider's color with its glyph, and each helper an agent started
shows as a small squircle in that color before the status indicator while the helper works.
People are circles colored from their identifier (your own avatar is copper). Projects use a
squircle of four colored tiles derived deterministically from the project ID. Files appear as a
dog-eared sheet tinted by file type.

### Status indicators
Every list row, card, tab, and tile header ends with a single status indicator, and it is the only
place a waiting state is shown: copper when waiting for input, a green check when done, and a
`caution` triangle when failed. While a program is working, a warm light orbits the edge of its
icon without any outer glow. A pulsing copper dot marks unread updates out of view.

### Buttons and selection
Buttons are flat rounded rectangles lit from the top left: copper for primary actions and subtle
neutral fills for secondary ones. Icon-only buttons turn copper only when toggled on and show a
tooltip after a brief hover delay.

Selected rows use a raised pill with a copper indicator bar. Segmented controls and tabs animate a
sliding raised thumb inside a recessed track.

### Terminal tiles and launcher
Terminals render inside rounded tiles matching their host's theme. Header controls stay subdued
until hovered or selected, and the focused tile gets a thin copper border. The chat view occupies
the same tile body. The header shows where the terminal runs as a single plain line of text rather
than badges or form fields. The launcher is styled as the terminal about to open, using this Mac's
theme.

### Canvas and drag-and-drop
A terminal and its open canvas form two lobes of one body, with a soft waist at the seam between
them. Canvas tab controls sit in a recessed well, with files and browser tabs sitting above their
content. Drop targets show a dashed copper outline while the dragged item appears as a lifted chip.

The shared browser and a computer's screen show one state on their picture, calm by default: the
picture alone while live; the last picture dimmed with a pulsing `glowOrange` dot while it comes
back, which turns into a Reconnecting pill if that takes a while; one line under an icon in a well
when the computer is away; and Try again only when something broke unexpectedly. A spinner without
words appears only when there is no picture yet, and a green check says Copied for a moment.

### Conversation and Dosi
Your own messages align right as bubbles; other people show an avatar and name, and agents show
their icon, owner, and terminal name. Mentions render as tinted chips.

Talking in a chat replaces the composer with a talk bar: bars that rise with your voice beside what
talking does, a stop key with a copper square in the place of the mic, and Send in its own place,
whose ring fills during the pause that sends. Words that did not go out yet wait above it as your
own message, faint where the recognizer may still change them.

Dosi's avatar doubles the copper cursor into two eyes on a dark squircle, sitting directly at the
bottom of the sidebar without a container box. Its face and a single monospace status line report
activity; nothing pops up unprompted.

## Motion

At most one element animates continuously for a given state: the orbiting light on a working
terminal icon or Dosi's face while working. Text and the wordmark stay static, and keyboard
navigation through lists updates immediately without lag. When reduced motion is enabled (in macOS
or Kodosi Settings) or when the window is hidden, transitions become instant and looping animations
stop.

## Guidelines

Do:

- Use the spring curves defined in `AppTheme.Motion`, and only animate in response to user actions
  or real state transitions.
- Maintain full keyboard focus, accessibility labels, stable accessibility identifiers, localized
  strings, and selectable message text.
- Pause hidden terminal surfaces, and run looping animations in Core Animation rather than SwiftUI
  timers.

Don't:

- Clip or scale the terminal surface; always inset it cleanly inside its tile.
- Add outer glows, blurs, colored shadows, gradients, gloss, or bright rims to icons, cursors, or
  buttons, or use copper decoratively.
- Add explanatory copy where an object, status indicator, or single short label is already clear.
