<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="Das Kodosi-Logo über den Worten „terminals, agents and friends. together.“ und einem Terminal-Cursor."></a>
</p>

<h3 align="center">Eine Terminal-App für dich, deine Coding-Agents und deine Freunde.</h3>

<p align="center">
  Sieh auf all deinen Computern, welcher Agent dich braucht, schau dir neben seinem Terminal an, was er erstellt,<br>
  und arbeite mit Freunden in geteilten Terminals. Ende-zu-Ende-verschlüsselt und Open Source.
</p>

<p align="center">
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/Herunterladen-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Kodosi für Mac herunterladen"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/Herunterladen-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Kodosi für Linux herunterladen"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/Herunterladen-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Kodosi für Windows herunterladen"></a>
  <a href="https://updates.kodosi.com/download/android"><img src="https://img.shields.io/badge/Herunterladen-Android-c98566?style=for-the-badge&labelColor=2a211d" alt="Kodosi für Android herunterladen"></a>
</p>

<p align="center">
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="Neuestes Release"></a>
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Apache License 2.0"></a>
</p>

<p align="center">
  <sub><a href="../../README.md">English</a> · <b>Deutsch</b> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <a href="../fr/README.md">Français</a> · <a href="../it/README.md">Italiano</a> · <a href="../ja/README.md">日本語</a> · <a href="../ko/README.md">한국어</a> · <a href="../zh-Hans/README.md">简体中文</a></sub>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="Die Kodosi-App auf dem Mac: Claude Code wartet auf eine Freigabe, während Codex arbeitet.">
  </picture>
</p>

## Was Kodosi kann

### Sieh, welcher Agent dich braucht

Claude Code, Codex und andere Coding-Agents halten regelmäßig an und warten auf deine Eingabe.
Kodosi vereint die Terminals all deiner Computer in einem Fenster, und jedes zeigt an, ob sein Agent
gerade arbeitet, auf dich wartet, fertig ist oder fehlgeschlagen ist. Mitteilungen erscheinen auf
dem Gerät, das du gerade nutzt, auch auf deinem Smartphone. Deine Agents laufen direkt über ihre
eigenen CLIs, mit ihren eigenen Einstellungen und Konten.

### Frag, wo du gebraucht wirst

Dosi ist ein Agent für deine anderen Agents. Dosi prüft deine Terminals, fasst zusammen, was auf
dich wartet, und antwortet im Terminal, sobald du entschieden hast, was zu tun ist.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Auf die Frage, wo du gebraucht wirst, prüft Dosi die Terminals, meldet, dass checkout auf eine Freigabe wartet, und fragt, ob es sie erlauben soll.">
  </picture>
</p>

### Sieh, was dein Agent erstellt

Pläne, Webseiten und Dokumente öffnen sich auf einem Canvas direkt neben dem Terminal. Du, deine
Freunde und der Agent könnt Live-Dokumente gleichzeitig gemeinsam bearbeiten.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Das Terminal von Claude Code neben seinem Canvas: ein Launch-Plan, den Leo bearbeitet, während Claude eine Änderung vorschlägt, außerdem eine Budgettabelle, Folien, ein PDF und ein geteilter Browser, in dem Claude eine Tasse in den Warenkorb legt.">
  </picture>
</p>

### Gemeinsam arbeiten

Teile ein laufendes Terminal mit einem Freund, oder bündle Terminals, einen Chat und Aufgaben in
einem Projekt für Menschen und ihre Agents.

<p align="center"><img src="../media/project.webp" width="900" alt="Ein Projekt mit einem geteilten Terminal und einer Unterhaltung zwischen Menschen und Agents."></p>

### Privat und offen

Terminals und Projekte nutzen Post-Quanten-Ende-zu-Ende-Verschlüsselung, sodass das Backend sie
nicht lesen kann. Kodosi ist Open Source, und du kannst den Server selbst betreiben.
[Sicherheit](SECURITY.md) · [Selbst hosten](../SELF-HOSTING.md) (Englisch)

## Kodosi herunterladen

| Plattform | Voraussetzungen | Installation |
| --- | --- | --- |
| Mac | Apple Silicon, macOS 26 oder neuer | Öffne das Disk-Image und ziehe Kodosi in den Ordner „Programme“, oder führe `brew install --cask johnkozaris/tap/kodosi` aus. Die App aktualisiert sich selbst. |
| Linux | x86-64, Ubuntu 24.04 oder Debian 13 oder neuer | Installiere das Debian-Paket. Es fügt das apt-Repository von Kodosi hinzu, sodass Systemupdates auch Kodosi aktuell halten. Das [neueste Release](https://github.com/johnkozaris/Kodosi/releases/latest) enthält außerdem ein Arch-Paket und ein portables Archiv. |
| Windows | x64, Windows 11 24H2 oder neuer | Bevor du das Installationsprogramm zum ersten Mal öffnest, füge als Administrator das [Kodosi-Zertifikat](https://updates.kodosi.com/windows/Kodosi.cer) unter Lokaler Computer › Vertrauenswürdige Personen hinzu. Die App aktualisiert sich selbst. |
| Android | ARM64, Android 15 oder neuer | Lade die APK auf dein Gerät herunter, öffne sie und erlaube die Installation, wenn Android danach fragt. Updates installierst du auf demselben Weg. |
| iPhone und iPad | iOS 26 oder neuer | Baue Kodosi aus dem Quellcode für [iPhone und iPad](../../clients/ios/README.md). |

Nach der Installation geht es weiter mit [Erste Schritte](GETTING_STARTED.md).

## Dokumentation

- [Erste Schritte](GETTING_STARTED.md)
- [Sicherheit](SECURITY.md) und das [Verschlüsselungsdesign](../CRYPTOGRAPHY.md) (Englisch)
- [Architektur](../ARCHITECTURE.md), [Entwicklung](../DEVELOPMENT.md) und
  [Mitwirken](../../CONTRIBUTING.md) (Englisch)
- [Selbst hosten](../SELF-HOSTING.md) (Englisch)

Melde Fehler in den [GitHub-Issues](https://github.com/johnkozaris/Kodosi/issues) und
Schwachstellen [vertraulich](SECURITY.md#eine-schwachstelle-melden).

Entwickelt von [John Kozaris](https://johnkozaris.com).

## Lizenz

Kodosi steht unter der [Apache License 2.0](../../LICENSE). Der Name und das Logo von Kodosi sind
von dieser Lizenz nicht abgedeckt; die Nutzungsregeln stehen in den
[Markenrichtlinien](../../TRADEMARKS.md) (Englisch). Kodosi baut auf
[Ghostty](https://ghostty.org), Rust, Swift, Qt und ASP.NET Core auf. Weitere Angaben zu Urhebern
und Lizenzen findest du in den
[Hinweisen zu Drittanbietern](../../terminal/ghostty/THIRD_PARTY_NOTICES.md) und den
[Bildnachweisen](../media/README.md).
