# Kodosi Ghostty

This folder is Kodosi's integration of [Ghostty](https://github.com/ghostty-org/ghostty),
based on [Lakr233/libghostty-spm](https://github.com/Lakr233/libghostty-spm). It provides the
macOS and iOS terminal views and the Linux, Windows and Android terminal engine.

```sh
./scripts/build.sh            # on Apple silicon macOS, Linux x86-64, or Windows 11 in Git Bash
./scripts/build.sh --android  # the Android engine, on macOS or Linux
swift test
./scripts/test.sh
```

See [provenance](PROVENANCE.md) for updates and checks, and the
[third-party notices](THIRD_PARTY_NOTICES.md).
