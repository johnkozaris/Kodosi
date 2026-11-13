<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="Le logo de Kodosi au-dessus des mots « terminals, agents and friends. together. » et d'un curseur de terminal."></a>
</p>

<h3 align="center">Une app de terminal pour vous, vos agents de code et vos amis.</h3>

<p align="center">
  Voyez quel agent a besoin de vous sur tous vos ordinateurs, voyez ce qu'il crée à côté de son terminal,<br>
  et travaillez avec vos amis dans les mêmes terminaux. Chiffré de bout en bout et open source.
</p>

<p align="center">
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/T%C3%A9l%C3%A9charger-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Télécharger Kodosi pour Mac"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/T%C3%A9l%C3%A9charger-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Télécharger Kodosi pour Linux"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/T%C3%A9l%C3%A9charger-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Télécharger Kodosi pour Windows"></a>
  <a href="https://updates.kodosi.com/download/android"><img src="https://img.shields.io/badge/T%C3%A9l%C3%A9charger-Android-c98566?style=for-the-badge&labelColor=2a211d" alt="Télécharger Kodosi pour Android"></a>
</p>

<p align="center">
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="Dernière version"></a>
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Licence Apache 2.0"></a>
</p>

<p align="center">
  <sub><a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <b>Français</b> · <a href="../it/README.md">Italiano</a> · <a href="../ja/README.md">日本語</a> · <a href="../ko/README.md">한국어</a> · <a href="../zh-Hans/README.md">简体中文</a></sub>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="L'app Kodosi pour Mac : Claude Code attend une approbation pendant que Codex travaille.">
  </picture>
</p>

## Ce que fait Kodosi

### Voyez quel agent a besoin de vous

Claude Code, Codex et d'autres agents de code s'arrêtent et vous attendent. Kodosi réunit les
terminaux de tous vos ordinateurs dans une fenêtre, et chacun indique si son agent travaille,
attend votre réponse, a terminé ou a échoué. Une notification vous prévient quand vous n'êtes
pas là, y compris sur votre téléphone. Vos agents tournent tels quels, avec leurs propres
réglages et connexions.

### Demandez ce qui a besoin de vous

Dosi, un agent pour vos agents, lit vos terminaux, vous dit ce qui vous attend et, quand vous le
souhaitez, répond pour vous dans le terminal.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Quand on lui demande ce qui a besoin de vous, Dosi lit les terminaux, indique que checkout attend une approbation et demande s'il doit l'accorder.">
  </picture>
</p>

### Voyez ce que crée votre agent

Ses plans, ses pages et ses documents s'ouvrent sur un canvas à côté du terminal, et vous
modifiez les documents avec vos amis et l'agent.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Le terminal de Claude Code avec son canvas : un plan de lancement que Leo modifie pendant que Claude suggère une modification, puis une feuille de budget, des diapositives, un PDF et un navigateur partagé où Claude ajoute une tasse au panier.">
  </picture>
</p>

### Travaillez-y ensemble

Donnez à un ami un terminal en cours, ou rassemblez des terminaux, une conversation et des
tâches dans un projet pour les personnes et leurs agents.

<p align="center"><img src="../media/project.webp" width="900" alt="Un projet avec un terminal partagé et une conversation entre personnes et agents."></p>

### Privé et ouvert

Les terminaux et les projets sont chiffrés de bout en bout avec une cryptographie
post-quantique, si bien que le service ne peut pas les lire. Kodosi est open source, et vous
pouvez héberger le service vous-même.
[Sécurité](SECURITY.md) · [Auto-hébergement](../SELF-HOSTING.md) (en anglais)

## Obtenir Kodosi

| | Prérequis | Installation |
| --- | --- | --- |
| **Mac** | Apple silicon, macOS 26 ou ultérieur | Ouvrez l'image disque et déplacez Kodosi dans Applications, ou lancez `brew install --cask johnkozaris/tap/kodosi`. L'app se met à jour toute seule. |
| **Linux** | x86-64, Ubuntu 24.04 ou Debian 13 ou ultérieur | Installez le paquet Debian. Il ajoute le dépôt apt de Kodosi, si bien que les mises à jour du système mettent aussi Kodosi à jour. La [version](https://github.com/johnkozaris/Kodosi/releases/latest) contient aussi un paquet Arch et une archive portable. |
| **Windows** | x64, Windows 11 24H2 ou ultérieur | Avant d'ouvrir le téléchargement pour la première fois, ajoutez en tant qu'administrateur le [certificat de Kodosi](https://updates.kodosi.com/windows/Kodosi.cer) à Ordinateur local › Personnes autorisées. L'app se met à jour toute seule. |
| **Android** | ARM64, Android 15 ou ultérieur | Téléchargez l'APK sur l'appareil, ouvrez-le et autorisez l'installation quand Android le demande. Une nouvelle version s'installe de la même façon. |
| **iPhone et iPad** | iOS 26 ou ultérieur | Compilez Kodosi pour votre [iPhone ou iPad](../../clients/ios/README.md). |

Suivez ensuite la [prise en main](GETTING_STARTED.md).

## En savoir plus

- [Prise en main](GETTING_STARTED.md)
- [Sécurité](SECURITY.md) et la [conception du chiffrement](../CRYPTOGRAPHY.md) (en anglais)
- [Architecture](../ARCHITECTURE.md), [développement](../DEVELOPMENT.md) et
  [contribution](../../CONTRIBUTING.md) (en anglais)
- [Auto-hébergement](../SELF-HOSTING.md) (en anglais)

Signalez les bugs dans les [issues GitHub](https://github.com/johnkozaris/Kodosi/issues) et
les vulnérabilités [en privé](SECURITY.md#signaler-une-vulnérabilité).

Kodosi est conçu par [John Kozaris](https://johnkozaris.com).

## Licence

Kodosi est distribué sous [licence Apache 2.0](../../LICENSE). Le nom et le logo de Kodosi ne
sont pas couverts par cette licence ; les [marques](../../TRADEMARKS.md) (en anglais)
expliquent comment les utiliser. Kodosi utilise [Ghostty](https://ghostty.org), Rust, Swift,
Qt et ASP.NET Core. Les [mentions de tiers](../../terminal/ghostty/THIRD_PARTY_NOTICES.md) et
les [crédits des images](../media/README.md) indiquent ce qu'il contient.
