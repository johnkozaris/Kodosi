# Contributing

Read [PRODUCT.md](PRODUCT.md), [AGENTS.md](AGENTS.md) and
[Development](docs/DEVELOPMENT.md).

- Open an [issue](https://github.com/johnkozaris/Kodosi/issues) for a bug or a larger
  change; describe the problem before proposing a new subsystem. Small fixes can go
  straight to a pull request. Report vulnerabilities
  [in private](docs/SECURITY.md#report-a-vulnerability).
- Keep each pull request focused. Explain the problem, the result and how you verified
  it, with a picture or recording for interface changes.
- Run the relevant checks from the root `justfile` and try the changed workflow with
  isolated data. Add regression tests where they protect real behavior.
- Keep generated contracts and Ghostty pins aligned, and preserve license notices.
- Keep credentials, signing keys, private terminal content and personal configuration
  out of Git, issues and logs.

Contributions are under the [MIT license](LICENSE) and the
[Code of Conduct](CODE_OF_CONDUCT.md).
