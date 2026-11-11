# Kodosi

Kodosi is a native place where people and their coding agents work together through
shared terminals and a project conversation. Keep it lean: make useful actions
available, and let people and agents decide how to use them.

## Who it is for

People who run coding agents in terminals, often on more than one computer, away from
the desk and with friends. They want to see which agent needs them, answer it from any
device, and let a friend or another agent step into the same work.

## What it is

- **Every terminal in one place.** The terminals of a person's computers, and the ones
  that friends share, are side by side on each of the person's devices. Each is a real
  process on the computer that hosts it.
- **Attention.** A terminal shows what its program reports: working, waiting for a
  person, done or failed. Kodosi tells the person when a terminal needs them.
- **Sharing.** A person shares a terminal with friends or with a project, and everyone
  it is shared with gets full control.
- **Projects.** A project holds a conversation, tasks, repositories and terminals from
  many computers. A person can work in it alone and invite friends when they want.
- **Canvas.** Each terminal has a canvas beside it, where its agent shows what it makes
  and uses a browser that the people share. Documents on the canvas are live: people and
  agents edit the same file at the same time.
- **Dosi.** The person's agent for their other agents. It sees every terminal, agent and
  project of the person, acts on them with the `kodosi` command and asks with choices.
- **Away from the desk.** The phone and tablet apps follow, answer and control the
  terminals of the person's computers and take part in projects.

## Decisions

**The terminal stays a terminal.** Full control means typing, resizing, interrupting and
closing, with the operating-system privileges of the user who hosts it. Kodosi puts no
permission tiers, sandbox or workflow between a person and a terminal, and programs get
what Ghostty gives them.

**Sharing is trust.** A shared terminal runs as its owner, and everyone in it can do what
the owner can do there. Kodosi protects against the network, the backend and people
without access, not against the people that someone chose. Removing or blocking a friend
ends what the two share.

**Agents stay with their providers.** Agents run through their own CLIs. Execution,
prompts, permissions, settings, memory and conversations stay with the provider. Kodosi
starts an agent by typing its command into a shell, tells it in a short note that it runs
in Kodosi, and gives it the `kodosi` command and skills. The agent decides when to use
them. Kodosi reads no provider credentials.

**People and agents are equals in a project.** They read and write the same conversation
and tasks, and they decide what to do next; nobody assigns work. Only a message that
names a terminal goes into that terminal, as one line that says who wrote it and from
where, and never while its agent waits for a person's answer or a person types there.

**One host on each computer.** The host owns the computer's terminals. The apps and the
`kodosi` command are equal clients of it: what one does, the others show, and an agent
can do from a shell what a person does in an app. A person's typing comes first.

**The work stays where it is.** A terminal's files stay on its computer. A file on the
canvas is a normal file there, a linked repository's provider stays authoritative for its
issues, and Git credentials stay on the participant's computer.

**End-to-end encryption.** The participants' devices encrypt terminal traffic and project
content, and private keys stay on devices. The backend connects participants and relays
ciphertext. It runs no commands and sees only each terminal's agent and reported state. A
new device shares nothing until an approved device approves it, and a recovery key keeps
friends and projects for a person who lost all devices.

**Terminals outlive their connections.** Reconnecting restores the current state without
disturbing other viewers and without repeating input whose delivery is uncertain. A stale
connection never controls a replacement terminal, and a lost connection does not change
sharing. Terminals come back with their screens when the app opens again after Quit.

**Calm.** Kodosi handles what it can by itself. It shows a failure only where the person
can act on it, in plain words and with one way to recover, and keeps the details in the
log.

## Experience

Keep terminals and the project conversation easy to reach. Starting work, inviting
someone, sharing a terminal and writing in the conversation take few steps. Use plain
language and native interaction, show progress where work happens, and add a control only
when it helps someone do the work.

Visible text uses one word for one thing: terminal, project, friend, agent, device.
Session, host, runtime, room and mission are internal words: the code calls a project a
room, and the runtime's contract calls it a mission.

Kodosi speaks English, German, Greek, Spanish, French, Italian, Japanese, Korean and
Simplified Chinese. The `kodosi` command, the logs and the details of an internal fault
stay English.

## The apps

- The Mac app and the Linux and Windows app run terminals on the computer's host and
  take part in projects. Each contains the `kodosi` command.
- The iPhone and iPad app and the Android app host nothing. They follow, answer and
  control terminals that run on computers.
- Apps update themselves in the background and never restart without asking, because a
  restart stops the host's terminals.
