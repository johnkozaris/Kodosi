# Kodosi Ghostty

Kodosi's integration of [Ghostty](https://github.com/ghostty-org/ghostty), originally derived from
[Lakr233/libghostty-spm](https://github.com/Lakr233/libghostty-spm). It supplies the native
terminal views on macOS and iOS and the VT engine on Linux, Windows, and Android.

```sh
./scripts/build.sh            # Apple silicon macOS, Linux x86-64, or Windows 11 (Git Bash)
./scripts/build.sh --android  # Android VT engine (run on macOS or Linux)
swift test
./scripts/test.sh
```

See [PROVENANCE.md](PROVENANCE.md) for pins, checks, and upgrade instructions, and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for third-party attributions.
