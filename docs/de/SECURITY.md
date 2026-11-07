# Sicherheit

[English](../SECURITY.md) · **Deutsch** · [Ελληνικά](../el/SECURITY.md) · [Español](../es/SECURITY.md) · [Français](../fr/SECURITY.md) · [Italiano](../it/SECURITY.md) · [日本語](../ja/SECURITY.md) · [한국어](../ko/SECURITY.md) · [简体中文](../zh-Hans/SECURITY.md)

Deine Geräte verschlüsseln den Terminal-Verkehr und die Inhalte der Räume Ende-zu-Ende.
Private Schlüssel bleiben auf den Geräten. Das Backend leitet Verbindungen weiter und
speichert verschlüsselte Raumdaten; es führt nie Befehle aus und sieht nie ihren Klartext.
Das [Verschlüsselungsdesign](../CRYPTOGRAPHY.md) (Englisch) beschreibt das Bedrohungsmodell,
die Mechanismen und die bekannten Grenzen; noch hat es keine unabhängige Stelle geprüft.

## Was Teilen erlaubt

Wer ein Terminal teilt, gibt die volle Kontrolle mit den Betriebssystemrechten des
Host-Benutzers. Alle in einem Raum können die geteilten Terminals des Raums nutzen, auch
Mitglieder, die später eingeladen werden. Wer ein Terminal teilt, teilt sonst nichts. Kodosi
vertraut allen, die ein Terminal oder einen Raum teilen, sowie den Programmen und Agents in
ihren Terminals, und schränkt nicht ein, was sie tun. Jedes deiner freigegebenen Geräte
steuert all deine Terminals und kann neue auf deinen anderen Computern starten, auf denen
Kodosi läuft.

Alle, die ein Terminal öffnen können, sehen auch die Dateien auf seinem Canvas und nutzen
seinen geteilten Browser; melde dich dort also nur an, wo alle von ihnen handeln dürfen. Ein
geteiltes Terminal hat nie deine gespeicherten Anmeldungen. Die Dateien auf einem Canvas sind
normale Dateien, und alle, die das Terminal öffnen können, können sie bearbeiten. Eine Seite
auf dem Canvas und ein Live-Dokument laufen in einem geschlossenen Frame ohne Netzwerk, ohne
Dateien und ohne Speicher, der bleibt. Solange ein geteilter Browser wach ist, lauschen sein
Debug-Port und der Treiber-Port jedes Fensters auf `127.0.0.1`, wo andere Konten dieses
Computers sie erreichen können. Eine Website im geteilten Browser speichert pro Aktion einer
Person oder eines Agents einen Download im Ordner des Terminals, und sie erreicht deine
Zwischenablage nur, wenn du kopierst. Wenn ein Gerät den Browser eines Terminals auf einem
anderen Computer öffnet, lauscht dieser Computer für ein paar Sekunden auf einem TCP-Port,
damit sich die beiden direkt verbinden können. Nur dieses Gerät kommt durch, mit der
Verschlüsselung des Terminals. Die Linux-App zeigt Seiten nur dort, wo Chromium sie in einer
Sandbox ausführen kann; unter Ubuntu erlaubt ihr AppArmor-Profil der App, ihrem Seitenprozess
und Kodosi Browser, die User-Namespaces anzulegen, die die Sandbox braucht. Aus demselben
Grund läuft der geteilte Browser unter Linux nicht als root. Die Apps enthalten Kodosi
Browser nicht. Wenn ein Computer seinen ersten geteilten Browser öffnet, holt sein Host den
Browser aus dem Feed auf `updates.kodosi.com` und sieht dort jeden Tag nach einer neuen
Version. Der Host nutzt einen Download nur, wenn der Feed die Signatur des Schlüssels der
Kodosi-Updates trägt und der Download den SHA-256 hat, den der Feed angibt.

## Was der Dienst sieht

Der Dienst sieht Konto- und Gerätedaten, Raummitgliedschaften, Metadaten von Räumen und
Terminals, den Agent und den Zustand jedes Terminals (etwa arbeitet oder wartet auf eine
Freigabe, nie seinen Text), Routing-Felder, Zeitpunkte und die Größen der Chiffretexte.
Verschlüsselung verbirgt diese Metadaten nicht und verhindert keinen Ausfall. Ein Gerät, das
du entfernst, verliert ab dann den Zugriff. Nachdem du ein verlorenes oder gestohlenes Gerät
entfernt hast, erstelle einen neuen Wiederherstellungsschlüssel, um dieses Gerät draußen zu
halten. Zugangsdaten für GitHub und Gitea bleiben auf deinem Computer.

Wer ein Konto löscht, entfernt seine Geräte, Wiederherstellungsdaten, Freunde, Freigaben und
eigenen Räume; Nachrichten in Räumen anderer bleiben ohne Konto stehen. 30 Tage lang behält
der Dienst Aussteller und Subjekt der gelöschten Anmeldung, um ältere Tokens abzuweisen.

## Eine Schwachstelle melden

Nutze die [vertrauliche Meldung von Schwachstellen](https://github.com/johnkozaris/Kodosi/security/advisories/new)
oder schreib an [security@kodosi.com](mailto:security@kodosi.com), mit der Revision, den
Schritten, den Auswirkungen und geschwärzten Logs. Halte Details aus öffentlichen Issues
heraus.
