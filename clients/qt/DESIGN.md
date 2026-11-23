# Kodosi for Linux and Windows: design

The app uses the visual system of the [Mac app](../macos/DESIGN.md): its colors,
typography, layout, depth, shapes, components and rules.

- Use `KodosiTheme` roles, not literals. What belongs to a terminal takes its roles from the
  terminal's `Look`, and the launcher from this computer's.
- Motion uses short eased durations in place of the Mac's springs. A sign that loops takes
  its steps from one clock that all signs share, as on Android, and steps less often where
  the processor draws the window.
- On Linux the desktop draws the window frame. On Windows Kodosi draws its own title
  zone and window buttons.
