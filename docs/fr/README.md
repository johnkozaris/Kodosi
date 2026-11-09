<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="Le logo de Kodosi au-dessus des mots « terminals, agents and friends. together. » et d'un curseur de terminal."></a>
</p>

<p align="center">
  <a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <b>Français</b> · <a href="../it/README.md">Italiano</a> · <a href="../ja/README.md">日本語</a> · <a href="../ko/README.md">한국어</a> · <a href="../zh-Hans/README.md">简体中文</a>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Licence Apache 2.0"></a>
  <a href="#obtenir-kodosi"><img src="https://img.shields.io/badge/macOS-26.4%2B%20%C2%B7%20Apple%20silicon-c98566?style=flat-square&labelColor=2a211d" alt="macOS 26.4 ou ultérieur sur Apple silicon"></a>
  <a href="#obtenir-kodosi"><img src="https://img.shields.io/badge/Linux-x86--64-c98566?style=flat-square&labelColor=2a211d" alt="Linux x86-64"></a>
  <a href="#obtenir-kodosi"><img src="https://img.shields.io/badge/Windows-11%2024H2%2B%20%C2%B7%20x64-c98566?style=flat-square&labelColor=2a211d" alt="Windows 11 24H2 ou ultérieur sur x64"></a>
  <a href="../../clients/ios/README.md"><img src="https://img.shields.io/badge/iOS-26%2B%20%C2%B7%20iPhone%20and%20iPad-c98566?style=flat-square&labelColor=2a211d" alt="iOS 26 ou ultérieur sur iPhone et iPad"></a>
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="Dernière version"></a>
</p>

[Kodosi](https://kodosi.com) est une app de terminal pour les agents de code, sur Mac, Linux
et Windows. Elle réunit les terminaux de tous vos ordinateurs dans une fenêtre, vous montre
quel agent a besoin de vous et laisse vos amis travailler dans les mêmes terminaux. Chaque
terminal a un canvas à côté, où l'agent montre ses plans, ses pages et ses documents, et
vous les modifiez ensemble. Les terminaux et les salons sont chiffrés de bout en bout.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="L'app Kodosi pour Mac : Claude Code attend une approbation pendant que Codex travaille.">
  </picture>
</p>

## Ce que fait Kodosi

**Tous vos terminaux dans une fenêtre.** Les terminaux de votre Mac, de votre ordinateur sous
Linux ou Windows et des ordinateurs de vos amis sont dans une seule barre latérale. Chacun est
un vrai shell sur son propre ordinateur, et vous en lancez un nouveau sur n'importe lequel de
vos ordinateurs depuis la même fenêtre.

**Voyez quel agent a besoin de vous.** Lancez Claude Code, Codex, Copilot CLI ou tout autre
agent de terminal. Chaque terminal indique quand son agent travaille, attend votre
approbation, a terminé ou a échoué, et une notification vous prévient pendant que Kodosi est
en arrière-plan.

**Dosi, votre agent pour vos agents.** Demandez à Dosi ce qui a besoin de vous. Il lit tous
vos terminaux et salons, vous le dit et agit pour vous : il tape la réponse dans un terminal,
lance des agents et ferme ce qui est terminé. Votre conversation avec Dosi vous suit sur
chaque ordinateur.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Quand on lui demande ce qui a besoin de vous, Dosi lit les terminaux, indique que checkout attend une approbation et demande s'il doit l'accorder.">
  </picture>
</p>

**Partagez un terminal en direct.** Donnez à un ami le contrôle total d'un terminal en cours
et de son agent.

**Un canvas à côté de chaque terminal.** Un agent vous montre une page, un plan ou une image
à côté de son terminal. Les documents, les feuilles, les présentations, les PDF et les
fichiers texte sont en direct : vous, vos amis et l'agent modifiez le même vrai fichier en
même temps. L'agent utilise aussi un navigateur que vous partagez avec lui : vous voyez ce
qu'il fait en vidéo en direct, et vous cliquez et tapez dans les mêmes pages.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Le terminal de Claude Code avec son canvas : un plan de lancement que Leo modifie pendant que Claude suggère une modification, puis une feuille de budget, des diapositives, un PDF et un navigateur partagé où Claude ajoute une tasse au panier.">
  </picture>
</p>

**Des salons pour les personnes et les agents.** Un salon réunit des terminaux partagés, une
conversation et des tâches. Les agents lisent et écrivent dans la conversation et prennent des
tâches, et les issues GitHub ou Gitea deviennent des tâches.

<p align="center"><img src="../media/room.webp" width="900" alt="Un salon avec un terminal partagé et une conversation entre personnes et agents."></p>

**Une branche pour chaque agent.** Lancez un agent sur une nouvelle branche, dans son propre
worktree Git, pour que deux agents ne modifient jamais les mêmes fichiers.

**Sur votre iPhone et votre iPad.** Suivez, répondez et contrôlez vos terminaux et salons
quand vous n'êtes pas devant votre ordinateur.

**Chiffré de bout en bout, post-quantique.** Vos clés restent sur vos appareils. Vos appareils
chiffrent les terminaux et les salons de bout en bout avec ML-KEM-768 combiné à X25519, si bien
que le service ne peut pas les lire. [Sécurité](SECURITY.md)

## Obtenir Kodosi

<p>
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/T%C3%A9l%C3%A9charger-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Télécharger Kodosi pour Mac"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/T%C3%A9l%C3%A9charger-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Télécharger Kodosi pour Linux"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/T%C3%A9l%C3%A9charger-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Télécharger Kodosi pour Windows"></a>
</p>

- **Mac** (Apple silicon, macOS 26.4 ou ultérieur) : ouvrez l'image disque et déplacez Kodosi
  dans Applications, ou lancez `brew install --cask johnkozaris/tap/kodosi`. L'app se met à
  jour toute seule.
- **Linux** (x86-64, Ubuntu 24.04 ou Debian 13 ou ultérieur) : installez le paquet Debian. Il
  ajoute le dépôt apt de Kodosi, si bien que les mises à jour du système mettent aussi Kodosi
  à jour. La [version](https://github.com/johnkozaris/Kodosi/releases/latest) contient aussi
  un paquet Arch et une archive portable.
- **Windows** (x64, Windows 11 24H2 ou ultérieur) : avant d'ouvrir le téléchargement pour la
  première fois, ajoutez en tant qu'administrateur le
  [certificat de Kodosi](https://updates.kodosi.com/windows/Kodosi.cer) à Ordinateur local ›
  Personnes autorisées. L'app se met à jour toute seule.
- **iPhone et iPad :** compilez Kodosi pour votre [iPhone ou iPad](../../clients/ios/README.md).

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
