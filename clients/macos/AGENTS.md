# KodosiDesktop

Read `../../PRODUCT.md` for shared product intent and current behavior,
`PRODUCT.md` for Mac behavior, and `docs/GHOSTTY_INTEGRATION.md` before changing
terminal dependencies, native APIs or pins.

## Boundaries

- Presentation belongs in `Sources/Features`, shared controls in
  `Sources/DesignSystem`, and the Rust seam in `Sources/Bridge`. Rust owns
  processes, identity, sharing, and terminal authority.
- Only `Packages/KodosiTerminal` imports `GhosttyTerminal`.
- Generate the Xcode project from `project.yml` and the C header from Rust.
  `Config/macOSDeploymentTarget` is the deployment target authority.
- The runtime is built from the same checkout. Root `Ghostty.lock` selects upstream
  revisions and native artifacts. Working-tree archives are non-publishable.
- The `kodosi` CLI runs from the signed app executable so it retains Keychain
  access. Do not split it into a separately signed binary.

## Rules

- Keep source free of explanatory comments; preserve directives and licenses.
  Use `Logger`, Swift Testing, and native UI checks for real workflows.
- Localize visible strings and give interactive controls stable dotted
  accessibility IDs and labels. Avoid production force-unwraps.
- Preserve preferences, provider data, credentials, and real user files.
  Never silently reset identity pins. Minimize keeps a terminal running;
  Close ends it; Quit ends local terminals.

## Checks

```sh
just lint
just test
just ci
```

Use the existing native build scripts. Keep focused regressions for contracts,
security, and lifecycle. For UI checks, isolate storage, use exact Peekaboo PID/window
receipts, and inspect settled results before retrying. Do not add automated
smoke drivers.

**Before a Developer ID release:** embed a provisioning profile granting the
Keychain access group and make `just verify-signed` check the profile and
entitlement. Local builds use an automatic development profile.
