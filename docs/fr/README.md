<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="Le logo de Kodosi au-dessus des mots « terminals, agents and friends. together. » et d'un curseur de terminal."></a>
</p>

<h3 align="center">Une app de terminal pour vous, vos agents de code et vos amis.</h3>

<p align="center">
  Voyez quel agent a besoin de vous sur tous vos ordinateurs, suivez ce qu'il crée à côté de son terminal<br>
  et travaillez avec vos amis dans des terminaux partagés. Chiffré de bout en bout et open source.
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
    <img src="../media/app-dark.webp" width="900" alt="L'app Kodosi sur Mac : Claude Code attend une approbation pendant que Codex travaille.">
  </picture>
</p>

## Ce que fait Kodosi

### Voyez quel agent a besoin de vous

Claude Code, Codex et les autres agents de code font régulièrement une pause en attendant votre
intervention. Kodosi réunit les terminaux de tous vos ordinateurs dans une seule fenêtre, et chacun
indique si son agent travaille, vous attend, a terminé ou a échoué. Les notifications arrivent sur
l'appareil que vous utilisez à ce moment-là, y compris votre téléphone. Vos agents s'exécutent
directement via leur propre CLI, avec leurs propres réglages et comptes.

### Demandez ce qui a besoin de vous

Dosi est un agent pour vos autres agents. Il consulte vos terminaux, résume ce qui vous attend et
répond dans le terminal dès que vous décidez quoi faire.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Lorsqu'on lui demande ce qui requiert votre attention, Dosi vérifie les terminaux, signale que checkout attend une approbation et demande s'il doit l'autoriser.">
  </picture>
</p>

### Voyez ce que crée votre agent

Plans, pages web et documents s'ouvrent sur un canvas juste à côté du terminal. Vous, vos amis et
l'agent pouvez modifier des documents en direct ensemble, au même moment.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Le terminal de Claude Code à côté de son canvas : un plan de lancement que Leo modifie pendant que Claude suggère un changement, accompagné d'une feuille de budget, de diapositives, d'un PDF et d'un navigateur partagé où Claude ajoute une tasse au panier.">
  </picture>
</p>

### Travaillez ensemble

Partagez un terminal en direct avec un ami, ou rassemblez terminaux, chat et tâches dans un projet
commun pour les personnes et leurs agents.

<p align="center"><img src="../media/project.webp" width="900" alt="Un projet réunissant un terminal partagé et une conversation entre personnes et agents."></p>

### Privé et ouvert

Les terminaux et les projets utilisent un chiffrement de bout en bout post-quantique, si bien que
le backend ne peut pas les lire. Kodosi est open source, et vous pouvez héberger le serveur
vous-même.
[Sécurité](SECURITY.md) · [Auto-hébergement](../SELF-HOSTING.md) (en anglais)

## Obtenir Kodosi

| Plateforme | Prérequis | Installation |
| --- | --- | --- |
| Mac | Apple Silicon, macOS 26 ou ultérieur | Ouvrez l'image disque et glissez Kodosi dans Applications, ou lancez `brew install --cask johnkozaris/tap/kodosi`. L'app se met à jour automatiquement. |
| Linux | x86-64, Ubuntu 24.04 ou Debian 13 ou ultérieur | Installez le paquet Debian. Il ajoute le dépôt apt de Kodosi afin que les mises à jour du système maintiennent Kodosi à jour. La [dernière version](https://github.com/johnkozaris/Kodosi/releases/latest) inclut aussi un paquet Arch et une archive portable. |
| Windows | x64, Windows 11 24H2 ou ultérieur | Avant d'ouvrir l'installeur pour la première fois, ajoutez en tant qu'administrateur le [certificat de Kodosi](https://updates.kodosi.com/windows/Kodosi.cer) dans Ordinateur local › Personnes autorisées. L'app se met à jour automatiquement. |
| Android | ARM64, Android 15 ou ultérieur | Téléchargez l'APK sur votre appareil, ouvrez-le et autorisez l'installation lorsque Android vous le demande. Les mises à jour s'installent de la même manière. |
| iPhone et iPad | iOS 26 ou ultérieur | Compilez Kodosi depuis les sources pour [iPhone et iPad](../../clients/ios/README.md). |

Une fois Kodosi installé, passez au guide de [prise en main](GETTING_STARTED.md).

## Documentation

- [Prise en main](GETTING_STARTED.md)
- [Sécurité](SECURITY.md) et la [conception du chiffrement](../CRYPTOGRAPHY.md) (en anglais)
- [Architecture](../ARCHITECTURE.md), [développement](../DEVELOPMENT.md) et
  [contribution](../../CONTRIBUTING.md) (en anglais)
- [Auto-hébergement](../SELF-HOSTING.md) (en anglais)

Signalez les bugs dans les [issues GitHub](https://github.com/johnkozaris/Kodosi/issues) et
les vulnérabilités [en privé](SECURITY.md#signaler-une-vulnérabilité).

Créé par [John Kozaris](https://johnkozaris.com).

## Licence

Kodosi est distribué sous [licence Apache 2.0](../../LICENSE). Le nom et le logo Kodosi ne sont pas
couverts par cette licence ; consultez [Marques](../../TRADEMARKS.md) (en anglais) pour les règles
d'utilisation. Kodosi est développé avec [Ghostty](https://ghostty.org), Rust, Swift, Qt et
ASP.NET Core. Consultez les [mentions de tiers](../../terminal/ghostty/THIRD_PARTY_NOTICES.md) et
les [crédits des médias](../media/README.md) pour les attributions.
