# Kodosi

Kodosi is a native app where people and their coding agents work together in shared terminals and
project chats. The goal is to stay lean: expose useful capabilities and let people and agents
decide how to use them.

## Who it's for

Developers running coding agents in terminals, often across multiple machines, away from their
desk, or alongside friends. They want to see at a glance which agent needs attention, answer it
from any device, and let a friend or another agent jump into the same work.

## Core capabilities

### Every terminal in one place
Terminals from all your computers, plus terminals friends share with you, appear side by side on
every device. Each terminal is a real process running on the computer that hosts it.

### Attention and status
Each terminal surfaces what its running program reports: working, waiting for input, done, or
failed, and shows the helpers its agent started for as long as they work. Kodosi notifies you when
a terminal needs attention.

### Chat view
Any terminal running a supported agent can also be viewed as a chat: your messages, the agent's
replies and actions, the helpers working for it, and its prompts with clickable answer buttons.
You can switch freely between the terminal screen and chat view, and anything you do in one shows
up in the other.

### Sharing
Share any terminal with friends or with a project, from any of your devices. Everyone with access
gets full control.

### Projects
A project groups a chat, tasks, linked repositories, and terminals from multiple computers. You
can use a project solo and invite friends whenever you want.

### Canvas
Every terminal has a canvas beside it where the agent can display files and use a shared browser.
Documents on the canvas are live, so people and agents can edit the same file simultaneously.
A computer can also show its screen on the canvas of its terminals, where its owner watches and
controls it from their other devices. Only the person at the computer turns this on, and only the
owner's devices ever see the screen, even in a terminal shared with friends.

### Dosi
Dosi is your meta-agent for managing other agents. It sees all your terminals, agents, and
projects, acts on them via the `kodosi` CLI, and asks you to pick from clear choices when a
decision is needed.

### Mobile access
The iOS and Android apps let you watch, answer, and control terminals running on your computers,
as well as participate in projects while away from your desk.

### Sleep and wake
A computer doesn't go to sleep by itself while an agent works on it, but an agent that waits for
you doesn't keep it awake. From any of your devices you can set a computer to stay awake always,
or never, put it to sleep and wake it again. Kodosi shows for each computer if it wakes, and names
the one setting to change when it doesn't. Before it puts a computer to sleep, it says when nothing
could wake it again.

## Design decisions

### The terminal stays a terminal
Full control means typing, resizing, interrupting, and closing, running with the host user's
normal OS privileges. Kodosi doesn't insert permission tiers, sandboxes, or workflow machinery
between you and a terminal. Programs get standard Ghostty terminal behavior.

### Sharing means trust
A shared terminal runs under its owner's account, so anyone in it can do whatever the owner can do
there. Kodosi defends against the network, the backend, and unauthorized users, not against people
you explicitly chose to share with. Removing or blocking a friend revokes everything shared
between you.

Every change that lets someone new into your terminals or projects asks you first in the Kodosi
app of the device where it starts, including changes the `kodosi` command asks for on behalf of an
agent or of anyone typing in your terminals. This guards against mistakes and agents, not against
a person in your shell, who acts as you. The computer that runs a terminal stays the only one that
writes who may enter it; your other devices ask it over the encrypted channel.

### Agents stay with their providers
Agents run through their own CLIs. Execution, prompts, permissions, settings, memory, and
conversation history remain with the provider. Kodosi launches an agent by typing its command into
a shell, passes a short note explaining that it's inside Kodosi, and makes the `kodosi` CLI and
skills available. The agent decides when to use them. Kodosi never reads provider credentials.

Copilot CLI is the one exception for settings. It only reads quiet output mode from its saved
config file, so Kodosi temporarily updates that setting while Copilot runs and restores it
afterward. Copilot also requires experimental features enabled to load Kodosi's chat extension
(which Copilot leaves enabled) and per-folder extension approval (which Kodosi grants for each
folder where it launches Copilot). Antigravity CLI loads plugins only from its own configuration
folder, so Kodosi links its plugin there while Kodosi runs and removes it when Kodosi stops.

### Chat is a view of the terminal
The agent still runs inside its terminal as usual; the chat is simply a structured view of that
session. Supported agents report activity through their native extension or server interface, and
messages, answers, and stops travel back the same way. Kodosi never scrapes the terminal screen or
simulates keypresses for chat, so UI updates to an agent don't break it. Answering in chat answers
the agent's real prompt, and permissions stay with the provider. A message sent while the agent
works joins the running turn, as typing in the agent's terminal does, when the agent's interface
takes it there; otherwise it waits in a queue for the next turn. Stopping a turn never drops a
message that the agent hasn't read. Agents that don't report chat events simply use the normal
terminal view. Each terminal opens in whichever view its agent works in, across all devices.

### People and agents are equals in a project
Both humans and agents read and write the same project chat and tasks, and decide what to pick up
next; nobody assigns work. A chat message is delivered into a terminal only when it mentions that
terminal by name, formatted as a single line stating who wrote it and where it came from. Delivery
waits if the agent is prompting a human or if someone is actively typing in that terminal.

### One host per computer
A single local host process owns all terminals on a computer. Desktop apps and the `kodosi` CLI
are equal clients of that host: actions from one immediately appear in the others, and an agent in
a shell can drive Kodosi just like a human in the UI. Interactive typing from a human always takes
priority.

### Data stays where it lives
A terminal's files stay on its host computer. Canvas files are ordinary files on disk, the
providers of linked repositories remain the source of truth for their issues, and Git credentials
never leave the user's machine.

### End-to-end encryption
Client devices encrypt terminal traffic and project data, and private keys never leave devices.
The backend connects peers and relays ciphertext; two devices belonging to the same user connect
directly whenever they can reach each other. The server runs no commands and only sees each
terminal's agent type and reported state. New devices can't access anything until an existing
approved device approves them, and a recovery key lets you keep your friends and projects if you
lose all your devices.

### Terminals outlive connections
Reconnecting restores current state without disrupting other viewers and without replaying input
whose delivery is uncertain. A stale connection can never control a replacement terminal, and
dropping offline doesn't alter sharing permissions. After you quit and reopen the app, your
terminals come back with their previous screen contents. An agent that a restart stopped in the
middle of a task goes on by itself after an update, and otherwise waits until someone continues it,
so nobody comes back to agents that started working on their own.

### Calm error handling
Kodosi recovers automatically wherever possible. When something does need user action, it reports
the failure in plain language right where you can act on it, with a single recovery step, and
keeps technical details in the log.

## UX and language conventions

Keep terminals and project chats immediately accessible. Starting work, inviting someone, sharing
a terminal, and posting in a project should take minimal steps. Use plain language and native
platform interactions, show progress where the work happens, and add a control only when it helps
someone do the work.

User-facing text uses one consistent term per concept: **terminal**, **project**, **friend**,
**agent**, and **device**. Terms like *session*, *host*, and *runtime* stay internal.

Kodosi is localized into English, German, Greek, Spanish, French, Italian, Japanese, Korean, and
Simplified Chinese. The `kodosi` CLI, logs, and internal error details stay in English.

## Apps

- The macOS, Linux, and Windows desktop apps run terminals on the computer's local host, join
  projects, and bundle the `kodosi` CLI.
- The iOS (iPhone and iPad) and Android apps don't host terminals; they watch, answer, and control
  terminals running on your computers.
- Desktop apps download updates in the background and restart to install them only while nobody
  uses Kodosi, no agent is busy, and no terminal runs another program, so an update never ends
  someone's work. One setting turns this off for people who want to choose the moment.
