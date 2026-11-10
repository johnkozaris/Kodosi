<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="Kodosi 标志，下方是“terminals, agents and friends. together.”字样和一个终端光标。"></a>
</p>

<p align="center">
  <a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <a href="../fr/README.md">Français</a> · <a href="../it/README.md">Italiano</a> · <a href="../ja/README.md">日本語</a> · <a href="../ko/README.md">한국어</a> · <b>简体中文</b>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Apache License 2.0"></a>
  <a href="#获取-kodosi"><img src="https://img.shields.io/badge/macOS-26%2B%20%C2%B7%20Apple%20silicon-c98566?style=flat-square&labelColor=2a211d" alt="Apple silicon 上的 macOS 26 或更高版本"></a>
  <a href="#获取-kodosi"><img src="https://img.shields.io/badge/Linux-x86--64-c98566?style=flat-square&labelColor=2a211d" alt="Linux x86-64"></a>
  <a href="#获取-kodosi"><img src="https://img.shields.io/badge/Windows-11%2024H2%2B%20%C2%B7%20x64-c98566?style=flat-square&labelColor=2a211d" alt="x64 上的 Windows 11 24H2 或更高版本"></a>
  <a href="../../clients/ios/README.md"><img src="https://img.shields.io/badge/iOS-26%2B%20%C2%B7%20iPhone%20and%20iPad-c98566?style=flat-square&labelColor=2a211d" alt="iPhone 和 iPad 上的 iOS 26 或更高版本"></a>
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="最新版本"></a>
</p>

[Kodosi](https://kodosi.com) 是一款面向编程 Agent 的终端应用，支持 Mac、Linux 和 Windows。它把你所有电脑上的终端放在一个窗口里，告诉你哪个 Agent 需要你，还能让好友在同一个终端里一起工作。每个终端旁都有画布，Agent 会在上面展示它的计划、页面和文档，你们可以一起编辑。终端和房间均采用端到端加密。

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="Kodosi Mac 应用：Claude Code 在等待批准，Codex 正在工作。">
  </picture>
</p>

## Kodosi 能做什么

**所有终端，一个窗口。** 你的 Mac、Linux 或 Windows 电脑，以及好友电脑上的终端，都在同一个侧边栏里。每个终端都是运行在各自电脑上的真实 shell，你可以在同一个窗口里，在自己的任何一台电脑上启动新终端。

**一眼看出哪个 Agent 需要你。** 运行 Claude Code、Codex、Copilot CLI 或任何其他终端 Agent。每个终端都会显示它的 Agent 是在工作、在等你批准、已完成还是已失败；Kodosi 在后台时，通知会提醒你。

**Dosi，管理 Agent 的 Agent。** 问问 Dosi 什么需要你处理。Dosi 会读取你所有的终端和房间，告诉你，并替你行动：把回答输入终端、启动 Agent、关闭已完成的任务。你和 Dosi 的对话会跟着你到每一台电脑。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="被问到什么需要你处理时，Dosi 读取终端，告诉你 checkout 在等待批准，并询问是否允许。">
  </picture>
</p>

**共享实时终端。** 把一个正在运行的终端及其 Agent 的完全控制权交给好友。

**每个终端旁都有画布。** Agent 会在终端旁给你展示页面、计划或图片。文档、表格、演示文稿、PDF 和文本文件都是实时的：你、你的好友和 Agent 可以同时编辑同一个真实文件。Agent 还会使用一个与你共享的浏览器：你能以实时视频看到它在做什么，也能在同样的页面里点击和输入。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Claude Code 的终端和它的画布：Leo 正在编辑的发布计划里，Claude 提出了一处修改；接着是预算表格、幻灯片、PDF，以及 Claude 把一个杯子加入购物车的共享浏览器。">
  </picture>
</p>

**为人和 Agent 准备的房间。** 房间里有共享终端、对话和任务。Agent 会在对话中阅读和发言、认领任务，GitHub 或 Gitea 的 Issue 也会变成任务。

<p align="center"><img src="../media/room.webp" width="900" alt="一个房间，里面有一个共享终端，以及人和 Agent 的对话。"></p>

**每个 Agent 一个分支。** 在新分支、独立的 Git worktree 中启动 Agent，两个 Agent 就永远不会改动同一批文件。

**iPhone 和 iPad 也能用。** 不在电脑旁时，也能关注、回复和控制你的终端和房间。

**端到端加密，抗量子。** 你的密钥只留在你的设备上。你的设备使用 ML-KEM-768 与 X25519 对终端和房间进行端到端加密，因此服务无法读取其内容。[安全](SECURITY.md)

## 获取 Kodosi

<p>
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/%E4%B8%8B%E8%BD%BD-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="下载 Mac 版 Kodosi"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/%E4%B8%8B%E8%BD%BD-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="下载 Linux 版 Kodosi"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/%E4%B8%8B%E8%BD%BD-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="下载 Windows 版 Kodosi"></a>
</p>

- **Mac**（Apple silicon，macOS 26 或更高版本）：打开磁盘映像，把 Kodosi 拖到“应用程序”，或运行 `brew install --cask johnkozaris/tap/kodosi`。应用会自动更新。
- **Linux**（x86-64，Ubuntu 24.04 或 Debian 13 或更高版本）：安装 Debian 软件包。它会添加 Kodosi 的 apt 仓库，所以系统更新时也会更新 Kodosi。[发布页](https://github.com/johnkozaris/Kodosi/releases/latest)还提供 Arch 软件包和便携压缩包。
- **Windows**（x64，Windows 11 24H2 或更高版本）：第一次打开下载的文件前，请以管理员身份把 [Kodosi 证书](https://updates.kodosi.com/windows/Kodosi.cer)添加到“本地计算机 › 受信任人”。应用会自动更新。
- **iPhone 和 iPad：**为你的 [iPhone 或 iPad](../../clients/ios/README.md) 构建 Kodosi。

然后按照[快速上手](GETTING_STARTED.md)操作。

## 了解更多

- [快速上手](GETTING_STARTED.md)
- [安全](SECURITY.md)以及[加密设计](../CRYPTOGRAPHY.md)（英文）
- [架构](../ARCHITECTURE.md)、[开发](../DEVELOPMENT.md)和[参与贡献](../../CONTRIBUTING.md)（英文）
- [自行托管](../SELF-HOSTING.md)（英文）

请在 [GitHub Issues](https://github.com/johnkozaris/Kodosi/issues) 中报告 bug，并[私下](SECURITY.md#报告漏洞)报告漏洞。

Kodosi 由 [John Kozaris](https://johnkozaris.com) 开发。

## 许可证

Kodosi 采用 [Apache License 2.0](../../LICENSE) 许可证。Kodosi 的名称和标志不在该许可证范围内，使用方式请参阅[商标](../../TRADEMARKS.md)（英文）。Kodosi 使用了 [Ghostty](https://ghostty.org)、Rust、Swift、Qt 和 ASP.NET Core。所包含的内容列在[第三方声明](../../terminal/ghostty/THIRD_PARTY_NOTICES.md)和[图片来源](../media/README.md)中。
