# Sicurezza

[English](../SECURITY.md) · [Deutsch](../de/SECURITY.md) · [Ελληνικά](../el/SECURITY.md) · [Español](../es/SECURITY.md) · [Français](../fr/SECURITY.md) · **Italiano** · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

Il traffico dei terminali e i contenuti dei progetti sono crittografati end-to-end dai tuoi
dispositivi, e le chiavi private non lasciano mai i dispositivi senza essere crittografate. Il
backend instrada le connessioni e archivia i dati crittografati dei progetti; non esegue mai comandi
e non ne vede mai il contenuto in chiaro. Per il modello completo delle minacce, i meccanismi
crittografici e i limiti noti, consulta il [design della crittografia](../CRYPTOGRAPHY.md) (in
inglese). Nessuna terza parte indipendente ha ancora sottoposto ad audit questo design o la sua
implementazione.

## Cosa concede la condivisione

Condividere un terminale concede ai destinatari il pieno controllo con i tuoi privilegi di sistema
operativo su quel computer. Condividere un terminale in un progetto lo rende accessibile a tutti i
tuoi amici presenti in quel progetto, compresi quelli che si uniscono in seguito. Poiché un
terminale condiviso gira con il tuo utente, chiunque vi acceda può anche eseguire `kodosi` come te e
leggere o modificare i file su quella macchina. Condividi i terminali solo con persone a cui
affideresti quel computer.

Kodosi non limita ciò che i partecipanti o i loro agenti possono fare dentro un terminale o un
progetto condiviso. Rimuovere un amico revoca immediatamente l'accesso ai tuoi terminali e rimuove
ciascuno di voi dai progetti di proprietà dell'altro. Bloccare un utente ha lo stesso effetto e
impedisce future richieste di amicizia.

Prima che qualcuno di nuovo entri nei tuoi terminali o progetti, Kodosi te lo chiede nell'app del
dispositivo da cui parte la modifica, anche quando la chiede il comando `kodosi`, per esempio per un
agente. Questo protegge da errori e agenti; chi scrive in un terminale che hai condiviso agisce come
te e può aggirare questa domanda.

Ciascuno dei tuoi dispositivi approvati può controllare tutti i tuoi terminali, scegliere con chi
condividerli, avviarne di nuovi sugli altri tuoi computer con Kodosi in esecuzione e inviare
messaggi a Dosi su quei computer (dove Dosi agisce con lo stesso accesso locale di quando lo usi
direttamente). Rimuovere un dispositivo ne revoca l'accesso ai terminali e a Dosi.

### Canvas e browser condiviso

Chiunque abbia accesso a un terminale può visualizzare e modificare i file sul suo canvas e usare il
suo browser condiviso: accedi nel browser condiviso solo ad account che tutti i partecipanti
possono usare. I terminali condivisi non riutilizzano mai gli accessi salvati del browser dei
terminali privati. I file del canvas sono normali file sul computer host.

Le pagine HTML e le viste dei documenti dal vivo girano dentro un frame isolato senza accesso alla
rete, senza accesso ai file e senza archiviazione persistente. Finché un browser condiviso è attivo,
la sua porta di debug e la porta del driver di ogni finestra restano in ascolto su `127.0.0.1` o
`::1`, quindi altri account utente locali sullo stesso computer possono raggiungerle. I siti nel
browser condiviso possono salvare nella cartella del terminale al massimo un download per ogni
azione dell'utente o dell'agente, e accedono agli appunti solo quando copi esplicitamente.

Su Linux, Kodosi esegue il rendering delle pagine solo dove la sandbox di Chromium può funzionare.
Su Ubuntu, Kodosi include un profilo AppArmor che autorizza l'app, il suo processo di rendering e
Kodosi Browser a creare gli user namespace richiesti dalla sandbox di Chromium. Per lo stesso
motivo, su Linux il browser condiviso non viene eseguito come root.

Le app di Kodosi non includono Kodosi Browser. La prima volta che apri un browser condiviso su un
computer, Kodosi lo scarica dalle release di Kodosi su GitHub, e verifica ogni giorno la
disponibilità di aggiornamenti nel feed degli aggiornamenti di Kodosi su `updates.kodosi.com`. Un
download viene accettato solo se il feed degli aggiornamenti reca una firma valida della chiave di
aggiornamento di Kodosi e se l'hash SHA-256 del file corrisponde a quello indicato nel feed.

### Cosa invia Kodosi Browser a Google

Kodosi Browser è Chromium, che contatta da solo questi servizi di Google, senza mai inviare loro i
tuoi cookie:

- Aggiornamenti dei suoi componenti, come l'elenco dei certificati revocati: circa ogni cinque ore
  chiede a `update.googleapis.com` se ci sono nuove versioni e le scarica dai server di download di
  Google (come `edgedl.me.gvt1.com`). Invia l'ID e la versione di ogni componente, la propria
  versione, il sistema operativo e il processore, ma nessun dato di navigazione.
- Navigazione sicura: mantiene elenchi di siti pericolosi ottenuti da `safebrowsing.googleapis.com`,
  inviando solo lo stato dei propri elenchi. Quando l'indirizzo di una pagina corrisponde a un
  elenco, invia i primi 4 byte di un hash dell'indirizzo, mai l'indirizzo stesso. Se il controllo
  anti-phishing che esegue sul tuo computer ritiene che una pagina sia di phishing, invia
  l'indirizzo di quella pagina a `sb-ssl.google.com` per averne conferma.
- Ora di rete: quando non è sicuro dell'ora, interroga `clients2.google.com`, che gli indica se il
  certificato di un sito è scaduto anche se il tuo orologio è sbagliato. La richiesta non contiene
  nulla su di te.

L'opzione **Previsioni dei moduli da Google** resta disattivata finché non la attivi in
**Impostazioni › Browser**. Una volta attivata, Kodosi Browser invia a
`content-autofill.googleapis.com` un hash di ogni modulo che mostra, calcolato dal nome del sito e
dai nomi dei campi del modulo, e Google risponde indicando a cosa serve ciascun campo. Quando invii
un modulo, indica anche che tipo di dati conteneva ciascun campo, mai ciò che hai digitato. Kodosi
Browser non si connette mai a `www.google.com` in previsione di una ricerca, e non chiede mai a
Google, con i tuoi cookie di Google, se la barra degli indirizzi di Chrome stessa può offrire la
Modalità AI.

Anche l'opzione **Cookie tra siti** resta disattivata finché non la attivi in
**Impostazioni › Browser**. Finché è disattivata, un sito mostrato dentro la pagina di un altro
sito, come un video di YouTube o un pulsante di accesso, lì non riceve i propri cookie, quindi non
può riconoscerti nei diversi siti che visiti.

### Lo schermo di un computer

Attivando la condivisione dello schermo nelle impostazioni di Kodosi di un computer, ognuno dei tuoi
dispositivi approvati può vedere e controllare l'intero schermo di quel computer, non solo i suoi
terminali, dal canvas dei suoi terminali. Gli amici non lo vedono mai, nemmeno in un terminale che
condividi con loro, e video, input e testo degli appunti viaggiano crittografati end-to-end come i
terminali stessi. Una parte separata di Kodosi si occupa della cattura e dell'input. Su un Mac ha le
autorizzazioni di registrazione dello schermo e di accessibilità necessarie, così i programmi nei
tuoi terminali non le ottengono. Su Linux il desktop concede la condivisione a Kodosi tramite il
proprio portale di condivisione dello schermo e mostra un suo avviso finché lo schermo è condiviso.
Mentre qualcuno guarda, la barra dei menu o l'area di notifica del computer indica quali dispositivi
stanno guardando; disattivare la condivisione, o premere lì **Interrompi la condivisione**, chiude
ogni visualizzazione.

Su un Mac, inoltre, questa parte di Kodosi chiede alla persona davanti al Mac, in un suo avviso,
prima di servire un account: una volta per ogni account, quando la condivisione viene attivata.
Nessun programma può far comparire questo avviso: può farlo solo il tuo clic sull'interruttore di
Kodosi. L'avviso nomina l'account, e il suo server quando non è quello di Kodosi, così un programma
del Mac che avvia un proprio Kodosi con un altro account non ottiene nulla se la persona lì non
consente quell'account. Solo questa parte di Kodosi può leggere o cambiare le risposte, che restano
su quel Mac, e disattivare la condivisione, o **Interrompi la condivisione**, le ritira tutte.

Mentre un dispositivo controlla lo schermo, il testo copiato sul computer va a quel dispositivo, e
incollare su di esso dà al computer il testo di quel dispositivo. Il testo che un gestore di
password contrassegna come privato resta sul computer. Finché la persona davanti al computer usa il
mouse o la tastiera, l'input di chi guarda attende un momento, e uno schermo bloccato resta
bloccato: chi guarda vede che è bloccato, non ciò che mostra.

## Connessioni dirette

Quando due dei tuoi dispositivi possono raggiungersi direttamente, sulla stessa LAN o tramite
attraversamento NAT, il traffico del terminale viaggia direttamente tra loro con la stessa
crittografia end-to-end anziché passare dal server relay. Entrambi i dispositivi vedono gli
indirizzi IP dell'altro. Quando non possono, il traffico passa via UDP dal server relay con la stessa
crittografia, e nessuno dei due dispositivi vede l'indirizzo IP dell'altro.

Per rilevare il proprio indirizzo pubblico, Kodosi interroga il server Kodosi, che
vede comunque quell'IP. Mentre cerca o mantiene una di queste connessioni (e per il minuto successivo), il
dispositivo tiene aperta una sola porta UDP e accetta connessioni unicamente da
dispositivi che visualizzano uno dei tuoi terminali. I terminali condivisi con altre persone passano sempre dal server relay, quindi gli
altri utenti non vedono mai i tuoi indirizzi IP.

## Cosa vede il servizio

Il backend vede i record di account e dispositivi, l'appartenenza ai progetti, i metadati di progetti
e terminali, il tipo di agente e lo stato segnalato di ciascun terminale (ad esempio al lavoro o in
attesa di approvazione, mai il testo del terminale), se hai già visualizzato quello stato, gli
header di instradamento, le tempistiche e le dimensioni del testo cifrato (tranne per il traffico
che segue un percorso peer-to-peer diretto). Vede inoltre lo stato di attività di ciascun
dispositivo (in Kodosi, in un'altra app desktop o inattivo), così da inviare le notifiche solo a un
dispositivo attivo. Non vede come si riattivano i tuoi computer: i loro indirizzi di rete e le
impostazioni di riattivazione viaggiano solo tra i tuoi dispositivi, crittografati end-to-end.

Le notifiche push mobili transitano tramite APNs di Apple o FCM di Google. Questi provider vedono
gli ID dei terminali in attesa, completati o non riusciti insieme al tipo di agente e allo stato,
mai i nomi o il contenuto dei terminali.

La crittografia non nasconde questi metadati né protegge da interruzioni del server. Rimuovere un
dispositivo ne interrompe l'accesso da quel momento in poi; se un dispositivo viene smarrito o
rubato, genera anche una nuova chiave di recupero per tenerlo fuori. Se qualcuno avvia una nuova
identità sul tuo account mentre hai ancora dispositivi attivi, il backend la posticipa di 24 ore e
avvisa i tuoi dispositivi esistenti così che tu possa annullarla. Le credenziali di GitHub e Gitea
restano sul tuo computer.

L'eliminazione del tuo account rimuove i suoi dispositivi, i dati di recupero, la lista amici, le
condivisioni attive e i progetti di tua proprietà. I messaggi che hai inviato nei progetti di altri
restano senza alcun account associato. Il backend conserva l'emittente e il soggetto dell'accesso
eliminato per 30 giorni al fine di rifiutare i token emessi in precedenza.

## Verificare un download

Ogni release include `SHA256SUMS` (il digest SHA-256 di ciascun artefatto) e `SHA256SUMS.asc`,
firmato con la stessa chiave di Kodosi che firma il repository apt. Posiziona entrambi i file
accanto al file scaricato ed esegui:

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv` (parte di GnuPG) deve confermare una firma valida della chiave
`E8007A36D199B6EE755B1EE9C612C00442E52299`, e `shasum` deve restituire `OK` per il tuo file.

## Segnalare una vulnerabilità

Usa la [segnalazione privata delle vulnerabilità su GitHub](https://github.com/johnkozaris/Kodosi/security/advisories/new)
oppure scrivi a [security@kodosi.com](mailto:security@kodosi.com) indicando commit o versione,
passaggi per riprodurre il problema, impatto e log ripuliti da dati sensibili. Non pubblicare
dettagli sulle vulnerabilità nelle issue pubbliche.
