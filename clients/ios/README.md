# Kodosi for iPhone and iPad

Native iOS app for monitoring and controlling remote terminals and projects. It embeds the viewer
build of the Rust runtime and Ghostty's terminal view, sharing core Swift code with macOS via the
[shared Swift package](../apple/Package.swift).

## Build and run

In addition to the [shared toolchains](../../docs/DEVELOPMENT.md#toolchains), you need Xcode
27.2+ with the iOS simulator and the pinned Rust toolchain with iOS targets installed
(`just ios-rust-targets-install`). The first build installs pinned SwiftLint, SwiftFormat, and
XcodeGen versions (from [Tools.env](../apple/Tools.env)) into `clients/apple/.tools/`.

```sh
just ios-build
```

Then run the `KodosiMobile` scheme in Xcode. Debug builds target a
[local backend](../../docs/DEVELOPMENT.md#run-the-backend) at `127.0.0.1:5180` with the
disposable Keycloak from `sign-in/keycloak.sh` at `localhost:8284` (override via
`KODOSI__AUTH__ISSUER`). Note that the iOS Simulator can reach loopback addresses, while a
physical device cannot.

## Checks and distribution

`just ios-check` runs linters, unit tests on the `Kodosi iPhone 17 Pro` simulator (override with
`KODOSI_IOS_SIMULATOR`), and a Release device build.

Device builds require the App IDs `com.kodosi.mobile` and `com.kodosi.mobile.notifications` with
Push Notifications enabled and the App Group `group.com.kodosi.mobile` (Xcode registers these
automatically when signed in to the team). App Store Connect accepts each build number for a
version only once; set `KODOSI_BUILD` (defaults to `1`) and increment it when re-uploading the
same version. Update [PrivacyInfo.xcprivacy](Resources/PrivacyInfo.xcprivacy) whenever the app
starts using a new
[required reason API](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api)
or sending new categories of data to the server.
