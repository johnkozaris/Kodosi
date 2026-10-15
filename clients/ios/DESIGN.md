# Kodosi for iPhone and iPad: design

The app uses the Mac's visual system from `../apple/Sources/KodosiDesign`; the
[Mac design reference](../macos/DESIGN.md) lists the tokens and rules. On iOS:

- Text styles follow Dynamic Type through `AppTextStyle`.
- Touch targets are at least 44 points; buttons are 34, 44 and 52 points high with
  10, 13 and 16 point radii.
- Navigation is native: a tab bar that becomes a sidebar on iPad, stacks with large
  titles, sheets with detents, swipe actions and context menus.
- The terminal sits in a dark rounded tile in both appearances, in JetBrains Mono at
  12 points on iPhone and 14 on iPad, never scaled.
- A waiting terminal shows the copper ring, its question and a tab badge. Haptics mark a
  terminal that starts waiting or finishes, and a sent message.
- Don't show controls for host-only actions.
