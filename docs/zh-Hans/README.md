<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="Kodosi 标志，下方是“terminals, agents and friends. together.”字样和一个终端光标。"></a>
</p>

<h3 align="center">一款为你、你的编程 Agent 和好友准备的终端应用。</h3>

<p align="center">
  随时看到所有电脑上哪个 Agent 需要你，在终端旁查看它构建的内容，<br>
  还能和好友在共享终端中一起工作。端到端加密，开源。
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
    <img src="../media/app-dark.webp" width="900" alt="Kodosi Mac 应用界面：Claude Code 正在等待批准，Codex 正在工作。">
  </picture>
</p>

## Kodosi 能做什么

### 看看哪个 Agent 需要你

Claude Code、Codex 等编程 Agent 经常会暂停下来等待你的输入。Kodosi 将你所有电脑上的终端汇聚在同一个窗口中，每个终端都会清晰显示其中的 Agent 是在工作中、等待你的操作、已完成还是已失败。通知只会发送到你当前正在使用的设备，包括你的手机。各个 Agent 均直接通过自身的 CLI 运行，继续使用它们原有的设置和账号。

### 问问有哪些事需要你处理

Dosi 是专门统筹其他 Agent 的 Agent。它会检查你的各个终端，汇总哪些事项正在等你处理，并在你做出决定后直接在终端里代为回复。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="当被问到有哪些事需要处理时，Dosi 检查各个终端，汇报 checkout 正在等待批准，并询问是否允许。">
  </picture>
</p>

### 查看 Agent 构建的内容

计划、网页和文档会直接在终端旁的画布上打开。你、好友以及 Agent 可以同时协作编辑实时文档。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Claude Code 的终端及其右侧画布：Leo 正在编辑发布计划，Claude 提出了一处修改建议；画布上还有预算表格、幻灯片、PDF，以及 Claude 正在将马克杯加入购物车的共享浏览器。">
  </picture>
</p>

### 一起协作

你可以将正在运行的终端共享给好友，也可以把终端、聊天和任务整合到同一个项目中，供大家和各自的 Agent 一起协作。

<p align="center"><img src="../media/project.webp" width="900" alt="一个包含共享终端以及人与 Agent 对话的项目。"></p>

### 私密且开放

终端和项目均采用抗量子端到端加密，后端无法读取其中的内容。Kodosi 是开源软件，你也可以自行部署服务器。
[安全](SECURITY.md) · [自行托管](../SELF-HOSTING.md)（英文）

## 获取 Kodosi

| 平台 | 系统要求 | 安装方式 |
| --- | --- | --- |
| Mac | Apple silicon，macOS 26 或更高版本 | 打开磁盘映像，将 Kodosi 拖入“应用程序”文件夹，或运行 `brew install --cask johnkozaris/tap/kodosi`。应用支持自动更新。 |
| Linux | x86-64，Ubuntu 24.04 或 Debian 13 及更高版本 | 安装 Debian 软件包。它会自动添加 Kodosi 的 apt 仓库，方便随系统更新同步升级 Kodosi。[最新版本](https://github.com/johnkozaris/Kodosi/releases/latest)页面还提供 Arch 软件包和便携压缩包。 |
| Windows | x64，Windows 11 24H2 或更高版本 | 首次打开安装程序前，请以管理员身份将 [Kodosi 证书](https://updates.kodosi.com/windows/Kodosi.cer)添加到“本地计算机 › 受信任人”。应用支持自动更新。 |
| Android | ARM64，Android 15 或更高版本 | 在设备上下载并打开 APK，在 Android 弹出提示时允许安装。后续更新也采用相同方式安装。 |
| iPhone 和 iPad | iOS 26 或更高版本 | 从源码为 [iPhone 和 iPad](../../clients/ios/README.md) 构建 Kodosi。 |

安装完成后，请阅读[快速上手](GETTING_STARTED.md)。

## 文档

- [快速上手](GETTING_STARTED.md)
- [安全](SECURITY.md)与[加密设计](../CRYPTOGRAPHY.md)（英文）
- [架构](../ARCHITECTURE.md)、[开发](../DEVELOPMENT.md)和[参与贡献](../../CONTRIBUTING.md)（英文）
- [自行托管](../SELF-HOSTING.md)（英文）

如发现问题，请在 [GitHub Issues](https://github.com/johnkozaris/Kodosi/issues) 中提交；如发现安全漏洞，请通过[私密渠道](SECURITY.md#报告漏洞)报告。

由 [John Kozaris](https://johnkozaris.com) 构建。

## 许可证

Kodosi 采用 [Apache License 2.0](../../LICENSE) 开源许可。Kodosi 的名称和标志不在该许可证涵盖范围内，使用规则请参阅[商标](../../TRADEMARKS.md)（英文）。Kodosi 基于 [Ghostty](https://ghostty.org)、Rust、Swift、Qt 和 ASP.NET Core 构建。相关归属声明见[第三方声明](../../terminal/ghostty/THIRD_PARTY_NOTICES.md)和[媒体素材来源](../media/README.md)。
