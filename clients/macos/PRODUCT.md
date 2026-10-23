# Kodosi for macOS

Shared intent: [PRODUCT.md](../../PRODUCT.md). Visual system: [DESIGN.md](DESIGN.md).

## Terminals

- New Terminal opens the launcher, the terminal about to open: a tab for each agent
  after the plain terminal, then one line with the folder, the account, the model with
  its effort, and the first prompt. With nothing open it sits in the middle of the main
  area; otherwise it is a sheet. Return on the Terminal tab opens a plain shell.
- A ring on the account key shows how much of the account's plan limit is used, and the
  account list shows the percent. Provider Usage in the sidebar menu opens a panel beside the
  menu with a dial for each limit of each signed-in account. A click on an account opens
  the launcher with it.
- When another of your computers runs Kodosi, the launcher's band names the computer.
  Choosing another computer shows its agents and choices, and the folder key lists its
  recent folders and starts in the newest one. Choose a folder… opens its home: a click
  opens a folder, the arrow row goes up, and Use takes the folder. The launcher keeps the
  last computer. Resume stays on this Mac.
- The sidebar groups terminals by folder and names the computer of a remote terminal. A
  repository folder offers a terminal on a new branch. Go to (⌘K) finds terminals, rooms,
  people and actions. A terminal is renamed in place.
- A terminal's mark shows its program's state; a waiting program shows what it needs
  beside the mark, also on a closed folder. Done, failed and a bell keep a sign until you
  look. Faces show who else has a terminal open.
- Hidden terminals keep their place and take no input. Sizing is automatic, and keys
  such as Ctrl-C keep their shell meaning. Terminals stay dark in the light appearance.
- One share sheet puts a terminal in a room or shares it with friends.

## Rooms

The conversation stays beside the room's terminals, tasks and repositories. "@"
mentions a person or a terminal, and a terminal mention opens it. A message can become a
task. Drafts and reading position survive switching rooms.

## Dosi

Dosi opens from its line at the bottom of the sidebar or with ⌘J, and the sidebar
widens for it. Terminal and room rows fold into named chips above the conversation,
terminals on one line and rooms on the next; when a line is full, a chip with the count
of the rest lists them. The line becomes the composer; New chat sits in the header and
asks first. The back button or Esc returns to the list, and ⌘. stops Dosi. While Dosi
works, its line shows the marks of what it touches, and a terminal that Dosi types in
shows what it typed. Until Dosi shows its next step or words, the conversation says that
it is thinking. A question waits in the conversation with a button for each answer and
a field for your own words, and the line says that Dosi has a question. A question about
a terminal also shows its short answers on the terminal's header, and an answer types
into that terminal; your own words go to Dosi instead. A collapsed sidebar keeps Dosi at
the bottom of the rail.
Settings › Dosi chooses its agent, account, model and effort, edits its instructions and
shows its folder. Each agent keeps its own account, model and effort. A choice is saved
at once, and Dosi uses it from the next message. Turned off there, Dosi leaves the
sidebar, the rail and the terminal tiles of this computer, and ⌘J does nothing.

## Providers

Preview saved Claude Code and Copilot CLI conversations without changing them. Resume
starts a new terminal through the provider's own CLI. Settings opens provider settings
without editing them.
