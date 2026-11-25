# Contributing

Start by reading [PRODUCT.md](PRODUCT.md), [AGENTS.md](AGENTS.md), and
[Development](docs/DEVELOPMENT.md).

- Open an [issue](https://github.com/johnkozaris/Kodosi/issues) for bugs or larger changes, and
  describe the problem before proposing a new subsystem. Small fixes can go straight to a pull
  request. Report security vulnerabilities [privately](docs/SECURITY.md#report-a-vulnerability).
- Keep pull requests focused. Explain the problem, the change, and how you tested it, and include
  a screenshot or recording for UI changes.
- Run the relevant checks from the root `justfile` and test the affected workflow against isolated
  data. Add regression tests where they protect real behavior.
- Keep generated protocol contracts and Ghostty pins in sync, and preserve license notices.
- Keep credentials, signing keys, private terminal content, and personal configs out of Git,
  issues, and logs.

Contributions are licensed under the [Apache License 2.0](LICENSE) and follow the
[Code of Conduct](CODE_OF_CONDUCT.md).
