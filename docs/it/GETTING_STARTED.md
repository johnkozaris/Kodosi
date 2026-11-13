# Primi passi

[English](../GETTING_STARTED.md) · [Deutsch](../de/GETTING_STARTED.md) · [Ελληνικά](../el/GETTING_STARTED.md) · [Español](../es/GETTING_STARTED.md) · [Français](../fr/GETTING_STARTED.md) · **Italiano** · [日本語](../ja/GETTING_STARTED.md) · [한국어](../ko/GETTING_STARTED.md) · [简体中文](../zh-Hans/GETTING_STARTED.md)

[Scarica Kodosi](README.md#scarica-kodosi) per il tuo computer. Questa guida usa i tasti del
Mac; su Linux e Windows, usa Ctrl+Maiusc con la stessa lettera.

## 1. Accedi

Apri Kodosi e seleziona Accedi in fondo alla barra laterale. Kodosi apre il tuo browser, dove
accedi. In un terminale, `kodosi auth login` mostra un codice da inserire in qualsiasi browser.

Senza un account, Kodosi esegue terminali su questo computer. Accedi per usare gli altri tuoi
computer, gli amici e i progetti.

## 2. Avvia un agente

Seleziona Nuovo terminale (⌘N). Scegli un agente e una cartella e, se vuoi, un account, un
modello e un livello di ragionamento. Scrivi un primo prompt e premi Invio. Kodosi scrive il
comando nella shell del nuovo terminale.

<p align="center"><img src="../media/launcher.webp" width="580" alt="Claude Code scelto nel launcher, con una cartella e un primo prompt."></p>

- Per un terminale semplice, premi ⌘D (Ctrl+Maiusc+O su Linux e Windows). Si apre subito
  nella cartella del terminale selezionato. ⌘T (Ctrl+Maiusc+T) lo apre con l'agente del
  terminale selezionato, con lo stesso account e le stesse scelte.
- I tasti seguono Ghostty. Da ⌘1 a ⌘9 selezioni un terminale (da Alt+1 ad Alt+9 su Linux e
  Windows), ⌥⌘ con una freccia seleziona quello accanto (Ctrl+Alt con una freccia) e ⌘W chiude
  il terminale selezionato. Impostazioni › Scorciatoie elenca ogni tasto e lo cambia.
- Se manca un agente, il launcher mostra il suo comando di installazione.
- Gli agenti partono con Accesso totale: modificano file ed eseguono comandi senza chiederti
  nulla. Perché un agente chieda prima, scegli Chiedi in Altro, dopo il modello. Lì scegli
  anche la verbosità, cioè quanto del loro lavoro mostrano Claude Code, Codex e Copilot CLI;
  partono da Essenziale. Copilot CLI legge la verbosità solo dalle proprie impostazioni,
  quindi Kodosi le cambia finché Copilot CLI gira in Essenziale e poi le ripristina.
- Un nuovo account ha una sua cartella di accesso, come `~/.claude-work`. Kodosi trova anche
  gli account che configura la tua shell, come
  `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`.
- In un terminale fermo al suo prompt, il pulsante delle azioni (due cursori) nella sua
  intestazione elenca gli agenti installati. Selezionane uno per avviarlo con la tua ultima
  scelta.
- Mentre un agente lavora, lo stesso pulsante delle azioni cambia il suo modello e il livello
  di ragionamento e, per alcuni agenti, le autorizzazioni, con i comandi dell'agente stesso,
  senza riavviarlo. Una modifica aspetta la fine del turno dell'agente, e Claude Code mantiene
  modello e livello di ragionamento anche per le sessioni successive.
- In zsh e bash (non nel bash 3.2 di macOS), ⌘↑ e ⌘↓ saltano da un prompt all'altro.
- Fai ⌘-clic su un link per aprirlo (Ctrl-clic su Linux e Windows). In un programma che usa
  il mouse, tieni premuto anche Maiusc. Su iPhone, tocca il link; un tocco non apre un link in
  un programma che usa il mouse. Il link si apre nel tuo browser, sul dispositivo che stai
  usando. Kodosi chiede prima di aprire un link in un'altra app. Non apre un file che può
  eseguire un programma, né un file o un indirizzo `localhost` di un terminale su un altro
  computer.
- Per dare un'immagine a un agente, come uno screenshot, incollala con ⌘V (Ctrl+Maiusc+V su
  Linux e Windows, Incolla su iPhone). Kodosi la salva sul computer del terminale e incolla il
  percorso del file, e l'agente la allega. Un terminale su un altro computer mostra quanta
  parte dell'immagine è arrivata. Il tasto dell'agente per le immagini, come Ctrl+V in Claude
  Code, legge gli appunti del computer su cui gira l'agente.
- Per dare a un agente una sua copia di un repository, passa il puntatore sulla cartella del
  repository nella barra laterale, seleziona il pulsante del branch e dai un nome a un branch.
  Kodosi avvia il terminale in un nuovo worktree Git. Quando il terminale termina, Kodosi
  rimuove il worktree se non è cambiato nulla. Impostazioni › Terminale stabilisce dove vanno
  queste cartelle.

Un terminale è una shell vera sul computer che lo esegue. Riduci a icona nasconde un terminale
e lo lascia in esecuzione; Chiudi lo termina insieme ai suoi programmi. Quando chiudi la
finestra, Kodosi e i tuoi terminali continuano a funzionare, e l'icona di Kodosi resta nella
barra dei menu o nell'area di notifica; su un desktop senza area di notifica, chiudere la
finestra chiude Kodosi. Quando esci da Kodosi, i tuoi terminali si fermano. Kodosi salva ciò
che mostrano sul tuo computer e da nessun'altra parte. Quando riapri Kodosi, ripartono nelle
loro cartelle, progetti e posizioni sulla scena, con lo schermo e la cronologia precedenti sopra
un nuovo prompt (su Windows, scorri verso l'alto per vederli). Claude Code, Codex e Copilot CLI
riprendono le loro conversazioni e, su un Mac o su Linux, mantengono il modello, il livello di
ragionamento, le autorizzazioni e l'output che hai scelto. Quando Kodosi non riesce a capire
quale conversazione mostrava un agente, l'agente mostra le sue conversazioni così puoi
riprenderne una. Nelle Impostazioni scegli cosa fa la chiusura della finestra.

Su un Mac, i programmi nei tuoi terminali hanno le autorizzazioni che dai a Kodosi, come in
qualsiasi app di terminale. Quando macOS dice di aver impedito a Kodosi di modificare le
app, un programma in un terminale ha provato a cambiare un'altra app. Per consentirlo,
attiva Kodosi in Gestione app; Impostazioni › Generale la apre. Lì consenti anche il
Controllo del dispositivo e accesso ai dati (Accessibilità prima di macOS 27) e la
Registrazione schermo e audio di sistema, per i programmi che controllano le app o vedono lo
schermo.

## 3. Scopri quale agente ha bisogno di te

L'icona di ogni terminale mostra cosa fa il suo programma.

| Vedi | Significa |
| --- | --- |
| Una luce calda che gira attorno all'icona | Al lavoro. Il bordo si riempie per mostrare una percentuale. |
| Una mano, un punto di domanda o una chiave | In attesa della tua approvazione, risposta o accesso. |
| Una spunta verde | Finito. |
| Un segnale di avviso giallo | Non riuscito. |

La spunta e l'avviso spariscono quando apri il terminale. Mentre Kodosi è in background, una
notifica ti avvisa quando uno dei tuoi terminali inizia ad attendere, finisce o non riesce. I
programmi comunicano il loro stato con il
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) o con
una barra di avanzamento. Un programma che non comunica nulla non mostra uno stato.

## 4. Guarda cosa mostra un agente

Un agente può mostrarti una pagina, Markdown, un'immagine, un PDF, un documento, un foglio, una
presentazione o l'app che esegue sul canvas del suo terminale, e usare un browser che condividi
con lui. Il canvas si apre con ciò che l'agente mostra mentre guardi il suo terminale, e quando
cambia un file del canvas o il browser, a meno che tu non l'abbia chiuso. Per aprirlo solo tu,
disattiva Apri il canvas automaticamente in Impostazioni › Generale. Claude Code e Copilot CLI
sanno come fare, anche quando li avvii tu, e così un Codex avviato da Kodosi; a un altro
agente, di' di leggere `kodosi skill canvas`. Quando il canvas ha qualcosa, l'intestazione del
terminale mostra un pulsante per i suoi file e uno per il suo browser, con un punto color rame
per le novità. Selezionane uno per aprire il canvas accanto al terminale; Più spazio dà la
scena al canvas e tiene il terminale piccolo nel suo angolo. Chiudi una parte con la × sul suo
pulsante e riaprila con +. Apri browser, nel pulsante delle azioni, apre il browser prima che
l'agente lo usi, e Aggiungi un file… mette un file sul canvas e nella cartella del terminale;
puoi anche trascinare un file sul canvas. Su iPhone, il pulsante del canvas è nella barra del
terminale, e un banner indica cosa l'agente mostra o cambia, con Apri.

- Un documento, un foglio, una presentazione, un PDF o un file di testo è dal vivo: tu, le
  persone nel terminale e l'agente lo modificate nello stesso momento, e resta un file
  normale nella sua cartella. Quando un documento ha revisioni, la sua barra ha un pulsante
  che le accetta o le rifiuta. Impostazioni › Documenti attiva o disattiva ogni
  visualizzatore, mostra i documenti come pagine o come testo che si adatta al canvas, e
  scurisce le pagine quando Kodosi è scuro.
- Una pagina non ha rete. I link in pagine, documenti, fogli, presentazioni e PDF si aprono nel
  tuo browser con le stesse regole dei link del terminale. Dove puoi modificare, fai ⌘-clic su
  un link (Ctrl-clic su Linux e Windows); altrove basta un clic, e su iPhone un tocco. Un link
  a una slide o a una cella dello stesso file porta lì.
- Seleziona delle parole in una pagina o in un documento, poi scegli «Fai una domanda su questo». Kodosi scrive
  il file e le parole nel terminale, e tu scrivi la domanda.
- Clicca e scrivi nel browser condiviso come nel tuo. L'agente vede le stesse pagine e gli
  stessi accessi, e il suo prossimo comando gli dice che hai usato il browser. Dopo 10 minuti
  senza uso il browser va in sospensione, e riapre la sua ultima pagina quando lo guardi o
  quando l'agente lo usa. Il primo browser su un computer scarica Kodosi Browser, quindi ci
  mette un momento.
- Per usare i tuoi accessi nei tuoi terminali privati, seleziona Copia accanto a un browser
  in Impostazioni › Browser. Rimuovi toglie ciò che hai copiato, e Rimuovi tutto rimuove
  ogni accesso e tutti i dati dei siti dei tuoi terminali privati. Su un Mac, quando macOS
  protegge da Kodosi gli accessi di un browser, come quelli di Safari, seleziona Consenti…
  accanto al browser per dare a Kodosi l'Accesso completo al disco.
- Sulla stessa rete, l'immagine del browser passa direttamente tra i tuoi dispositivi. macOS e
  iOS chiedono una volta l'accesso alla rete locale; senza, l'immagine passa dal servizio.
- Tutte le persone che possono aprire il terminale vedono il suo canvas e usano il suo
  browser. Quando il terminale si chiude, il suo canvas e gli accessi del browser spariscono.

## 5. Chiedi a Dosi

Dosi è il tuo agente per gli altri tuoi agenti. Seleziona la faccia di Dosi in fondo alla
barra laterale (⌘J). Dosi funziona con Claude Code o Codex; Impostazioni › Dosi sceglie quale,
con il suo account, il modello e il livello di ragionamento.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Alla domanda su cosa ha bisogno di te, Dosi legge i terminali, dice che checkout attende un'approvazione e chiede se concederla.">
  </picture>
</p>

Chiedi a Dosi cosa ha bisogno di te, o chiedigli di fare qualcosa. Dosi legge i tuoi terminali
e i tuoi progetti, risponde, e scrive, avvia e chiude cose per te. Quando serve una scelta,
chiede con dei pulsanti, e puoi anche rispondere con parole tue. La tua conversazione con Dosi
è la stessa su tutti i tuoi dispositivi. Su iPhone o iPad, la scheda Dosi scrive a Dosi sul
computer che scegli sopra il messaggio, all'inizio quello che ha risposto per ultimo. Un
messaggio per un computer offline aspetta finché non esegue di nuovo Kodosi, e Non inviare
lo ritira.

La riga accanto alla faccia di Dosi ti dice quando Dosi lavora, quando ha risposto e quando ha
una domanda. Su un computer senza agenti, mostra Configura Dosi e apre Impostazioni › Dosi. Per
rimuovere Dosi da un computer, disattiva Usa Dosi in Impostazioni › Dosi.

## 6. Aggiungi gli altri tuoi computer

Accedi sul nuovo computer con lo stesso account. Mostra un codice, e i computer che usi già ti
chiedono di approvarlo. Scegli Approva su uno di loro e inserisci il codice. Un iPhone o un
iPad si aggiunge allo stesso modo. Poi ogni computer mostra i terminali di tutti i tuoi
computer.

Per avviare un terminale su un altro dei tuoi computer, scegli quel computer all'estremità
destra della banda del launcher. Il pulsante della cartella elenca allora le sue cartelle
recenti, e Scegli una cartella… apre le cartelle della sua home. Su un iPhone o un iPad,
seleziona + in Terminali, o il + accanto a uno dei tuoi computer. Su quel computer deve essere
aperto Kodosi, o deve girare `kodosi host` in un terminale.

Crea anche lì una chiave di recupero e conservala bene. Se perdi tutti i tuoi dispositivi,
accedi da uno nuovo e scegli «Usa la mia chiave di recupero» per mantenere amici e progetti.
Rimuovi un dispositivo perso da un altro dispositivo. Se un dispositivo è stato rubato, crea
anche una nuova chiave di recupero.

## 7. Lavora con gli amici

Apri Amici dal menu accanto al tuo account. Invia il tuo invito, oppure incolla l'invito o il
nome utente di un amico. Un invito permette a Kodosi di verificare il tuo amico. Su un
telefono, mostra il tuo invito come codice: il tuo amico lo scansiona con la fotocamera o
in Kodosi e conferma di fidarsi di te prima che la richiesta parta.

<p align="center"><img src="../media/friends.webp" width="900" alt="La pagina Amici con due amici verificati."></p>

Per condividere un terminale, seleziona Condividi nel pulsante delle azioni della sua
intestazione e scegli amici o progetti. Le persone con cui condividi ne hanno il pieno
controllo, con i tuoi privilegi su quel computer. Gli altri tuoi terminali restano privati.

Un progetto riunisce terminali da qualsiasi cartella e computer, con una conversazione e
attività. Seleziona + accanto a Progetti, poi avvia un terminale nel progetto o condividine uno
al suo interno, e invita gli amici quando vuoi.

<p align="center"><img src="../media/project.webp" width="900" alt="Un progetto con un terminale condiviso e una conversazione tra persone e agenti."></p>

- Tutti i membri possono usare i terminali condivisi e condividere i propri. Chi entra dopo
  riceve i terminali e tutta la conversazione. I file restano sul computer di ciascun
  terminale.
- Metti a disposizione delle attività, prendile in carico, rilasciale, chiudile con un
  risultato o riaprile.
- Collega repository GitHub o Gitea per trasformare le issue in attività; quando modifichi una
  di queste attività, Kodosi modifica la sua issue. GitHub usa il tuo accesso a `gh` o il
  credential helper di Git, e Gitea il credential helper o `KODOSI_GITEA_TOKEN` per il server
  in `KODOSI_GITEA_URL`. Le tue credenziali restano sul tuo computer.
- Scrivi `@` e il nome di un terminale nella chat, e Kodosi dà all'agente di quel terminale il
  tuo messaggio con il tuo nome e il progetto; l'agente decide cosa farne. Claude Code e Copilot
  CLI in un terminale di Kodosi, e un Codex avviato da Kodosi, sanno di girare in Kodosi, e
  `kodosi skill` stampa la [skill](../../runtime/skills/kodosi/SKILL.md) che permette a un
  agente di partecipare. In un terminale di Kodosi, il comando trova il progetto; altrove,
  aggiungi `--project <name>`.

## 8. Usa la riga di comando

Il comando `kodosi` lavora sugli stessi terminali dell'app, e l'app mostra cosa fa.

| Comando | Cosa fa |
| --- | --- |
| `kodosi terminal start --agent claude --prompt "fix the failing test"` | Avvia un agente con la tua ultima scelta. `--account`, `--model`, `--effort`, `--permissions` e `--output` la cambiano. |
| `kodosi terminal start --directory <repository> --branch <name>` | Avvia un terminale su un nuovo branch, nel suo worktree. |
| `kodosi terminal start --computer <computer> --directory <folder>` | Avvia un terminale su un altro dei tuoi computer. Una cartella relativa, o che inizia con `~`, è nella tua home su quel computer. `kodosi devices list` mostra quali computer sono online. |
| `kodosi agent list` | Mostra gli agenti, i loro account e la tua ultima scelta. |
| `kodosi agent usage` | Mostra quanto dei limiti del piano ha usato ogni account connesso di Claude Code, Codex e Copilot CLI. |
| `kodosi terminal list` | Mostra i tuoi terminali con i loro nomi e ID. Indica l'uno o l'altro come `<terminal>`. |
| `kodosi terminal read <terminal>` | Stampa lo schermo. `--history` aggiunge le righe scorse via. |
| `kodosi terminal message <terminal> "Can you review my branch?"` | Dà un messaggio all'agente di un terminale, con chi lo invia e da dove. |
| `kodosi terminal agent <terminal> --model sonnet` | Cambia modello, livello di ragionamento o autorizzazioni dell'agente di un terminale mentre lavora. |
| `kodosi terminal input <terminal> --enter "make test"` | Scrive una riga. |
| `kodosi terminal keys <terminal> ctrl-c` | Preme dei tasti. |
| `kodosi terminal wait <terminal> --until done,failed` | Attende uno stato. |
| `kodosi terminal show <terminal>` | Mostra il terminale nell'app. |
| `kodosi dosi say "What needs me?"` | Chiede a Dosi. `kodosi dosi show` stampa la conversazione, e `kodosi dosi stop` ferma Dosi. |
| `kodosi show <file>` | Mostra un file sul canvas di questo terminale, che si apre per chi lo guarda; `kodosi show <address>` mostra un indirizzo web nel suo browser. `kodosi canvas` elenca cosa mostra, e `kodosi canvas remove <file>` toglie un file. |
| `kodosi browser open <address>` | Apre una pagina nel browser condiviso di questo terminale. Funzionano anche gli altri comandi di agent-browser. |
| `kodosi skill` | Stampa la skill che permette a un agente di lavorare con le persone e gli agenti in Kodosi. `kodosi skill canvas` stampa la skill per il canvas. |
| `kodosi dosi skill` | Stampa la skill che permette a un altro agente di agire come Dosi. |
| `kodosi host` | Mantiene attivi i tuoi terminali dopo che esci dall'app. `kodosi host stop` ferma un host che gira in background. |

`message`, `input` e `keys` si fermano con un messaggio mentre scrivi in quel terminale.
`message` si ferma anche mentre l'agente attende la risposta di una persona.

## Se qualcosa non si connette

Controlla che il computer del terminale sia online, con Kodosi aperto, e che il terminale sia
ancora condiviso. Un nuovo dispositivo può richiedere un'approvazione, e un amico con
un'identità cambiata può richiedere una verifica in Amici. Dopo una connessione persa, Kodosi
mostra di nuovo lo stato attuale. Quando un terminale di un altro computer dice Offline,
questo dispositivo non raggiunge il server: controlla la sua rete. Kodosi si riconnette da
solo, e Riprova si connette subito. Quando Kodosi dice che un computer è offline, i suoi
terminali si riaprono quando torna, a meno che non siano terminati lì. Per togliere un
terminale del genere dalla tua lista, seleziona la sua X. Nascondi finché non torna lo mostra
di nuovo quando il computer torna, e Chiudi per sempre lo chiude in quel momento. Se Kodosi
dice di non essere sicuro che il tuo input sia arrivato, guarda il terminale prima di
riscrivere il comando.
