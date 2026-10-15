# Linux and Windows app

Read `PRODUCT.md` and `DESIGN.md` here before changing the interface.

- The Rust runtime owns terminals, identity, sharing, security and data. Qt owns the
  interface, presentation models, accessibility and desktop integration. QML presents
  typed state and user intent and never becomes another protocol, terminal, security
  or data authority.
- Prefer Qt and the standard library. Translate visible text with `qsTr` and give
  interactive QML objects stable accessibility identifiers and names.
- Platform code lives in `src/platform/linux` and `src/platform/windows`; platform build
  rules in `cmake/Linux.cmake` and `cmake/Windows.cmake`.
- Check with `just check` on Linux x86-64 and `just windows-check` on Windows 11, and try
  important workflows in the real desktop.
