# Kodosi for iPhone and iPad: design

The app uses the visual system of the [Mac app](../macos/DESIGN.md), with native iOS
navigation:

- Text follows Dynamic Type, and touch targets are at least 44 points.
- A tab bar becomes a sidebar on iPad; stacks have large titles, and sheets have
  detents. Use swipe actions and context menus.
- The terminal is in a dark rounded tile in both appearances, in JetBrains Mono at
  12 points on iPhone and 14 on iPad, or two points smaller or larger when the person
  picks it in Settings, and it does not follow Dynamic Type.
- The key row over the keyboard never scrolls: its keys share the width evenly, as wide as
  the keyboard's own keys, with a wider gap between groups.
- A waiting terminal shows what it needs beside its mark. Haptics mark one of your
  terminals that starts waiting or finishes, and a sent message.
- Show no controls for actions that only a computer can do.
