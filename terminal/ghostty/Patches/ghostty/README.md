# Ghostty patches

Patches apply in alphabetical order to a clean checkout at the commits pinned by `MacOSGhostty.ref`
and `LinuxGhostty.ref`. Each platform builds from its patched tree; each Apple slice (macOS, iOS,
and iOS Simulator) builds the renderer and VT engine into one combined library.

Keep only patches required for host-managed I/O, terminal checkpoints, native input handling, and
combined Apple binaries. Aim for one clean patch per requirement without compatibility shims, and
drop a patch as soon as upstream provides the behavior. Never introduce a second terminal state
authority, private Apple APIs, or test-only code paths in production builds.

The patch directory itself is the source of truth; build and provenance checks verify what
actually ships.
