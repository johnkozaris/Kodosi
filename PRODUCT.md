# Kodosi

Kodosi is a native place for people and their coding agents to work together through
shared terminals and a project conversation. Keep it lean. Make useful actions available
and let people and agents decide how to use them.

## Terminals and projects

The interface calls the shared spaces projects; the code and runtime contracts call them
rooms and missions.

- A terminal is a real process on the computer that hosts it. Full control means typing,
  resizing, interrupting and closing, with the host user's operating-system privileges.
- A person's approved devices can use all of that person's terminals.
- A person can share a terminal with friends or with a project, and everyone it is
  shared with gets full control. A terminal in a project goes to the members who are
  friends of its owner, including friends invited later. A shared terminal runs as its
  owner, with the owner's `kodosi` command and files on that computer.
- Removing a friend ends what the two share: each loses the terminals of the other and
  leaves the projects that the other owns. Blocking a person also does this, and the
  blocked person cannot send friend requests until the block ends.
- A project holds a conversation, tasks, repositories and terminals from many computers,
  folders and repositories. A person can work in it alone and invite friends when they
  want. Every member can share their own terminals into it, and new members get the full
  history.
- Anyone in a project can post, put tasks up for grabs, claim, release, close and reopen
  them. A task can carry context, repositories and a result. People and agents decide
  what to do next.
- A project can link GitHub or Gitea repositories and turn their issues into tasks, and a
  new task can open an issue. The provider stays authoritative, so changing a linked task
  updates the issue. Provider credentials stay on the participant's computer, and tasks
  need no Git provider. A person picks repositories from the checkouts of their terminals
  and from the GitHub account that `gh` uses.

## People and agents

People and agents read and write the same project conversation. Agents take part through
the `kodosi` command and its skill, when they choose to.

Agents run through their own CLIs. Execution, prompts, permissions, settings, memory and
conversations stay with the provider. When Kodosi starts Claude Code, Codex or Copilot
CLI, it adds only a short note: the agent runs in Kodosi with people and other agents,
messages can come, it decides with its own judgment and its current work what to do with
them, and it shows what it makes to look at on the terminal's canvas. Claude Code and
Copilot CLI get the note and Kodosi's skills also when a person starts them in a terminal
by hand. Codex gets them only from Kodosi, so a desktop app says so on a terminal where a
person started Codex by hand.

A project message that names a terminal with `@` goes to the agent in that terminal at
once, also while it works, and an agent can send a message to the agent of another
terminal with `kodosi terminal message`. Kodosi types each message as one line that says
who wrote it and from where. It never types while the agent waits for a person's answer or
while a person types there. Nothing else from a project goes into a terminal.

Dosi is the person's agent for their other agents. It sees every terminal, agent and
project of the person, acts on them with the `kodosi` command, shows them in the app and
asks with choices. It runs on the computer where the person writes, with the agent,
account, model and effort chosen in Settings, and works in its own folder. One encrypted
conversation follows the person to all their devices. On a phone, the person writes to
Dosi on one of their computers, at first the one that answered last, and that computer
answers. A message for a computer that is away waits until it runs Kodosi again, or until
the person takes it back or starts a new chat. Any device answers Dosi's questions, and
Dosi goes on on the computer that asked. A computer where Dosi is off answers a message
from another device with a note that Dosi is off there.

## Terminal behavior

- Terminals show the state that their programs report: working, waiting for a person,
  done or failed. Everyone who can open a terminal sees its state and who has it open.
  While the app is in the background, a terminal that the person owns or has open sends a
  notification when it starts to wait, finishes or fails. On an iPhone or iPad, the
  person's own terminals send it also while Kodosi does not run, when the terminal still
  waits, finished or failed 15 seconds later, and a tap on it opens the terminal.
- Minimize hides a view and keeps its terminal running. Close ends the terminal and its
  programs. Closing the window leaves the app and the host running, with Kodosi's icon in
  the menu bar or the system tray; on a desktop without a system tray, it quits. Settings
  can change what closing the window does.
- A terminal of another computer comes to a device only while it shows there: on the stage
  in the grid, alone in single view, with its canvas or in its project, in a window that
  is on the screen. When it shows again, it keeps its last screen until the current one
  comes, and what the person types waits for it.
- Quit stops the computer's host and its terminals, but not a host that runs in a terminal
  with `kodosi host`. When the app opens again, the host starts those terminals again with
  their names, folders, projects and sharing, and with their earlier screens and history
  above a new prompt. Agents continue their conversations. An open view of such a terminal
  on another device moves to it. When a terminal ends by itself, no other terminal takes
  its keyboard focus. The `kodosi` command starts a host but never the earlier terminals.
- The apps and the `kodosi` command are equal clients of the computer's host. What one
  does, the others show. A person's typing comes first, so input from the command stops
  with a message while a person types in that terminal.
- Reconnecting restores the current state without disturbing other viewers or repeating
  input whose delivery is uncertain. A stale connection never controls a replacement
  terminal. A lost connection does not change sharing. After a short drop, such as when
  the computer wakes, the terminals connect again by themselves, and what the person
  typed or asked during the drop goes out once when the connection is back. A terminal
  that cannot connect says so on its tile, not in a message that stays after the
  connection is back.
- When the device cannot reach the server for longer than a short drop, Kodosi is offline.
  The terminals of the device keep working and say nothing of it. A terminal of another
  computer says Offline, keeps its last screen and has Try again, which connects at once,
  and so does the launcher where it would start a terminal on another computer. Input that
  Kodosi did not send shows on its terminal until new input goes through, not in messages.
- When the computer of a terminal is offline, the terminal says so, keeps its last screen,
  takes no input and opens again when that computer is back. The person can hide it from
  their own lists until then, or close it for everyone when the computer is back. A
  project still shows its hidden terminals.
- Programs get what Ghostty gives them, such as Kitty images and clipboard writes. A
  clipboard read asks first only in another person's terminal.
- A pasted picture goes to the computer of the terminal as a file, and the terminal gets
  the path of the file as a paste, so an agent attaches it. While a picture goes to another
  computer, its terminal shows how much arrived, and pasting it again sends nothing more.
- Any program can show a link, so a link opens by Ghostty's rules. A web or mail link
  opens at once. A file or folder opens, but not a file that can run a program. A link to
  another app asks first. A link with hidden characters or an incomplete address does not
  open; the app tells why and can copy the link.
- A link opens on the device where the person selects it, in the person's own browser or
  app. Only agents open pages in the shared browser. In a terminal of another computer, a
  file or a local address such as `localhost` is on that computer, so it does not open;
  the app names that computer.

## Canvas

- Each terminal has a canvas with two parts: its files and its shared browser. Its agent
  shows what it makes there with `kodosi show`: a web page, Markdown or text, a picture, a
  PDF, a document, a sheet or a presentation, or an address, such as the app that it runs,
  in the shared browser. A page that loads from the network opens in the shared browser too.
  The canvas then opens at it on each device where the terminal is in front, or when the
  person comes to the terminal, once, and only for a show after the app there started. It
  opens the same way when the agent uses the shared browser, or a file on the canvas
  changes, after a minute without that, so a canvas that a person closed stays closed while
  the agent goes on. A change that people make in a live document does not open it. A person
  adds a file with Add a File or a drop, on any device: it goes into the terminal's folder on
  the terminal's computer, a taken name gets a number, and the limit is 32 MB. The canvas
  shows the newest first and keeps 20. When a file changes, each device shows the new
  version. Save a Copy keeps a copy on the device.
- A document (.docx), sheet (.xlsx), presentation (.pptx), PDF or text file (.txt, .json,
  .yaml, .yml, .toml or .xml) on the canvas is live. The people and agents of the terminal
  edit it at the same time, each sees where the others are, and each edit reaches all of
  them at once. The file stays a normal file on the terminal's computer, and Kodosi saves
  it there a moment after each change. A change that another program saves merges in. When
  the computer is away, a person goes on, and the edits wait on the device. A person
  accepts or rejects the tracked changes of a document, one by one or all at once. A text
  file of more than 10,000 lines or 512 KB, or one that is not UTF-8, is a page.
- Settings › Documents turns each viewer on or off on the device: the document, sheet,
  presentation, PDF and text viewer. A viewer that is off shows a preview, or Save a Copy
  where the app has no preview. Documents show as pages or as text that fits the canvas,
  and documents, sheets and PDFs can go dark with the app.
- The terminal has a key for each part that has something, with a dot for news, and the
  files key names a file for a moment when the agent shows or changes it. A key opens
  the canvas at its part. An open canvas follows the newest file until the person picks
  one. A person closes a part on their device until it changes, and the plus key opens
  it again or opens the browser. Each device keeps its own state of the keys and the
  canvas.
- A page runs its scripts in a closed frame. It reaches no network, no files and no
  storage that stays. A link in a page, document, sheet, presentation or PDF opens on the
  device by the link rules. Where the person can edit, a link opens with ⌘-click
  (Ctrl-click on Linux and Windows); elsewhere, and on a touch screen, a click or a tap
  opens it. A link to a slide or to a cell of the same file goes there.
- A terminal can have one shared browser. The agent uses it with `kodosi browser` and
  agent-browser's commands, and the people see it on the canvas as live video and click
  and type in it, each with a named pointer. Words in its address bar that are no address
  search with Google. It has one size: the view where a person used it last. When a
  person used it, the agent's next browser command says so. It runs Kodosi Browser, a
  Chromium that Kodosi gets the first time, or when a person gets it in Settings ›
  Browser, and keeps up to date. It never shows a window or an icon on the computer, and
  it blocks ads and trackers. It sleeps after 10 minutes with no use and nobody looking,
  and opens its last page again when it wakes. While the agent acts in it, a pointer with
  the agent's mark shows where. A download lands in the terminal's folder and on the
  canvas.
- A person keeps one set of sign-ins on each computer, and all their private terminals
  use it. Settings › Browser shows and gets the computer's Kodosi Browser, copies the
  sign-ins of another browser on that computer, says for how many sites, and removes what
  it copied or all browsing data of the private terminals. A shared terminal has a
  browser profile of its own, so its shared browser has no kept sign-in.
- Ask about this types the item and the selected words into the terminal, and the person
  writes the question.
- Everyone who can open the terminal sees its canvas, edits its files and uses its
  browser. Closing the terminal removes its canvas and leaves its files where they are; a
  shared terminal's browser profile goes with it.

## Starting work

- New terminal opens the launcher. The host finds the installed agents (Claude Code,
  Codex, Copilot CLI, Cursor, Gemini CLI and OpenCode) with their accounts, models and
  efforts, and keeps the last choice, so the apps and `kodosi terminal start --agent`
  start the same way. Kodosi types the agent's command into the shell of the new terminal
  and does no more.
- The launcher also chooses how the agent asks before it acts, with the agent's own flags:
  Ask; Auto, where the agent's own safety check approves what is safe; or Unrestricted,
  where it never asks. For Claude Code and Codex it also chooses how much of their work they
  show: Quiet, Standard or Detailed. It starts at Unrestricted, or Ask for an agent without
  it, and at Quiet, keeps the last choice of each agent, and a terminal that starts again
  keeps them.
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
- Each Claude Code, Codex and Copilot CLI account of a computer has its own shade of the
  agent's color, in the order of the launcher. A terminal's mark has the shade of the
  account that its agent runs under, everywhere the terminal shows, and the account in the
  launcher has the same shade. The host tells only the place of the account, not its name or
  folder. A Windows computer cannot see the account of a running agent, so its terminals keep
  the agent's own color.
- A person can preview saved Claude Code and Copilot CLI conversations and resume them
  through the provider's own CLI.

## Encryption

The participants' devices encrypt terminal traffic and project content end to end.
Private keys stay on devices. The backend connects participants and relays encrypted
content; it does not run commands or read terminal or project plaintext. It sees only each
terminal's agent and reported state, such as working or waiting for an approval.

A recovery key approves a new device when no other device can, and keeps friends and
projects. Without it, a person who lost all devices starts fresh with a new identity. They
keep their friends, not verified, and the friends trust that identity again. Starting
fresh on an account that has devices waits a day; those devices show it and can stop it.

## Experience

Keep terminals and the project conversation easy to reach. Starting work, inviting
someone, sharing a terminal and writing in the conversation take few steps. Use plain
language and native interaction. Add controls only when they help someone do the work.

Visible text uses one word for one thing: terminal, project, friend, agent, device.
Session, host, runtime, room and mission are internal words.

Kodosi speaks English, German, Greek, Spanish, French, Italian, Japanese, Korean and
Simplified Chinese. Each app follows the language of the system, and its settings can
choose another; a new choice applies when the app opens again. The sign-in pages open in
the language of the app. The `kodosi` command, the logs and the details of an internal
fault stay English.

A terminal opened in a project keeps the conversation beside it. Tasks and repositories
share that workspace. Drafts and reading position survive moving between them. Show
progress where work happens and give each failure a clear recovery action.

## The apps

- The Mac app and the Linux and Windows app run terminals on the computer's host and take
  part in projects. The iPhone and iPad app follows, answers and controls the terminals of
  the person's computers and friends, starts terminals on the person's computers, and
  takes part in projects.
- Each desktop app contains the `kodosi` command, so that people and agents can do from a
  shell what the apps do.
- Apps update themselves in the background and never restart without asking, because a
  restart stops the host's terminals. An app too old for the backend asks for an update,
  and an app that updates itself looks for the update at once and offers the restart.
- A new device shares nothing and says so until an approved device of the person approves
  it. The person's approved computers show its request.
- A server can define plans that limit an account's devices, projects, people, friends
  and project data. A full project loses its oldest messages first; tasks and repositories
  stay.
- A server limits how many user names an account can try, because a user name tells
  whether its account exists, and bans an account that keeps trying. The apps of a banned
  account sign out and say why; its terminals keep running.
- A person deletes their account through the sign-in service's account page. What they
  wrote in other projects stays, and their terminals keep running.
