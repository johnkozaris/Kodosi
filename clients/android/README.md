# Kodosi for Android

The Android app follows and controls remote terminals, starts them on your computers,
and joins projects, friends and Dosi. It embeds the viewer build of the Rust runtime as
a shared library and draws each terminal from Ghostty's state. It hosts no terminals.
See [PRODUCT.md](PRODUCT.md) for its behavior and [DESIGN.md](DESIGN.md) for its theme.

You need JDK 21, the Android SDK in `~/Library/Android/sdk` (`ANDROID_HOME` names another)
with the platform, build tools, NDK and CMake that
[libs.versions.toml](gradle/libs.versions.toml) pins, the pinned Rust toolchain with the
Android target and cargo-ndk (`just android-tools-install`), Bun for the document views,
and `just` and Python 3. On Linux the default SDK is `.tools/android/sdk/` and the JDK is
`.tools/java/`; `ANDROID_HOME` and `JAVA_HOME` override those paths. On a Mac:

```sh
brew install openjdk@21
brew install --cask android-commandlinetools
android --sdk="$HOME/Library/Android/sdk" sdk install platform-tools platforms/android-37.0 \
  build-tools/37.0.0 ndk/30.0.16248370 cmake/4.1.2 emulator \
  system-images/android-37.0/google_apis/arm64-v8a
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

Run checks from the repository root:

| Command | Does |
| --- | --- |
| `just android-check` | Build, lint the Android runtime and Kotlin, run Android lint and JVM tests, and check translations |
| `just android-format` | Format Kotlin and Gradle Kotlin scripts |
| `just android-test` | Run JVM regression tests |
| `just android-device-test` | Build and run terminal, MediaCodec and Keystore boundary tests on an attached ARM64 device |
| `just android-release-build` | Build an optimized release APK |
| `just android-bundle-build` | Build an optimized Android App Bundle |

The Debug APK is `app/build/outputs/apk/debug/app-debug.apk`. With no signing environment,
the release APK is `app/build/outputs/apk/release/app-release-unsigned.apk`. The bundle is
`app/build/outputs/bundle/release/app-release.aab`.

For distribution, set `KODOSI_ANDROID_KEYSTORE` to the absolute path of your key store,
`KODOSI_ANDROID_KEY_ALIAS` to its alias, and `KODOSI_ANDROID_STORE_PASSWORD` and
`KODOSI_ANDROID_KEY_PASSWORD` to their passwords. All four must be set together. The
same recipes then sign the APK (`app-release.apk`) and bundle. Use the existing upload
key for an app that is already in Google Play, and keep keys and passwords outside the
repository and command history. Google Play App Signing uses the upload key for the
bundle and the app-signing key for the APKs that it delivers.

Native libraries and APK packaging use 16 KB alignment. Run the boundary tests on
Android 15 and on a current 16 KB system image. `adb shell getconf PAGE_SIZE` reports
16384 on a 16 KB image. Android's x86_64 `google_apis_ps16k` image simulates 16 KB
userspace; use an ARM64 16 KB image or device to test an actual 16 KB kernel.

The runtime checks certificates with Android's own verifier through the Android part of
`rustls-platform-verifier`, which Gradle takes from the rustls project's Maven archive. Its
version in [libs.versions.toml](gradle/libs.versions.toml) is the one of
`rustls-platform-verifier-android` in `runtime/Cargo.lock`, and
[verification-metadata.xml](gradle/verification-metadata.xml) pins its checksums; change
the three together.

The app needs Android 15 (API level 35, released in 2024) and targets Android 17
(API level 37). It builds for 64-bit ARM only. The Android SDK floor also sets the
viewer runtime's NDK platform. Per-app language, notification permission, device
authentication and app-switcher privacy use the platform APIs directly, without
older Android compatibility paths. Android supplies edge-to-edge window layout;
the app sets system-bar icon appearance from its own light or dark theme.

## Push notifications

Put a Firebase Android configuration at `app/google-services.json` to enable push.
The file is ignored by Git. It must contain a client whose package matches the build:
`com.kodosi.mobile` for release or `com.kodosi.mobile.debug` for Debug. A build without
the file or without a matching client works with push off. The build supplies
`FirebaseOptions` directly; it does not need the Google Services Gradle plugin.

The backend must have FCM HTTP v1 configured. After device approval, the app asks for
notification permission and registers its FCM token through the runtime. Token changes
use WorkManager to retry registration with a network connection. Notification payloads
contain only identifiers and state; the app keeps a bounded private name cache. No
server service-account credential belongs in the app.

## Code and assets

Compose screens collect the account, terminal, project, people, Dosi and canvas stores.
`AppModel` owns the native runtime and routes its ordered events; `CommandSink` pairs
requests with their replies and rejects replies from another account. The application
keeps that state across Activity recreation. Navigation3 restores small navigation
state, not terminal or document contents.

JNI bridges the viewer and Ghostty. The credential bridge encrypts saved keys and
sign-in data with AES-GCM and an Android Keystore key. Document editors use the shared
document views; the shared browser decodes its host's H.264 frames with MediaCodec.
Files use the Storage Access Framework rather than broad storage permission.

The build packages the notices of the runtime's Rust crates, JVM dependencies, Ghostty,
JetBrains Mono, provider marks and document views. Settings shows them under Licenses.
The runtime and app each link their own Ghostty terminal library.
