# Kodosi for Android

Native Android app for watching and controlling remote terminals, launching terminals on your
computers, and using projects, friends, and Dosi. It embeds the viewer build of the Rust runtime
as a shared library and renders terminals from Ghostty's state without hosting local terminals.
See [PRODUCT.md](PRODUCT.md) for product behavior and [DESIGN.md](DESIGN.md) for visual design.

## Prerequisites and setup

Requires JDK 21, the Android SDK at `~/Library/Android/sdk` (override with `ANDROID_HOME`) with
`platform-tools` plus the exact `compileSdk`, `buildTools`, `ndk`, and `cmake` versions pinned in
[libs.versions.toml](gradle/libs.versions.toml), and the pinned Rust toolchain with the Android
target and `cargo-ndk` (`just android-tools-install`). On Linux, default paths are
`.tools/android/sdk/` and `.tools/java/` (overridable via `ANDROID_HOME` and `JAVA_HOME`).

Setup on macOS from the repository root:

```sh
brew install openjdk@21
brew install --cask android-commandlinetools
pin() { sed -n "s/^$1 = \"\(.*\)\"\$/\1/p" clients/android/gradle/libs.versions.toml; }
android --sdk="$HOME/Library/Android/sdk" sdk install platform-tools \
  "platforms/android-$(pin compileSdk)" "build-tools/$(pin buildTools)" "ndk/$(pin ndk)" "cmake/$(pin cmake)"
just android-tools-install
```

To use an emulator, also install `emulator` and an ARM64 system image.

## Building and running

`just android-build` compiles the Rust runtime with `cargo-ndk` and builds the Debug APK. Gradle
stores its cache and daemons in `.tools/gradle/` inside the main checkout so worktrees share them.
`just android-run` installs and launches the app on the connected device or emulator via `adb`.

Debug builds point to a [local backend](../../docs/DEVELOPMENT.md#run-the-backend) at
`127.0.0.1:5180` and the disposable Keycloak from `sign-in/keycloak.sh` at `127.0.0.1:8284`
(overridable via `KODOSI__BACKEND__API` and `KODOSI__AUTH__ISSUER` at build time). On an emulator,
forward both ports first with `adb reverse tcp:5180 tcp:5180` and `adb reverse tcp:8284 tcp:8284`.
Release builds point to the production Kodosi backend.

Run checks from the repository root:

| Command | Purpose |
| --- | --- |
| `just android-check` | Build, lint Android Rust and Kotlin, run Android Lint and JVM tests, and verify translations |
| `just android-format` | Format Kotlin and Gradle Kotlin scripts |
| `just android-test` | Run JVM unit tests |
| `just android-device-test` | Run terminal, MediaCodec, and Keystore tests on an attached ARM64 device |
| `just android-release-build` | Build an optimized release APK |
| `just android-bundle-build` | Build an optimized Android App Bundle (AAB) |

Outputs:

- Debug APK: `app/build/outputs/apk/debug/app-debug.apk`
- Unsigned release APK: `app/build/outputs/apk/release/app-release-unsigned.apk`
- App Bundle: `app/build/outputs/bundle/release/app-release.aab`

## Signing, versioning, and compatibility

To sign release builds (`app-release.apk` and AAB), set all four variables together:
`KODOSI_ANDROID_KEYSTORE` (absolute path), `KODOSI_ANDROID_KEY_ALIAS`,
`KODOSI_ANDROID_STORE_PASSWORD`, and `KODOSI_ANDROID_KEY_PASSWORD`. Keep keys and passwords out of
Git and shell history. For apps already on Google Play, sign with the existing upload key; Play
App Signing handles delivery signing.

Android requires monotonically increasing version codes, and Google Play accepts each version code
only once. The version code combines the app version and `KODOSI_BUILD` (defaults to `1`); bump
`KODOSI_BUILD` to upload a new build of the same version.

Native libraries and APK packaging use 16 KB page alignment. Run the boundary tests on Android 15
and on a 16 KB system image (`adb shell getconf PAGE_SIZE` prints `16384`). Android's x86_64
`google_apis_ps16k` image only emulates 16 KB userspace; use an ARM64 16 KB image or hardware
device to test a real 16 KB kernel.

TLS certificate verification uses Android's platform verifier via `rustls-platform-verifier` (pulled
from rustls's Maven repository). Keep the version in [libs.versions.toml](gradle/libs.versions.toml),
`rustls-platform-verifier-android` in `runtime/Cargo.lock`, and the checksums in
[verification-metadata.xml](gradle/verification-metadata.xml) updated together.

The app targets 64-bit ARM (`arm64-v8a`) only, requires Android 15 (API 35), and targets Android
17 (API 37); the runtime's NDK platform matches the SDK floor. Per-app language, notification
permissions, device authentication, app-switcher privacy, and edge-to-edge layout use native
Android 15+ APIs directly without legacy compatibility paths.

## Push notifications

To enable push notifications, drop a Firebase config at `app/google-services.json` (gitignored)
containing a client matching the build's package name (`com.kodosi.mobile` for Release or
`com.kodosi.mobile.debug` for Debug). Builds without it run cleanly with push disabled. The build
supplies `FirebaseOptions` directly and doesn't need the Google Services Gradle plugin.

The backend must have FCM HTTP v1 configured. After device approval, the app requests notification
permission, registers with FCM by its Firebase installation ID, and passes that ID to the backend
through the runtime. When FCM reports a new installation ID, WorkManager sends it to the backend
once the phone is online. Push payloads only carry terminal IDs, agents, and states;
human-readable names come from a bounded local cache. Never ship server service-account credentials
in the app.

## Architecture overview

Compose screens observe dedicated stores for account, terminals, projects, friends, Dosi, and
canvas. `AppModel` owns the native runtime and dispatches ordered events; `CommandSink` correlates
commands with replies and discards replies from another account. Application-scoped state survives
Activity recreation, while Navigation3 persists lightweight navigation state only (not terminal or
document payloads).

JNI bridges the Rust viewer runtime and Ghostty. Credential storage encrypts keys and auth tokens
with AES-GCM using an Android Keystore key. Document editors load the shared web document views,
the shared browser decodes H.264 video frames via `MediaCodec`, and file import/export uses the
Storage Access Framework rather than broad storage permissions.

The build bundles third-party notices for Rust crates, JVM dependencies, Ghostty, JetBrains Mono,
provider icons, and document views (displayed under **Settings › Licenses**). The runtime and the
Android app each link their own Ghostty VT library.
