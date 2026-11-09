# Sicherheit

[English](../SECURITY.md) · **Deutsch** · [Ελληνικά](../el/SECURITY.md) · [Español](../es/SECURITY.md) · [Français](../fr/SECURITY.md) · [Italiano](../it/SECURITY.md) · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

Deine Geräte verschlüsseln den Terminal-Verkehr und die Inhalte der Räume Ende-zu-Ende.
Private Schlüssel bleiben auf den Geräten. Das Backend leitet Verbindungen weiter und
speichert verschlüsselte Raumdaten; es führt nie Befehle aus und sieht nie ihren Klartext.
Das [Verschlüsselungsdesign](../CRYPTOGRAPHY.md) (Englisch) beschreibt das Bedrohungsmodell,
die Mechanismen und die bekannten Grenzen; noch hat es keine unabhängige Stelle geprüft.

## Was Teilen erlaubt

Wer ein Terminal teilt, gibt die volle Kontrolle mit den Betriebssystemrechten des
Host-Benutzers. Ein Terminal in einem Raum geht an die Freunde seines Eigentümers in diesem
Raum, auch an Freunde, die später beitreten. Wenn du einen Freund entfernst, verliert er
deine Terminals, und jeder von euch verlässt die Räume, die dem anderen gehören. Ein
geteiltes Terminal läuft als du: Die Personen darin können auch `kodosi` als du ausführen
und deine Dateien auf diesem Computer lesen und ändern. Teile es also nur mit Personen,
denen du diesen Computer anvertraust. Kodosi vertraut allen, die ein Terminal oder einen
Raum teilen, sowie den Programmen und Agents in ihren Terminals, und schränkt nicht ein, was
sie tun. Jedes deiner freigegebenen Geräte steuert all deine Terminals und kann neue auf
deinen anderen Computern starten, auf denen Kodosi läuft. Von jedem deiner Geräte aus kannst du Dosi auf deinen Computern schreiben, und Dosi handelt dort genauso, wie wenn du direkt am Computer schreibst. Ein Gerät, das du entfernst, kann Dosi nicht mehr schreiben.

Alle, die ein Terminal öffnen können, sehen auch die Dateien auf seinem Canvas und nutzen
seinen geteilten Browser; melde dich dort also nur an, wo alle von ihnen handeln dürfen. Der
geteilte Browser eines geteilten Terminals nutzt nie deine gespeicherten Anmeldungen. Die
Dateien auf einem Canvas sind normale Dateien, und alle, die das Terminal öffnen können,
können sie bearbeiten. Eine Seite auf dem Canvas und ein Live-Dokument laufen in einem
geschlossenen Frame ohne Netzwerk, ohne Dateien und ohne Speicher, der bleibt. Solange ein
geteilter Browser wach ist, lauschen sein Debug-Port und der Treiber-Port jedes Fensters auf
`127.0.0.1`, wo andere Konten dieses Computers sie erreichen können. Eine Website im
geteilten Browser speichert pro Aktion einer Person oder eines Agents einen Download im
Ordner des Terminals, und sie erreicht deine Zwischenablage nur, wenn du kopierst. Wenn ein
Gerät den Browser eines Terminals auf einem anderen Computer öffnet, lauscht dieser Computer
für ein paar Sekunden auf einem TCP-Port, damit sich die beiden direkt verbinden können. Nur
dieses Gerät kommt durch, mit der Verschlüsselung des Terminals. Die Linux-App zeigt Seiten
nur dort, wo Chromium sie in einer Sandbox ausführen kann; unter Ubuntu erlaubt ihr
AppArmor-Profil der App, ihrem Seitenprozess und Kodosi Browser, die User-Namespaces
anzulegen, die die Sandbox braucht. Aus demselben Grund läuft der geteilte Browser unter
Linux nicht als root. Die Apps enthalten Kodosi Browser nicht. Wenn ein Computer seinen
ersten geteilten Browser öffnet, holt sein Host den Browser aus dem Feed auf
`updates.kodosi.com` und sieht dort jeden Tag nach einer neuen Version. Der Host nutzt einen
Download nur, wenn der Feed die Signatur des Schlüssels der Kodosi-Updates trägt und der
Download den SHA-256 hat, den der Feed angibt.

## Was der Dienst sieht

Der Dienst sieht Konto- und Gerätedaten, Raummitgliedschaften, Metadaten von Räumen und
Terminals, den Agent und den Zustand jedes Terminals (etwa arbeitet oder wartet auf eine
Freigabe, nie seinen Text), Routing-Felder, Zeitpunkte und die Größen der Chiffretexte.
Verschlüsselung verbirgt diese Metadaten nicht und verhindert keinen Ausfall. Ein Gerät, das
du entfernst, verliert ab dann den Zugriff. Nachdem du ein verlorenes oder gestohlenes Gerät
entfernt hast, erstelle einen neuen Wiederherstellungsschlüssel, um dieses Gerät draußen zu
halten. Wenn jemand auf deinem Konto neu beginnt, während du Geräte hast, wartet das einen
Tag, und deine Geräte zeigen es an, damit du es stoppen kannst. Zugangsdaten für GitHub und
Gitea bleiben auf deinem Computer.

Wer ein Konto löscht, entfernt seine Geräte, Wiederherstellungsdaten, Freunde, geteilten
Terminals und eigenen Räume; Nachrichten in Räumen anderer bleiben ohne Konto stehen. 30 Tage lang behält
der Dienst Aussteller und Subjekt der gelöschten Anmeldung, um ältere Tokens abzuweisen.

## Einen Download prüfen

Jedes Release enthält `SHA256SUMS`, den SHA-256-Wert jeder seiner Dateien, und
`SHA256SUMS.asc`, die Signatur dieser Liste mit dem Schlüssel von Kodosi, der auch das
apt-Repository signiert. Leg die beiden Dateien neben deinen Download und führ dann diese
Befehle aus:

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv` aus GnuPG muss eine gültige Signatur des Schlüssels
`E8007A36D199B6EE755B1EE9C612C00442E52299` melden, und `shasum` muss für deinen Download OK melden.

## Eine Schwachstelle melden

Nutze die [vertrauliche Meldung von Schwachstellen](https://github.com/johnkozaris/Kodosi/security/advisories/new)
oder schreib an [security@kodosi.com](mailto:security@kodosi.com), mit der Revision, den
Schritten, den Auswirkungen und geschwärzten Logs. Halte Details aus öffentlichen Issues
heraus.
