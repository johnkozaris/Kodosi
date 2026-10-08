# Ghostty integration

Rust in `runtime/` owns processes, terminal state, identity, and
transport. `terminal/ghostty/` renders host-managed sessions and handles native
input, selection, and accessibility; it does not start processes. Only
`Packages/KodosiTerminal` imports `GhosttyTerminal` into the app.

A returning view receives current terminal state before continued output.
Stale views must not control replacement sessions. Hiding a view must not
discard its state or let it consume input. Keep user paste direct and
terminal-originated clipboard requests fail-closed.

Each executable links one native Ghostty image, not a second terminal parser
or state authority. The macOS renderer and terminal engine share that image;
Linux has its own artifact.

Root `Ghostty.lock` selects upstream revisions and artifact digests for both
platforms. The runtime and native client use the same repository revision.
For native updates, follow [Ghostty provenance](../../../terminal/ghostty/PROVENANCE.md):
review upstream, rebuild, verify artifacts and notices, and update the central lock
with the measured outputs. Validate the integration in a real terminal. Preserve
licenses and corresponding source. Working-tree checks are non-publishable.
