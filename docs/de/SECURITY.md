# Sicherheit

[English](../SECURITY.md) · **Deutsch** · [Ελληνικά](../el/SECURITY.md) · [Español](../es/SECURITY.md) · [Français](../fr/SECURITY.md) · [Italiano](../it/SECURITY.md) · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

Terminal-Datenverkehr und Projektinhalte werden von deinen Geräten Ende-zu-Ende-verschlüsselt, und
private Schlüssel verlassen deine Geräte nie unverschlüsselt. Das Backend vermittelt Verbindungen
und speichert verschlüsselte Projektdaten; es führt niemals Befehle aus und sieht nie ihren
Klartext. Das vollständige Bedrohungsmodell, die kryptografischen Verfahren und bekannte
Einschränkungen beschreibt das [Verschlüsselungsdesign](../CRYPTOGRAPHY.md) (Englisch). Weder das
Design noch die Implementierung wurden bisher von einer unabhängigen Stelle geprüft.

## Was Teilen erlaubt

Wenn du ein Terminal teilst, erhalten die Empfänger die volle Kontrolle mit deinen
Betriebssystemrechten auf diesem Computer. Teilst du ein Terminal in ein Projekt, können alle deine
Freunde in diesem Projekt darauf zugreifen, auch diejenigen, die später beitreten. Da ein geteiltes
Terminal unter deinem Konto läuft, kann jede Person darin auch `kodosi` als du ausführen sowie
Dateien auf diesem Computer lesen und ändern. Teile Terminals daher nur mit Personen, denen du
diesen Computer anvertraust.

Kodosi schränkt nicht ein, was Beteiligte oder ihre Agents in einem geteilten Terminal oder Projekt
tun können. Wenn du einen Freund entfernst, wird der Zugriff auf deine Terminals sofort entzogen,
und ihr werdet gegenseitig aus den Projekten des jeweils anderen entfernt. Das Blockieren einer
Person hat dieselbe Wirkung und verhindert künftige Freundschaftsanfragen.

Bevor jemand Neues Zugang zu deinen Terminals oder Projekten bekommt, fragt Kodosi dich in der App
des Geräts, auf dem die Änderung beginnt, auch wenn der Befehl `kodosi` sie anfordert, zum Beispiel
für einen Agent. Das schützt vor Versehen und vor Agents; wer in einem Terminal tippt, das du
geteilt hast, handelt als du und kann diese Frage umgehen.

Jedes deiner freigegebenen Geräte kann alle deine Terminals steuern, bestimmen, mit wem sie geteilt
sind, neue Terminals auf deinen anderen Computern mit laufendem Kodosi starten und Dosi auf diesen
Computern schreiben (wo Dosi mit demselben lokalen Zugriff handelt wie bei direkter Nutzung am
Computer). Das Entfernen eines Geräts entzieht ihm den Zugriff auf deine Terminals und auf Dosi.

### Canvas und geteilter Browser

Alle Personen mit Zugriff auf ein Terminal können Dateien auf dessen Canvas ansehen und bearbeiten
sowie den geteilten Browser nutzen. Melde dich in einem geteilten Browser daher nur bei Konten an,
die alle Beteiligten verwenden dürfen. Geteilte Terminals greifen niemals auf deine gespeicherten
Browser-Anmeldungen aus privaten Terminals zurück. Canvas-Dateien sind reguläre Dateien auf dem
Host-Computer.

HTML-Seiten und Live-Dokumentansichten laufen in einem isolierten Frame ohne Netzwerkzugriff, ohne
Dateizugriff und ohne dauerhaften Speicher. Solange ein geteilter Browser aktiv ist, binden sein
Debug-Port und der Treiber-Port jedes Fensters an `127.0.0.1` oder `::1`, sodass andere lokale
Betriebssystemkonten auf demselben Computer sie erreichen können. Websites im geteilten Browser
können pro Aktion einer Person oder eines Agents höchstens einen Download im Ordner des Terminals
speichern und erhalten nur dann Zugriff auf deine Zwischenablage, wenn du ausdrücklich kopierst.

Unter Linux rendert Kodosi Seiten nur dort, wo die Chromium-Sandbox laufen kann. Für Ubuntu liefert
Kodosi ein AppArmor-Profil mit, das der App, ihrem Renderer-Prozess und dem Kodosi Browser erlaubt,
die von der Chromium-Sandbox benötigten User-Namespaces anzulegen. Aus demselben Grund läuft der
geteilte Browser unter Linux nicht als root.

Die Kodosi-Apps enthalten den Kodosi Browser nicht im Paket. Wenn du auf einem Computer zum ersten
Mal einen geteilten Browser öffnest, lädt Kodosi ihn aus den Releases von Kodosi auf GitHub herunter
und prüft täglich im Update-Feed von Kodosi auf `updates.kodosi.com`, ob es Updates gibt. Downloads
werden nur akzeptiert, wenn der Update-Feed eine gültige Signatur des Kodosi-Updateschlüssels trägt
und der SHA-256-Wert der Datei mit dem Feed übereinstimmt.

### Was der Kodosi Browser an Google sendet

Der Kodosi Browser ist Chromium und ruft von sich aus diese Dienste von Google auf, aber keinen
davon mit deinen Cookies:

- Updates seiner Komponenten, etwa der Liste widerrufener Zertifikate: Ungefähr alle fünf Stunden
  fragt er bei `update.googleapis.com` nach neuen Versionen und lädt sie von den Download-Servern
  von Google herunter (zum Beispiel `edgedl.me.gvt1.com`). Er sendet die ID und Version jeder
  Komponente, seine eigene Version sowie Betriebssystem und Prozessor, aber keine Browserdaten.
- Safe Browsing: Er führt Listen gefährlicher Websites, die er von `safebrowsing.googleapis.com`
  bezieht, und sendet dabei nur den Stand seiner Listen. Passt die Adresse einer Seite zu einer
  Liste, sendet er die ersten 4 Bytes eines Hashwerts der Adresse, nie die Adresse selbst. Hält die
  Phishing-Prüfung, die er auf deinem Computer ausführt, eine Seite für Phishing, sendet er die
  Adresse dieser Seite zur Bestätigung an `sb-ssl.google.com`.
- Netzwerkzeit: Wenn er sich der Uhrzeit nicht sicher ist, fragt er `clients2.google.com`. Die
  Antwort zeigt ihm, ob das Zertifikat einer Website abgelaufen ist, auch wenn deine Uhr falsch
  geht. Die Anfrage enthält nichts über dich.

**Formularvorhersagen von Google** bleiben ausgeschaltet, bis du sie unter
**Einstellungen › Browser** einschaltest. Dann sendet der Kodosi Browser für jedes Formular, das er
anzeigt, einen Hashwert an `content-autofill.googleapis.com`, gebildet aus dem Namen der Website und
den Namen der Felder des Formulars, und Google antwortet, wofür jedes Feld da ist. Wenn du ein
Formular absendest, teilt er außerdem mit, welche Art von Daten jedes Feld enthielt, nie aber, was
du eingegeben hast. Der Kodosi Browser verbindet sich nie schon vor einer Suche mit `www.google.com`
und fragt Google nie mit deinen Google-Cookies, ob die Adressleiste von Chrome selbst den KI-Modus
anbieten darf.

Auch **Websiteübergreifende Cookies** bleiben ausgeschaltet, bis du sie unter
**Einstellungen › Browser** einschaltest. Solange sie ausgeschaltet sind, erhält eine Website, die
in der Seite einer anderen Website erscheint, etwa ein YouTube-Video oder ein Anmelde-Button, dort
ihre Cookies nicht und kann dich deshalb nicht über die Websites hinweg erkennen, die du besuchst.

## Direkte Verbindungen

Wenn sich zwei deiner eigenen Geräte direkt erreichen können, im selben lokalen Netzwerk oder per
NAT-Traversal, läuft der Terminal-Datenverkehr mit derselben Ende-zu-Ende-Verschlüsselung direkt
zwischen ihnen statt über den Relay-Server. Beide Geräte sehen dabei gegenseitig ihre IP-Adressen.

Um die eigene öffentliche Adresse zu ermitteln, fragt Kodosi einen STUN-Server von Cloudflare ab,
der diese IP-Adresse sieht. Solange ein Gerät eine direkte Verbindung sucht oder hält (sowie eine
Minute danach), hält es einen einzelnen UDP-Port offen und akzeptiert dort ausschließlich
Verbindungen von deinen eigenen freigegebenen Geräten. Terminals, die mit anderen Personen geteilt
sind, laufen immer über den Relay-Server, sodass andere Nutzer deine IP-Adressen nie sehen.

## Was der Dienst sieht

Das Backend sieht Konto- und Gerätedaten, Projektmitgliedschaften, Projekt- und Terminal-Metadaten,
den Agent-Typ und den gemeldeten Status jedes Terminals (etwa ob es arbeitet oder auf eine Freigabe
wartet, niemals jedoch Terminaltext), ob du diesen Status bereits gesehen hast, Routing-Header,
Zeitpunkte sowie die Größen der Chiffretexte (außer bei Datenverkehr über eine direkte
Peer-to-Peer-Verbindung). Außerdem sieht es den Aktivitätsstatus jedes Geräts (in Kodosi, in einer
anderen Desktop-App oder inaktiv), damit Mitteilungen nur an ein aktives Gerät gehen. Es sieht
nicht, wie deine Computer aufwachen: Ihre Netzwerkadressen und Aufweck-Einstellungen gehen nur
Ende-zu-Ende-verschlüsselt zwischen deinen eigenen Geräten hin und her.

Mobile Push-Mitteilungen laufen über APNs von Apple oder FCM von Google. Diese Anbieter sehen die
IDs von wartenden, fertigen oder fehlgeschlagenen Terminals sowie deren Agent-Typ und Status,
niemals aber Terminalnamen oder Inhalte.

Die Verschlüsselung verbirgt diese Metadaten nicht und schützt nicht vor Serverausfällen. Das
Entfernen eines Geräts beendet dessen Zugriff ab diesem Zeitpunkt; geht ein Gerät verloren oder
wird es gestohlen, erstelle zusätzlich einen neuen Wiederherstellungsschlüssel, um dieses Gerät
auszusperren. Wenn jemand auf deinem Konto eine neue Identität anlegt, während du noch aktive
Geräte hast, verzögert das Backend diesen Vorgang um 24 Stunden und benachrichtigt deine
bestehenden Geräte, damit du ihn abbrechen kannst. GitHub- und Gitea-Zugangsdaten bleiben auf
deinem Computer.

Wenn du dein Konto löschst, werden deine Geräte, Wiederherstellungsdaten, Freundesliste, aktiven
Freigaben und eigenen Projekte entfernt. Nachrichten, die du in Projekten anderer geschrieben hast,
bleiben ohne zugeordnetes Konto erhalten. Das Backend speichert Aussteller und Subjekt der
gelöschten Anmeldung 30 Tage lang, um zuvor ausgestellte Tokens abzuweisen.

## Einen Download prüfen

Jedes Release enthält `SHA256SUMS` (die SHA-256-Prüfsumme jeder Datei) und `SHA256SUMS.asc`,
signiert mit demselben Kodosi-Schlüssel, der auch das apt-Repository signiert. Lege beide Dateien
neben deinen Download und führe Folgendes aus:

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv` (aus GnuPG) muss eine gültige Signatur des Schlüssels
`E8007A36D199B6EE755B1EE9C612C00442E52299` melden, und `shasum` muss für deine Datei `OK` ausgeben.

## Eine Schwachstelle melden

Nutze die [vertrauliche Meldung von Schwachstellen auf GitHub](https://github.com/johnkozaris/Kodosi/security/advisories/new)
oder sende eine E-Mail an [security@kodosi.com](mailto:security@kodosi.com) mit Commit oder
Version, Schritten zur Reproduktion, Auswirkungen und geschwärzten Logs. Bitte veröffentliche keine
Details zu Schwachstellen in öffentlichen Issues.
