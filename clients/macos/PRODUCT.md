# Kodosi for macOS

[PRODUCT.md](../../PRODUCT.md) has the shared intent and [DESIGN.md](DESIGN.md) the
visual system. This app is the reference for the desktop: the Linux and Windows app
follows it.

- **One window.** A sidebar lists the terminals by computer and folder, then the
  terminals of friends, and the projects. The stage beside it shows the open terminals
  as a grid or one at a time. Go to finds every terminal, project, person and action
  from the keyboard.
- **The launcher is the terminal about to open.** It chooses the computer, the folder,
  the agent with its account and choices, and a first prompt. Quick Terminal and Same
  Agent open a terminal without it.
- **A terminal's header holds its actions.** They start an agent or change the running
  one, share the terminal, open its canvas and rename it.
- **The canvas opens beside its terminal.** It can take the stage while the terminal
  stays live in its corner.
- **A project keeps its conversation beside its work:** its terminals, tasks and
  repositories.
- **Dosi lives in the sidebar,** which widens for the conversation. A question about a
  terminal also shows on that terminal.
- **Keys.** The app's keys are Ghostty's for the actions that both have, and Settings
  changes each one. Every other key goes to the program in the terminal.
- **Permissions.** The programs in a terminal have the macOS permissions that the person
  gives Kodosi, as in any terminal app. Settings shows them and asks macOS for them.
- **Provider conversations.** The app previews an agent's saved conversations without
  changing them, and resumes one in a new terminal through the provider's own CLI.
- **Trust.** Before a person adds or accepts a friend, the app asks them to stop and
  read, because a friend can type in each terminal that is shared with them.
