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
terminal, toggle its canvas, and rename it.

A terminal supports both a terminal screen view and a chat view. It opens in whichever view its
agent works in, the header switches between them, the terminal screen stays live while chat is
shown, and you can switch the agent's model from either view.

## Canvas, projects, and Dosi

The canvas opens beside its terminal and can expand to take over the main stage while keeping the
terminal live in a small corner tile.

On a terminal of another of your Macs, the canvas can show that Mac's screen: pick a display or a
window, pinch to zoom, and point, click, and type there. The Mac's pointer shape becomes your own
while it is over the screen. App shortcuts stay with Kodosi, as in a terminal.

Each project puts its chat alongside its work: terminals, tasks, and linked repositories.

Dosi lives at the bottom of the sidebar, which widens to hold the conversation. If Dosi asks a
question about a specific terminal, that prompt also appears directly on the terminal.

## Shortcuts, permissions, and trust

App shortcuts match Ghostty's defaults for shared actions and are customizable in Settings. All
other keystrokes go directly to the program running in the terminal.

Programs running inside Kodosi inherit the macOS permissions granted to Kodosi, just like any
other terminal app. Settings displays permission status and triggers macOS permission prompts.
Sharing this Mac's screen with your other devices is off until you turn it on in Settings, and
Kodosi Screen, a helper inside the app, holds the permissions that it takes, so terminals do not
gain them.

The app can preview an agent's saved provider conversations read-only without modifying them, and
resume any conversation in a new terminal via the provider's CLI.

Before adding or accepting a friend, the app pauses to confirm you understand the trust model,
since friends get full interactive control over any terminal you share with them.
