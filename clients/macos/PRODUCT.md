# Kodosi for macOS

Shared intent: [PRODUCT.md](../../PRODUCT.md). Visual system: [DESIGN.md](DESIGN.md).

## Terminals

- New Terminal opens the launcher, the terminal about to open: a tab for each agent
  after the plain terminal, then one line with the folder, the account, the model with
  its effort, and the first prompt. With nothing open it sits in the middle of the main
  area; otherwise it is a sheet. Return on the Terminal tab opens a plain shell.
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

## Providers

Preview saved Claude Code and Copilot CLI conversations without changing them. Resume
starts a new terminal through the provider's own CLI. Settings opens provider settings
without editing them.

## Experience

Keep the terminal central and the actions clear, and show objects before explanations.
Motion follows an action or a real change of state and never moves the terminal
surface. Focus, accessibility, localization and reduced motion matter; Settings can
reduce motion when the system does not.
