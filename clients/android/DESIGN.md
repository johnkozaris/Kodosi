# Kodosi for Android: design

The app uses the visual system of the [Mac app](../macos/DESIGN.md), with native Android
navigation:

- The colors are the Mac palette, light or dark as the phone is, from edge to edge under
  the system bars. Material components take the Kodosi roles: copper for primary, the
  surface for the background, the hairline for outlines.
- Text is the system font at the sizes of the [iPhone app](../ios/DESIGN.md), in scaled
  pixels, so it follows the font size that the person picks. Touch targets are at least
  48 dp.
- Cards and buttons are raised, lit from the top left: a highlight on the top edge, a
  hairline and a soft shadow below, with the Mac's radii in dp.
- The terminal is in a dark rounded tile in both appearances, in JetBrains Mono at
  12 dp, and it does not follow the font size. The keys above the keyboard are small
  raised caps.
- One screen stacks on another, and the system back gesture leaves it. Motion stops when
  the phone turns animations off.
- Show no controls for actions that only a computer can do.
