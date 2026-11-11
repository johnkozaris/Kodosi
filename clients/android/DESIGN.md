# Kodosi for Android: design

The app uses the visual system of the [Mac app](../macos/DESIGN.md), with native Android
navigation:

- The colors are the Mac palette, light or dark as the system or the app's setting
  chooses, from edge to edge under the system bars. Do not use wallpaper colors.
  Material surfaces take every Kodosi color and text role, with no tonal tint.
  System-bar icons follow the app's appearance,
  even when it differs from the system. Keep Android's navigation contrast treatment.
- Text is the system font at the sizes of the [iPhone app](../ios/DESIGN.md), in scaled
  pixels, so it follows the font size that the person picks. Touch targets are at least
  48 dp.
- Use the shared Kodosi controls on every screen. Cards have the Mac's raised surface,
  buttons its solid bezel and pressed face, and fields sink into wells with a copper
  focus ring. Regular keys and fields use the iPhone's 13-point radius in dp, not a
  Material capsule. People are circles; project marks and agent marks keep their
  colors and shapes across clients.
- The terminal is in a dark rounded tile in both appearances, in JetBrains Mono at
  12 dp on phones and 14 dp on tablets, adjusted by its own text-size setting rather
  than the system font size. The keys above the keyboard are small raised caps.
- Four tabs (Terminals, Projects, Dosi and Settings) keep their own stacks. Friends opens
  from Settings, and its requests count there. The navigation rests on ground, with
  copper symbols and words for selection, without a filled Material pill.
  Projects hide the bottom navigation on compact windows. It becomes a rail on wide windows;
  projects put the conversation beside the selected work on wide screens. Navigation3
  supplies the back stack and predictive back. Motion follows the system animation
  setting. Section docks have a well and a raised thumb, with only the chosen
  section's name. Composers lift while focused and keep their send key within the field.
- Sign-in, file picking and saving, notification settings and device authentication
  use Android's own surfaces. Error states keep their explanation and recovery action
  on the screen that needs them.
- Show no controls for actions that only a computer can do.
