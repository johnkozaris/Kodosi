# Kodosi for Linux and Windows

[PRODUCT.md](../../PRODUCT.md) has the shared intent and [DESIGN.md](DESIGN.md) the
visual system.

The app looks and behaves like the [Mac app](../macos/PRODUCT.md). Every control works
with the keyboard and screen readers and shows its focus.

- **Keys.** A program in a terminal gets every plain Ctrl key, so the app's own keys are
  Ctrl+Shift with the Mac's key.
- **Updates.** The package manager on Linux and App Installer on Windows update the app.
- **Pages.** Pages and live documents show only in Chromium's sandbox. Where the system
  does not allow that sandbox, as for root, a file is a card that opens it in its own app
  or saves a copy.
