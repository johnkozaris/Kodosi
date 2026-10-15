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
content; it does not run commands or read terminal or room plaintext. It sees
only the agent of each terminal and the state that its program reports, such as
working or waiting for an approval.

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
  Linux and builds for Windows 11 on x64 and ARM64 with ConPTY and an MSIX package; it
  is not yet validated on Windows hardware.
- The iPhone and iPad app follows, answers and controls the terminals of the person's
  computers and friends, and takes part in rooms. It starts no terminals.
- `kodosi room` and its bundled skill let existing agents take part through the
  running host.
- Terminals show the state their programs report through OSC 7501 or a progress bar:
  working, waiting for a person, done or failed. Everyone who can open a terminal sees
  it before they open it: the host sends the agent and the state through the backend,
  without the program's text. Each terminal shows who has it open, the person at its
  host computer included. While the app is not in front, a terminal that the person
  owns or has open sends a notification when it starts to wait, finishes or fails.
- The home starts a new terminal with an agent. The host finds the installed agents
  (Claude Code, Codex, Copilot CLI, Cursor, Gemini CLI and OpenCode), each agent's
  accounts (sign-in folders such as `~/.claude-work`), models and efforts, and the
  shell commands that start each account. The person picks them and can type a first
  prompt; the new terminal's shell types the command. A missing agent offers its
  official install command. A new account is an empty sign-in folder.
- A terminal at its prompt offers the person's start commands. Kodosi types the
  command into the shell and does nothing more.
- A terminal can start on a new branch in its own Git worktree. When it ends, the host
  removes the worktree and the branch only if nothing changed. Kodosi keeps no record
  of these folders.
- A server can define plans. A plan limits an account's devices, owned rooms, people
  and data in each owned room, people who see one of its terminals, friends and open
  friend requests; an account without a plan gets the default plan. A full room loses
  its oldest messages first, and the apps say so; tasks and repositories stay.
- A person deletes their account through the sign-in service's account page. The
  service removes the account, devices, friends, sharing and owned rooms; what the
  person wrote in other rooms stays. Terminals keep running.
- Direct terminal sharing and Claude Code and Copilot CLI conversation preview and
  resume remain available.
