# Architecture

```mermaid
flowchart LR
    App["Desktop app"] -->|local connection| Host["Kodosi host: Rust runtime"]
    CLI["kodosi command"] -->|local connection| Host
    Host -->|PTY| Shell["Shell / agent CLI"]
    Host <-->|HTTPS + encrypted WebSockets| Backend["Kodosi backend"]
    Phone["iPhone, iPad and Android apps: viewer runtime"] <-->|HTTPS + encrypted WebSockets| Backend
    Other["Another participant's host"] <-->|HTTPS + encrypted WebSockets| Backend
    Backend --> DB[(PostgreSQL)]
    Host --> OIDC["OIDC sign-in"]
```

Each computer runs one Kodosi host, a process of the Rust runtime. The host runs shells
on a PTY and owns the computer's terminals, identity, encryption and projects. The
desktop apps and the `kodosi` command are its clients over a local connection, and the
first client that needs the host starts it. Every client gets the state of terminals and
projects, so what one client does, the others show. The host keeps terminal state with
Ghostty's terminal library, and the apps draw it with Ghostty.

The iPhone, iPad and Android apps embed the viewer build of the runtime, which hosts no
terminals.

The host saves its terminals while they change. After Quit, a new host starts them again
when an app asks, each with its earlier screen and the command that continues its
agent's conversation. A restored terminal has a new ID and names the one that it
replaces, so views on other devices follow it.

## Terminals

The computer that starts a terminal hosts it: it owns the process, the terminal state
and the order of its output. Each view gets a checkpoint and then ordered updates, with
bounded buffering. A view on another device gets them over its own end-to-end encrypted
channel, which the backend relays. When the view and the host are devices of one person
that reach each other, on one network or through their routers, the channel goes on a
direct QUIC connection of the two devices instead, and back to the relay when that
connection fails, without a new checkpoint and without losing or repeating a frame. Its
relay pipe stays open, so the backend still ends the channel when access ends. When a
view's typing or another of its requests waits and its channel has not answered for a
moment, the viewing device reports the terminal as stalled, and its apps say that it
reconnects while the typing goes on. The host does not replay input whose delivery is
uncertain, and a stale view cannot control a replacement terminal. Work that a short drop
of the channel holds back goes out once on the next channel. The terminal comes first on its
channel: its output goes before the frames of a document or the browser, and while a person
types, those frames fill only a moment of the link.

Each app tells its host which terminals are on its screen. The channel to a terminal of
another computer is open only while a client shows the terminal or has work for it. A
hidden terminal keeps its last screen, and its state and alerts come from the backend.

The view in focus sets the size of its terminal. Input from a view is a person's, and
input from the `kodosi` command gives way while a person types.

The runtime decides what a terminal shows of its program: whether it works, what it
waits for and what it does. The program's own report decides, and each client presents
the result.

The host sends each terminal's agent and state to the backend, without its text, and the
backend shows them to everyone who can open the terminal.

A terminal that starts to wait, finishes or fails tells its owner once, on the device that
the owner uses. Each app tells its runtime whether the person uses Kodosi, uses another
app on that computer, or does not use the device, and each runtime tells the backend. The
host of a terminal holds the mark that the owner did not see the new state yet and sends
it with the state. A person who uses Kodosi gets the status sign only. For a person who
uses another app on a computer, the runtime of that computer shows a notification. For a
person who uses none of the devices, the backend sends a push notification to the phones,
with a sound only for a terminal that waits. It waits a short time first, and a computer
whose screen is still on shows its notification before, so that a person who only gave no
input for a while can answer there. A terminal that still waits when its owner stops using
every device gets its mark again and tells the owner again. When the owner looks at the
terminal on one device, its host removes the mark, and the notification leaves every
device.

A device can call another computer of the same person. The call is an end-to-end
encrypted channel like a view, but to the host itself, which answers the launcher's
requests as one more client. The host takes calls only from the person's own approved
devices.

## Backend and projects

One ASP.NET Core process serves the API and the WebSockets and stores its records in
PostgreSQL. It relays terminal traffic without storing it.

Devices encrypt and sign project messages, tasks and repository links. The backend
stores the ciphertext, numbers the items and tells members' devices that a project
changed; each device fetches the newer items. A deleted message is its author's next
version of it and keeps its place. A person who clears the conversation leaves the
backend a position, so that each of their devices hides the same messages. Issue actions
run on the participant's computer with its own credentials. See [Security](SECURITY.md)
and the [encryption design](CRYPTOGRAPHY.md).

## Agents

Kodosi reaches an agent only through what the agent's own CLI offers. Each terminal
carries Kodosi's note for agents and the place of Kodosi's skills in its environment, and
the host writes them into a folder of its own. Kodosi writes nothing into an agent's own
folders, so an agent outside a Kodosi terminal does not change.

The host types a project message that names one of its terminals into the agent in that
terminal, and the `kodosi` command sends such a line to an agent on any computer. A change
of a running agent's model, effort, fast mode or permissions is also typed, as the agent's
own command. The host types only while the agent can take it: not while it waits for a
person, and not while a person types there.

## Canvas

The host keeps the canvas of each terminal: the files that its agent showed, and its
shared browser. The list travels with the terminal's state to each client, and to other
devices on the terminal's encrypted channel. An app reads a file's content, and adds a
file, over that same channel.

### Live documents

[documents/](../documents/) is a separate system: its crates and views import nothing of
Kodosi. One task on the host owns each open file. Each change goes through it in order,
and what it takes goes on to every view, so there is no CRDT: a change that names a
position is valid only on the latest state, and otherwise the view moves its edit and
sends it again. A change that the file could not save goes back to its sender alone. The
host takes each client's batch at most once and remembers that when the document closes
and opens again, so a view drops a batch that was refused, or that the host may have taken
already, and says so, rather than send it again. The host keeps a journal, saves the file
in place and merges what another program writes.

The views are one static page in each app, in a closed web page with no way to the
network. Their frames go through the app to the host, over the terminal's encrypted
channel from another device. Agents use the same server with `kodosi doc`. The Tauri app
in `documents/app/` is a second host of the same crates and a separate build.

### Shared browser

[browser/](../browser/) is a separate system too. The host runs one window of Kodosi
Browser for each terminal: Chromium on a ready-built CEF that draws off screen and shows
nothing on the computer. The host gets it from a signed feed and keeps it up to date.

While a device looks, the engine encodes the page as video. One picture serves every
device that looks: the page takes the size of the person who used it last and the pixels
of the sharpest view, as far as the host's encoder takes them, and a page that stays
still is coded again until it is sharp. The frames, and the clicks and keys that come
back, travel on the terminal's encrypted channel. The agent drives its window with
agent-browser through a port that the host serves, where it sees only the tabs of its own
terminal.

A person's private terminals share one kept browser profile. A shared terminal has a
profile of its own, which goes with the terminal.

## Languages

Each part takes the words of the language that it shows. The apps keep their own words,
and the runtime marks each sentence for people and gives it to an app as an English
template with values, which the app's runtime fills in the app's language. The `kodosi`
command, the host and the logs use English. The backend answers in English, and the
runtime turns its known answers into such sentences. The sign-in pages, the account page
and the e-mails are the Keycloak theme in [sign-in/](../sign-in/).

## Contracts

The runtime, the apps and the backend meet at the contracts in [protocol/](../protocol/)
and the [C header](../clients/apple/Sources/KodosiKit/kodosi_runtime.h); change them
together. The backend names the range of API and terminal protocol versions that it
serves. An older app asks the person to update, and a newer one says that the server is
behind.
