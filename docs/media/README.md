# Media assets

Screenshots and animations are captured from the macOS app using fabricated users and sample data
against a local backend. When replacing an asset:

- Use a disposable database and a dedicated `KODOSI_DATA_ROOT` per demo user (see
  [isolated validation](../DEVELOPMENT.md#isolated-validation)).
- Configure `<KODOSI_DATA_ROOT>/harness-home/.zshrc` with a neutral prompt.
- Never capture real terminals, usernames, hostnames, or local paths, and preserve the existing
  filename.

## Credits

The Kodosi logo, color palette, and UI artwork are project assets. Terminal text uses JetBrains
Mono under the
[SIL Open Font License](../../terminal/ghostty/ThirdPartyNotices/licenses/jetbrains-mono-OFL-1.1.txt).
Claude and Codex icons come from LobeHub under the
[MIT license](../../clients/apple/Notices/LobeHub-Icons-MIT.txt); see
[provider icon attributions](../../clients/apple/Notices/Provider-Icons.txt). All third-party
product names and trademarks belong to their respective owners and do not imply endorsement.
