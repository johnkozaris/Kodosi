# Kodosi Ghostty

Kodosi's integration of [Ghostty](https://github.com/ghostty-org/ghostty), based on
[Lakr233/libghostty-spm](https://github.com/Lakr233/libghostty-spm). It supplies the
macOS terminal view and Linux terminal engine. Rust owns processes and terminal
state; this integration renders host-managed sessions.

```sh
./Script/build.sh
swift test
./Script/test.sh
```

Build on Apple silicon macOS or x86-64 Linux for that platform's artifact.
See [provenance](PROVENANCE.md) for updates and
[third-party notices](THIRD_PARTY_NOTICES.md) for redistribution obligations.
