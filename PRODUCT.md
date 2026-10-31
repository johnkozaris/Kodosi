# Kodosi

Kodosi is a native place for people and their coding agents to work together through
shared terminals and a room conversation. Keep it lean. Make useful actions available
and let people and agents decide how to use them.

## Terminals and rooms

The interface calls the shared spaces rooms; runtime contracts call them missions.

- A terminal is a real process on the computer that hosts it. Full control means typing,
  resizing, interrupting and closing, with the host user's operating-system privileges.
- A person's approved devices can use all of that person's terminals.
- A person can share a terminal with friends or with a room, and everyone it is shared
  with gets full control. Room sharing follows the room's membership, including people invited
  later. Sharing a terminal shares nothing else of its computer.
- A room holds a conversation, tasks, repositories and terminals from many computers,
  folders and repositories. Every member can share their own terminals into it, and new
  members get the full history.
- Anyone in a room can post, put tasks up for grabs, claim, release, close and reopen
  them. A task can carry context, repositories and a result. People and agents decide
  what to do next.
- A room can link GitHub or Gitea repositories and turn their issues into tasks, and a
  new task can open an issue. The provider stays authoritative, so changing a linked task
  updates the issue. Provider credentials stay on the participant's computer, and tasks
  need no Git provider. A person picks repositories from the checkouts of their terminals
  and from the GitHub account that `gh` uses.

## People and agents

People and agents read and write the same room conversation. Agents take part through
the `kodosi` command and its skill, when they choose to.

Agents run through their own CLIs. Execution, prompts, permissions, settings, memory and
conversations stay with the provider. When Kodosi starts Claude Code, Codex or Copilot
CLI, it adds only a short note: the agent runs in Kodosi with people and other agents,
messages can come, and it decides with its own judgment and its current work what to do
with them.

A room message that names a terminal with `@` goes to the agent in that terminal at once,
also while it works, and an agent can send a message to the agent of another terminal
with `kodosi terminal message`. Kodosi types each message as one line that says who wrote
it and from where. It never types while the agent waits for a person's answer or while a
person types there. Nothing else from a room goes into a terminal.

Dosi is the person's agent for their other agents. It sees every terminal, agent and room
of the person, acts on them with the `kodosi` command, shows them in the app and asks
with choices. It runs on the computer where the person writes, with the agent, account,
model and effort chosen in Settings, and works in its own folder. One encrypted
conversation follows the person to all their computers.

## Terminal behavior

- Terminals show the state that their programs report: working, waiting for a person,
  done or failed. Everyone who can open a terminal sees its state and who has it open.
  While the app is in the background, a terminal that the person owns or has open sends a
  notification when it starts to wait, finishes or fails.
- Minimize hides a view and keeps its terminal running. Close ends the terminal and its
  programs. Closing the window leaves the app and the host running, with Kodosi's icon in
  the menu bar or the system tray; on a desktop without a system tray, it quits. Settings
  can change what closing the window does.
- Quit stops the computer's host and its terminals, but not a host that runs in a
  terminal with `kodosi host`. When the app opens again, the host starts those terminals
  again with their names, folders, rooms and sharing, and with their earlier screens and
  history above a new prompt. Agents continue their conversations. An open view of such
  a terminal on another device moves to it. When a terminal ends by itself, no other
  terminal takes its keyboard focus. The `kodosi` command starts a host but never the
  earlier terminals.
- The apps and the `kodosi` command are equal clients of the computer's host. What one
  does, the others show. A person's typing comes first, so input from the command stops
  with a message while a person types in that terminal.
- Reconnecting restores the current state without disturbing other viewers or repeating
  input whose delivery is uncertain. A stale connection never controls a replacement
  terminal. A lost connection does not change sharing.
- When the computer of a terminal is offline, the terminal says so, keeps its last screen,
  takes no input and opens again when that computer is back. The person can hide it from
  their own lists until then, or close it for everyone when the computer is back. A room
  still shows its hidden terminals.
- Programs get what Ghostty gives them, such as Kitty images and clipboard writes. A
  clipboard read asks first only in another person's terminal.
- Any program can show a link, so a link opens by Ghostty's rules. A web or mail link
  opens at once. A file or folder opens, but not a file that can run a program. A link to
  another app asks first. A link with hidden characters or an incomplete address does not
  open; the app tells why and can copy the link.
- A link opens on the device where the person selects it. In a terminal of another
  computer, a file or a local address such as `localhost` is on that computer, so it does
  not open; the app names that computer.

## Canvas

- Each terminal has a canvas with two parts: its files and its shared browser. Its agent
  puts files there with `kodosi show`: a web page, Markdown or text, a picture, a PDF or
  an Office file. The canvas shows the newest first and keeps 20. When a file changes,
  each device shows the new version.
- The terminal has a key for each part that has something, with a dot for news, and the
  files key names a file for a moment when the agent shows or changes it. A key opens
  the canvas at its part. An open canvas follows the newest file until the person picks
  one. A person closes a part on their device until it changes, and the plus key opens
  it again or opens the browser. Each device keeps its own state of the keys and the
  canvas.
- A page runs its scripts in a closed frame. It reaches no network, no files and no
  storage that stays. A link in a page opens on the device by the link rules.
- A terminal can have one shared browser. The agent uses it with `kodosi browser` and
  agent-browser's commands, and the people see it on the canvas and click and type in it,
  with the same sign-ins. It has one size: the view where a person used it last. When a
  person used it, the agent's next browser command says so. It runs the Chrome, Chromium,
  Edge or Brave of the terminal's computer, sleeps after 10 minutes with no use and
  nobody looking, and opens its last page again when it wakes. While the agent acts in
  it, a pointer with the agent's mark shows where.
- Ask about this types the item and the selected words into the terminal, and the person
  writes the question.
- Everyone who can open the terminal sees its canvas and uses its browser. Closing the
  terminal removes its canvas and deletes its browser's sign-ins.

## Starting work

- New terminal opens the launcher. The host finds the installed agents (Claude Code,
  Codex, Copilot CLI, Cursor, Gemini CLI and OpenCode) with their accounts, models and
  efforts, and keeps the last choice, so the apps and `kodosi terminal start --agent`
  start the same way. Kodosi types the agent's command into the shell of the new terminal
  and does no more.
- Quick terminal skips the launcher and opens a plain terminal on this computer at once:
  in the folder of the selected terminal, or else in the launcher's folder. What the
  person types while it opens goes into it.
- The app's keys are Ghostty's for the actions that Kodosi has too, plus Kodosi's own, and
  Settings changes each of them. Any other key goes to the program in the terminal.
- The launcher can also start the terminal on another of the person's computers where
  Kodosi runs.
- A terminal can start on a new branch in its own Git worktree, which the host removes
  when the terminal ends if nothing changed.
- The launcher shows how much of its plan limits each Claude Code, Codex and Copilot CLI
  account has used. The agent's own program reads the numbers when the person looks;
  Kodosi reads no credentials, and the numbers do not go to the backend.
- A person can preview saved Claude Code and Copilot CLI conversations and resume them
  through the provider's own CLI.

## Encryption

The participants' devices encrypt terminal traffic and room content end to end.
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

## The apps

- The Mac app and the Linux and Windows app run terminals on the computer's host and take
  part in rooms. The iPhone and iPad app follows, answers and controls the terminals of
  the person's computers and friends, takes part in rooms, and starts no terminals.
- Each desktop app contains the `kodosi` command, so that people and agents can do from a
  shell what the apps do.
- Apps update themselves in the background and never restart without asking, because a
  restart stops the host's terminals. An app too old for the backend asks for an update.
- Until an approved device of the person approves it, a new device says so and shares
  nothing. The person's approved
  computers show its request.
- A server can define plans that limit an account's devices, rooms, people, friends and
  room data. A full room loses its oldest messages first; tasks and repositories stay.
- A person deletes their account through the sign-in service's account page. What they
  wrote in other rooms stays, and their terminals keep running.
