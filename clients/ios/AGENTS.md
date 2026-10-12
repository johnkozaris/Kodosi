# KodosiMobile

Read `../../PRODUCT.md` for shared product intent and current behavior, `PRODUCT.md`
for the iPhone and iPad experience, and `DESIGN.md` for its visual system.

## Boundaries

- The app is a viewer. The runtime is built with `--no-default-features`: it shows,
  controls, and talks about terminals that run on other computers, and it never
  starts a process. Do not fake host-only features.
- Presentation belongs in `Sources`. The Rust seam and the protocol models live in
  `../apple/Sources/KodosiBridge`, shared controls in `../apple/Sources/KodosiDesign`,
  the Ghostty view in `../apple/Sources/KodosiTerminal`, and the live terminal pipeline
  in `../apple/Sources/KodosiTerminalSessions`; the Mac app uses the same code. Change
  shared code so both apps keep working. Only `KodosiTerminal` imports
  `GhosttyTerminal`.
- One `@Observable` store per area (account, terminals, rooms, people) receives the
  runtime events that `AppModel` routes to it. Views read stores and call actions;
  they do not send protocol commands themselves.
- Generate the Xcode project from `project.yml`. The Rust header is generated and
  shared; never edit it by hand.

## Rules

- Keep source free of explanatory comments; preserve directives and licenses.
  Use `Logger`, Swift Testing, and native checks for real workflows.
- Localize visible strings and give interactive controls stable dotted
  accessibility IDs and labels. Support Dynamic Type, VoiceOver, reduced motion,
  landscape, and iPad windows. Avoid production force-unwraps.
- Preserve credentials and real accounts. Validate with a local backend and test
  accounts; never sign in to a real account to test.

## Checks

```sh
just lint
just test
just check
```

Do not add automated smoke drivers.
