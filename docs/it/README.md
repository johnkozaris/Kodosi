<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="Il logo di Kodosi sopra le parole «terminals, agents and friends. together.» e un cursore di terminale."></a>
</p>

<p align="center">
  <a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <a href="../fr/README.md">Français</a> · <b>Italiano</b> · <a href="../ja/README.md">日本語</a> · <a href="../ko/README.md">한국어</a> · <a href="../zh-Hans/README.md">简体中文</a>
</p>

<p align="center">
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Licenza Apache 2.0"></a>
  <a href="#scarica-kodosi"><img src="https://img.shields.io/badge/macOS-26.4%2B%20%C2%B7%20Apple%20silicon-c98566?style=flat-square&labelColor=2a211d" alt="macOS 26.4 o successivo su Apple silicon"></a>
  <a href="#scarica-kodosi"><img src="https://img.shields.io/badge/Linux-x86--64-c98566?style=flat-square&labelColor=2a211d" alt="Linux x86-64"></a>
  <a href="#scarica-kodosi"><img src="https://img.shields.io/badge/Windows-11%2024H2%2B%20%C2%B7%20x64-c98566?style=flat-square&labelColor=2a211d" alt="Windows 11 24H2 o successivo su x64"></a>
  <a href="../../clients/ios/README.md"><img src="https://img.shields.io/badge/iOS-26%2B%20%C2%B7%20iPhone%20and%20iPad-c98566?style=flat-square&labelColor=2a211d" alt="iOS 26 o successivo su iPhone e iPad"></a>
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="Ultima release"></a>
</p>

[Kodosi](https://kodosi.com) è un'app di terminale per gli agenti di coding su Mac, Linux e
Windows. Tiene i terminali di tutti i tuoi computer in una finestra, ti mostra quale agente ha
bisogno di te e permette ai tuoi amici di lavorare negli stessi terminali. Terminali e stanze
sono crittografati end-to-end.

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="L'app Kodosi per Mac: Claude Code attende un'approvazione mentre Codex lavora.">
  </picture>
</p>

## Cosa fa Kodosi

**Tutti i terminali in una finestra.** I terminali del tuo Mac, del tuo computer Linux o
Windows e dei computer dei tuoi amici sono in un'unica barra laterale. Ognuno è una shell vera
sul proprio computer, e dalla stessa finestra ne avvii uno nuovo su qualsiasi tuo computer.

**Scopri quale agente ha bisogno di te.** Usa Claude Code, Codex, Copilot CLI o qualsiasi
altro agente da terminale. Ogni terminale mostra quando il suo agente lavora, attende la tua
approvazione, ha finito o non è riuscito, e una notifica ti avvisa mentre Kodosi è in
background.

**Dosi, il tuo agente per i tuoi agenti.** Chiedi a Dosi cosa ha bisogno di te. Legge tutti i
tuoi terminali e le tue stanze, te lo dice e agisce per te: scrive la risposta in un
terminale, avvia agenti e chiude ciò che è finito. La tua conversazione con Dosi ti segue su
ogni computer.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Alla domanda su cosa ha bisogno di te, Dosi legge i terminali, dice che checkout attende un'approvazione e chiede se concederla.">
  </picture>
</p>

**Condividi un terminale dal vivo.** Dai a un amico il pieno controllo di un terminale in
esecuzione e del suo agente.

**Un canvas accanto a ogni terminale.** Un agente ti mostra una pagina, un piano o
un'immagine accanto al suo terminale. Documenti, fogli, presentazioni, PDF e file di testo
sono dal vivo: tu, i tuoi amici e l'agente modificate lo stesso file reale nello stesso
momento. L'agente usa anche un browser che condividi con lui: vedi cosa fa in un video dal
vivo, e clicchi e scrivi nelle stesse pagine.

**Stanze per persone e agenti.** Una stanza contiene terminali condivisi, una conversazione e
attività. Gli agenti leggono e scrivono nella conversazione e prendono in carico attività, e le
issue di GitHub o Gitea diventano attività.

<p align="center"><img src="../media/room.webp" width="900" alt="Una stanza con un terminale condiviso e una conversazione tra persone e agenti."></p>

**Un branch per ogni agente.** Avvia un agente su un nuovo branch, nel suo worktree Git, così
due agenti non modificano mai gli stessi file.

**Sul tuo iPhone e iPad.** Segui, rispondi e controlla i tuoi terminali e le tue stanze quando
sei lontano dal computer.

**Crittografia end-to-end, post-quantistica.** Le tue chiavi restano sui tuoi dispositivi. I
tuoi dispositivi crittografano terminali e stanze end-to-end con ML-KEM-768 insieme a X25519,
così il servizio non può leggerli. [Sicurezza](SECURITY.md)

## Scarica Kodosi

<p>
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/Scarica-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Scarica Kodosi per Mac"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/Scarica-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Scarica Kodosi per Linux"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/Scarica-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Scarica Kodosi per Windows"></a>
</p>

- **Mac** (Apple silicon, macOS 26.4 o successivo): apri l'immagine disco e sposta Kodosi in
  Applicazioni, oppure esegui `brew install --cask johnkozaris/tap/kodosi`. L'app si aggiorna
  da sola.
- **Linux** (x86-64, Ubuntu 24.04 o Debian 13 o successivo): installa il pacchetto Debian.
  Aggiunge il repository apt di Kodosi, così gli aggiornamenti di sistema aggiornano anche
  Kodosi. La [release](https://github.com/johnkozaris/Kodosi/releases/latest) ha anche un
  pacchetto per Arch e un archivio portatile.
- **Windows** (x64, Windows 11 24H2 o successivo): prima di aprire il download la prima volta,
  aggiungi come amministratore il [certificato di Kodosi](https://updates.kodosi.com/windows/Kodosi.cer)
  in Computer locale › Persone attendibili. L'app si aggiorna da sola.
- **iPhone e iPad:** compila Kodosi per il tuo [iPhone o iPad](../../clients/ios/README.md).

Poi segui la guida [Primi passi](GETTING_STARTED.md).

## Per saperne di più

- [Primi passi](GETTING_STARTED.md)
- [Sicurezza](SECURITY.md) e il [design della crittografia](../CRYPTOGRAPHY.md) (in inglese)
- [Architettura](../ARCHITECTURE.md), [sviluppo](../DEVELOPMENT.md) e
  [come contribuire](../../CONTRIBUTING.md) (in inglese)
- [Self-hosting](../SELF-HOSTING.md) (in inglese)

Segnala i bug nelle [issue di GitHub](https://github.com/johnkozaris/Kodosi/issues) e le
vulnerabilità [in privato](SECURITY.md#segnalare-una-vulnerabilità).

Kodosi è creato da [John Kozaris](https://johnkozaris.com).

## Licenza

Kodosi è distribuito con la [licenza Apache 2.0](../../LICENSE). Il nome e il logo di Kodosi
non rientrano in quella licenza; i [marchi](../../TRADEMARKS.md) (in inglese) spiegano come
puoi usarli. Kodosi usa [Ghostty](https://ghostty.org), Rust, Swift, Qt e ASP.NET Core. Le
[note di terze parti](../../terminal/ghostty/THIRD_PARTY_NOTICES.md) e i
[crediti delle immagini](../media/README.md) elencano cosa include.
