# Kodosi for macOS

See [PRODUCT.md](../../PRODUCT.md) for shared product decisions and [DESIGN.md](DESIGN.md) for
the visual system. The macOS app is the desktop reference client; the Linux and Windows app
matches its behavior.

## Window and navigation

The app lives in a single window. A sidebar lists your terminals grouped by computer and folder,
followed by shared terminals from friends, and then projects. The main stage renders open
terminals as a grid or one at a time. A keyboard command palette (**Go to**) jumps to any
terminal, project, person, or action.

## Starting and controlling terminals

The launcher represents the terminal about to open: choose the host computer, folder, agent
(along with its account and options), and an optional first prompt. Each agent remembers its last
options for the next launch, and you can edit those defaults in Settings. Quick Terminal, Same
Agent, and a folder's `+` button bypass the launcher to open a terminal immediately.

Each terminal's header holds its controls: start an agent or adjust the running one, share the
terminal, toggle its canvas, and rename it. A terminal on its own branch shows how many lines the
branch changed, and clicking the count shows the diff on the canvas. When a restart stopped its
agent in the middle of a task, the header offers to continue it.

A terminal supports both a terminal screen view and a chat view. It opens in whichever view its
agent works in, the header switches between them, the terminal screen stays live while chat is
shown, and you can switch the agent's model from either view. Pointing at a message or a finished
answer in the chat shows a key that copies it whole, as written.

In the chat view, the mic key lets you talk instead of typing. Your words appear as your next
message while you speak, and the message sends after a long pause, so you can stop to think; Send
sends it without waiting for the pause. While the agent asks a question, only Send answers it with
your words. Click the words to change them, or go back to typing: words that did not go out stay in
the composer. One chat listens at a time, so talking in another chat ends the first. Speech turns
into text on this Mac, so no audio leaves it, and replies stay text. A Mac that can't turn speech
into text in any of your languages shows no mic key.

## Canvas, projects, and Dosi

The canvas opens beside its terminal and can expand to take over the main stage while keeping the
terminal live in a small corner tile.

Each project puts its chat alongside its work: terminals, tasks, and linked repositories.

Dosi lives at the bottom of the sidebar, which widens to hold the conversation. If Dosi asks a
question about a specific terminal, that prompt also appears directly on the terminal.

## Shortcuts, permissions, and trust

App shortcuts match Ghostty's defaults for shared actions and are customizable in Settings. All
other keystrokes go directly to the program running in the terminal.

Programs running inside Kodosi inherit the macOS permissions granted to Kodosi, just like any
other terminal app. Settings displays permission status and triggers macOS permission prompts.

The app can preview an agent's saved provider conversations read-only without modifying them, and
resume any conversation in a new terminal via the provider's CLI.

Kodosi asks before someone becomes a friend, since friends get full interactive control over any
terminal you share with them, and before any other change lets someone new into your terminals or
projects. The question appears as an alert on Kodosi's front window, or on its own while no window
is open. While you work in another app, Kodosi bounces its Dock icon and waits instead of taking
the focus.
