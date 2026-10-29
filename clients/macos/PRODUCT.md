# Kodosi for macOS

[PRODUCT.md](../../PRODUCT.md) has the shared intent and [DESIGN.md](DESIGN.md) the
visual system. The Linux and Windows app follows this app.

## Terminals

- New Terminal (⌘N) opens the launcher, the terminal about to open: a tab for each agent
  after the plain terminal, then one line with the folder, the account, the model with
  its effort, and the first prompt. With nothing open it is in the middle of the main
  area; otherwise it is a sheet. Return on the Terminal tab opens a plain shell.
- When another of your computers runs Kodosi, the launcher's band names the computer.
  Another computer shows its own agents, choices, recent folders and home folders. The
  launcher keeps the last computer; Resume stays on this Mac.
- The account key in the launcher shows how much of its plan limit the account has used.
  Provider Usage in the account menu shows each limit of each signed-in account, and a
  click on an account opens the launcher with it.
- The sidebar groups terminals by folder, and by computer when they run on more than
  one: this Mac, then your other computers, then each friend with the friend's face. A
  friend's terminal that you have not opened has no folder yet and shows directly under
  the friend. The + of a folder on this Mac or on another computer of yours starts a
  terminal there. A small square on a terminal's mark has the color of its room, or is
  grey when the terminal is shared with people. Pinned terminals stay at the top. A
  repository folder offers a terminal on a new branch. Go to (⌘K) finds terminals,
  rooms, people and actions. You rename a terminal in place.
- Drag a terminal by its header onto another terminal to take its place in the grid, or
  drag it from the sidebar onto a terminal to open it there.
- A terminal's mark shows its program's state, and a waiting program shows what it needs
  beside the mark, also on a closed folder. Done, failed and a bell keep a sign until you
  look. Faces show who else has a terminal open.
- Hidden terminals keep their place and take no input. Sizing is automatic, and keys
  such as Ctrl-C keep their shell meaning.
- One share sheet puts a terminal in a room or shares it with friends. When you drag your
  terminal onto a room, Kodosi asks first. You remove it again from its tab in the room.

## Canvas

The canvas key in a terminal's header opens the canvas as a tile next to the terminal;
in a room, the canvas is a tab after the terminal's tab. Chips at its top pick an item,
and Remove from canvas in a chip's menu takes it off. Pictures, PDFs and Office files
show with Quick Look. Open in its app opens a file of this Mac, and Save a copy saves a
file of another computer. In the shared browser, the bar has back, forward, reload, the
address, the tabs and the last action of the agent, and the picture takes clicks,
scrolling and keys. Ask about this names the terminal's agent.

## Rooms

The conversation stays beside the room's terminals, tasks and repositories. "@"
mentions a person or a terminal, and a terminal mention opens it. A message can become a
task. When you hide the conversation, it folds into a key in the header that shows who
wrote last and how many messages are new.

To link a repository, pick the checkout of one of your terminals or one of your GitHub
repositories, or paste an address. Connect GitHub opens a terminal for `gh auth login`.
In the Tasks tab, you add the newest open issues of the room's repositories, or any issue
by its number, and a new task can open an issue.

## Friends

Before you add or accept a friend, a sheet asks you to stop and read. A friend can type in
each terminal that you share with them, so add only a person that you trust completely.

## Dosi

Dosi opens from its line at the bottom of the sidebar or with ⌘J, and the sidebar widens
for it, with the terminals and rooms as chips above the conversation. Esc returns to the
list, ⌘. stops Dosi, and New chat asks first. A question waits in the conversation with a
button for each answer and a field for your own words. A question about a terminal also
shows its short answers on that terminal's header, and an answer there types into the
terminal; your own words go to Dosi. Settings › Dosi chooses its agent, account, model
and effort, which Dosi uses from the next message, edits its instructions and shows its
folder. When you turn Dosi off there, it leaves the sidebar, the rail and the terminal
tiles of this computer.

## Providers

Preview saved Claude Code and Copilot CLI conversations without changing them. Resume
starts a new terminal through the provider's own CLI. Settings opens provider settings
without editing them.
