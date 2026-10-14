# Kodosi Ghostty

Kodosi's integration of [Ghostty](https://github.com/ghostty-org/ghostty), based on
[Lakr233/libghostty-spm](https://github.com/Lakr233/libghostty-spm). It supplies the
macOS and iOS terminal views and the Linux and Windows terminal engine. Rust owns
processes and terminal state; this integration renders host-managed sessions.

```sh
./scripts/build.sh
swift test
./scripts/test.sh
```

Build on Apple silicon macOS for the macOS, iOS and iOS Simulator slices, on x86-64
Linux, or on Windows 11 x64 or ARM64 (in Git Bash) for that platform's artifact.
Commit the Windows x64 artifact that the Windows Ghostty library workflow builds
on a clean runner, not one from a local build. See
[provenance](PROVENANCE.md) for updates and checks, and
[third-party notices](THIRD_PARTY_NOTICES.md) for redistribution obligations.
