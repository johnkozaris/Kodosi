# Kodosi for iPhone and iPad: design

The app uses the visual system of the [Mac app](../macos/DESIGN.md), with native iOS
navigation:

- Text follows Dynamic Type, and touch targets are at least 44 points.
- A tab bar becomes a sidebar on iPad; stacks have large titles, and sheets have
  detents. Use swipe actions and context menus.
- The terminal is in a rounded tile in the look of the computer that it runs on, in
  JetBrains Mono at a size that the person picks in Settings. It does not follow Dynamic
  Type. Its key row takes its look too, and the chat face keeps the app's appearance.
- The key row over the keyboard never scrolls: its keys share the width evenly.
- Haptics mark one of your terminals that starts waiting or finishes, and a sent
  message.
- Show no controls for actions that only a computer can do.
