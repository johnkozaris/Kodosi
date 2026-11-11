# Kodosi for Linux and Windows: design

The app uses the visual system of the [Mac app](../macos/DESIGN.md): its colors,
typography, layout, depth, shapes, components and rules.

- Use `KodosiTheme` roles, not literals.
- Motion uses short eased durations in place of the Mac's springs.
- On Linux the desktop draws the window frame. On Windows Kodosi draws its own title
  zone and window buttons.
