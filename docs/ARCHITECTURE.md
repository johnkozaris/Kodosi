# Architecture

```mermaid
flowchart LR
    App["Desktop app"] -->|local connection| Host["Kodosi host: Rust runtime"]
    CLI["kodosi command"] -->|local connection| Host
    Host -->|PTY| Shell["Shell / agent CLI"]
    Host <-->|HTTPS + encrypted WebSockets| Backend["Kodosi backend"]
    Phone["iPhone and iPad app: viewer runtime"] <-->|HTTPS + encrypted WebSockets| Backend
    Other["Another participant's host"] <-->|HTTPS + encrypted WebSockets| Backend
    Backend --> DB[(PostgreSQL)]
    Host --> OIDC["OIDC sign-in"]
```

Each person's computer runs one Kodosi host, a process of the Rust runtime. It runs
shells on a PTY (ConPTY on Windows) and owns terminals, identity, encryption and rooms.
The desktop apps and the `kodosi` command are its clients over a local connection; the
first client that needs the host starts it in the background. Each client gets the
answers to its own requests, and every client gets the state of terminals and rooms, so
what one client does, the others show. The host keeps terminal state with Ghostty's
terminal library, and the apps draw it with Ghostty. The iPhone and iPad app embeds the
viewer build of the runtime and hosts no terminals.

Quit in a desktop app stops a background host and its terminals. A host that runs in a
terminal with `kodosi host` stops only there. An app that finds a background host of an
earlier version restarts it, and asks first when that host still has terminals.

The host saves its terminals in `terminals.json` in its data folder while they change,
and again as it stops, with the conversation that each agent shows. It reads Claude
Code's own records of the conversation that each Claude process shows, and for Codex and
Copilot CLI it types their own list of conversations to continue. A new host keeps the
saved terminals until an app sends `session.restore`; then it starts each one again with
a new terminal ID, and the terminal's `restoredFrom` names the ID that it replaces. The
host also gives `restoredFrom` to the backend, so that views on other devices can follow
the terminal. A restored terminal joins its room or its people again when the account
that chose them is signed in.

As it stops, the host also saves the primary screen and history of each terminal that
comes back. It writes them as VT text to a file in `screens/` in its data folder. The file
has at most 1 MiB of the newest lines, and only the user can read it. A restored terminal
starts with that text and a `Restored` line above the output of its shell, and the host
then deletes the file. A Windows terminal moves the text into its history first, because
ConPTY draws the screen as if it were empty.

## Terminals

The computer that starts a terminal hosts it: it owns the process, the terminal state
and the order of its output. Each view gets a checkpoint and then ordered updates, with
bounded buffering; a view on another device gets them over its own end-to-end encrypted
channel through the backend. The host does not replay input whose delivery is uncertain,
and a stale view cannot control a replacement terminal.

Each app tells its host which terminals are on its screen and whether the person is in
the app; the second part decides only alerts. The host keeps the channel of a terminal on
another computer open while a client shows it, or while a client that sends no such
report, such as the `kodosi` command, has it open. The channel of a hidden terminal closes
when nothing is in flight. Its views keep their screen, input and canvas requests wait,
and the next channel resumes them with a new checkpoint, without the reconnecting state.

A pasted picture travels as a file. The app gives its host the picture, and a view of a
terminal on another computer sends it on its channel, only when that host does not have it
yet. Twice the bytes that the link carries in its shortest round trip are in transit, so
the link stays full and input waits behind the picture for about one round trip. The
terminal's host saves it in a private folder of the terminal, which goes when the terminal
ends, and pastes the path of the file as input of the view.

A client asks for the grid of the view that will show a new terminal, so its program
starts at its final width. The view in focus sets the size of its terminal. When that
view leaves, the focused view whose size the terminal had before sets it again. Input from a view is a person's;
input from the `kodosi` command stops with a message while a person types in the
terminal. When a terminal stops working or starts to wait while no client shows it, the
host marks it until a client shows it.

A terminal starts with its host's environment and Kodosi's terminal variables. A host
that a client starts gets the user session's environment from Windows or the systemd
user manager, plus the client's `KODOSI` settings, and otherwise the client's
environment.

The host also sends each terminal's agent and state, unencrypted and without its text,
to the backend, which shows the newest to everyone who can open the terminal while the
host is connected.

A device can call another computer of the same person whose host is connected. The call
is an end-to-end encrypted channel like a view, but to the host itself. The caller sends
the launcher's requests (agents and choices, folders, a new terminal) as desktop
commands, and the host answers them as one more client. The host takes calls only from
the person's own approved devices and runs no other command.

## Backend and rooms

One ASP.NET Core process serves the API and the WebSockets, holds the live connections
under a PostgreSQL lease, and stores its records in PostgreSQL. It relays terminal
traffic without storing it.

Devices encrypt and sign room messages, tasks and repository links. The backend stores the ciphertext, numbers the items and tells members' devices that a room
changed; each device fetches the newer items. Issue actions run on the participant's
computer with its own credentials. See [Security](SECURITY.md) and the
[encryption design](CRYPTOGRAPHY.md).

## Agents

Each terminal has Kodosi's note for agents in `KODOSI_AGENT_CONTEXT`. The launcher passes
it to Claude Code and Codex, and Copilot CLI reads it from the host's `agents` folder
through `COPILOT_CUSTOM_INSTRUCTIONS_DIRS`. A host types the new room messages that name
one of its terminals with `@` into the agent in that terminal, as one line with who wrote
it and from where. `kodosi terminal message <terminal> "text"` sends such a line to an
agent on any computer, as a terminal control. The terminal types a message only while
its agent reports a state other than waiting for a person and nobody types there, and
Enter follows as a key of its own.

The host also writes a plugin folder with the `kodosi`, `kodosi-canvas` and
`kodosi-documents` skills in its `agents` folder at each start. Claude Code finds it
through `CLAUDE_CODE_PLUGIN_DIRS`, after the person's own folders, and the launcher gives
it to Copilot CLI with `--plugin-dir`.
Other agents can read a skill with `kodosi skill`. Kodosi writes nothing into an agent's
own folders, so an agent outside a Kodosi terminal does not change.

## Canvas

The host keeps the canvas of each terminal: the files that an agent showed with
`kodosi show <file>`, by path, each with a revision. Once a second, while the canvas has
files, it compares their size and change time, and a change gives a new revision.
Markdown, text and data files become pages with the style of the skill's page template.
The list goes with the terminal's state to each client and to each device on the
terminal's encrypted channel. An app asks for the content of one revision with
`canvas.request` and gets it in parts of 256 KiB, so a device of another computer reads
it over the same channel. Each app keeps what it read by revision.

A person adds a file with `canvas.request` in parts too. The host writes the parts beside
the file's place in the terminal's folder, so an agent never sees a part of it, and a new
empty file claims the free name before the whole file moves there.

### Live documents

[documents/](../documents/) is a decoupled system: its crates and views import nothing of
Kodosi. One task on the host owns each open file and keeps a plain document value. Each
change goes through that value in order, reaches the journal, and then goes to every view
with its rev. A key-named change is always valid. A change that names a position is valid
only on the latest positions; else the view moves its edit and sends it again. So there is
no CRDT. The host saves two seconds after the last change and at most 15 seconds after the
first, swaps only the changed parts of the file, and renames it into place. After a crash
it opens the file and replays the journal. An outside write merges by comparing it with the
last saved state. The document closes 60 seconds after its last view leaves.

The view bundle is one static page in each app, in a closed web page with no way to the
network. Each app keeps the web page of a file apart from its canvas, so a page that leaves
the screen keeps its place and its undo, and it closes after five minutes out of sight.
A view's frames go through the app to `document.open`, `document.send` and
`document.frame`. On the same computer the host's local connection carries them. From
another device they travel on the terminal's encrypted channel as pipe frames, when the
pacer has room and no terminal output waits. Agents use the same server with `kodosi doc`,
which the host runs by its arguments. The Tauri app in `documents/app/` is a second host,
with one viewer for each window. It is a separate build with its own workspace and
`Cargo.lock`, so Kodosi links only the crates.

### Shared browser

[browser/](../browser/) is decoupled too. The host opens one window of Kodosi Browser for
each terminal: plain Chromium with no engine patches, started hidden with a debug port on
`127.0.0.1` and the capture extension. On Linux it runs on its own Xvfb display. The first
window gets the engine from a signed feed. The host looks at the feed each day: a browser
that starts after an update uses the new engine, a browser that runs keeps its own, and the
old engine goes when the host starts again.

While a device looks, the extension's tab capture feeds `VideoEncoder`, and H.264 frames
reach the host on a local socket with a one-time token. A viewer's pipe carries these
frames, and the viewer sends back an ack for each one. One encoder serves all viewers of a
window, and the host steers its bitrate from the round trip, so no queue forms. A viewer
that falls behind gets nothing until the next key frame. The Mac and iPhone apps show frames
on a display layer. The Linux and Windows app decodes with OpenH264 in the runtime library.
Clicks, keys and text come back on the pipe, and the host gives them to the page on the
debug port. A `<select>`, a right-click, a dialog and a file picker reach the viewer as
state, and the app draws them.

A viewer on another device asks on the terminal's channel for a straight connection. The
host opens a port from 47360 to 47379 for a few seconds, and answers with its addresses.
The viewer tries them at once, and the channel's TLS runs on TCP with the same device keys.
The host lets only the viewer's device through. When no address answers, the pipe uses the
terminal's channel, where terminal output goes first. The path stays until the pipe ends,
and the view then joins again.

The host runs each `kodosi browser <command>` in the agent's folder as agent-browser on that
debug port, with the terminal's own session, Kodosi's settings and none of the person's
`AGENT_BROWSER_*` variables. agent-browser leaves the port after a minute with no command,
because a page can notice a client there. While a command runs, the browser's state names
the agent, what it does and the point of its target, which agent-browser's `get box --json`
gives, and the command waits a moment, so the agent's pointer arrives first.

After 10 minutes with no use and nobody looking, the window closes. The browser exits with
its last window, and the terminal's restore record keeps the last page. A person's private
terminals share one kept profile. A shared terminal has a profile of its own, which goes
with the terminal, and a new host deletes the profiles that no terminal has. Settings ›
Browser reads the cookies of another browser's store before a browser starts, puts them
into the kept profile on the debug port, and notes which ones, so a button can remove them
again. Another button deletes the kept profile.

## Contracts

The runtime, the apps and the backend meet at the contracts in [protocol/](../protocol/)
and the [C header](../clients/apple/Sources/KodosiKit/kodosi_runtime.h); change them
together. The backend's `/health/live` names the range of API and terminal protocol
versions it serves. An older app asks the person to update, and a newer one says that
the server is behind.
