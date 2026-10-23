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

Humans and agents read and write the same room conversation. Agents take part through
the `kodosi` command and its room skill.

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
programs. Closing the window leaves the app and the host running, with Kodosi's icon in
the menu bar or the system tray; on a desktop without a system tray, it quits. Settings
can change what closing the window does. Quit stops the computer's host and its
terminals, but not a host that runs in a terminal with `kodosi host`, and not terminals
on other computers.

The apps and the `kodosi` command are equal clients of the computer's host: what one
does, the others show. A person's typing comes first: input from the command stops with
a message while a person types in that terminal.

Reconnecting restores the current state without disturbing other viewers or repeating
input whose delivery is uncertain. A stale connection never controls a replacement
terminal. A lost connection does not change sharing. When the computer of a terminal is
offline, the terminal says so, keeps its last screen and takes no input; it opens again
when that computer is back.

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

## What the apps do

- The Mac app and the Linux and Windows app run terminals on the computer's host and
  take part in rooms. The iPhone and iPad app follows, answers and controls the
  terminals of the person's computers and friends, takes part in rooms, and starts no
  terminals.
- Each app contains the `kodosi` command. `kodosi terminal` starts, reads, types into,
  waits for, shows and shares terminals and starts agents in them; `kodosi room` and its
  skill let agents take part in rooms.
- Dosi is the person's agent for their other agents. It sees every terminal, agent and
  room of the person, acts on them with the `kodosi` command, shows them in the app and
  asks with choices. It runs on the computer where the person writes, with the agent,
  account, model and effort chosen in Settings, and works in its own folder. One
  encrypted conversation follows the person to all their computers. `kodosi dosi say`,
  `show` and `stop` reach it from a shell, and `kodosi dosi skill` lets another agent act
  as Dosi.
- Apps update themselves in the background and never restart without asking, because a
  restart ends the host's terminals. After a system update on Linux, the app offers to
  restart. An app too old for the backend asks for an update.
- Until it is approved, a new device says so and shares nothing. The person's approved
  computers show its request and lead to the approval.
- Terminals show the state their programs report: working, waiting for a person, done
  or failed. Everyone who can open a terminal sees its state and who has it open. While
  the app is in the background, a terminal the person owns or has open sends a
  notification when it starts to wait, finishes or fails.
- New terminal opens the launcher. The host finds the installed agents (Claude Code,
  Codex, Copilot CLI, Cursor, Gemini CLI and OpenCode), their accounts, models and
  efforts. The person picks them and may type a first prompt; the new terminal's shell
  types the command. A missing agent offers its install command. The host keeps the last
  choice, so the apps and `kodosi terminal start --agent` start the same way.
- The launcher can also start the terminal on another of the person's computers where
  Kodosi runs. It then shows that computer's agents, accounts and last choice, its recent
  folders, and the folders in its home, one level at a time. Only the person's own
  approved devices can ask, and the requests and answers stay end-to-end encrypted.
  `kodosi terminal start --computer` does the same.
- The launcher shows how much of its plan limits each Claude Code, Codex and Copilot CLI
  account has used. Usage in the menu beside the person's name shows each signed-in
  account with a dial for each limit, and a click opens the launcher with that account.
  The agent's own program reads the numbers when the person opens these, at most once a
  minute. Kodosi reads no credentials, and the numbers do not go to the backend; those
  of another computer come end-to-end encrypted. `kodosi agent usage` prints them.
- A terminal at its prompt offers a mark for each installed agent, which starts it with
  the person's last choice. Kodosi types the command into the shell and does no more.
- Programs get what Ghostty gives them: Kitty images, clipboard writes, and clipboard
  reads, which ask first in another person's terminal. Kodosi marks each prompt of a
  login zsh or bash (not macOS's bash 3.2), so ⌘↑ and ⌘↓ (Ctrl+Shift+↑ and ↓ on Linux and
  Windows) jump between prompts. A terminal shows when its program runs on another
  computer, in a container or virtual machine, or as root.
- A room shows a dot when someone else, or one of the person's agents, wrote in it since
  the person last looked.
- A terminal can start on a new branch in its own Git worktree, which the host removes
  when the terminal ends if nothing changed.
- A server can define plans that limit an account's devices, rooms, people, friends and
  room data. A full room loses its oldest messages first; tasks and repositories stay.
- A person deletes their account through the sign-in service's account page; what they
  wrote in other rooms stays. Terminals keep running.
- A terminal can also be shared directly with friends, and saved Claude Code and
  Copilot CLI conversations can be previewed and resumed.
