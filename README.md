<p align="center">
  <a href="https://kodosi.com"><img src="docs/media/banner.webp" width="800" alt="The Kodosi logo above the words 'terminals, agents and friends. together.' and a terminal cursor."></a>
</p>

<h3 align="center">A terminal app for you, your coding agents, and your friends.</h3>

<p align="center">
  See which agent needs you across all your computers, view what it builds beside its terminal,<br>
  and work in shared terminals with friends. End-to-end encrypted and open source.
</p>

<p align="center">
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/Download-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Download Kodosi for Mac"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/Download-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Download Kodosi for Linux"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/Download-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Download Kodosi for Windows"></a>
  <a href="https://updates.kodosi.com/download/android"><img src="https://img.shields.io/badge/Download-Android-c98566?style=for-the-badge&labelColor=2a211d" alt="Download Kodosi for Android"></a>
</p>

<p align="center">
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Apache License 2.0"></a>
</p>

<p align="center">
  <sub><b>English</b> · <a href="docs/de/README.md">Deutsch</a> · <a href="docs/el/README.md">Ελληνικά</a> · <a href="docs/es/README.md">Español</a> · <a href="docs/fr/README.md">Français</a> · <a href="docs/it/README.md">Italiano</a> · <a href="docs/ja/README.md">日本語</a> · <a href="docs/ko/README.md">한국어</a> · <a href="docs/zh-Hans/README.md">简体中文</a></sub>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="docs/media/app-light.webp">
    <img src="docs/media/app-dark.webp" width="900" alt="The Kodosi Mac app with Claude Code waiting for approval while Codex is working.">
  </picture>
</p>

## What Kodosi does

### See which agent needs you

Claude Code, Codex, and other coding agents regularly pause and wait for input. Kodosi puts the
terminals from all your computers in one window, and each one shows whether its agent is working,
waiting for you, done, or failed. Notifications go to whichever device you're currently using,
including your phone. Your agents run directly through their own CLIs, with their own settings
and accounts.

### Ask what needs you

Dosi is an agent for your other agents. It checks your terminals, summarizes what's waiting on
you, and responds in the terminal once you decide what to do.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="docs/media/dosi-still.webp">
    <img src="docs/media/dosi.webp" width="900" alt="When asked what needs attention, Dosi checks the terminals, reports that checkout is waiting for approval, and asks whether to allow it.">
  </picture>
</p>

### See what your agent builds

Plans, web pages, and documents open on a canvas right beside the terminal. You, your friends,
and the agent can edit live documents together at the same time.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="docs/media/canvas-still.webp">
    <img src="docs/media/canvas.webp" width="900" alt="Claude Code's terminal beside its canvas: a launch plan Leo is editing while Claude suggests a change, along with a budget sheet, slides, a PDF, and a shared browser where Claude adds a mug to the cart.">
  </picture>
</p>

### Work together

Share a live terminal with a friend, or put terminals, a chat, and tasks together in a project
for people and their agents.

<p align="center"><img src="docs/media/project.webp" width="900" alt="A project with a shared terminal and a conversation between people and agents."></p>

### Private and open

Terminals and projects use post-quantum end-to-end encryption, so the backend can't read them.
Kodosi is open source, and you can run the server yourself.
[Security](docs/SECURITY.md) · [Self-hosting](docs/SELF-HOSTING.md)

## Get Kodosi

| Platform | Requirements | Install |
| --- | --- | --- |
| Mac | Apple silicon, macOS 26 or later | Open the disk image and drag Kodosi to Applications, or run `brew install --cask johnkozaris/tap/kodosi`. The app updates itself. |
| Linux | x86-64, Ubuntu 24.04 or Debian 13 or later | Install the Debian package. It adds the Kodosi apt repository so system updates keep Kodosi up to date. The [latest release](https://github.com/johnkozaris/Kodosi/releases/latest) also includes an Arch package and a portable archive. |
| Windows | x64, Windows 11 24H2 or later | Before opening the installer for the first time, add the [Kodosi certificate](https://updates.kodosi.com/windows/Kodosi.cer) to Local Computer › Trusted People as an administrator. The app updates itself. |
| Android | ARM64, Android 15 or later | Download the APK on your device, open it, and allow the install when Android prompts. Install updates the same way. |
| iPhone and iPad | iOS 26 or later | Build Kodosi from source for [iPhone and iPad](clients/ios/README.md). |

Once installed, head to [Getting started](docs/GETTING_STARTED.md).

## Documentation

- [Getting started](docs/GETTING_STARTED.md)
- [Security](docs/SECURITY.md) and the [encryption design](docs/CRYPTOGRAPHY.md)
- [Architecture](docs/ARCHITECTURE.md), [development](docs/DEVELOPMENT.md), and [contributing](CONTRIBUTING.md)
- [Self-hosting](docs/SELF-HOSTING.md)

File bugs in [GitHub issues](https://github.com/johnkozaris/Kodosi/issues) and report
vulnerabilities [privately](docs/SECURITY.md#report-a-vulnerability).

Built by [John Kozaris](https://johnkozaris.com).

## License

Kodosi is licensed under the [Apache License 2.0](LICENSE). The Kodosi name and logo aren't
covered by that license; see [Trademarks](TRADEMARKS.md) for usage rules. Kodosi is built with
[Ghostty](https://ghostty.org), Rust, Swift, Qt, and ASP.NET Core. See the
[third-party notices](terminal/ghostty/THIRD_PARTY_NOTICES.md) and
[media credits](docs/media/README.md) for attributions.
