<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="Kodosi 标志，下方是“terminals, agents and friends. together.”字样和一个终端光标。"></a>
</p>

<h3 align="center">一款为你、你的编程 Agent 和好友准备的终端应用。</h3>

<p align="center">
  看到你所有电脑上哪个 Agent 需要你，在终端旁看到它做出的东西，<br>
  还能和好友在同一个终端里一起工作。端到端加密，开源。
</p>

<p align="center">
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/%E4%B8%8B%E8%BD%BD-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="下载 Mac 版 Kodosi"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/%E4%B8%8B%E8%BD%BD-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="下载 Linux 版 Kodosi"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/%E4%B8%8B%E8%BD%BD-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="下载 Windows 版 Kodosi"></a>
  <a href="https://updates.kodosi.com/download/android"><img src="https://img.shields.io/badge/%E4%B8%8B%E8%BD%BD-Android-c98566?style=for-the-badge&labelColor=2a211d" alt="下载 Android 版 Kodosi"></a>
</p>

<p align="center">
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="最新版本"></a>
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Apache License 2.0"></a>
</p>

<p align="center">
  <sub><a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <a href="../fr/README.md">Français</a> · <a href="../it/README.md">Italiano</a> · <a href="../ja/README.md">日本語</a> · <a href="../ko/README.md">한국어</a> · <b>简体中文</b></sub>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="Kodosi Mac 应用：Claude Code 在等待批准，Codex 正在工作。">
  </picture>
</p>

## Kodosi 能做什么

### 看看哪个 Agent 需要你

Claude Code、Codex 等编程 Agent 会停下来等你。Kodosi 把你所有电脑上的终端放在一个窗口里，每个终端都会显示它的 Agent 是在工作、在等你回答、已完成还是已失败。不在电脑旁时，通知会提醒你，手机上也能收到。你的 Agent 照原样运行，沿用它们自己的设置和登录信息。

### 问问什么需要你处理

Dosi 是管理你的 Agent 的 Agent。它会读取你的终端，告诉你有什么在等你处理，并按你的选择替你在终端里作答。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="被问到什么需要你处理时，Dosi 读取终端，告诉你 checkout 在等待批准，并询问是否允许。">
  </picture>
</p>

### 看看你的 Agent 做出了什么

它的计划、页面和文档会在终端旁的画布上打开，你可以和好友以及 Agent 一起编辑这些文档。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Claude Code 的终端和它的画布：Leo 正在编辑的发布计划里，Claude 提出了一处修改；接着是预算表格、幻灯片、PDF，以及 Claude 把一个杯子加入购物车的共享浏览器。">
  </picture>
</p>

### 一起协作

把一个正在运行的终端交给好友，或者把终端、对话和任务汇集到一个为人和他们的 Agent 准备的项目里。

<p align="center"><img src="../media/project.webp" width="900" alt="一个项目，里面有一个共享终端，以及人和 Agent 的对话。"></p>

### 私密且开放

终端和项目采用抗量子的端到端加密，因此服务无法读取其内容。Kodosi 是开源的，你也可以自行运行这项服务。[安全](SECURITY.md) · [自行托管](../SELF-HOSTING.md)（英文）

## 获取 Kodosi

| | 要求 | 安装 |
| --- | --- | --- |
| **Mac** | Apple silicon，macOS 26 或更高版本 | 打开磁盘映像，把 Kodosi 拖到“应用程序”，或运行 `brew install --cask johnkozaris/tap/kodosi`。应用会自动更新。 |
| **Linux** | x86-64，Ubuntu 24.04 或 Debian 13 或更高版本 | 安装 Debian 软件包。它会添加 Kodosi 的 apt 仓库，所以系统更新时也会更新 Kodosi。[发布页](https://github.com/johnkozaris/Kodosi/releases/latest)还提供 Arch 软件包和便携压缩包。 |
| **Windows** | x64，Windows 11 24H2 或更高版本 | 第一次打开下载的文件前，请以管理员身份把 [Kodosi 证书](https://updates.kodosi.com/windows/Kodosi.cer)添加到“本地计算机 › 受信任人”。应用会自动更新。 |
| **Android** | ARM64，Android 15 或更高版本 | 在设备上下载 APK 并打开，Android 询问时允许安装。新版本也用同样的方法安装。 |
| **iPhone 和 iPad** | iOS 26 或更高版本 | 为你的 [iPhone 或 iPad](../../clients/ios/README.md) 构建 Kodosi。 |

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
