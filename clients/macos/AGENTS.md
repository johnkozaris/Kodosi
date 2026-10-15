# Mac app

Read `PRODUCT.md` and `DESIGN.md` here before changing the interface.

- Presentation lives in `Sources/Features`. Code shared with the iOS app lives in
  `../apple`. Only `../apple/Sources/KodosiTerminal` imports `GhosttyTerminal`. Rust
  owns processes, identity, sharing and terminal state.
- Generate the Xcode project from `project.yml` and the C header from Rust.
  `Config/macOSDeploymentTarget` sets the deployment target.
- The `kodosi` CLI runs from the signed app executable to keep Keychain access; don't
  split it into another binary.
- Localize visible strings, give controls stable dotted accessibility IDs and labels,
  and avoid force-unwraps in production code. Use `Logger` and Swift Testing.
- Never silently reset identity pins or touch real preferences, provider data,
  credentials or files.
- Check with `just lint`, `just test` and `just check` here. For UI checks, isolate
  storage and use exact Peekaboo PID and window receipts.
- Developer ID releases embed a provisioning profile for the Keychain access group;
  `just signature-check` checks the profile, certificate and entitlements.
