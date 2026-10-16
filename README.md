<p align="center">
  <img src="docs/media/banner.webp" width="800" alt="Kodosi — terminals, agents and friends. Together.">
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-c98566?style=flat-square&labelColor=2a211d" alt="MIT license"></a>
  <a href="clients/macos/README.md"><img src="https://img.shields.io/badge/macOS-26.4%2B%20%C2%B7%20Apple%20silicon-c98566?style=flat-square&labelColor=2a211d" alt="macOS 26.4 or later on Apple silicon"></a>
  <a href="clients/qt/README.md"><img src="https://img.shields.io/badge/Linux-x86--64-c98566?style=flat-square&labelColor=2a211d" alt="Linux x86-64"></a>
  <a href="clients/ios/README.md"><img src="https://img.shields.io/badge/iOS-26%2B%20%C2%B7%20iPhone%20and%20iPad-c98566?style=flat-square&labelColor=2a211d" alt="iOS 26 or later on iPhone and iPad"></a>
  <img src="https://img.shields.io/badge/status-in%20development-8a7a70?style=flat-square&labelColor=2a211d" alt="In development">
</p>

Kodosi is a native app where people and their coding agents work together. Your
terminals, your other computers, your friends' terminals and the agents in them are in
one place, end-to-end encrypted. The iPhone and iPad app follows the same terminals and
rooms when you are away.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="docs/media/app-light.webp">
    <img src="docs/media/app-dark.webp" width="900" alt="The Kodosi Mac app with four terminals on three computers.">
  </picture>
</p>

## What it does

**Every terminal, on every computer.** A terminal is a real shell on the computer that
started it. Open it from your other devices, along with the terminals friends share
with you.

<p align="center"><img src="docs/media/overview.webp" width="760" alt="Six terminals on three computers, grouped by computer."></p>

**See which agent needs you.** Each terminal shows when its program works, waits for
you, is done or failed. A notification tells you when Kodosi is in the background.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="docs/media/status.webp">
    <img src="docs/media/status.gif" width="790" alt="A terminal works, waits for an approval, works again and is done.">
  </picture>
</p>

**Bring your agents.** Claude Code, Codex, Copilot CLI or any terminal agent runs in its
own harness with its own settings and sign-in. Pick an agent, account, model and folder,
type a first prompt, and a new terminal starts it.

<p align="center"><img src="docs/media/start.webp" width="550" alt="A terminal at its prompt with marks that start Claude Code, Codex and a second Claude account."></p>

**A branch for each agent.** Name a branch and Kodosi starts a terminal in its own Git
worktree, so two agents never change the same files.

<p align="center"><img src="docs/media/branch.webp" width="750" alt="The New branch panel beside a repository folder."></p>

**Work together in a room.** A room holds terminals, a conversation and tasks. Share a
terminal with the room once and every member can use it. Agents join through the
bundled [room skill](runtime/skills/kodosi-room/SKILL.md), and GitHub or Gitea issues
can become tasks.

<p align="center"><img src="docs/media/room.webp" width="900" alt="A room with shared terminals and a conversation of people and agents."></p>
<p align="center"><img src="docs/media/tasks.webp" width="600" alt="Room tasks up for grabs, in progress and done."></p>

**End-to-end encrypted.** Private keys stay on your devices. The service relays
encrypted traffic and cannot read your terminals or rooms; it sees only whether each
agent works, waits or is done. [Security](docs/SECURITY.md)

<p align="center"><img src="docs/media/share.webp" width="560" alt="The share panel of a terminal."></p>

## Build

You need a Mac with Apple silicon and macOS 26.4 or later, or Linux x86-64 (Ubuntu 24.04
recommended), plus the pinned Rust toolchain, `just` and Python 3. Kodosi also needs a
backend and an OIDC sign-in service; for development run a
[local backend](docs/DEVELOPMENT.md#run-the-backend).

```sh
just mac-build      # Mac app; then run the KodosiDesktop scheme in Xcode
just linux-build    # Linux app; then run ./clients/qt/build/dev/src/kodosi-qt
just ios-build      # iPhone and iPad app; then run the KodosiMobile scheme in Xcode
```

Platform guides: [macOS](clients/macos/README.md), [Linux and Windows](clients/qt/README.md),
[iPhone and iPad](clients/ios/README.md).

## Documentation

- [Getting started](docs/GETTING_STARTED.md): terminals, agents, sharing and rooms.
- [Development](docs/DEVELOPMENT.md) and [contributing](CONTRIBUTING.md).
- [Architecture](docs/ARCHITECTURE.md) and [self-hosting](docs/SELF-HOSTING.md).
- [Security](docs/SECURITY.md) and the [encryption design](docs/CRYPTOGRAPHY.md).

Kodosi is in active development and has no release yet. Report bugs in
[GitHub issues](https://github.com/johnkozaris/Kodosi/issues) and vulnerabilities
[in private](docs/SECURITY.md#report-a-vulnerability).

## License

Kodosi is [MIT licensed](LICENSE). The name and logo are not: see
[Trademarks](TRADEMARKS.md). Built with [Ghostty](https://ghostty.org), Rust, Swift, Qt
and ASP.NET Core; see the [third-party notices](terminal/ghostty/THIRD_PARTY_NOTICES.md)
and [picture credits](docs/media/README.md).
