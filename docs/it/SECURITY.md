# Sicurezza

[English](../SECURITY.md) · [Deutsch](../de/SECURITY.md) · [Ελληνικά](../el/SECURITY.md) · [Español](../es/SECURITY.md) · [Français](../fr/SECURITY.md) · **Italiano** · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

I tuoi dispositivi crittografano end-to-end il traffico dei terminali e il contenuto delle
stanze. Le chiavi private restano sui dispositivi. Il backend instrada le connessioni e
conserva i dati crittografati delle stanze; non esegue mai comandi e non ne vede mai il testo
in chiaro. Il [design della crittografia](../CRYPTOGRAPHY.md) (in inglese) descrive il modello
delle minacce, i meccanismi e i limiti noti; nessuna parte indipendente lo ha ancora
verificato.

## Cosa concede la condivisione

Condividere un terminale concede il pieno controllo, con i privilegi di sistema operativo
dell'utente dell'host. Tutti in una stanza possono usare i terminali condivisi della stanza,
compresi i membri invitati dopo. Condividere un terminale non condivide nient'altro. Kodosi si
fida di tutti quelli che condividono un terminale o una stanza, e dei programmi e degli agenti
nei loro terminali, e non limita ciò che fanno. Ognuno dei tuoi dispositivi approvati
controlla tutti i tuoi terminali e può avviarne di nuovi sugli altri tuoi computer che
eseguono Kodosi.

Chi può aprire un terminale vede anche i file sul suo canvas e usa il suo browser condiviso,
quindi accedi lì solo dove tutti possono agire. Un terminale condiviso non ha mai i tuoi
accessi salvati. I file su un canvas sono file normali, e chi può aprire il terminale può
modificarli. Una pagina sul canvas e un documento dal vivo girano in un frame chiuso, senza
rete, senza file e senza memoria che resti. Finché un browser condiviso è attivo, la sua porta
di debug e la porta del driver di ogni finestra ascoltano su `127.0.0.1`, dove possono
raggiungerle altri account di quel computer. Un sito nel browser condiviso salva un download
nella cartella del terminale per ogni azione di una persona o di un agent, e arriva ai tuoi
appunti solo quando copi. Quando un dispositivo apre il browser di un terminale su un altro
computer, quel computer ascolta su una porta TCP per pochi secondi, così i due si collegano
direttamente. Passa solo quel dispositivo, con la crittografia del terminale. L'app per Linux
mostra le pagine solo dove Chromium può isolarle in una sandbox; su Ubuntu il suo profilo
AppArmor permette all'app, al suo processo delle pagine e a Kodosi Browser di creare gli user
namespace che servono alla sandbox. Per lo stesso motivo, su Linux il browser condiviso non
gira come root. Le app non contengono Kodosi Browser. Quando un computer apre il suo primo
browser condiviso, il suo host scarica il browser dal feed su `updates.kodosi.com` e lì cerca
ogni giorno una nuova versione. L'host usa un download solo quando il feed ha la firma della
chiave degli aggiornamenti di Kodosi e il download ha lo SHA-256 indicato dal feed.

## Cosa vede il servizio

Il servizio vede i dati di account e dispositivi, i membri delle stanze, i metadati di stanze
e terminali, l'agente e lo stato di ogni terminale (ad esempio al lavoro o in attesa di
un'approvazione, mai il suo testo), i campi di instradamento, i tempi e le dimensioni del
testo cifrato. La crittografia non nasconde questi metadati e non evita un'interruzione del
servizio. Un dispositivo che rimuovi perde l'accesso da quel momento. Dopo aver rimosso un
dispositivo perso o rubato, crea una nuova chiave di recupero per tenerlo fuori. Le
credenziali di GitHub e Gitea restano sul tuo computer.

Eliminare un account rimuove i suoi dispositivi, i dati di recupero, gli amici, le
condivisioni e le stanze di sua proprietà; i messaggi nelle stanze di altri restano, senza
account. Per 30 giorni il servizio conserva l'emittente e il soggetto dell'accesso eliminato
per rifiutare i token precedenti.

## Verificare un download

Ogni release ha `SHA256SUMS`, l'hash SHA-256 di ciascuno dei suoi file, e `SHA256SUMS.asc`,
la firma di questa lista con la chiave di Kodosi che firma anche il repository apt. Metti i
due file accanto al tuo download, poi esegui:

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv`, di GnuPG, deve indicare una firma valida della chiave
`E8007A36D199B6EE755B1EE9C612C00442E52299`, e `shasum` deve mostrare OK per il tuo download.

## Segnalare una vulnerabilità

Usa la [segnalazione privata delle vulnerabilità](https://github.com/johnkozaris/Kodosi/security/advisories/new)
o scrivi a [security@kodosi.com](mailto:security@kodosi.com) con la revisione, i passaggi,
l'impatto e i log senza dati sensibili. Non mettere dettagli nelle issue pubbliche.
