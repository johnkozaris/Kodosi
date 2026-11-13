<p align="center">
  <a href="https://kodosi.com"><img src="../media/banner.webp" width="800" alt="Il logo di Kodosi sopra le parole «terminals, agents and friends. together.» e un cursore di terminale."></a>
</p>

<h3 align="center">Un'app di terminale per te, i tuoi agenti di coding e i tuoi amici.</h3>

<p align="center">
  Scopri quale agente ha bisogno di te su tutti i tuoi computer, guarda cosa crea accanto al suo terminale<br>
  e lavora negli stessi terminali con i tuoi amici. Crittografato end-to-end e open source.
</p>

<p align="center">
  <a href="https://updates.kodosi.com/download/macos"><img src="https://img.shields.io/badge/Scarica-Mac-c98566?style=for-the-badge&labelColor=2a211d" alt="Scarica Kodosi per Mac"></a>
  <a href="https://updates.kodosi.com/download/linux"><img src="https://img.shields.io/badge/Scarica-Linux-c98566?style=for-the-badge&labelColor=2a211d" alt="Scarica Kodosi per Linux"></a>
  <a href="https://updates.kodosi.com/download/windows"><img src="https://img.shields.io/badge/Scarica-Windows-c98566?style=for-the-badge&labelColor=2a211d" alt="Scarica Kodosi per Windows"></a>
  <a href="https://updates.kodosi.com/download/android"><img src="https://img.shields.io/badge/Scarica-Android-c98566?style=for-the-badge&labelColor=2a211d" alt="Scarica Kodosi per Android"></a>
</p>

<p align="center">
  <a href="https://github.com/johnkozaris/Kodosi/releases/latest"><img src="https://img.shields.io/github/v/release/johnkozaris/Kodosi?style=flat-square&labelColor=2a211d&color=c98566&label=release" alt="Ultima release"></a>
  <a href="../../LICENSE"><img src="https://img.shields.io/badge/license-Apache_2.0-c98566?style=flat-square&labelColor=2a211d" alt="Licenza Apache 2.0"></a>
</p>

<p align="center">
  <sub><a href="../../README.md">English</a> · <a href="../de/README.md">Deutsch</a> · <a href="../el/README.md">Ελληνικά</a> · <a href="../es/README.md">Español</a> · <a href="../fr/README.md">Français</a> · <b>Italiano</b> · <a href="../ja/README.md">日本語</a> · <a href="../ko/README.md">한국어</a> · <a href="../zh-Hans/README.md">简体中文</a></sub>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: light)" srcset="../media/app-light.webp">
    <img src="../media/app-dark.webp" width="900" alt="L'app Kodosi per Mac: Claude Code attende un'approvazione mentre Codex lavora.">
  </picture>
</p>

## Cosa fa Kodosi

### Scopri quale agente ha bisogno di te

Claude Code, Codex e altri agenti di coding si fermano e ti aspettano. Kodosi tiene i terminali
di tutti i tuoi computer in una finestra, e ognuno mostra se il suo agente lavora, attende la
tua risposta, ha finito o non è riuscito. Una notifica ti avvisa quando non ci sei, anche sul
telefono. I tuoi agenti girano così come sono, con le loro impostazioni e il loro accesso.

### Chiedi cosa ha bisogno di te

Dosi, un agente per i tuoi agenti, legge i tuoi terminali, ti dice cosa ti sta aspettando e,
quando lo decidi tu, risponde per te nel terminale.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Alla domanda su cosa ha bisogno di te, Dosi legge i terminali, dice che checkout attende un'approvazione e chiede se concederla.">
  </picture>
</p>

### Guarda cosa crea il tuo agente

I suoi piani, le pagine e i documenti si aprono su un canvas accanto al terminale, e tu
modifichi i documenti insieme ai tuoi amici e all'agente.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/canvas-still.webp">
    <img src="../media/canvas.webp" width="900" alt="Il terminale di Claude Code con il suo canvas: un piano di lancio che Leo modifica mentre Claude suggerisce una modifica, poi un foglio del budget, delle slide, un PDF e un browser condiviso dove Claude aggiunge una tazza al carrello.">
  </picture>
</p>

### Lavoraci insieme

Dai a un amico un terminale in esecuzione, oppure riunisci terminali, una conversazione e
attività in un progetto per le persone e i loro agenti.

<p align="center"><img src="../media/project.webp" width="900" alt="Un progetto con un terminale condiviso e una conversazione tra persone e agenti."></p>

### Privato e aperto

Terminali e progetti sono crittografati end-to-end con crittografia post-quantistica, così il
servizio non può leggerli. Kodosi è open source e puoi eseguire il servizio per conto tuo.
[Sicurezza](SECURITY.md) · [Self-hosting](../SELF-HOSTING.md) (in inglese)

## Scarica Kodosi

| | Requisiti | Installazione |
| --- | --- | --- |
| **Mac** | Apple silicon, macOS 26 o successivo | Apri l'immagine disco e sposta Kodosi in Applicazioni, oppure esegui `brew install --cask johnkozaris/tap/kodosi`. L'app si aggiorna da sola. |
| **Linux** | x86-64, Ubuntu 24.04 o Debian 13 o successivo | Installa il pacchetto Debian. Aggiunge il repository apt di Kodosi, così gli aggiornamenti di sistema aggiornano anche Kodosi. La [release](https://github.com/johnkozaris/Kodosi/releases/latest) ha anche un pacchetto per Arch e un archivio portatile. |
| **Windows** | x64, Windows 11 24H2 o successivo | Prima di aprire il download la prima volta, aggiungi come amministratore il [certificato di Kodosi](https://updates.kodosi.com/windows/Kodosi.cer) in Computer locale › Persone attendibili. L'app si aggiorna da sola. |
| **Android** | ARM64, Android 15 o successivo | Scarica l'APK sul dispositivo, aprilo e consenti l'installazione quando Android lo chiede. Una nuova versione si installa allo stesso modo. |
| **iPhone e iPad** | iOS 26 o successivo | Compila Kodosi per il tuo [iPhone o iPad](../../clients/ios/README.md). |

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
