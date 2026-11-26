# Kodosi for iPhone and iPad: design

The iOS app shares the [macOS visual system](../macos/DESIGN.md) adapted to native iOS
navigation:

- Text scales with Dynamic Type, and all touch targets are at least 44 pt.
- Navigation uses a tab bar on iPhone and switches to a sidebar on iPad; navigation stacks use
  large titles, sheets use detents, and lists support swipe actions and context menus.
- Terminals sit inside a rounded tile matching their host computer's light or dark theme, rendered
  in JetBrains Mono at a user-selected font size in Settings (independent of Dynamic Type). The
  accessory key row above the keyboard adopts the terminal's theme, while the chat view follows
  the app's appearance.
- The accessory key row never scrolls; its keys divide the available width evenly.
- Haptics fire when one of your terminals starts waiting or finishes, and when you send a message.
- Talking in a chat replaces the composer with a talk bar: bars that rise with your voice beside
  what talking does, a stop key with a copper square in the place of the mic, and Send in its own
  place, whose ring fills during the pause that sends. Words that did not go out yet wait above it
  as your message.
- Desktop-only controls are omitted rather than shown disabled.
