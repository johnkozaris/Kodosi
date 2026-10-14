# Kodosi for macOS

Shared product intent and current behavior: [Kodosi's PRODUCT.md](../../PRODUCT.md).
Visual system: [DESIGN.md](DESIGN.md).

## Words

Visible text uses one word for one thing: terminal, room, friend, agent, device.
Resume is the word for saved provider conversations. Session, host, runtime, and
mission are internal words.

## Terminals

New Terminal opens a shell right away. The source list groups terminals by working
folder and shows which computer runs a remote terminal. With no terminal open, the
main area starts with the launcher, then shows every terminal by computer. Go to (⌘K)
finds a terminal, a room, a person, or an action. Rename a terminal in place.

A terminal carries the mark of the program in it. While an agent works, its mark and
title show it. A program that waits for you puts a sign of what it needs beside its
mark, and a closed folder shows that sign too. A terminal that is done, that failed,
or that rang its bell keeps a sign until you look at it. Faces show who else has a
terminal open, the person at its computer included. While Kodosi is not in front,
the terminals that you own or have open send a system notification for these changes.

A terminal at its prompt offers the start commands of this computer as marks among its
actions. Kodosi adds a command for an agent the first time that the agent runs here.
Settings holds the list, so a second account is one more command.

The launcher is the terminal about to open. Its band has a tab for each agent of this
Mac, with the plain terminal first; an agent that this Mac just got says so in its tab.
Below the band is a prompt with the folder and the command. Each part of the command
opens its list in the terminal, under the line. The folder shows only its name. The
account list groups the commands by the sign-in that they use. The model and the effort
are behind the dots; without them the agent uses its own. After the command, the person
types the agent's first prompt, or a command for a plain terminal; a faint suggestion
types itself there once, and Tab takes it. Start or Return opens the terminal, which
runs the line. An agent that is not here shows its install command, and a new account
signs in when the agent first starts.

The folder of a Git repository in the sidebar offers a terminal on a new branch. You
give the branch a name; it gets its own folder and a terminal starts there. A folder
with no changes goes away when its terminal ends.

Minimize hides a terminal but keeps it running. Close ends it and its programs.
Closing the window leaves Kodosi running; Quit ends terminals that run on this Mac,
not on other computers. Hidden terminals keep their place without accepting
accidental input. Sizing should feel automatic, and keys like Ctrl-C should
retain their shell meaning. Terminals stay dark in the light appearance.

One share sheet puts a terminal in a room or shares it with friends.

## Rooms

Make it easy to join a room, contribute a terminal, open someone else's shared
terminal, and participate in the room conversation. Sharing with the room gives its
members full terminal control, including members invited later. Keep the hosting
computer and working folder clear.

Conversation stays beside the room's terminals, tasks, or connected repositories.
Mention a person or a room terminal with "@"; a terminal mention opens that terminal.
A message can become a task. Keep drafts and reading position when switching rooms or
work. Agents use the shared runtime's room CLI and bundled skill.

## Providers

Preview saved Claude Code and Copilot CLI conversations without changing them.
Keep long histories readable.
Resume starts a new terminal through the provider's own CLI, with its usual
prompts and permissions. Kodosi opens original provider settings without
editing them.

## Mac experience

Keep the terminal central and the actions clear. Show the objects of the job, with
short text, before explanations. Use warm charcoal, cream, and copper. Motion follows
an action or a real change of state, and never moves the terminal surface itself.
Native focus, accessibility, localization, and reduced motion matter.
