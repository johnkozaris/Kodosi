# Kodosi

Kodosi is a native place for people and their coding agents to work together through
shared terminals and a room conversation. Keep it lean: make useful actions available
and let people and agents decide how to use them.

## Rooms and terminals

The interface calls these shared spaces rooms; runtime contracts call them missions.

- A terminal is a real process on its host computer.
- Share a terminal with a room once. Every member gets full control, including people
  invited later. Room sharing follows the room's membership.
- Any member can contribute their own terminals. A room can hold terminals from many
  computers, folders and repositories.
- Sharing a terminal does not share the host's other terminals.
- A person's approved devices can use all of that person's terminals.

Full control means typing, resizing, interrupting and closing, with the host user's
operating-system privileges.

## People and agents

Humans and agents read and write the same room conversation. Agents get room context
and actions through simple runtime tools and a skill that work from their existing
harnesses.

New members get the full history. Anyone can post messages, put tasks up for grabs,
claim, release, close and reopen them. A task can carry context, repositories, and a
completion note or pull request link. People and agents decide what to do next.

A room can link several GitHub or Gitea repositories and turn their issues into tasks.
The provider stays authoritative: changing a linked task updates the issue, and
refreshing the room picks up provider changes. Provider credentials stay on the
participant's computer. Native tasks need no Git provider.

Agents run through their own CLIs. Execution, prompts, permissions, settings, memory
and conversations stay with the provider. Kodosi supplies the shared space.

## Terminal behavior

Minimize hides a view and keeps its terminal running. Close ends the terminal and its
programs. Closing a window leaves the host running. Quit ends the terminals that app
hosts, not terminals on other computers.

Reconnecting restores the current state without disturbing other viewers or repeating
input whose delivery is uncertain. A stale connection never controls a replacement
terminal. A lost connection does not change sharing.

## Encryption

Terminal traffic and room content are end-to-end encrypted between participants.
Private keys stay on devices. The backend connects participants and relays encrypted
content; it does not run commands or read terminal or room plaintext. It sees only each
terminal's agent and reported state, such as working or waiting for an approval.

A recovery key approves a new device when no other device can, and keeps friends and
rooms. Without it, a person who lost all devices starts fresh with a new identity, and
friends trust that identity again.

## Experience

Keep terminals and the room conversation easy to reach. Starting work, inviting
someone, sharing a terminal and writing in the conversation take few steps. Use plain
language and native interaction. Add controls only when they help someone do the work.

Visible text uses one word for one thing: terminal, room, friend, agent, device.
Session, host, runtime and mission are internal words.

A terminal opened in a room keeps the conversation beside it. Tasks and repositories
share that workspace. Drafts and reading position survive moving between them. Show
progress where work happens and give each failure a clear recovery action.

## Current implementation

- The runtime and backend implement room terminal sharing, encrypted conversation and
  task history, repository links, and GitHub and Gitea issue actions.
- The Mac app and the Qt app host terminals and take part in rooms. The Qt app runs on
  Linux and on Windows 11 24H2 (x64 and ARM64, ConPTY, MSIX); it is not yet validated on
  Windows hardware. The iPhone and iPad app follows, answers and controls the terminals
  of the person's computers and friends, takes part in rooms, and starts no terminals.
- Each app contains the `kodosi` CLI. `kodosi room` and its bundled skill let existing
  agents take part through the running host.
- Apps update themselves in the background and never restart without asking, because a
  restart ends the host's terminals. An app too old for the backend asks for an update.
- Terminals show the state their programs report (OSC 7501 or a progress bar): working,
  waiting for a person, done or failed. Everyone who can open a terminal sees its state
  and who has it open. While the app is in the background, a terminal the person owns or
  has open sends a notification when it starts to wait, finishes or fails.
- New terminal opens the launcher. The host finds the installed agents (Claude Code,
  Codex, Copilot CLI, Cursor, Gemini CLI and OpenCode), their accounts (sign-in folders
  such as `~/.claude-work`, shown by e-mail, including those the person's shell gives an
  agent), models and efforts. The person picks them and may type a first prompt; the new
  terminal's shell types the command. A missing agent offers its install command.
- A terminal at its prompt offers a mark for each installed agent, which starts it with
  the person's last choice. Kodosi types the command into the shell and does no more.
- A room shows a dot when someone else, or one of the person's agents, wrote in it since
  the person last looked.
- A terminal can start on a new branch in its own Git worktree, which the host removes
  when the terminal ends if nothing changed.
- A server can define plans that limit an account's devices, rooms, people, friends and
  room data. A full room loses its oldest messages first; tasks and repositories stay.
- A person deletes their account through the sign-in service's account page; what they
  wrote in other rooms stays. Terminals keep running.
- Direct terminal sharing and Claude Code and Copilot CLI conversation preview and
  resume remain available.
