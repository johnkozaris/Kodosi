# Kodosi for iPhone and iPad

The native iOS app follows your terminals and rooms. It embeds the viewer build of the
Rust runtime and Ghostty's terminal view, and shares code with the Mac app through the
[shared Swift package](../apple/Package.swift).

You need Xcode with the iOS 26 SDK and simulator, the pinned Rust toolchain with the iOS
targets (`just ios-rust-targets-install`), and `just`, Python 3, `xcodegen`,
`swiftformat` and `swiftlint`.

```sh
just ios-build
```

Then run the `KodosiMobile` scheme in Xcode. Debug builds use a
[local backend](../../docs/DEVELOPMENT.md#run-the-backend) at `127.0.0.1:5180`, which a
simulator can reach and a device cannot.

`just ios-check` lints, runs the unit tests on the `Kodosi iPhone 17 Pro` simulator
(`KODOSI_IOS_SIMULATOR` names another) and builds Release for devices. Update
[PrivacyInfo.xcprivacy](Resources/PrivacyInfo.xcprivacy) when the app starts to use
another
[required reason API](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api)
or sends new data to the service.
