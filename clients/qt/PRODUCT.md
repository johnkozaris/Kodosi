# Kodosi for Linux and Windows

See [PRODUCT.md](../../PRODUCT.md) for shared product decisions and [DESIGN.md](DESIGN.md) for
the visual system.

The Qt desktop app mirrors the [macOS app](../macos/PRODUCT.md) in layout and behavior. Every
control is keyboard-accessible, works with screen readers, and shows visible focus indicators.

## Platform differences

- Plain `Ctrl` keystrokes always pass through to programs inside the terminal, so app shortcuts
  use `Ctrl+Shift` paired with the corresponding macOS shortcut key.
- Updates are handled natively by the system package manager on Linux and App Installer on
  Windows.
- HTML pages and live document editors only load when Chromium's sandbox is available. In
  environments that block the sandbox (such as running as root on Linux), canvas files fall back
  to cards that let you open the file externally or save a copy.
- Linux and Windows computers don't share their screens. The app shows a Mac's screen on the
  canvas of that Mac's terminals, as the Mac app does.
