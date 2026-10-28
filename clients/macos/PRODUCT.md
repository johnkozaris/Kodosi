# Kodosi for macOS

Shared intent: [PRODUCT.md](../../PRODUCT.md). Visual system: [DESIGN.md](DESIGN.md). The
Linux and Windows app follows this app.

## Terminals

- New Terminal (⌘N) opens the launcher, the terminal about to open: a tab for each agent
  after the plain terminal, then one line with the folder, the account, the model with
  its effort, and the first prompt. With nothing open it sits in the middle of the main
  area; otherwise it is a sheet. Return on the Terminal tab opens a plain shell.
- When another of your computers runs Kodosi, the launcher's band names the computer.
  Another computer shows its own agents, choices, recent folders and home folders. The
  launcher keeps the last computer; Resume stays on this Mac.
- The account key in the launcher shows how much of the account's plan limit is used.
  Provider Usage in the account menu shows each limit of each signed-in account, and a
  click on an account opens the launcher with it.
- The sidebar groups terminals by folder. A folder on another computer ends with that
  computer's name, and a friend's folder with the friend's face. A friend's terminal that
  you have not opened has no folder yet and stays under the friend's name. Pinned
  terminals stay at the top. A repository folder offers a terminal on a new branch. Go
  to (⌘K) finds terminals, rooms, people and actions. A terminal is renamed in place.
- Drag a terminal by its header onto another terminal to take its place in the grid, or
  drag it from the sidebar onto a terminal to open it there.
- A terminal's mark shows its program's state, and a waiting program shows what it needs
  beside the mark, also on a closed folder. Done, failed and a bell keep a sign until you
  look. Faces show who else has a terminal open.
- Hidden terminals keep their place and take no input. Sizing is automatic, and keys
  such as Ctrl-C keep their shell meaning.
- One share sheet puts a terminal in a room or shares it with friends. Dragging your
  terminal onto a room asks first, and its tab in the room removes it again.

## Rooms

The conversation stays beside the room's terminals, tasks and repositories. "@"
mentions a person or a terminal, and a terminal mention opens it. A message can become a
task. Hidden, the conversation folds into a key in the header that shows who wrote last
and how many messages are new.

Linking a repository lists the checkouts of your terminals and your GitHub repositories,
or takes a pasted address; Connect GitHub opens a terminal for `gh auth login`. The Tasks
tab adds the newest open issues of the room's repositories, or any issue by its number,
and a new task can open an issue.

## Friends

Before you add or accept a friend, a sheet stops you: a friend can type in each terminal
that you share with them, so add only a person that you trust completely.

## Dosi

Dosi opens from its line at the bottom of the sidebar or with ⌘J, and the sidebar widens
for it, with the terminals and rooms as chips above the conversation. Esc returns to the
list, ⌘. stops Dosi, and New chat asks first. A question waits in the conversation with a
button for each answer and a field for your own words. A question about a terminal also
shows its short answers on that terminal's header, and an answer there types into the
terminal; your own words go to Dosi. Settings › Dosi chooses its agent, account, model
and effort, which Dosi uses from the next message, edits its instructions and shows its
folder. Turned off there, Dosi leaves the sidebar, the rail and the terminal tiles of
this computer.

## Providers

Preview saved Claude Code and Copilot CLI conversations without changing them. Resume
starts a new terminal through the provider's own CLI. Settings opens provider settings
without editing them.
