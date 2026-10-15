# iPhone and iPad app

Read `PRODUCT.md` and `DESIGN.md` here before changing the interface.

- The app is a viewer. The runtime is built with `--no-default-features` and never
  starts a process; don't show or fake host-only features.
- Presentation lives in `Sources`. Shared code lives in `../apple` and must keep both
  apps working. Only `KodosiTerminal` imports `GhosttyTerminal`.
- One `@Observable` store per area (account, terminals, rooms, people) receives the
  runtime events that `AppModel` routes to it. Views call store actions, not runtime
  commands.
- Generate the Xcode project from `project.yml`. `../apple/Config/iOSDeploymentTarget`
  sets the deployment target.
- Localize visible strings, give controls stable dotted accessibility IDs and labels,
  and support Dynamic Type, VoiceOver, reduced motion, landscape and iPad windows.
- Test with a local backend and test accounts, never a real account. Check with
  `just lint`, `just test` and `just check` here.
