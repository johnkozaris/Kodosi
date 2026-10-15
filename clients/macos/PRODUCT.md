# Kodosi for macOS

Shared intent: [PRODUCT.md](../../PRODUCT.md). Visual system: [DESIGN.md](DESIGN.md).

## Terminals

- New Terminal opens a shell at once. The sidebar groups terminals by folder and names
  the computer of a remote terminal. With nothing open, the main area shows the launcher
  and then every terminal by computer. Go to (⌘K) finds terminals, rooms, people and
  actions. A terminal is renamed in place.
- A terminal carries its program's mark, which shows the program's state. A waiting
  program gets a sign of what it needs beside its mark, and a closed folder shows that
  sign too. Done, failed and a bell keep a sign until you look. Faces show who else has
  a terminal open, the person at its computer included. While Kodosi is not in front,
  the terminals that you own or have open send a system notification.
- The launcher is the terminal about to open: a tab for each agent after the plain
  terminal, then a prompt with the folder, account, model, effort and first prompt.
- A repository folder in the sidebar offers a terminal on a new branch.
- Hidden terminals keep their place and take no input. Sizing is automatic, and keys
  such as Ctrl-C keep their shell meaning. Terminals stay dark in the light appearance.
- One share sheet puts a terminal in a room or shares it with friends.

## Rooms

The conversation stays beside the room's terminals, tasks and repositories. "@"
mentions a person or a terminal, and a terminal mention opens it. A message can become a
task. Drafts and reading position survive switching rooms.

## Providers

Preview saved Claude Code and Copilot CLI conversations without changing them. Resume
starts a new terminal through the provider's own CLI. Kodosi opens provider settings
without editing them.

## Experience

Keep the terminal central and the actions clear, and show objects before explanations.
Motion follows an action or a real change of state and never moves the terminal
surface. Native focus, accessibility, localization and reduced motion matter.
