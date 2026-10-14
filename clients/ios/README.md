# Kodosi for iPhone and iPad

The native iOS app follows your terminals and rooms. It embeds the shared Rust runtime
as a viewer and Ghostty's terminal view.

## Requirements

- Xcode with the iOS 26 SDK and an iOS 26 simulator runtime.
- The Rust toolchain pinned at the repository root, with the iOS targets:
  `just ios-rust-targets-install`.
- `just`, Python 3, `xcodegen`, `swiftformat`, and `swiftlint` on your build PATH.

The app shares the runtime bridge, the design system, and the terminal view and
pipeline with the Mac app through the [shared Swift package](../apple/Package.swift).

## Build and run

From the repository root:

```sh
just ios-build
```

This generates `KodosiMobile.xcodeproj`, builds the viewer runtime library for the
simulator, and builds the app. Open the generated project in Xcode, select the
`KodosiMobile` scheme and an iPhone or iPad simulator, and run it.

Debug builds target the local backend at `127.0.0.1:5180`, like the Mac app; follow
[backend setup](../../docs/DEVELOPMENT.md#run-the-backend) to run it. A simulator
reaches that address on your Mac; a device does not. Release builds use the public
service addresses.

Generate the project from `project.yml`; do not edit generated project files.
[iOSDeploymentTarget](../apple/Config/iOSDeploymentTarget) is the record of the iOS
deployment target. [VERSION](../../VERSION) gives the version of
the iOS app, the Mac app, the Linux and Windows app and the CLI.

## Privacy manifest

[Resources/PrivacyInfo.xcprivacy](Resources/PrivacyInfo.xcprivacy) lists the account
data that the Kodosi service keeps and the reasons for the app's file-date and
UserDefaults reads. Update it when the app or a library it links starts to use another
[required reason API](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api)
or sends new data to the service.

## Check

```sh
just ios-check
```

This lints the sources, runs the unit tests on a `Kodosi iPhone 17 Pro` simulator
(made when it is missing; set `KODOSI_IOS_SIMULATOR` to choose another), and builds
the Release configuration for devices. To format the Swift sources, run `just format`
from this client directory.

See [product behavior](PRODUCT.md), [design reference](DESIGN.md), and
[contributor setup](../../docs/DEVELOPMENT.md).
