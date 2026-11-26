# Kodosi for Android: design

The Android app adapts the [macOS design system](../macos/DESIGN.md) to native Android navigation
and ergonomics:

- Colors match the macOS light and dark palettes edge to edge behind system bars (dynamic
  wallpaper colors are not used). Material surfaces adopt Kodosi color and text roles without
  tonal elevation tinting. System-bar icon contrast follows the app's active theme even when it
  differs from the OS theme, while preserving Android's navigation bar contrast handling.
- UI text uses the system font at the [iOS type scale](../ios/DESIGN.md) in `sp` so it honors
  user font scaling. Touch targets are at least 48 dp.
- Screens use shared Kodosi components throughout: raised cards, solid-bezel buttons with pressed
  states, and recessed fields with copper focus rings. Standard buttons and fields use a 13 dp
  corner radius (matching iOS) instead of Material capsules. User avatars are circles, and project
  and agent icons match across all platforms.
- Terminals sit inside a rounded tile matching their host computer's light or dark theme
  regardless of the app's theme, using JetBrains Mono at 12 dp on phones and 14 dp on tablets
  (scaled by the terminal font setting rather than system font scale). Accessory keys above the
  keyboard render as small raised caps in the terminal's theme. Terminal chat follows the app's
  theme.
- Four top-level tabs (**Terminals**, **Projects**, **Dosi**, and **Settings**) maintain
  independent back stacks via Navigation3 with predictive back. **Friends** lives under Settings
  with a pending-request count. Navigation sits on the base ground with copper active icons and
  labels (no Material pill indicator). Compact layouts hide bottom navigation inside project
  detail; wide layouts switch to a navigation rail and place project chat alongside tasks,
  repositories, or terminals.
- Animations follow the system animator scale, and looping indicators stay still inside decorative
  illustrations and in battery-saver mode. Section selectors use a recessed track with a sliding
  raised thumb showing only the active label. Message composers lift on focus and keep their send
  button inside the field.
- Sign-in, file picking/saving, notification preferences, and biometric prompts delegate to native
  Android system UI. Error states show their explanation and recovery action inline.
- Talking in a chat replaces the composer with a talk bar: bars that rise with your voice beside
  what talking does, a stop key with a copper square in the place of the mic, and Send in its own
  place, whose ring fills during the pause that sends. Words that did not go out yet wait above it
  as your message.
- Desktop-only controls are omitted.
