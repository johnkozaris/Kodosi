# Ghostty patches

The patches apply in lexical order to clean source at the commits selected by
`MacOSGhostty.ref` and `LinuxGhostty.ref`. Each platform builds from its
patched checkout; each Apple slice (macOS, iOS, iOS Simulator) shares one
renderer and terminal-engine image.

Keep only patches Kodosi needs for host-managed I/O, terminal checkpoints,
native input and policy, and the combined Apple images. Prefer one patch per
requirement, applied cleanly without a compatibility variant; remove it when
upstream supplies the behavior. Do not add a second terminal authority, private
Apple APIs, or test-only behavior to production builds.

The patch files are the inventory. Build and provenance checks verify what is
actually shipped.
