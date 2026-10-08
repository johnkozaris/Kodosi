# Kodosi for Linux agent guide

Read `../../PRODUCT.md` for shared product intent and current behavior,
`PRODUCT.md` for Linux behavior, and `DESIGN.md` before changing the interface.

## Boundary

Kodosi for Linux is the native Linux presentation client. The shared Rust runtime owns
terminal and session behavior, identity, sharing, security, and shared data.
Qt owns the native interface, presentation models, accessibility, and desktop
integration.

Keep that boundary deep and simple: QML presents typed state and user intent. It
does not become another protocol, terminal, security, or data authority.

## Development

- Follow the pinned sources and current protocol.
- Prefer Qt and the standard library; keep dependencies project-local.
- Keep code self-explanatory. Comment only non-obvious constraints.
- Give interactive QML objects stable accessibility identifiers and names.
- Translate user-visible text with `qsTr`.
- Run `just check` on Linux x86-64 and validate important workflows in the real
  desktop interface.
