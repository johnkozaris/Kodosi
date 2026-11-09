<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="「terminals, agents and friends. together.」という言葉とターミナルのカーソルの上にある Kodosi のロゴ。"></a>
</p>

<p align="center">
  <a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <a href="../fr/README.md">Français</a> · <a href="../it/README.md">Italiano</a> · <b>日本語</b> · <a href="../ko/README.md">한국어</a> · <a href="../zh-Hans/README.md">简体中文</a>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Apache License 2.0"></a>
  <a href="#kodosi-を入手"><img src="https://img.shields.io/badge/macOS-26.4%2B%20%C2%B7%20Apple%20silicon-c98566?style=flat-square&labelColor=2a211d" alt="Apple silicon 搭載の macOS 26.4 以降"></a>
  <a href="#kodosi-を入手"><img src="https://img.shields.io/badge/Linux-x86--64-c98566?style=flat-square&labelColor=2a211d" alt="Linux x86-64"></a>
  <a href="#kodosi-を入手"><img src="https://img.shields.io/badge/Windows-11%2024H2%2B%20%C2%B7%20x64-c98566?style=flat-square&labelColor=2a211d" alt="x64 の Windows 11 24H2 以降"></a>
  <a href="../../clients/ios/README.md"><img src="https://img.shields.io/badge/iOS-26%2B%20%C2%B7%20iPhone%20and%20iPad-c98566?style=flat-square&labelColor=2a211d" alt="iPhone と iPad の iOS 26 以降"></a>
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="最新リリース"></a>
</p>

[Kodosi](https://kodosi.com) は、Mac、Linux、Windows で使えるコーディングエージェント向けのターミナルアプリです。すべてのコンピュータのターミナルを 1 つのウインドウにまとめ、どのエージェントがあなたを必要としているかを示し、フレンドと同じターミナルで作業できます。ターミナルとルームはエンドツーエンドで暗号化されます。

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="Mac の Kodosi アプリ。Claude Code が承認を待ち、Codex が作業しています。">
  </picture>
</p>

## Kodosi でできること

**すべてのターミナルを 1 つのウインドウに。** Mac、Linux や Windows のコンピュータ、そしてフレンドのコンピュータのターミナルが 1 つのサイドバーに並びます。どれもそのコンピュータで動く本物のシェルで、同じウインドウから自分のどのコンピュータにも新しいターミナルを起動できます。

**どのエージェントがあなたを必要としているかがわかる。** Claude Code、Codex、Copilot CLI など、ターミナルで動くエージェントなら何でも使えます。各ターミナルには、エージェントが作業中か、承認待ちか、完了したか、失敗したかが表示され、Kodosi がバックグラウンドにあるときは通知でお知らせします。

**Dosi、エージェントのためのエージェント。** 何に対応が必要か Dosi に聞いてみましょう。Dosi はすべてのターミナルとルームを読んで答え、代わりに動きます。ターミナルに答えを入力し、エージェントを起動し、終わったものを閉じます。Dosi との会話はどのコンピュータにも引き継がれます。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="何に対応が必要か聞かれた Dosi が、ターミナルを読み、checkout が承認を待っていると伝え、許可するかどうかを尋ねています。">
  </picture>
</p>

**動いているターミナルを共有。** 実行中のターミナルとそのエージェントのフル操作をフレンドに渡せます。

**各ターミナルの隣にキャンバス。** エージェントはページ、計画、画像をターミナルの隣に表示します。ドキュメント、シート、プレゼンテーション、PDF、テキストファイルはライブで、あなたとフレンドとエージェントが同じ実ファイルを同時に編集でき、エージェントの変更は承認または却下できます。エージェントはあなたと共有するブラウザも使います。その操作はライブ映像で見えて、同じページであなたもクリックや入力ができます。

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Claude Code のターミナルとキャンバス。Leo が編集しているローンチ計画に Claude が変更を提案し、続いて予算のシート、スライド、PDF、共有ブラウザで Claude がマグをカートに入れます。">
  </picture>
</p>

**人とエージェントのためのルーム。** ルームには共有ターミナル、会話、タスクがあります。エージェントは会話を読んで投稿し、タスクを引き受けます。GitHub や Gitea の Issue はタスクになります。

<p align="center"><img src="../media/room.webp" width="900" alt="共有ターミナルと、人とエージェントの会話があるルーム。"></p>

**エージェントごとにブランチを。** エージェントを専用の Git worktree の新しいブランチで起動すれば、2 つのエージェントが同じファイルを変更することはありません。

**iPhone と iPad でも。** コンピュータから離れているときも、ターミナルとルームを確認し、答え、操作できます。

**エンドツーエンド暗号化、耐量子。** 鍵はあなたのデバイスから出ません。デバイスはターミナルとルームを ML-KEM-768 と X25519 の組み合わせでエンドツーエンドに暗号化するため、サービス側で読むことはできません。[セキュリティ](SECURITY.md)

## Kodosi を入手

<p>
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/%E3%83%80%E3%82%A6%E3%83%B3%E3%83%AD%E3%83%BC%E3%83%89-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Mac 版 Kodosi をダウンロード"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/%E3%83%80%E3%82%A6%E3%83%B3%E3%83%AD%E3%83%BC%E3%83%89-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Linux 版 Kodosi をダウンロード"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/%E3%83%80%E3%82%A6%E3%83%B3%E3%83%AD%E3%83%BC%E3%83%89-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Windows 版 Kodosi をダウンロード"></a>
</p>

- **Mac**（Apple silicon、macOS 26.4 以降）：ディスクイメージを開いて Kodosi をアプリケーションフォルダに移動するか、`brew install --cask johnkozaris/tap/kodosi` を実行します。アプリは自動でアップデートされます。
- **Linux**（x86-64、Ubuntu 24.04 または Debian 13 以降）：Debian パッケージをインストールします。Kodosi の apt リポジトリが追加されるので、システムのアップデートで Kodosi もアップデートされます。[リリース](https://github.com/johnkozaris/Kodosi/releases/latest) には Arch 用パッケージとポータブル版のアーカイブもあります。
- **Windows**（x64、Windows 11 24H2 以降）：ダウンロードしたファイルを初めて開く前に、管理者として [Kodosi の証明書](https://updates.kodosi.com/windows/Kodosi.cer) をローカルコンピューター › 信頼されたユーザーに追加してください。アプリは自動でアップデートされます。
- **iPhone と iPad**：[iPhone または iPad](../../clients/ios/README.md) 向けに Kodosi をビルドします。

続いて[はじめに](GETTING_STARTED.md)をお読みください。

## さらに詳しく

- [はじめに](GETTING_STARTED.md)
- [セキュリティ](SECURITY.md)と[暗号化の設計](../CRYPTOGRAPHY.md)（英語）
- [アーキテクチャ](../ARCHITECTURE.md)、[開発](../DEVELOPMENT.md)、[コントリビュート](../../CONTRIBUTING.md)（英語）
- [セルフホスティング](../SELF-HOSTING.md)（英語）

バグは [GitHub の Issue](https://github.com/johnkozaris/Kodosi/issues) に、脆弱性は[非公開で](SECURITY.md#脆弱性を報告する)報告してください。

Kodosi は [John Kozaris](https://johnkozaris.com) が開発しています。

## ライセンス

Kodosi は [Apache License 2.0](../../LICENSE) で提供されています。Kodosi の名前とロゴはこのライセンスの対象外です。使い方は[商標](../../TRADEMARKS.md)（英語）をご覧ください。Kodosi は [Ghostty](https://ghostty.org)、Rust、Swift、Qt、ASP.NET Core を使っています。含まれるものは[サードパーティの表示](../../terminal/ghostty/THIRD_PARTY_NOTICES.md)と[画像のクレジット](../media/README.md)に記載しています。
