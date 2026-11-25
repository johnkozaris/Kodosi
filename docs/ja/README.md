<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="「terminals, agents and friends. together.」の文字とターミナルのカーソルの上にある Kodosi のロゴ。"></a>
</p>

<h3 align="center">あなたと、コーディングエージェントと、フレンドのためのターミナルアプリ。</h3>

<p align="center">
  すべてのコンピュータでどのエージェントが対応を待っているかがわかり、エージェントが作るものをターミナルの隣で見られます。<br>
  フレンドと共有ターミナルで一緒に作業できます。エンドツーエンド暗号化に対応したオープンソースです。
</p>

<p align="center">
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/%E3%83%80%E3%82%A6%E3%83%B3%E3%83%AD%E3%83%BC%E3%83%89-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Mac 版 Kodosi をダウンロード"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/%E3%83%80%E3%82%A6%E3%83%B3%E3%83%AD%E3%83%BC%E3%83%89-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Linux 版 Kodosi をダウンロード"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/%E3%83%80%E3%82%A6%E3%83%B3%E3%83%AD%E3%83%BC%E3%83%89-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Windows 版 Kodosi をダウンロード"></a>
  <a href="https://updates.kodosi.com/download/android"><img src="https://img.shields.io/badge/%E3%83%80%E3%82%A6%E3%83%B3%E3%83%AD%E3%83%BC%E3%83%89-Android-c98566?style=for-the-badge&labelColor=2a211d" alt="Android 版 Kodosi をダウンロード"></a>
</p>

<p align="center">
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="最新リリース"></a>
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Apache License 2.0"></a>
</p>

<p align="center">
  <sub><a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <a href="../fr/README.md">Français</a> · <a href="../it/README.md">Italiano</a> · <b>日本語</b> · <a href="../ko/README.md">한국어</a> · <a href="../zh-Hans/README.md">简体中文</a></sub>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="Mac 版 Kodosi アプリ。Claude Code が承認を待ち、Codex が作業している画面。">
  </picture>
</p>

## Kodosi でできること

### どのエージェントが対応を待っているかを見る

Claude Code や Codex などのコーディングエージェントは、作業の途中でたびたび手を止めて入力を待ちます。Kodosi はすべてのコンピュータのターミナルを 1 つのウインドウにまとめ、各ターミナルでエージェントが作業中か、あなたの対応待ちか、完了したか、失敗したかを表示します。通知はスマートフォンを含め、今使っているデバイスに届きます。エージェントはそれぞれの設定とアカウントのまま、独自の CLI を通じて直接動作します。

### 何に対応が必要か聞く

Dosi は、ほかのエージェントをまとめるエージェントです。各ターミナルを確認してあなたの対応待ちになっている内容をまとめ、あなたが方針を決めると代わりにターミナルへ応答します。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="何に対応が必要か尋ねられた Dosi が、各ターミナルを確認して checkout が承認待ちであると報告し、許可するかどうかを確認している画面。">
  </picture>
</p>

### エージェントが作るものを見る

計画書やウェブページ、ドキュメントはターミナルのすぐ隣のキャンバスに開きます。ライブドキュメントは、あなた、フレンド、エージェントが同時に編集できます。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Claude Code のターミナルと隣のキャンバス。Leo が編集中のローンチ計画に Claude が変更を提案し、さらに予算シート、スライド、PDF、そして Claude がマグカップをカートに入れている共有ブラウザが並んでいます。">
  </picture>
</p>

### 一緒に作業する

実行中のターミナルをフレンドと共有したり、人とエージェントのためのプロジェクトにターミナル、チャット、タスクをまとめたりできます。

<p align="center"><img src="../media/project.webp" width="900" alt="共有ターミナルと、人とエージェントの会話が並ぶプロジェクト画面。"></p>

### プライベートでオープン

ターミナルとプロジェクトは耐量子エンドツーエンド暗号化で保護されているため、バックエンドから内容を読むことはできません。Kodosi はオープンソースで、サーバーを自分で運用することもできます。
[セキュリティ](SECURITY.md) · [セルフホスティング](../SELF-HOSTING.md)（英語）

## Kodosi を入手

| プラットフォーム | 動作環境 | インストール |
| --- | --- | --- |
| Mac | Apple silicon、macOS 26 以降 | ディスクイメージを開いて Kodosi をアプリケーションフォルダへドラッグするか、`brew install --cask johnkozaris/tap/kodosi` を実行します。アプリは自動でアップデートされます。 |
| Linux | x86-64、Ubuntu 24.04 または Debian 13 以降 | Debian パッケージをインストールします。Kodosi の apt リポジトリが追加されるため、システムのアップデートで Kodosi も最新に保たれます。[最新リリース](https://github.com/johnkozaris/Kodosi/releases/latest)には Arch パッケージとポータブルアーカイブも含まれています。 |
| Windows | x64、Windows 11 24H2 以降 | インストーラーを初めて開く前に、管理者として [Kodosi の証明書](https://updates.kodosi.com/windows/Kodosi.cer)を「ローカルコンピューター › 信頼されたユーザー」に追加してください。アプリは自動でアップデートされます。 |
| Android | ARM64、Android 15 以降 | 端末に APK をダウンロードして開き、Android の確認画面でインストールを許可します。アップデートも同じ手順でインストールします。 |
| iPhone と iPad | iOS 26 以降 | [iPhone と iPad](../../clients/ios/README.md) 向けにソースから Kodosi をビルドします。 |

インストールが終わったら、[はじめに](GETTING_STARTED.md)へ進んでください。

## ドキュメント

- [はじめに](GETTING_STARTED.md)
- [セキュリティ](SECURITY.md)と[暗号化の設計](../CRYPTOGRAPHY.md)（英語）
- [アーキテクチャ](../ARCHITECTURE.md)、[開発](../DEVELOPMENT.md)、[コントリビュート](../../CONTRIBUTING.md)（英語）
- [セルフホスティング](../SELF-HOSTING.md)（英語）

バグは [GitHub の Issue](https://github.com/johnkozaris/Kodosi/issues) へ、脆弱性は[非公開で](SECURITY.md#脆弱性を報告する)報告してください。

Kodosi は [John Kozaris](https://johnkozaris.com) が開発しています。

## ライセンス

Kodosi は [Apache License 2.0](../../LICENSE) のもとで公開されています。Kodosi の名称とロゴはこのライセンスの対象外です。使用ルールは[商標](../../TRADEMARKS.md)（英語）をご覧ください。Kodosi は [Ghostty](https://ghostty.org)、Rust、Swift、Qt、ASP.NET Core で構築されています。帰属表示については[サードパーティの表示](../../terminal/ghostty/THIRD_PARTY_NOTICES.md)および[メディアのクレジット](../media/README.md)をご覧ください。
