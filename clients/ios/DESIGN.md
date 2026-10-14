# Kodosi for iPhone and iPad: design

The iOS app uses the Mac's visual system from the shared package
`../apple/Sources/KodosiDesign`: the same colors, marks, materials, and springs.
[The Mac design reference](../macos/DESIGN.md) lists the tokens.

## What changes on iOS

- Text styles follow Dynamic Type. The shared `AppTextStyle` maps each style to a
  system text style on iOS and keeps fixed sizes on the Mac.
- Touch targets are at least 44 points. The shared button sizes are 34, 44, and 52
  points high on iOS, with corner radii of 10, 13, and 16 points.
- Navigation is native: a tab bar on iPhone that becomes a sidebar on iPad, stacks
  with large titles, sheets with detents, swipe actions, and context menus.
- The terminal sits in a dark tile with rounded corners, in light and dark appearance.
  Its font is JetBrains Mono at 12 points on iPhone and 14 points on iPad.
- Attention uses the Mac's signs: a copper hand, question or key beside the agent mark
  while it waits, and the question under it. Only your own terminals that wait put a
  badge on the tab. Haptics mark one of your terminals that starts waiting or
  finishes, and a sent message.

## Do's and Don'ts

- **Do** use the springs in `AppTheme.Motion`, and animate only for an action or a
  real change of state. Respect reduced motion.
- **Do** give every control a stable dotted accessibility identifier and a label.
- **Do** keep the terminal surface unscaled; inset it in its tile.
- **Don't** show controls for host-only actions.
- **Don't** add explanatory text where an object, a state, or one short line is
  sufficient.
- **Don't** copy a terminal into a second list because it waits. Its row shows the
  sign.

Sources: `Sources/`, `../apple/Sources/KodosiDesign/`, and
`../apple/Sources/KodosiTerminal/`.
