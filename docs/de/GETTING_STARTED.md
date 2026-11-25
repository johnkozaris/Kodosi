# Erste Schritte

[English](../GETTING_STARTED.md) · **Deutsch** · [Ελληνικά](../el/GETTING_STARTED.md) · [Español](../es/GETTING_STARTED.md) · [Français](../fr/GETTING_STARTED.md) · [Italiano](../it/GETTING_STARTED.md) · [日本語](../ja/GETTING_STARTED.md) · [한국어](../ko/GETTING_STARTED.md) · [简体中文](../zh-Hans/GETTING_STARTED.md)

[Lade Kodosi](README.md#kodosi-herunterladen) für deinen Computer herunter. Die Kurzbefehle unten
verwenden macOS-Tasten; unter Linux und Windows ersetzt du ⌘ durch Strg+Umschalt, sofern nichts
anderes angegeben ist.

## 1. Anmelden

Öffne Kodosi und klicke unten in der Seitenleiste auf **Anmelden**. Kodosi öffnet deinen Browser,
damit du dich anmelden kannst. In einem Terminal erhältst du mit `kodosi auth login` einen
Gerätecode, den du in einem beliebigen Browser eingeben kannst.

Lokale Terminals funktionieren auch ohne Anmeldung. Melde dich an, um deine anderen Computer zu
verbinden, Freunde hinzuzufügen und Projekte zu nutzen.

## 2. Einen Agent starten

Drücke ⌘N (oder klicke auf **Neues Terminal**), um den Launcher zu öffnen. Wähle einen Agent und
einen Ordner, bei Bedarf zusätzlich ein Konto, ein Modell und den Denkaufwand, gib einen ersten
Prompt ein und drücke die Eingabetaste. Kodosi tippt den Startbefehl in eine neue Shell.

<p align="center"><img src="../media/launcher.webp" width="580" alt="Der Launcher mit ausgewähltem Claude Code, einem Ordner und einem ersten Prompt."></p>

Ist ein Agent noch nicht installiert, zeigt der Launcher den passenden Installationsbefehl an.

### Terminals schnell öffnen

- Drücke ⌘D (Strg+Umschalt+O unter Linux und Windows) für eine normale Shell im Ordner des
  ausgewählten Terminals.
- Drücke ⌘T (Strg+Umschalt+T), um ein neues Terminal mit demselben Agent, demselben Konto und
  denselben Einstellungen wie im ausgewählten Terminal zu öffnen.
- Klicke in der Seitenleiste auf den `+`-Button neben einem Ordner, um ein weiteres Terminal mit
  dem Agent zu starten, der dort bereits läuft, mit demselben Konto und denselben Einstellungen
  (oder eine normale Shell, falls dort keiner läuft).
- Klicke in einem freien Terminal am Shell-Prompt auf den Aktionen-Button (zwei Regler) in der
  Kopfzeile, um einen beliebigen installierten Agent mit deinen zuletzt genutzten Einstellungen zu
  starten.

### Tastaturkurzbefehle

Die Kurzbefehle entsprechen den Ghostty-Standards und lassen sich unter **Einstellungen ›
Kurzbefehle** anpassen:

- ⌘1 bis ⌘9 (Alt+1 bis Alt+9 unter Linux und Windows): nach Position zu einem Terminal wechseln.
- ⌥⌘ + Pfeiltasten (Strg+Alt + Pfeiltasten): zum benachbarten Terminal wechseln.
- ⌘W: das ausgewählte Terminal schließen.
- ⌘↑ und ⌘↓: in zsh und aktuellem bash zwischen Shell-Prompts springen (nicht in der mit macOS
  gelieferten bash 3.2).

### Berechtigungen, Ausführlichkeit und Schnellmodus

- Standardmäßig starten Agents ohne Einschränkungen, sodass sie Dateien bearbeiten und Befehle
  ausführen können, ohne nachzufragen. Eine Ausnahme ist Antigravity CLI: Es startet im eigenen
  Auto-Modus, der vor nicht umkehrbaren Aktionen weiterhin nachfragt. Soll ein Agent vorher um
  Erlaubnis fragen, wähle **Fragen** unter **Mehr** (nach der Modellauswahl).
- Unter **Mehr** legst du außerdem die Ausführlichkeit der Ausgabe für Claude Code, Codex, Copilot
  CLI und Antigravity CLI fest (standardmäßig **Knapp**). Da Copilot CLI die Ausführlichkeit nur
  aus seinen gespeicherten Einstellungen liest, passt Kodosi die Konfiguration von Copilot
  vorübergehend an, solange Copilot CLI im Modus **Knapp** läuft, und stellt sie danach wieder her.
- Claude Code und Codex bieten einen Schnellmodus (schnellere Antworten zu höheren Kosten), der
  neben der Auswahl für Modell und Denkaufwand umgeschaltet wird. Der Schnellmodus gilt nur für
  diesen einen Start und ist beim nächsten Mal wieder ausgeschaltet.

### Konten und Wechsel im laufenden Betrieb

- Wenn du ein neues Konto hinzufügst, erhält es einen eigenen, getrennten Konfigurationsordner, zum
  Beispiel `~/.claude-work`. Kodosi erkennt auch Konten, die über Shell-Aliase eingerichtet sind,
  etwa `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`.
- Während ein Agent läuft, kannst du über den Aktionen-Button (zwei Regler) in der Kopfzeile ohne
  Neustart das Modell, den Denkaufwand, den Schnellmodus und (bei unterstützten Agents) die
  Berechtigungen wechseln. Kodosi sendet dafür den eigenen Slash-Befehl des Agents, sobald der
  laufende Schritt abgeschlossen ist. Beachte, dass Claude Code Änderungen an Modell, Denkaufwand
  und Schnellmodus auch für künftige Sitzungen speichert.

### Links, Bilder, Worktrees und Designs

Halte ⌘ gedrückt und klicke auf einen Link (Strg-Klick unter Linux und Windows), um ihn im
Standardbrowser des Geräts zu öffnen, das du gerade nutzt; halte zusätzlich die Umschalttaste
gedrückt, wenn das laufende Programm die Maus abfängt. Auf dem iPhone tippst du auf einen Link
(deaktiviert, solange ein Programm die Maus abfängt). Kodosi fragt nach, bevor ein Link an eine
andere App übergeben wird, und blockiert sowohl das Öffnen ausführbarer Dateien als auch lokale
Dateipfade und `localhost`-URLs von entfernten Computern.

Um einem Agent einen Screenshot oder ein Bild zu zeigen, füge es mit ⌘V ein (Strg+Umschalt+V unter
Linux und Windows oder **Einsetzen** auf dem iPhone). Kodosi speichert die Datei auf dem Computer,
auf dem das Terminal läuft, und fügt den Dateipfad ein, sodass der Agent sie anhängen kann; bei
entfernten Terminals wird dabei der Übertragungsfortschritt angezeigt. Die eigene
Einfüge-Tastenkombination eines Agents (etwa Strg+V in Claude Code) liest weiterhin die
Zwischenablage des Computers, auf dem der Agent läuft.

Um einem Agent eine eigene Kopie eines Git-Repositorys zu geben, fahre in der Seitenleiste über den
Repository-Ordner, klicke auf den Branch-Button und gib einen Branch-Namen ein. Kodosi öffnet das
Terminal in einem neuen Git-Worktree und entfernt diesen beim Schließen des Terminals automatisch
wieder, wenn nichts geändert wurde. Wo Worktrees gespeichert werden, legst du unter **Einstellungen
› Terminal** fest.

Jedes Terminal ist hell oder dunkel wie Kodosi auf seinem Host-Computer, und zwar auf allen
Geräten, die es anzeigen. Programme im Terminal erhalten Hinweise zum Designwechsel, sodass sich
Werkzeuge wie Claude Code (mit `/theme` auf Auto) automatisch anpassen. Wenn du dunkle Terminals
bevorzugst, während Kodosi selbst im hellen Modus läuft, aktiviere **Terminals dunkel lassen**
unter **Einstellungen › Terminal**.

### Schließen, Beenden und macOS-Berechtigungen

Jedes Terminal ist eine echte Shell auf seinem Host-Computer. **Minimieren** blendet ein Terminal
aus, lässt es aber weiterlaufen; **Schließen** beendet die Shell samt ihren Kindprozessen. Wenn du
das App-Fenster schließt, laufen Kodosi und deine Terminals in der Menüleiste oder im Infobereich
weiter (außer auf Linux-Desktops ohne Infobereich, wo das Schließen des Fensters Kodosi beendet).
Das Verhalten beim Schließen des Fensters kannst du in den **Einstellungen** ändern.

Wenn du Kodosi beendest, werden deine Terminals gestoppt, und Kodosi speichert ihren
Bildschirminhalt lokal auf deinem Computer (niemals auf dem Server). Beim nächsten Start öffnet
Kodosi deine Terminals wieder in denselben Ordnern, Projekten und an denselben Positionen auf der
Bühne, mit dem bisherigen Bildschirminhalt und Verlauf über einem neuen Prompt (unter Windows musst
du dafür nach oben scrollen). Claude Code, Codex und Copilot CLI setzen ihre Unterhaltungen
automatisch fort und behalten unter macOS und Linux außerdem das gewählte Modell, den Denkaufwand,
den Schnellmodus, die Berechtigungen und die Ausführlichkeit bei. Kann Kodosi nicht erkennen,
welche Unterhaltung ein Agent zuletzt offen hatte, listet der Agent seine Unterhaltungen zur
Auswahl auf.

Unter macOS erben Programme in Kodosi die macOS-Berechtigungen, die du Kodosi erteilt hast, genau
wie in jeder anderen Terminal-App. Blockiert macOS ein Programm beim Ändern einer anderen App,
aktiviere Kodosi unter **App-Verwaltung** (erreichbar über **Einstellungen › Allgemein**). Dort
kannst du auch **Gerätesteuerung und Datenzugriff** (vor macOS 27 **Bedienungshilfen**) sowie
**Aufnahme von Bildschirm & Systemaudio** für Programme erlauben, die Apps automatisieren oder den
Bildschirm erfassen.

## 3. Sehen, welcher Agent dich braucht

Das Symbol jedes Terminals zeigt seinen aktuellen Status an:

| Anzeige | Bedeutung |
| --- | --- |
| Warmes Licht, das das Symbol umkreist | Arbeitet (ein sich füllender Ring zeigt den Fortschritt in Prozent, sofern das Programm ihn meldet). |
| Hand, Fragezeichen oder Schlüssel | Wartet auf deine Freigabe, Antwort oder Anmeldung. |
| Grüner Haken | Fertig. |
| Gelbes Warndreieck | Fehlgeschlagen. |

Haken und Warndreieck verschwinden, sobald du das Terminal auf einem beliebigen Gerät öffnest.

Wenn ein Terminal zu warten beginnt, fertig wird oder fehlschlägt, benachrichtigt dich Kodosi
einmal auf dem Gerät, das du gerade nutzt:

- Ist Kodosi im Fokus, siehst du lediglich die Statusanzeige.
- Arbeitest du am Computer in einer anderen App, zeigt Kodosi dort eine Desktop-Mitteilung an.
- Bist du an keinem deiner Computer aktiv, schickt Kodosi eine Push-Mitteilung an dein Smartphone.
  Wenn ein wartendes Terminal noch ungesehen ist, sobald du deinen Computer verlässt, wird die
  Mitteilung dann an dein Smartphone weitergeleitet.

Nur wartende Terminals spielen einen Benachrichtigungston ab. In den **Einstellungen** kannst du
Mitteilungen pro Gerät stummschalten. Programme melden ihren Status über das
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) oder
standardmäßige Terminal-Fortschrittssequenzen; Programme ohne Statusmeldung zeigen einfach keine
Anzeige.

### Chat-Ansicht

Terminals mit Claude Code sowie über Kodosi gestartete Sitzungen mit Codex oder Copilot CLI bieten
in der Kopfzeile des Terminals zusätzlich eine **Chat**-Ansicht. Der Chat zeigt deine Nachrichten,
die Antworten und Tool-Aktivitäten des Agents sowie offene Rückfragen mit anklickbaren
Antwort-Buttons. Du kannst direkt im Chat antworten oder den Agent stoppen; gibst du während einer
Rückfrage eigenen Text ein, erfährt der Agent, was er stattdessen tun soll. Alles, was du im Chat
machst, passiert zugleich im zugrunde liegenden Terminal.

## 4. Sehen, was ein Agent auf dem Canvas zeigt

Agents können HTML-Seiten, Markdown, Bilder, PDFs, Office-Dokumente, Tabellen, Präsentationen oder
laufende Web-Apps auf einem **Canvas** neben dem Terminal anzeigen und einen geteilten Browser
steuern. Standardmäßig öffnet sich das Canvas automatisch, sobald der Agent etwas anzeigt oder
aktualisiert, während du das Terminal ansiehst (außer du hast es geschlossen). Um es nur selbst zu
öffnen, schalte **Canvas automatisch öffnen** unter **Einstellungen › Allgemein** aus.

Claude Code, Copilot CLI und Antigravity CLI wissen auch dann, wie sie das Canvas nutzen, wenn du
sie manuell startest, und Codex, Cursor und OpenCode wissen es, wenn Kodosi sie startet (OpenCode
unter macOS und Linux). Jedem anderen Agent kannst du sagen, dass er `kodosi skill canvas`
ausführen soll.

Sobald das Canvas Inhalte hat, erscheinen in der Kopfzeile des Terminals Buttons für **Dateien**
und **Browser**, jeweils mit einem kupferfarbenen Punkt bei neuen Inhalten:

- Klicke auf einen der beiden Buttons, um das Canvas neben dem Terminal zu öffnen.
- Klicke auf **Mehr Platz**, damit das Canvas die Bühne einnimmt und das Terminal klein in einer
  Ecke bleibt.
- Schließe einen Bereich über das `×` auf seinem Button und öffne ihn mit `+` wieder.
- Öffne den Browser manuell über **Browser öffnen** im Aktionen-Menü oder füge Dateien über
  **Datei hinzufügen…** (oder per Drag-and-drop auf das Canvas) hinzu, wodurch die Datei auch in
  den Ordner des Terminals kopiert wird.
- Auf dem iPhone befindet sich der Canvas-Button in der Leiste des Terminals, und ein Banner meldet
  neue oder aktualisierte Einträge mit einem **Öffnen**-Button.

### Live-Dokumente und Seiten

- Dokumente (`.docx`), Tabellen (`.xlsx`), Präsentationen (`.pptx`), PDFs und Textdateien sind
  live: Du, alle anderen im Terminal und der Agent könnt sie gleichzeitig bearbeiten, während sie
  normale Dateien auf der Festplatte bleiben. Enthält ein Dokument nachverfolgte Änderungen, bietet
  die Symbolleiste eine Funktion zum Annehmen oder Ablehnen. Unter **Einstellungen › Dokumente**
  kannst du einzelne Ansichten ein- und ausschalten, zwischen Seitenansicht und fließendem Text
  wechseln sowie dunkle Seiten im Dunkelmodus aktivieren.
- HTML-Seiten auf dem Canvas laufen vollständig offline ohne Netzwerkzugriff. Links in Seiten,
  Dokumenten, Tabellen, Folien und PDFs öffnen sich in deinem eigenen Browser nach denselben
  Sicherheitsregeln wie Terminal-Links (⌘-Klick oder Strg-Klick in bearbeitbaren Ansichten,
  normaler Klick sonst, Tippen auf dem iPhone). Interne Links zu Folien oder Zellen springen direkt
  an die jeweilige Stelle.
- Markiere Text auf einer Seite oder in einem Dokument und klicke auf **Dazu fragen**, um Dateiname
  und Auswahl direkt in das Terminal zu übernehmen, damit du deine Frage stellen kannst.
- Klicke mit der rechten Maustaste auf einen Datei-Tab (oder halte ihn auf Mobilgeräten gedrückt),
  um eine Kopie zu speichern, diese oder alle Dateien vom Canvas zu entfernen oder die Datei in den
  Papierkorb des Host-Computers zu verschieben.

### Geteilter Browser

- Klicke und tippe im geteilten Browser wie in einem normalen Browser. Der Agent sieht dieselben
  Seiten und Anmeldungen, und die Ausgabe seines nächsten Browser-Befehls teilt ihm mit, dass du
  den Browser benutzt hast.
- Nach 10 Minuten Inaktivität geht der Browser in den Ruhezustand und öffnet die letzte Seite
  wieder, sobald du ihn wieder verwendest oder der Agent darauf zugreift. Wenn du auf einem
  Computer zum ersten Mal einen geteilten Browser öffnest, lädt Kodosi automatisch den Kodosi
  Browser herunter, deshalb dauert es einen Moment.
- Um deine bestehenden Browser-Anmeldungen in privaten Terminals weiterzuverwenden, klicke unter
  **Einstellungen › Browser** neben dem jeweiligen Browser auf **Kopieren**. Mit **Entfernen**
  löschst du die kopierten Daten wieder, und **Alle entfernen** löscht sämtliche Anmeldungen und
  Websitedaten in privaten Terminals. Wenn macOS den Zugriff auf die Daten eines Browsers
  einschränkt (etwa bei Safari), klicke neben dem Browser auf **Erlauben…**, um
  Festplattenvollzugriff zu gewähren.
- Alle Personen mit Zugriff auf ein Terminal sehen dessen Canvas und teilen sich den Browser. Beim
  Schließen des Terminals werden das Canvas geleert und die Browser-Anmeldungen geteilter Terminals
  verworfen.

## 5. Dosi fragen

Dosi ist dein Agent zum Koordinieren deiner anderen Agents. Klicke unten in der Seitenleiste auf
Dosis Gesicht (oder drücke ⌘J). Dosi läuft über Claude Code oder Codex; den Anbieter sowie Konto,
Modell und Denkaufwand legst du unter **Einstellungen › Dosi** fest.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Auf die Frage, wo du gebraucht wirst, prüft Dosi die Terminals, meldet, dass checkout auf eine Freigabe wartet, und fragt, ob es sie erlauben soll.">
  </picture>
</p>

Frag Dosi, was deine Aufmerksamkeit braucht, oder beauftrage es direkt mit einer Aufgabe. Dosi kann
deine Terminals und Projekte prüfen, Rückfragen beantworten, Terminals starten oder schließen und
Nachrichten weiterleiten. Steht eine Entscheidung an, zeigt Dosi Auswahlbuttons zusammen mit einem
Eingabefeld für eine Antwort in eigenen Worten.

Deine Unterhaltung mit Dosi bleibt auf all deinen Geräten synchron. Auf dem iPhone und iPad wählst
du direkt über dem Eingabefeld den Computer aus, dessen Dosi du schreiben möchtest (standardmäßig
derjenige, der zuletzt geantwortet hat). Nachrichten an einen Computer im Offline-Zustand bleiben
in der Warteschlange, bis er wieder online ist; mit **Nicht senden** kannst du eine wartende
Nachricht abbrechen.

Die Statuszeile neben Dosis Gesicht zeigt, ob Dosi arbeitet, geantwortet hat oder auf eine Antwort
wartet. Ist auf dem Computer noch kein Agent installiert, steht dort **Dosi einrichten**, und ein
Klick öffnet **Einstellungen › Dosi**. Um Dosi auf einem Computer zu deaktivieren, schalte **Dosi
verwenden** unter **Einstellungen › Dosi** aus.

## 6. Weitere Computer und Smartphones hinzufügen

Melde dich auf einem neuen Computer mit demselben Konto an. Er zeigt einen Bestätigungscode an, und
deine bisherigen Geräte bitten dich um Freigabe. Klicke auf einem bereits freigegebenen Computer
auf **Freigeben** und gib den Code ein, oder scanne ihn mit einem freigegebenen Smartphone.

Ein Smartphone oder Tablet fügst du mit einem einzigen Scan hinzu: Öffne auf einem freigegebenen
Computer die **Einstellungen**, klicke unter **Gerät hinzufügen** auf **Code anzeigen** und scanne
den Code mit dem neuen Gerät. Nach der Freigabe erscheinen die Terminals all deiner Computer auf
jedem Gerät.

### Terminals auf anderen Computern starten

Um ein Terminal auf einem anderen Computer zu starten, wähle diesen rechts in der Leiste des
Launchers aus. Das Ordnermenü listet dann die zuletzt genutzten Ordner dieses Rechners auf, und
über **Ordner auswählen…** durchsuchst du sein Home-Verzeichnis. Auf dem iPhone und iPad tippst du
auf `+` unter **Terminals** oder neben einem bestimmten Computer. Auf dem Zielcomputer muss Kodosi
geöffnet sein (oder `kodosi host` in einem Terminal laufen).

Wenn zwei deiner Geräte einander direkt erreichen können, läuft der Terminal- und
Browser-Datenverkehr für geringere Latenz direkt zwischen ihnen (Peer-to-Peer). In einem lokalen
Netzwerk fragen macOS, iOS und Android einmal nach der Berechtigung für das lokale Netzwerk; lehnst
du sie ab, läuft der Verkehr ersatzweise über den Relay-Server.

### Ruhezustand und wach bleiben

Ein Computer geht nicht von selbst in den Ruhezustand, während ein Agent in einem seiner Terminals
arbeitet, und er kann einige Minuten nach dem Ende seiner Agenten wieder in den Ruhezustand gehen.
Ein Agent, der auf deine Antwort wartet, hält ihn nicht wach. Um das zu ändern, wähle auf diesem
Computer unter **Einstellungen › Allgemein** bei **Wach bleiben** eine Option: **Während Agenten
arbeiten**, **Immer** oder **Nie**. **Immer** hält einen Computer nur wach, solange er am Strom
hängt; im Akkubetrieb bleibt er wach, während Agenten arbeiten. Du kannst einen Computer weiterhin
selbst in den Ruhezustand versetzen.

Um einen anderen deiner Computer in den Ruhezustand zu versetzen, öffne die **Einstellungen**,
klicke unter **Deine Geräte** neben ihm auf **⋯** und wähle **Ruhezustand…**. Auf einem Smartphone
tippst du auf den Computer, um seine Seite zu öffnen. Seine Terminals pausieren, bis er wieder
aufwacht. Wenn ihn danach nichts mehr wecken könnte, sagt Kodosi es dir vorher. Im selben Menü oder
auf der Seite wählst du, wann dieser Computer wach bleibt.

### Einen Computer aufwecken

Um einen Computer aus dem Ruhezustand aufzuwecken, klicke in den **Einstellungen** neben ihm (auf
einem Smartphone auf seiner Seite), in einem seiner Terminals oder in der Computerliste eines neuen
Terminals auf **Aufwecken**. Das Wecksignal muss aus dem Netzwerk des schlafenden Computers kommen.
Kodosi sendet es von dem Gerät, das du gerade benutzt, wenn es im selben Netzwerk ist, und sonst von
einem anderen deiner Computer dort, der wach ist. Ein iPhone oder iPad bittet immer einen deiner
Computer darum.

Ein Computer wacht über ein Netzwerkkabel auf, nicht über WLAN, und nur, wenn sein System es
erlaubt. Die **Einstellungen** zeigen für jeden Computer, was Kodosi erwartet, oder die eine
Einstellung, die du ändern musst:

- Auf einem Mac: Aktiviere in den Systemeinstellungen **Ruhezustand bei Netzwerkzugriff beenden**.
- Unter Windows: Erlaube im Geräte-Manager, dass der Netzwerkadapter den Computer aktiviert.
- Unter Linux: Aktiviere Wake-on-LAN für die Kabelverbindung.

Ein Mac sendet das Wecksignal nur, wenn Kodosi das lokale Netzwerk nutzen darf. Erlaube es in den
Systemeinstellungen unter **Datenschutz & Sicherheit**.

Um sicherzugehen, wähle im Menü eines wachen Computers **Aufwecken testen…** (auf einem Smartphone
auf seiner Seite). Kodosi versetzt ihn in den Ruhezustand, weckt ihn auf und merkt sich das
Ergebnis. Bleib beim ersten Test in der Nähe des Computers: Wenn er nicht aufwacht, wecke ihn von
Hand.

### Wiederherstellungsschlüssel

Erstelle in den **Einstellungen** einen Wiederherstellungsschlüssel und bewahre ihn sicher auf.
Falls du jemals alle deine Geräte verlierst, meldest du dich auf einem neuen Gerät an und wählst
**„Meinen Wiederherstellungsschlüssel verwenden“**, um deine Freunde und Projekte zu behalten.
Entferne verlorene Geräte über ein verbliebenes Gerät; wurde ein Gerät gestohlen, erstelle danach
zusätzlich einen neuen Wiederherstellungsschlüssel.

## 7. Mit Freunden arbeiten

Öffne **Freunde** über das Menü neben deinem Konto. Sende deine Einladung oder füge die
Einladung oder den Benutzernamen eines Freundes ein. Über eine Einladung kann Kodosi die Identität
deines Freundes automatisch verifizieren. Auf Mobilgeräten kannst du deine Einladung als QR-Code
anzeigen, den ein Freund mit seiner Kamera oder direkt in Kodosi scannt; das Vertrauen wird dennoch
vor dem Absenden der Anfrage bestätigt.

<p align="center"><img src="../media/friends.webp" width="900" alt="Die Seite Freunde mit zwei verifizierten Freunden."></p>

### Ein Terminal teilen

Öffne das Aktionen-Menü in der Kopfzeile eines Terminals, klicke auf **Teilen** und wähle Freunde
oder Projekte aus. Alle Personen, mit denen du teilst, erhalten die volle Kontrolle mit deinen
Betriebssystemrechten auf diesem Computer. Deine übrigen Terminals bleiben privat. Du kannst von
jedem deiner Geräte aus teilen, auch vom Handy, solange der Computer, auf dem das Terminal läuft,
online ist. Kodosi fragt dich jedes Mal, bevor jemand Neues Zugang bekommt.

### Projekte

Ein Projekt bündelt Terminals aus beliebigen Ordnern und von beliebigen Computern zusammen mit
einem gemeinsamen Chat und einer Aufgabenliste. Klicke auf `+` neben **Projekte**, starte Terminals
direkt im Projekt (oder teile bestehende hinein) und lade Freunde ein, sobald du bereit bist.

<p align="center"><img src="../media/project.webp" width="900" alt="Ein Projekt mit einem geteilten Terminal und einer Unterhaltung zwischen Menschen und Agents."></p>

- Alle Mitglieder können geteilte Terminals nutzen und ihre eigenen teilen. Wer später beitritt,
  sieht sofort die Terminals des Projekts und den gesamten Chatverlauf. Dateien verbleiben auf dem
  jeweiligen Host-Computer des Terminals.
- Erstelle offene Aufgaben, übernimm sie, gib sie wieder frei, schließe sie mit einer kurzen
  Zusammenfassung ab oder öffne sie erneut.
- Verknüpfe GitHub- oder Gitea-Repositorys, um Issues mit Aufgaben zu synchronisieren: Änderungen
  an einer verknüpften Aufgabe aktualisieren direkt das zugehörige Issue. GitHub verwendet deine
  `gh`-Anmeldung oder den Git-Credential-Helper; Gitea nutzt den Git-Credential-Helper oder
  `KODOSI_GITEA_TOKEN` zusammen mit `KODOSI_GITEA_URL`. Zugangsdaten verlassen niemals deinen
  Computer.
- Erwähne ein Terminal im Chat mit `@<terminal-name>`, um deine Nachricht (versehen mit deinem
  Namen und dem Projekt) direkt an den dort laufenden Agent zuzustellen; der Agent entscheidet
  selbst, wie er reagiert. Claude Code, Copilot CLI und Antigravity CLI in Kodosi (sowie Codex,
  Cursor und OpenCode, wenn sie von Kodosi gestartet wurden, wobei OpenCode unter macOS und Linux
  unterstützt wird) wissen automatisch, dass sie in Kodosi laufen. Mit `kodosi skill` gibst du den
  [Skill](../../runtime/skills/kodosi/SKILL.md) für jeden anderen Agent aus. In einem
  Kodosi-Terminal erkennen Projektbefehle das aktuelle Projekt automatisch; außerhalb gibst du
  `--project <name>` an.
- Löschst du eine Nachricht von dir oder deinem Agent, erscheint für alle ein Platzhalter für die
  gelöschte Nachricht. **Chat leeren** blendet den Verlauf bis zu diesem Zeitpunkt auf all deinen
  Geräten aus, lässt ihn für andere Mitglieder aber unverändert.

## 8. Die Befehlszeile nutzen

Die `kodosi`-CLI steuert exakt dieselben Terminals wie die Desktop-App, und Änderungen werden in
der App sofort sichtbar.

| Befehl | Was er tut |
| --- | --- |
| `kodosi terminal start --agent claude --prompt "fix the failing test"` | Startet einen Agent mit deinen zuletzt genutzten Einstellungen. Mit `--account`, `--model`, `--effort`, `--fast`, `--permissions` und `--output` kannst du sie überschreiben. |
| `kodosi terminal start --directory <repository> --branch <name>` | Startet ein Terminal auf einem neuen Branch in einem eigenen Git-Worktree. |
| `kodosi terminal start --computer <computer> --directory <folder>` | Startet ein Terminal auf einem anderen deiner Computer. Relative Pfade und `~` werden dort relativ zu deinem Home-Verzeichnis aufgelöst. Mit `kodosi devices list` siehst du, welche Computer online sind. |
| `kodosi agent list` | Listet installierte Agents, ihre Konten und deine zuletzt genutzten Einstellungen auf. |
| `kodosi agent usage` | Zeigt die aktuelle Tarifnutzung und die Rücksetzzeiten für angemeldete Konten von Claude Code, Codex und Copilot CLI an. |
| `kodosi terminal list` | Listet Terminals mit ihren Namen und IDs auf (beides lässt sich als `<terminal>` angeben). |
| `kodosi terminal read <terminal>` | Gibt den aktuellen Bildschirminhalt aus. Mit `--history` wird auch der Scrollback-Verlauf eingeschlossen. |
| `kodosi terminal message <terminal> "Can you review my branch?"` | Sendet eine Nachricht mit Absender und Herkunft an den Agent in einem Terminal. |
| `kodosi terminal agent <terminal> --model sonnet` | Ändert Modell, Denkaufwand, Schnellmodus oder Berechtigungen eines laufenden Agents. |
| `kodosi terminal input <terminal> --enter "make test"` | Tippt Text in ein Terminal ein. |
| `kodosi terminal keys <terminal> ctrl-c` | Sendet Tastendrücke an ein Terminal. |
| `kodosi terminal wait <terminal> --until done,failed` | Wartet, bis das Terminal einen der angegebenen Zustände erreicht. |
| `kodosi terminal show <terminal>` | Holt das Terminal in der App in den Vordergrund. |
| `kodosi dosi say "What needs me?"` | Sendet eine Nachricht an Dosi. `kodosi dosi show` gibt die Unterhaltung aus und `kodosi dosi stop` bricht eine laufende Antwort ab. |
| `kodosi show <file>` | Öffnet eine Datei auf dem Canvas dieses Terminals, das sich für die Personen öffnet, die das Terminal ansehen. Mit `kodosi show <address>` öffnest du eine URL im geteilten Browser, mit `kodosi show browser` bringst du den Browser so wie er ist nach vorn, mit `kodosi canvas` listest du die Canvas-Einträge auf und mit `kodosi canvas remove <file>` entfernst du einen davon. |
| `kodosi browser open <address>` | Öffnet eine URL im geteilten Browser dieses Terminals (alle anderen agent-browser-Befehle funktionieren ebenfalls). |
| `kodosi skill` | Gibt den Agent-Skill für die Zusammenarbeit in Kodosi aus; `kodosi skill canvas` gibt den Canvas-Skill aus. |
| `kodosi dosi skill` | Gibt den Skill aus, mit dem ein anderer Agent als Dosi auftreten kann. |
| `kodosi devices sleep <computer>` | Versetzt einen anderen deiner Computer in den Ruhezustand. Seine Terminals pausieren, bis er wieder aufwacht. Mit `kodosi devices keep-awake` und `agents`, `always` oder `never` wählst du, wann dieser Computer wach bleibt. |
| `kodosi devices wake <computer>` | Weckt einen deiner Computer aus dem Ruhezustand auf und wartet, bis er online ist. `kodosi devices test-wake <computer>` versetzt einen Computer in den Ruhezustand, weckt ihn auf und merkt sich das Ergebnis. `kodosi devices list` zeigt, was Kodosi von jedem Computer erwartet. |
| `kodosi host` | Führt den Host eigenständig aus, sodass Terminals weiterlaufen, nachdem du die App beendet hast. Mit `kodosi host stop` stoppst du einen im Hintergrund laufenden Host. |

`message`, `input` und `keys` brechen ab, solange du aktiv in diesem Terminal tippst. `message`
bricht außerdem ab, solange der Agent auf eine menschliche Antwort wartet.

## Wenn etwas keine Verbindung bekommt

- Prüfe, ob der Host-Computer des Terminals online ist, Kodosi dort läuft und das Terminal noch
  geteilt ist.
- Ein neu hinzugefügtes Gerät wartet möglicherweise noch auf deine Freigabe, oder ein Freund mit
  geänderter Identität muss unter **Freunde** erneut verifiziert werden.
- Nach einer verlorenen Verbindung zeigt Kodosi wieder den aktuellen Zustand. Zeigt ein entferntes
  Terminal **Offline** an, erreicht dieses Gerät den Server nicht; prüfe deine Netzwerkverbindung.
  Kodosi verbindet sich automatisch wieder, oder klicke auf **Erneut versuchen**, um es sofort
  erneut zu probieren.
- Geht ein Host-Computer offline, verbinden sich seine Terminals automatisch wieder, sobald er
  zurück ist (sofern sie in der Zwischenzeit nicht geschlossen wurden). Um ein Offline-Terminal aus
  deiner Liste zu entfernen, klicke auf sein `×`: **Ausblenden, bis es wieder da ist** blendet es
  vorübergehend aus, und **Endgültig schließen** schließt es dauerhaft, sobald der Computer wieder
  verbunden ist.
- Warnt Kodosi, dass die Zustellung deiner Eingabe unsicher war, prüfe zuerst den Bildschirm des
  Terminals, bevor du den Befehl erneut ausführst.
