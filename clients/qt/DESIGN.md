# Kodosi for Linux and Windows: design

The Qt app implements the [macOS design system](../macos/DESIGN.md): colors, typography, layout,
elevation, shapes, components, and interaction rules.

- Always use `KodosiTheme` semantic roles instead of raw color values. Anything belonging to a
  terminal derives its roles from that terminal's `Look`; the launcher uses the local computer's
  theme.
- Text, icons next to text, and controls that contain text get their sizes from the theme's font
  roles, `controlHeight`, `fieldHeight`, or `scaled()`, so they follow the chosen text size as they
  do on macOS.
- Motion uses short eased transitions in place of macOS springs. Looping indicators tick from a
  single shared clock (matching Android) and run at a lower frame rate when software rendering is
  active.
- On Linux, the window manager draws native window decorations. On Windows, Kodosi renders a
  custom title bar and window controls.
