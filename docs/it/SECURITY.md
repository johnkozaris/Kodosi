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
computer, Kodosi lo scarica da `updates.kodosi.com` e verifica ogni giorno la disponibilità di
aggiornamenti. Un download viene accettato solo se il feed degli aggiornamenti reca una firma valida
della chiave di aggiornamento di Kodosi e se l'hash SHA-256 del file corrisponde a quello indicato
nel feed.

## Connessioni dirette

Quando due dei tuoi dispositivi possono raggiungersi direttamente, sulla stessa LAN o tramite
attraversamento NAT, il traffico del terminale viaggia direttamente tra loro con la stessa
crittografia end-to-end anziché passare dal server relay. Entrambi i dispositivi vedono gli
indirizzi IP dell'altro.

Per rilevare il proprio indirizzo pubblico, Kodosi interroga un server STUN di Cloudflare, che
osserva quell'IP. Mentre cerca o mantiene una connessione diretta (e per il minuto successivo), il
dispositivo tiene aperta una sola porta UDP e accetta connessioni unicamente dai tuoi dispositivi
approvati. I terminali condivisi con altre persone passano sempre dal server relay, quindi gli
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
