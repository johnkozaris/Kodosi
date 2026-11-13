<p align="center">
  <a href="https://kodosi.com"><img src="docs/media/banner.webp" width="800" alt="The Kodosi logo above the words 'terminals, agents and friends. together.' and a terminal cursor."></a>
</p>

<h3 align="center">A terminal app for you, your coding agents and your friends.</h3>

<p align="center">
  See which agent needs you on all your computers, see what it makes beside its terminal,<br>
  and work in the same terminals with your friends. End-to-end encrypted and open source.
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
    <img src="docs/media/app-dark.webp" width="900" alt="The Kodosi Mac app: Claude Code waits for an approval while Codex works.">
  </picture>
</p>

## What Kodosi does

### See which agent needs you

Claude Code, Codex and other coding agents stop and wait for you. Kodosi keeps the terminals
of all your computers in one window, and each one shows whether its agent works, waits for
your answer, is done or failed. A notification reaches you when you are away, also on your
phone. Your agents run as they are, with their own settings and sign-in.

### Ask what needs you

Dosi, an agent for your agents, reads your terminals, tells you what waits for you and, when
you choose, answers in the terminal for you.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="docs/media/dosi-still.webp">
    <img src="docs/media/dosi.webp" width="900" alt="Asked what needs you, Dosi reads the terminals, says that checkout waits for an approval and asks whether to allow it.">
  </picture>
</p>

### See what your agent makes

Its plans, pages and documents open on a canvas beside the terminal, and you edit the
documents together with your friends and the agent.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="docs/media/canvas-still.webp">
    <img src="docs/media/canvas.webp" width="900" alt="Claude Code's terminal with its canvas: a launch plan that Leo edits while Claude suggests a change, then a budget sheet, slides, a PDF and a shared browser where Claude adds a mug to the cart.">
  </picture>
</p>

### Work on it together

Give a friend a running terminal, or gather terminals, a conversation and tasks in a project
for people and their agents.

<p align="center"><img src="docs/media/project.webp" width="900" alt="A project with a shared terminal and a conversation of people and agents."></p>

### Private and open

Terminals and projects are end-to-end encrypted with post-quantum cryptography, so the
service cannot read them. Kodosi is open source, and you can run the service yourself.
[Security](docs/SECURITY.md) · [Self-hosting](docs/SELF-HOSTING.md)

## Get Kodosi

| | Needs | Install |
| --- | --- | --- |
| **Mac** | Apple silicon, macOS 26 or later | Open the disk image and move Kodosi to Applications, or run `brew install --cask johnkozaris/tap/kodosi`. The app updates itself. |
| **Linux** | x86-64, Ubuntu 24.04 or Debian 13 or later | Install the Debian package. It adds the Kodosi apt repository, so system updates update Kodosi. The [release](https://github.com/johnkozaris/Kodosi/releases/latest) also has an Arch package and a portable archive. |
| **Windows** | x64, Windows 11 24H2 or later | Before you open the download the first time, add the [Kodosi certificate](https://updates.kodosi.com/windows/Kodosi.cer) to Local Computer › Trusted People, as an administrator. The app updates itself. |
| **Android** | ARM64, Android 15 or later | Download the APK on the device, open it and allow the install when Android asks. Install a new release the same way. |
| **iPhone and iPad** | iOS 26 or later | Build Kodosi for your [iPhone or iPad](clients/ios/README.md). |

Then follow [Getting started](docs/GETTING_STARTED.md).

## Learn more

- [Getting started](docs/GETTING_STARTED.md)
- [Security](docs/SECURITY.md) and the [encryption design](docs/CRYPTOGRAPHY.md)
- [Architecture](docs/ARCHITECTURE.md), [development](docs/DEVELOPMENT.md) and
  [contributing](CONTRIBUTING.md)
- [Self-hosting](docs/SELF-HOSTING.md)

Report bugs in [GitHub issues](https://github.com/johnkozaris/Kodosi/issues) and
vulnerabilities [in private](docs/SECURITY.md#report-a-vulnerability).

Kodosi is made by [John Kozaris](https://johnkozaris.com).

## License

Kodosi is under the [Apache License 2.0](LICENSE). The Kodosi name and logo are not under
that license; [Trademarks](TRADEMARKS.md) says how you can use them. Kodosi uses
[Ghostty](https://ghostty.org), Rust, Swift, Qt and ASP.NET Core. The
[third-party notices](terminal/ghostty/THIRD_PARTY_NOTICES.md) and the
[picture credits](docs/media/README.md) list what it includes.
