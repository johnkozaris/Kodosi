<p align="center">
  <img src="docs/media/banner.webp" width="800" alt="The Kodosi logo above the words 'terminals, agents and friends. together.' and a terminal cursor.">
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Apache License 2.0"></a>
  <a href="#get-kodosi"><img src="https://img.shields.io/badge/macOS-26.4%2B%20%C2%B7%20Apple%20silicon-c98566?style=flat-square&labelColor=2a211d" alt="macOS 26.4 or later on Apple silicon"></a>
  <a href="#get-kodosi"><img src="https://img.shields.io/badge/Linux-x86--64-c98566?style=flat-square&labelColor=2a211d" alt="Linux x86-64"></a>
  <a href="#get-kodosi"><img src="https://img.shields.io/badge/Windows-11%2024H2%2B%20%C2%B7%20x64-c98566?style=flat-square&labelColor=2a211d" alt="Windows 11 24H2 or later on x64"></a>
  <a href="clients/ios/README.md"><img src="https://img.shields.io/badge/iOS-26%2B%20%C2%B7%20iPhone%20and%20iPad-c98566?style=flat-square&labelColor=2a211d" alt="iOS 26 or later on iPhone and iPad"></a>
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="Latest release"></a>
</p>

Kodosi is a terminal app for coding agents on Mac, Linux and Windows. It keeps the
terminals of all your computers in one window, shows which agent needs you, and lets your
friends work in the same terminals. Everything is end-to-end encrypted.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="docs/media/app-light.webp">
    <img src="docs/media/app-dark.webp" width="900" alt="The Kodosi Mac app: Claude Code waits for an approval while Codex works.">
  </picture>
</p>

## What Kodosi does

**Every terminal in one window.** The terminals of your Mac, your Linux or Windows computer
and your friends' computers are in one sidebar. Each one is a real shell on its own
computer, and you start a new one on any of your computers from the same window.

**Know which agent needs you.** Run Claude Code, Codex, Copilot CLI or any other terminal
agent. Each terminal shows when its agent works, waits for your approval, is done or
failed, and a notification tells you while Kodosi is in the background.

**Dosi, your agent for your agents.** Ask Dosi what needs you. It reads all your terminals
and rooms, tells you, and acts for you: it types the answer into a terminal, starts agents
and closes what is done. Your conversation with Dosi follows you to every computer.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="docs/media/dosi-still.webp">
    <img src="docs/media/dosi.webp" width="900" alt="Asked what needs you, Dosi reads the terminals, says that checkout waits for an approval and asks whether to allow it.">
  </picture>
</p>

**Share a live terminal.** Give a friend full control of a running terminal and its agent.

**A canvas next to each terminal.** An agent shows you a page, a plan, a picture, a PDF or
an Office file beside its terminal, and uses a browser that you share with it: you see what
it does, and you click and type in the same pages.

**Rooms for people and agents.** A room holds shared terminals, a conversation and tasks.
Agents read and post in the conversation and take tasks, and GitHub or Gitea issues
become tasks.

<p align="center"><img src="docs/media/room.webp" width="900" alt="A room with a shared terminal and a conversation of people and agents."></p>

**A branch for each agent.** Start an agent on a new branch in its own Git worktree, so
two agents never change the same files.

**On your iPhone and iPad.** Follow, answer and control your terminals and rooms when you
are away from your computer.

**End-to-end encrypted, post-quantum.** Your keys stay on your devices. Your devices encrypt
terminals and rooms end to end with ML-KEM-768 together with X25519, so the service cannot
read them. [Security](docs/SECURITY.md)

## Get Kodosi

<p>
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/Download-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Download Kodosi for Mac"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/Download-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Download Kodosi for Linux"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/Download-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Download Kodosi for Windows"></a>
</p>

- **Mac** (Apple silicon, macOS 26.4 or later): open the disk image and move Kodosi to
  Applications, or run `brew install --cask johnkozaris/tap/kodosi`. The app updates
  itself.
- **Linux** (x86-64, Ubuntu 24.04 or Debian 13 or later): install the Debian package. It
  adds the Kodosi apt repository, so system updates update Kodosi. The
  [release](https://github.com/johnkozaris/Kodosi/releases/latest) also has an Arch
  package and a portable archive.
- **Windows** (x64, Windows 11 24H2 or later): before you open the download the first
  time, add the [Kodosi certificate](https://updates.kodosi.com/windows/Kodosi.cer) to
  Local Computer › Trusted People, as an administrator. The app updates itself.
- **iPhone and iPad:** build Kodosi for your [iPhone or iPad](clients/ios/README.md).

Then follow [Getting started](docs/GETTING_STARTED.md).

## Learn more

- [Getting started](docs/GETTING_STARTED.md)
- [Security](docs/SECURITY.md) and the [encryption design](docs/CRYPTOGRAPHY.md)
- [Architecture](docs/ARCHITECTURE.md), [development](docs/DEVELOPMENT.md) and
  [contributing](CONTRIBUTING.md)
- [Self-hosting](docs/SELF-HOSTING.md)

Report bugs in [GitHub issues](https://github.com/johnkozaris/Kodosi/issues) and
vulnerabilities [in private](docs/SECURITY.md#report-a-vulnerability).

## License

Kodosi is under the [Apache License 2.0](LICENSE). The Kodosi name and logo are not under
that license; [Trademarks](TRADEMARKS.md) says how you can use them. Kodosi uses
[Ghostty](https://ghostty.org), Rust, Swift, Qt and ASP.NET Core. The
[third-party notices](terminal/ghostty/THIRD_PARTY_NOTICES.md) and the
[picture credits](docs/media/README.md) list what it includes.
