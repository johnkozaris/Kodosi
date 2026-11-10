# Erste Schritte

[English](../GETTING_STARTED.md) · **Deutsch** · [Ελληνικά](../el/GETTING_STARTED.md) · [Español](../es/GETTING_STARTED.md) · [Français](../fr/GETTING_STARTED.md) · [Italiano](../it/GETTING_STARTED.md) · [日本語](../ja/GETTING_STARTED.md) · [한국어](../ko/GETTING_STARTED.md) · [简体中文](../zh-Hans/GETTING_STARTED.md)

[Lade Kodosi](README.md#kodosi-herunterladen) für deinen Computer herunter. Diese Anleitung
nennt die Tasten des Macs; unter Linux und Windows nimmst du Strg+Umschalt mit demselben
Buchstaben.

## 1. Anmelden

Öffne Kodosi und wähle unten in der Seitenleiste Anmelden. Kodosi öffnet deinen Browser, in
dem du dich anmeldest. In einem Terminal zeigt `kodosi auth login` einen Code, den du in
einem beliebigen Browser eingibst.

Ohne Konto führt Kodosi Terminals auf diesem Computer aus. Melde dich an, um deine anderen
Computer, Freunde und Räume zu nutzen.

## 2. Einen Agent starten

Wähle Neues Terminal (⌘N). Wähle einen Agent und einen Ordner und, wenn du willst, ein Konto,
ein Modell und einen Denkaufwand. Tippe einen ersten Prompt und drücke Return. Kodosi tippt den
Befehl in die Shell des neuen Terminals.

<p align="center"><img src="../media/launcher.webp" width="580" alt="Claude Code im Launcher ausgewählt, mit einem Ordner und einem ersten Prompt."></p>

- Für ein einfaches Terminal drückst du ⌘T (Strg+Umschalt+T unter Linux und Windows). Es
  öffnet sich sofort im Ordner des ausgewählten Terminals.
- Die Tasten folgen Ghostty. ⌘1 bis ⌘9 wählen ein Terminal aus (Alt+1 bis Alt+9 unter Linux
  und Windows), ⌥⌘ mit einem Pfeil wählt das daneben (Strg+Alt mit einem Pfeil), und ⌘W
  schließt das ausgewählte Terminal. Einstellungen › Kurzbefehle listet jede Taste auf und
  ändert sie.
- Fehlt ein Agent, zeigt der Launcher seinen Installationsbefehl.
- Agents starten mit Vollzugriff: Sie ändern Dateien und führen Befehle aus, ohne dich zu
  fragen. Soll ein Agent zuerst fragen, wähle Fragen am Button nach dem Modell. Dort wählst
  du auch, wie viel ihrer Arbeit Claude Code und Codex zeigen; sie beginnen bei Knapp.
- Ein neues Konto bekommt seinen eigenen Anmeldeordner, zum Beispiel `~/.claude-work`.
  Kodosi findet auch die Konten, die deine Shell einrichtet, etwa
  `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`.
- Steht ein Terminal an seinem Prompt, listet der Aktionen-Button (zwei Regler) in seiner
  Kopfzeile die installierten Agents. Wähle einen, um ihn mit deiner letzten Wahl zu starten.
- In zsh und bash (nicht in der bash 3.2 von macOS) springen ⌘↑ und ⌘↓ zwischen den Prompts.
- Klicke mit ⌘ auf einen Link, um ihn zu öffnen (Strg-Klick unter Linux und Windows). In
  einem Programm, das die Maus nutzt, hältst du zusätzlich Umschalt. Auf dem iPhone tippst du
  auf den Link; in einem Programm, das die Maus nutzt, öffnet ein Tippen keinen Link. Der
  Link öffnet sich in deinem eigenen Browser auf dem Gerät, das du gerade nutzt. Kodosi
  fragt, bevor es einen Link in einer anderen App öffnet. Es öffnet keine Datei, die ein
  Programm ausführen kann, und keine Datei oder `localhost`-Adresse eines Terminals auf
  einem anderen Computer.
- Um einem Agent ein Bild zu geben, etwa einen Screenshot, fügst du es mit ⌘V ein
  (Strg+Umschalt+V unter Linux und Windows, Einsetzen auf dem iPhone). Kodosi speichert es
  auf dem Computer des Terminals und fügt den Pfad der Datei ein, und der Agent hängt sie an.
  Ein Terminal auf einem anderen Computer zeigt, wie viel vom Bild schon angekommen ist. Die
  eigene Bildertaste des Agents, etwa Strg+V in Claude Code, liest die Zwischenablage des
  Computers, auf dem der Agent läuft.
- Um einem Agent seine eigene Kopie eines Repositorys zu geben, fährst du in der
  Seitenleiste über den Ordner des Repositorys, wählst den Branch-Button und gibst einem
  Branch einen Namen. Kodosi startet das Terminal in einem neuen Git-Worktree. Wenn das
  Terminal endet, entfernt Kodosi den Worktree, sofern sich nichts geändert hat.
  Einstellungen › Terminal legt fest, wo diese Ordner landen.

Ein Terminal ist eine echte Shell auf dem Computer, der es ausführt. Minimieren blendet ein
Terminal aus und lässt es weiterlaufen; Schließen beendet es und seine Programme. Wenn du das
Fenster schließt, laufen Kodosi und deine Terminals weiter, und das Kodosi-Symbol bleibt in
der Menüleiste oder im Infobereich; auf einem Desktop ohne Infobereich beendet das Schließen
des Fensters Kodosi. Wenn du Kodosi beendest, stoppen deine Terminals. Kodosi speichert, was
sie zeigen, auf deinem Computer und sonst nirgends. Wenn du Kodosi wieder öffnest, starten sie
erneut in ihren Ordnern, Räumen und Plätzen auf der Bühne, mit ihrem früheren Bildschirm und
Verlauf über einem neuen Prompt (unter Windows scrollst du nach oben, um ihn zu sehen). Claude
Code, Codex und Copilot CLI setzen ihre Unterhaltungen fort, und auf einem Mac oder unter Linux
behalten sie das Modell, den Denkaufwand, die Berechtigungen und die Ausgabe, die du gewählt
hast. Wenn Kodosi nicht erkennen kann, welche Unterhaltung ein Agent zeigte, listet der Agent
seine Unterhaltungen auf, damit du eine fortsetzen kannst. In den Einstellungen wählst du, was
das Schließen des Fensters tut.

Auf einem Mac haben die Programme in deinen Terminals die Berechtigungen, die du Kodosi
gibst, wie in jeder Terminal-App. Wenn macOS meldet, dass Kodosi daran gehindert wurde, Apps
zu ändern, wollte ein Programm in einem Terminal eine andere App ändern. Um das zu erlauben,
schalte Kodosi in der App-Verwaltung ein; Einstellungen › Allgemein öffnet sie. Dort
erlaubst du auch Gerätesteuerung und Datenzugriff (vor macOS 27 Bedienungshilfen) und
Aufnahme von Bildschirm & Systemaudio, für Programme, die Apps steuern oder den Bildschirm
sehen.

## 3. Sehen, welcher Agent dich braucht

Die Markierung jedes Terminals zeigt, was sein Programm tut.

| Du siehst | Es bedeutet |
| --- | --- |
| Ein warmes Licht, das um die Markierung läuft | Arbeitet. Der Rand füllt sich, um einen Prozentwert zu zeigen. |
| Eine Hand, ein Fragezeichen oder einen Schlüssel | Wartet auf deine Freigabe, Antwort oder Anmeldung. |
| Einen grünen Haken | Fertig. |
| Ein gelbes Warnzeichen | Fehlgeschlagen. |

Haken und Warnzeichen verschwinden, wenn du das Terminal öffnest. Solange Kodosi im
Hintergrund ist, sagt dir eine Mitteilung, wenn eines deiner Terminals zu warten beginnt,
fertig wird oder fehlschlägt. Programme melden ihren Zustand über das
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) oder
einen Fortschrittsbalken. Ein Programm, das nichts meldet, zeigt keinen Zustand.

## 4. Sehen, was ein Agent zeigt

Ein Agent kann dir auf dem Canvas seines Terminals eine Seite, Markdown, ein Bild, ein PDF, ein
Dokument, eine Tabelle, eine Präsentation oder die App zeigen, die er ausführt, und einen
Browser nutzen, den du mit ihm teilst. Das Canvas öffnet sich mit dem, was der Agent zeigt,
während du sein Terminal ansiehst. Claude Code und Copilot CLI wissen, wie das geht, auch wenn
du sie selbst startest, und ebenso ein Codex, den Kodosi startet; einem anderen Agent sagst du,
er soll `kodosi skill canvas` lesen. Hat das Canvas etwas, zeigt die Kopfzeile des Terminals
einen Button für seine Dateien und einen für seinen Browser, mit einem kupferfarbenen Punkt für
Neues. Wähle einen, um das Canvas neben dem Terminal zu öffnen; Mehr Platz gibt dem Canvas die
Bühne und hält das Terminal klein in seiner Ecke. Schließe einen Teil mit dem × an seinem Button
und öffne ihn mit + wieder. Browser öffnen im Aktionen-Button öffnet den Browser, bevor der
Agent ihn nutzt, und Datei hinzufügen… legt eine Datei auf das Canvas und in den Ordner des
Terminals; du kannst eine Datei auch auf das Canvas ziehen. Auf dem iPhone ist der Canvas-Button
in der Leiste des Terminals.

- Ein Dokument, eine Tabelle, eine Präsentation, ein PDF oder eine Textdatei ist live: Du,
  die Personen im Terminal und der Agent bearbeiten es gleichzeitig, und es bleibt eine
  normale Datei in seinem Ordner. Hat ein Dokument nachverfolgte Änderungen, hat seine
  Leiste einen Button, der sie annimmt oder ablehnt. Einstellungen › Dokumente schaltet jede
  Ansicht ein oder aus, zeigt Dokumente als Seiten oder als Text, der sich dem Canvas
  anpasst, und macht Seiten dunkel, wenn Kodosi dunkel ist.
- Eine Seite hat kein Netzwerk. Links in Seiten, Dokumenten, Tabellen, Präsentationen und PDFs
  öffnen sich in deinem eigenen Browser, nach denselben Regeln wie Terminal-Links. Wo du
  bearbeiten kannst, klickst du mit ⌘ auf einen Link (Strg-Klick unter Linux und Windows);
  sonst öffnet ihn ein Klick, und auf dem iPhone ein Tippen. Ein Link auf eine Folie oder
  eine Zelle derselben Datei springt dorthin.
- Markiere Wörter auf einer Seite oder in einem Dokument und wähle dann Dazu fragen. Kodosi
  tippt die Datei und die Wörter in das Terminal, und du schreibst die Frage.
- Klicke und tippe im geteilten Browser wie in deinem eigenen. Der Agent sieht dieselben
  Seiten und Anmeldungen, und sein nächster Befehl sagt ihm, dass du den Browser benutzt
  hast. Nach 10 Minuten ohne Nutzung schläft der Browser, und er öffnet seine letzte Seite
  wieder, wenn du hinsiehst oder der Agent ihn nutzt. Der erste Browser auf einem Computer
  lädt Kodosi Browser, darum dauert er einen Moment.
- Um deine Anmeldungen in deinen privaten Terminals zu nutzen, wähle in
  Einstellungen › Browser neben einem Browser Kopieren. Entfernen nimmt weg, was du kopiert
  hast, und Alle entfernen entfernt jede Anmeldung und alle Websitedaten deiner privaten
  Terminals. Wenn macOS auf einem Mac die Anmeldungen eines Browsers vor Kodosi schützt, wie
  die von Safari, wähle Erlauben… neben dem Browser, um Kodosi den Festplattenvollzugriff zu
  geben.
- In einem Netzwerk geht das Bild des Browsers direkt zwischen deinen Geräten hin und her.
  macOS und iOS fragen einmal nach dem Zugriff auf das lokale Netzwerk; ohne ihn geht das
  Bild über den Dienst.
- Alle, die das Terminal öffnen können, sehen sein Canvas und nutzen seinen Browser. Wenn
  das Terminal geschlossen wird, verschwinden sein Canvas und die Anmeldungen des Browsers.

## 5. Dosi fragen

Dosi ist dein Agent für deine anderen Agents. Wähle Dosis Gesicht unten in der Seitenleiste
(⌘J). Dosi läuft mit Claude Code oder Codex; Einstellungen › Dosi legt fest, womit, samt Konto,
Modell und Denkaufwand.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="../media/dosi-still.webp">
    <img src="../media/dosi.webp" width="900" alt="Auf die Frage, wo du gebraucht wirst, liest Dosi die Terminals, sagt, dass checkout auf eine Freigabe wartet, und fragt, ob es sie erlauben soll.">
  </picture>
</p>

Frag Dosi, was dich braucht, oder bitte es, etwas zu tun. Dosi liest deine Terminals und
Räume, antwortet und tippt, startet und schließt Dinge für dich. Braucht es eine
Entscheidung, fragt es mit Buttons, und du kannst auch mit eigenen Worten antworten. Deine
Unterhaltung mit Dosi ist auf all deinen Geräten dieselbe. Auf einem iPhone oder iPad
schreibt der Tab Dosi an Dosi auf dem Computer, den du über der Nachricht wählst, zuerst an
den, der zuletzt geantwortet hat. Eine Nachricht für einen Computer, der offline ist,
wartet, bis er Kodosi wieder ausführt, und Nicht senden nimmt sie zurück.

Die Zeile neben Dosis Gesicht sagt dir, wann Dosi arbeitet, wann es geantwortet hat und wann
es eine Frage hat. Auf einem Computer ohne Agent steht dort Dosi einrichten, und sie öffnet
Einstellungen › Dosi. Um Dosi von einem Computer zu entfernen, schalte Dosi verwenden in
Einstellungen › Dosi aus.

## 6. Deine anderen Computer hinzufügen

Melde dich auf dem neuen Computer mit demselben Konto an. Er zeigt einen Code, und die
Computer, die du schon nutzt, bitten dich, ihn freizugeben. Wähle auf einem davon Freigeben
und gib den Code ein. Ein iPhone oder iPad kommt auf dieselbe Weise dazu. Danach zeigt jeder
Computer die Terminals all deiner Computer.

Um ein Terminal auf einem anderen deiner Computer zu starten, wähle diesen Computer am rechten
Ende der Leiste des Launchers. Der Ordner-Button listet dann seine letzten Ordner, und Ordner
auswählen… öffnet die Ordner in seinem Home-Verzeichnis. Auf einem iPhone oder iPad wählst
du + in Terminals oder das + neben einem deiner Computer. Auf diesem Computer muss Kodosi
geöffnet sein oder `kodosi host` in einem Terminal laufen.

Erstelle dort auch einen Wiederherstellungsschlüssel und bewahre ihn sicher auf. Wenn du all
deine Geräte verlierst, meldest du dich auf einem neuen an und wählst „Meinen
Wiederherstellungsschlüssel verwenden“, um deine Freunde und Räume zu behalten. Entferne ein
verlorenes Gerät von einem anderen Gerät aus. Wurde ein Gerät gestohlen, erstelle auch einen
neuen Wiederherstellungsschlüssel.

## 7. Mit Freunden arbeiten

Öffne Freunde über das Menü neben deinem Konto. Schick deine Einladung oder füge die
Einladung oder den Benutzernamen eines Freundes ein. Mit einer Einladung kann Kodosi deinen
Freund verifizieren.

<p align="center"><img src="../media/friends.webp" width="900" alt="Die Seite Freunde mit zwei verifizierten Freunden."></p>

Um ein Terminal zu teilen, wähle Teilen im Aktionen-Button seiner Kopfzeile und wähle
Freunde oder Räume. Die Personen, mit denen du teilst, bekommen die volle Kontrolle, mit
deinen Rechten auf diesem Computer. Deine anderen Terminals bleiben privat.

Ein Raum enthält geteilte Terminals, eine Unterhaltung und Aufgaben. Wähle + neben Räume,
füge Freunde hinzu und starte dann ein Terminal im Raum oder teile eines hinein.

<p align="center"><img src="../media/room.webp" width="900" alt="Ein Raum mit einem geteilten Terminal und einer Unterhaltung von Menschen und Agents."></p>

- Alle Mitglieder können die geteilten Terminals nutzen und ihre eigenen teilen. Wer später
  dazukommt, bekommt die Terminals und die ganze Unterhaltung. Dateien bleiben auf dem
  Computer des jeweiligen Terminals.
- Stelle Aufgaben zur Verfügung, übernimm sie, gib sie ab, schließe sie mit einem Ergebnis
  ab oder öffne sie wieder.
- Verknüpfe GitHub- oder Gitea-Repositorys, um Issues zu Aufgaben zu machen; wenn du so eine
  Aufgabe änderst, ändert Kodosi ihr Issue. GitHub nutzt deine Anmeldung bei `gh` oder den
  Git-Credential-Helper, und Gitea den Credential-Helper oder `KODOSI_GITEA_TOKEN` für den
  Server in `KODOSI_GITEA_URL`. Deine Zugangsdaten bleiben auf deinem Computer.
- Schreib `@` und den Namen eines Terminals in den Chat, und Kodosi gibt dem Agent in diesem
  Terminal deine Nachricht mit deinem Namen und dem Raum; der Agent entscheidet, was er damit
  tut. Claude Code und Copilot CLI in einem Kodosi-Terminal und ein Codex, den Kodosi startet,
  wissen, dass sie in Kodosi laufen, und `kodosi skill` gibt den
  [Skill](../../runtime/skills/kodosi/SKILL.md) aus, mit dem ein Agent mitmacht. In einem
  Kodosi-Terminal findet der Befehl den Raum; anderswo ergänzt du `--room <name>`.

## 8. Die Befehlszeile nutzen

Der Befehl `kodosi` arbeitet auf denselben Terminals wie die App, und die App zeigt, was er
tut.

| Befehl | Was er tut |
| --- | --- |
| `kodosi terminal start --agent claude --prompt "fix the failing test"` | Startet einen Agent mit deiner letzten Wahl. `--account`, `--model`, `--effort`, `--permissions` und `--output` ändern sie. |
| `kodosi terminal start --directory <repository> --branch <name>` | Startet ein Terminal auf einem neuen Branch, in seinem eigenen Worktree. |
| `kodosi terminal start --computer <computer> --directory <folder>` | Startet ein Terminal auf einem anderen deiner Computer. Ein relativer Ordner oder einer, der mit `~` beginnt, liegt dort in deinem Home-Verzeichnis. `kodosi devices list` zeigt, welche Computer online sind. |
| `kodosi agent list` | Zeigt die Agents, ihre Konten und deine letzte Wahl. |
| `kodosi agent usage` | Zeigt, wie viel der Tariflimits jedes angemeldete Konto von Claude Code, Codex und Copilot CLI verbraucht hat. |
| `kodosi terminal list` | Zeigt deine Terminals mit ihren Namen und IDs. Gib eines davon als `<terminal>` an. |
| `kodosi terminal read <terminal>` | Gibt den Bildschirm aus. `--history` fügt die Zeilen hinzu, die weggescrollt sind. |
| `kodosi terminal message <terminal> "Can you review my branch?"` | Gibt dem Agent in einem Terminal eine Nachricht, mit Absender und Herkunft. |
| `kodosi terminal input <terminal> --enter "make test"` | Tippt eine Zeile. |
| `kodosi terminal keys <terminal> ctrl-c` | Drückt Tasten. |
| `kodosi terminal wait <terminal> --until done,failed` | Wartet auf einen Zustand. |
| `kodosi terminal show <terminal>` | Zeigt das Terminal in der App. |
| `kodosi dosi say "What needs me?"` | Fragt Dosi. `kodosi dosi show` gibt die Unterhaltung aus, und `kodosi dosi stop` stoppt Dosi. |
| `kodosi show <file>` | Zeigt eine Datei auf dem Canvas dieses Terminals, das sich für die Menschen öffnet, die es ansehen; `kodosi show <address>` zeigt eine Webadresse in seinem Browser. `kodosi canvas` listet, was es zeigt, und `kodosi canvas remove <file>` nimmt eine Datei weg. |
| `kodosi browser open <address>` | Öffnet eine Seite im geteilten Browser dieses Terminals. Die anderen agent-browser-Befehle funktionieren auch. |
| `kodosi skill` | Gibt den Skill aus, mit dem ein Agent mit den Menschen und Agents in Kodosi arbeitet. `kodosi skill canvas` gibt den Skill für das Canvas aus. |
| `kodosi dosi skill` | Gibt den Skill aus, mit dem ein anderer Agent als Dosi handelt. |
| `kodosi host` | Hält deine Terminals am Laufen, nachdem du die App beendet hast. `kodosi host stop` stoppt einen Host, der im Hintergrund läuft. |

`message`, `input` und `keys` brechen mit einer Meldung ab, solange du in diesem Terminal
tippst. `message` bricht auch ab, solange der Agent auf die Antwort einer Person wartet.

## Wenn etwas keine Verbindung bekommt

Prüfe, ob der Computer des Terminals online ist und Kodosi dort geöffnet ist, und ob das
Terminal noch geteilt ist. Ein neues Gerät kann eine Freigabe brauchen, und ein Freund mit
geänderter Identität kann eine Verifizierung unter Freunde brauchen. Nach einer verlorenen
Verbindung zeigt Kodosi wieder den aktuellen Zustand. Sagt Kodosi, dass ein Computer offline
ist, öffnen sich seine Terminals wieder, sobald er zurück ist, außer sie sind dort beendet
worden. Um so ein Terminal aus deiner Liste zu nehmen, wähle sein X. Ausblenden, bis es
wieder da ist zeigt es wieder, wenn der Computer zurück ist, und Endgültig schließen schließt
es dann. Sagt Kodosi, dass es nicht sicher ist, ob deine Eingabe angekommen ist, sieh dir das
Terminal an, bevor du den Befehl noch einmal tippst.
