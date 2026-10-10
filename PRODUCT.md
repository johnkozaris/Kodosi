# Kodosi

Kodosi is a native place for people and their coding agents to work together through
shared terminals and a room conversation. Keep it lean: make useful actions available
and let people and agents decide how to use them.

## Rooms and terminals

The interface calls these shared spaces Rooms; runtime contracts use Missions.

- A terminal is a real process on its host computer.
- Share a terminal with a room once. Every room member gets full control, including
  people invited later. Room sharing follows the room's membership.
- Any member can contribute their own terminals. Alice can use Bob's shared terminals
  and Bob can use Alice's.
- A room can contain terminals from many machines and folders. It does not require a
  common repository or filesystem.
- Sharing a terminal into a room does not share the host's other terminals.
- Approved own devices can also use the owner's terminals.

Full control includes typing, resizing, interrupting, and closing. People choose to
share that control, with the host user's operating-system privileges. Restrictions on
those actions, sandboxing, and arbitration of concurrent file edits are not product
requirements.

## People and agents in the conversation

Humans and agents need to read and contribute to the same room conversation. Give
agents access to room context and actions through simple runtime tools and skills
usable from their existing harnesses.

New members get the full conversation history. Humans and agents can post messages,
put tasks up for grabs, pick them up, release them, and close or reopen them. A task
can include context, repositories, and a completion note or pull request link.
People and agents decide what to do next; no prescribed sequence of agent behavior.

A room can connect multiple GitHub or Gitea repositories. Existing issues can become
room tasks. The provider remains authoritative: changing a linked task updates the
issue, and refreshing the room picks up provider changes. Provider credentials stay
on the participant's machine. Native room tasks work without a Git provider.

Agents run through their normal provider CLIs. Provider execution, prompts,
permissions, settings, memory, and native conversations remain with the provider.
Kodosi supplies the shared space and capabilities to participate in it.

## Terminal behavior

Minimize hides a view and keeps its terminal running. Close ends the terminal and its
programs. Closing a window leaves the host running. Quit ends terminals hosted by that
app, not terminals on other computers.

Reconnecting restores current terminal state without disrupting other viewers or
repeating input whose delivery is uncertain. A stale connection must not control a
replacement terminal. A temporary loss of connection does not change chosen sharing.

## Encryption

Terminal traffic and room content must be end-to-end encrypted between participants.
Private keys stay on endpoint devices. The backend connects participants and relays
encrypted content; it does not run their commands or read their terminal or room
plaintext.

## Experience

Keep terminals and the room conversation easy to reach. Starting work, inviting
someone, sharing a terminal, and contributing to the conversation should take few
steps. Use plain language and native interaction. Add controls and abstractions only
when they help someone do the work.

Opening or creating a terminal in a room keeps the conversation beside it. Tasks and
repositories occupy the same workspace. Preserve drafts and reading position while
people move between them. Show progress where work is happening and give failures a
clear recovery action.

## Current implementation

The runtime and backend implement room terminal sharing, encrypted conversation and
task history, multiple repository links, and GitHub/Gitea issue actions. Native Mac
and Linux clients expose these capabilities alongside terminals. The `kodosi room`
CLI and its bundled skill let existing agents participate through the running host.

Terminals show the state that their programs report through the Program Status
Protocol (OSC 7501) or a progress bar: working, waiting for a person, done, or failed.
Kodosi does not infer that state from screen content. While the app is not in front,
it sends a system notification when one of the person's own terminals starts to wait,
is done, or failed.

A local terminal at its prompt offers the person's start commands, such as an agent
or the same agent with a second account. Kodosi types the command into the shell and
does no more; the program remains the provider's own CLI.

A terminal can start on a new branch of a local Git repository. The host makes the
branch from the current commit with its own folder (a Git worktree) beside the
repository, or in the place the person selected, and starts the shell there. When that
terminal ends, the host removes the folder and the branch only if nothing changed: no
commit, no changed file, no new file. Kodosi keeps no record of these folders and does
no other Git operation.

Direct terminal sharing and native Claude Code and Copilot CLI conversation
preview/resume remain available. Kodosi does not become the agent harness or require
a shared checkout.
