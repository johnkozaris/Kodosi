# Kodosi for Android

The Android app follows your terminals. It embeds the viewer build of the Rust runtime as
a shared library, and draws each terminal from the state that Ghostty's terminal library
keeps, like the [Linux and Windows app](../qt/README.md).

You need JDK 21, the Android SDK in `~/Library/Android/sdk` (`ANDROID_HOME` names another)
with the platform, build tools, NDK and CMake that
[libs.versions.toml](gradle/libs.versions.toml) pins, the pinned Rust toolchain with the
Android target and cargo-ndk (`just android-tools-install`), and `just` and Python 3. On a
Mac:

```sh
brew install openjdk@21
brew install --cask android-commandlinetools
sdkmanager --sdk_root="$HOME/Library/Android/sdk" platform-tools "platforms;android-37.0" \
  "build-tools;37.0.0" "ndk;30.0.16248370" "cmake;4.1.2" emulator \
  "system-images;android-37.0;google_apis;arm64-v8a"
just android-tools-install
```

`just android-build` builds the runtime with cargo-ndk, then the Debug app; Gradle keeps
its files in `.tools/gradle/`. `just android-run` installs the app on the device or
emulator that `adb` sees and opens it. Debug builds use a
[local backend](../../docs/DEVELOPMENT.md#run-the-backend) at `127.0.0.1:5180` that signs
in with the disposable Keycloak of `sign-in/keycloak.sh` at `127.0.0.1:8284`, or the
`KODOSI__BACKEND__API` and `KODOSI__AUTH__ISSUER` of the build's environment. An emulator
reaches them after `adb reverse tcp:5180 tcp:5180` and `adb reverse tcp:8284 tcp:8284`.
Release builds use the Kodosi service.

`just android-check` lints the runtime for Android, checks the Kotlin style with ktlint,
runs Android lint and the unit tests, and builds the Debug app.

The runtime checks certificates with Android's own verifier through the Android part of
`rustls-platform-verifier`, which Gradle takes from the rustls project's Maven archive. Its
version in [libs.versions.toml](gradle/libs.versions.toml) is the one of
`rustls-platform-verifier-android` in `runtime/Cargo.lock`, and
[verification-metadata.xml](gradle/verification-metadata.xml) pins its checksums; change
the three together.

The app needs Android 10 (API level 29), the first with the native thread-local storage
that Rust, Zig and the NDK use. The runtime, Ghostty and the app build for that level and
for 64-bit ARM only.
