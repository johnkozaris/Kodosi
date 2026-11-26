# Getting started

**English** · [Deutsch](de/GETTING_STARTED.md) · [Ελληνικά](el/GETTING_STARTED.md) · [Español](es/GETTING_STARTED.md) · [Français](fr/GETTING_STARTED.md) · [Italiano](it/GETTING_STARTED.md) · [日本語](ja/GETTING_STARTED.md) · [한국어](ko/GETTING_STARTED.md) · [简体中文](zh-Hans/GETTING_STARTED.md)

[Download Kodosi](../README.md#get-kodosi) for your computer. Shortcuts below use macOS keys; on
Linux and Windows, replace ⌘ with Ctrl+Shift unless noted otherwise.

## 1. Sign in

Open Kodosi and click **Sign in** at the bottom of the sidebar. Kodosi opens your browser to sign
in. From a terminal, you can run `kodosi auth login` to get a device code you can enter in any
browser.

You can run local terminals without signing in. Sign in to connect your other computers, add
friends, and use projects.

## 2. Start an agent

Press ⌘N (or click **New terminal**) to open the launcher. Pick an agent and a folder, optionally
choose an account, model, and effort level, enter an initial prompt, and press Return. Kodosi
types the launch command into a new shell.

<p align="center"><img src="media/launcher.webp" width="580" alt="The launcher configured with Claude Code, a folder, and an initial prompt."></p>

If an agent isn't installed yet, the launcher shows its install command.

### Quick ways to open terminals

- Press ⌘D (Ctrl+Shift+O on Linux and Windows) for a plain shell in the selected terminal's folder.
- Press ⌘T (Ctrl+Shift+T) to open a new terminal using the selected terminal's agent, account, and
  settings.
- Click the `+` button next to a folder in the sidebar to start another terminal with the agent
  already running in that folder, using the same account and settings (or a plain shell if none
  is running there).
- In an idle terminal at a shell prompt, click the actions button (two sliders) in the header to
  launch any installed agent with your last-used settings.

### Keyboard shortcuts

Shortcuts match Ghostty defaults and can be customized in **Settings › Shortcuts**:

- ⌘1 through ⌘9 (Alt+1 through Alt+9 on Linux and Windows): switch to a terminal by position.
- ⌥⌘ + arrow keys (Ctrl+Alt + arrow keys): move to an adjacent terminal.
- ⌘W: close the selected terminal.
- ⌘↑ and ⌘↓: jump between shell prompts in zsh and modern bash (not macOS's built-in bash 3.2).

### Permissions, verbosity, and fast mode

- Agents start unrestricted by default, so they can edit files and run commands without prompting.
  Antigravity CLI is the exception: it starts in its own auto mode, which still asks before
  irreversible actions. To make an agent ask for permission first, select **Ask** under **More**
  (after the model picker).
- Under **More**, you can also set the output verbosity for Claude Code, Codex, Copilot CLI, and
  Antigravity CLI (they default to **Quiet**). Because Copilot CLI only reads verbosity from its
  saved settings, Kodosi temporarily updates Copilot's config while Copilot CLI runs in Quiet mode
  and restores it afterward.
- Claude Code and Codex support a fast mode (faster responses at higher cost), toggled alongside
  the model and effort pickers. Fast mode applies only to that launch and resets to off next time.

### Accounts and live switching

- Adding a new account gives it an isolated config folder, such as `~/.claude-work`. Kodosi also
  detects accounts defined via shell aliases, like `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`.
- While an agent is running, use the header's actions button (two sliders) to switch its model,
  effort, fast mode, and (for supported agents) permissions without restarting. Kodosi sends the
  agent's own slash command once the current turn finishes. Note that Claude Code remembers model,
  effort, and fast mode changes for future sessions too.

### Links, images, worktrees, and themes

Hold ⌘ and click a link (Ctrl-click on Linux and Windows) to open it in your default browser on the
device you're using; hold Shift as well if the running program captures the mouse. On iPhone, tap a
link (disabled while a program captures the mouse). Kodosi asks before handing a link to another
app, and blocks opening executable files as well as local file paths or `localhost` URLs from
remote computers.

To share a screenshot or image with an agent, paste it with ⌘V (Ctrl+Shift+V on Linux and Windows,
or Paste on iPhone). Kodosi saves it on the terminal's host machine and pastes its file path so the
agent can attach it, showing upload progress for remote terminals. An agent's own paste shortcut
(such as Ctrl+V in Claude Code) still reads the clipboard of the machine where the agent runs.

To give an agent an isolated copy of a Git repository, hover the repository folder in the sidebar,
click the branch button, and enter a branch name. Kodosi opens the terminal in a new Git worktree,
and removes it automatically when the terminal closes if nothing changed. You can configure where
worktrees are stored in **Settings › Terminal**.

Each terminal is light or dark like Kodosi on its host computer, on every device viewing it.
Programs inside the terminal receive theme notifications, so tools like Claude Code (with `/theme`
set to Auto) adapt automatically. If you prefer dark terminals while Kodosi itself is in light
mode, turn on **Keep terminals dark** in **Settings › Terminal**.

### Closing, quitting, and macOS permissions

Every terminal is a real shell running on its host computer. **Minimize** hides a terminal while
keeping it alive; **Close** terminates the shell and its child processes. Closing the app window
leaves Kodosi and your terminals running in the menu bar or system tray (except on Linux desktops
without a system tray, where closing the window quits Kodosi). You can change window-close
behavior in **Settings**.

When you quit Kodosi, your terminals stop, and Kodosi saves their screen state locally on your
machine (never on the server). The next time you open Kodosi, your terminals reopen in the same
folders, projects, and stage positions with their prior screen and scrollback above a fresh prompt
(scroll up on Windows to see it). Claude Code, Codex, and Copilot CLI automatically resume their
conversations, and on macOS and Linux they also keep your chosen model, effort, fast mode,
permissions, and verbosity. If Kodosi can't identify which conversation an agent was running, the
agent lists its conversations so you can pick one.

On macOS, programs inside Kodosi inherit the macOS permissions granted to Kodosi, just like in any
other terminal app. If macOS blocks a program from modifying another app, enable Kodosi under
**App Management** (accessible from **Settings › General**). You can also grant **Device Control
and Data Access** (called **Accessibility** before macOS 27) and **Screen & System Audio
Recording** there for tools that automate apps or capture the screen.

## 3. See which agent needs you

Each terminal's icon shows its current status:

| Indicator | Meaning |
| --- | --- |
| Warm light orbiting the icon | Working (a filling ring shows percentage progress when reported). |
| Hand, question mark, or key | Waiting for your approval, answer, or sign-in. |
| Green check | Done. |
| Yellow warning triangle | Failed. |

The check and warning clear as soon as you open that terminal on any device.

When a terminal starts waiting, finishes, or fails, Kodosi notifies you once on the device you're
actually using:

- If Kodosi is in focus, you just see the status indicator.
- If you're using another app on a computer, Kodosi shows a desktop notification there.
- If you're away from all your computers, Kodosi sends a push notification to your phone. If a
  waiting terminal is still unseen when you step away from your computer, it forwards to your
  phone then.

Only waiting terminals play a notification sound. You can mute notifications per device in
**Settings**. Programs report status using the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) or standard
terminal progress sequences; programs that don't report status simply show no indicator.

### Chat view

Terminals running Claude Code, or Codex and Copilot CLI launched through Kodosi, also offer a
**Chat** view in the terminal header. Chat displays your messages, the agent's replies and tool
activity, and any pending questions with clickable buttons. You can reply or stop the agent right
from chat; if you type custom text while it's asking a question, it tells the agent what to do
instead. Everything you do in chat happens in the underlying terminal as well.

## 4. See what an agent shows on the canvas

Agents can display HTML pages, Markdown, images, PDFs, Office documents, spreadsheets,
presentations, or running web apps on a **canvas** beside the terminal, and drive a shared
browser. By default, the canvas opens automatically when the agent shows or updates something
while you're viewing that terminal (unless you closed it). To open it only yourself, turn off
**Open the canvas by itself** in **Settings › General**.

Claude Code, Copilot CLI, and Antigravity CLI know how to use the canvas even when launched
manually, and Codex, Cursor, and OpenCode know when launched by Kodosi (OpenCode on macOS and
Linux). For any other agent, tell it to run `kodosi skill canvas`.

Once the canvas has content, the terminal header adds buttons for **Files** and **Browser**, with a
copper dot whenever something is new:

- Click either button to open the canvas beside the terminal.
- Click **More room** to let the canvas take over the stage while shrinking the terminal into a
  corner.
- Close a panel with the `×` on its button, and reopen it with `+`.
- Open the browser manually via **Open Browser** in the actions menu, or add files via **Add a
  File** (or drag and drop onto the canvas), which also copies the file into the terminal's folder.
- On iPhone, the canvas button lives in the terminal bar, and a banner announces new or updated
  items with an **Open** button.

### Live documents and pages

- Documents (`.docx`), spreadsheets (`.xlsx`), presentations (`.pptx`), PDFs, and text files are
  live: you, anyone sharing the terminal, and the agent can edit them simultaneously while they
  stay regular files on disk. When a document contains tracked changes, the toolbar includes a
  control to accept or reject them. Under **Settings › Documents**, you can toggle individual
  viewers, switch between paginated and flowing text views, and enable dark pages in dark mode.
- HTML pages on the canvas run completely offline with no network access. Links in pages,
  documents, sheets, slides, and PDFs open in your own browser under the same safety rules as
  terminal links (⌘-click or Ctrl-click inside editable views; regular click elsewhere; tap on
  iPhone). Internal links to slides or cells jump straight there.
- Highlight text on a page or in a document and click **Ask about this** to send the file name and
  selected text straight into the terminal so you can type your question.
- Right-click a file tab (or long-press on mobile) to save a copy, remove that file or all files
  from the canvas, or move the file to the host computer's Trash.

### Shared browser

- Click and type inside the shared browser just like a normal browser. The agent sees the same
  pages and logins, and the output of its next browser command tells it that you used the browser.
- After 10 minutes of inactivity, the browser sleeps and reopens its last page when you or the
  agent use it again. The very first time you open a shared browser on a computer, Kodosi
  downloads Kodosi Browser automatically, so it takes a moment.
- To reuse your existing browser logins in private terminals, click **Copy** next to a browser in
  **Settings › Browser**. Click **Remove** to clear what you copied, or **Remove All** to wipe all
  logins and site data across private terminals. On macOS, if the OS restricts access to a
  browser's data (such as Safari), click **Allow…** next to that browser to grant Full Disk
  Access.
- Anyone with access to a terminal sees its canvas and shares its browser. Closing the terminal
  clears its canvas and discards shared-terminal browser logins.

### A computer's screen

- To see and control a computer from your phone or another computer, turn on sharing on that
  computer: **Let my other devices see and control this Mac** in **Settings › General** on a Mac,
  or **Let my other devices see and control this computer** in **Settings › Account** on Linux and
  Windows.
  - On a Mac, macOS then asks you to let Kodosi Screen, a part of Kodosi, record and control the
    screen. Kodosi Screen alone holds these permissions, so the programs in your terminals don't
    get them. If macOS doesn't ask, **Allow…** in the same settings shows where to turn them on.
  - On Linux, sharing works with KDE Plasma 6 and GNOME on Wayland. On KDE, turning sharing on is
    all it takes; GNOME asks once whether Kodosi may share the screen and remembers your answer.
  - Windows needs no permission. Kodosi can't control programs that run as administrator, and
    the screen shows as locked while Windows asks for administrator approval. The icon that says
    who watches may sit behind the ^ in the taskbar until you pin it.
- Choose **Open Screen** in the actions menu of a terminal on that computer, or let its agent run
  `kodosi show screen`. Pick another display or, on a Mac, a single window above the screen, pinch
  to zoom in, and point, click, and type as if you sat at the computer. Shortcuts work by the letter
  you press, and ⌘ works as Ctrl on Linux and Windows. Text you copy there lands on your device, and
  what you paste comes from your device.
- When the screen can't show, it says why in one line: the computer is locked, asleep, offline, on
  another account, or not sharing. It shows again by itself once that changes.
- Only your own devices see the screen, even in a terminal that you share with friends. While
  someone watches, the computer's menu bar or system tray says which devices watch, with a button
  that stops the sharing until someone at the computer turns it on again.

## 5. Ask Dosi

Dosi is your agent for coordinating your other agents. Click Dosi's face at the bottom of the
sidebar (or press ⌘J). Dosi runs on Claude Code or Codex, configured along with its account,
model, and effort in **Settings › Dosi**.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="media/dosi-still.webp">
    <img src="media/dosi.webp" width="900" alt="When asked what needs attention, Dosi checks the terminals, reports that checkout is waiting for approval, and asks whether to allow it.">
  </picture>
</p>

Ask Dosi what needs your attention, or tell it to handle something. It can inspect your terminals
and projects, answer prompts, start or close terminals, and relay messages. When a decision is
needed, Dosi presents buttons for choices alongside a box to reply in your own words.

Your conversation with Dosi stays in sync across all your devices. On iPhone and iPad, pick which
computer's Dosi to message right above the input box (defaulting to the one that replied last).
Messages sent to an offline computer queue up until it comes back online, and you can cancel a
queued message with **Do not send**.

The status line next to Dosi's face shows when it's working, replied, or waiting on a question. If
no agent is installed on that computer yet, it displays **Set up Dosi** and opens **Settings ›
Dosi**. To disable Dosi on a computer, turn off **Use Dosi** in **Settings › Dosi**.

## 6. Add your other computers and phones

Sign in on a new computer with the same account. It displays a verification code, and your existing
devices prompt you to approve it. Click **Approve** on an approved computer and enter the code, or
scan it from an approved phone.

Adding a phone or tablet takes one scan: on an approved computer, open **Settings**, click **Show
a code** under **Add a device**, and scan it with the new device. Once approved, all your
computers' terminals show up everywhere.

### Starting terminals remotely

To launch a terminal on another computer, pick that computer on the right side of the launcher bar.
The folder menu then lists recent folders on that machine, and **Choose a folder…** browses its
home directory. On iPhone and iPad, tap `+` in **Terminals** or next to a specific computer. The
target computer needs Kodosi open (or `kodosi host` running in a terminal).

When two of your devices can reach each other directly, terminal and browser traffic flows peer to
peer for lower latency. On a local network, macOS, iOS, and Android prompt once for local network
permission; if declined, traffic falls back to the relay server.

### Recovery keys

Create a recovery key in **Settings** and store it somewhere safe. If you ever lose all your
devices, sign in on a new one and select **"Use my recovery key"** to keep your friends and
projects. Remove lost devices from any remaining device; if a device was stolen, generate a fresh
recovery key afterward too.

## 7. Work with friends

Open **Friends** from the menu beside your account. Send your invite, or paste a friend's invite
or username. Using an invite lets Kodosi verify your friend's identity automatically. On
mobile, you can display your invite as a QR code for a friend to scan with their camera or inside
Kodosi; they still confirm trust before the request is sent.

<p align="center"><img src="media/friends.webp" width="900" alt="The Friends page showing two verified friends."></p>

### Sharing a terminal

Open the actions menu in a terminal's header, click **Share**, and select friends or projects.
Anyone you share with gets full control with your OS permissions on that machine. Your other
terminals stay private.

### Projects

A project pulls together terminals from any folder or computer alongside a shared chat and task
list. Click `+` next to **Projects**, then start terminals inside the project (or share existing
ones into it) and invite friends when ready.

<p align="center"><img src="media/project.webp" width="900" alt="A project with a shared terminal and a conversation between people and agents."></p>

- Every member can use shared terminals and share their own. People who join later immediately see
  the project's terminals and full chat history. Files remain on each terminal's host computer.
- Create tasks up for grabs, claim them, release them, close them with a summary note, or reopen
  them.
- Link GitHub or Gitea repositories to sync issues with tasks: updating a linked task updates the
  underlying issue. GitHub uses your `gh` login or Git credential helper; Gitea uses the Git
  credential helper or `KODOSI_GITEA_TOKEN` with `KODOSI_GITEA_URL`. Credentials never leave your
  machine.
- Mention a terminal in chat with `@<terminal-name>` to deliver your message (tagged with your
  name and project) straight to the agent running there; the agent decides how to respond. Claude
  Code, Copilot CLI, and Antigravity CLI inside Kodosi (plus Codex, Cursor, and OpenCode when
  launched by Kodosi, with OpenCode on macOS and Linux) automatically know they're in Kodosi. Run
  `kodosi skill` to print the [skill](../runtime/skills/kodosi/SKILL.md) for any other agent.
  Inside a Kodosi terminal, project commands detect the current project automatically; outside,
  pass `--project <name>`.
- Deleting a message you or your agent posted replaces it with a deleted placeholder for everyone.
  **Clear chat** hides history up to that point across all your devices while leaving it intact
  for other members.

## 8. Use the command line

The `kodosi` CLI controls the exact same terminals as the desktop app, and changes show up in the
app immediately.

| Command | What it does |
| --- | --- |
| `kodosi terminal start --agent claude --prompt "fix the failing test"` | Launches an agent with your last-used settings. Override with `--account`, `--model`, `--effort`, `--fast`, `--permissions`, and `--output`. |
| `kodosi terminal start --directory <repository> --branch <name>` | Starts a terminal on a new branch inside its own Git worktree. |
| `kodosi terminal start --computer <computer> --directory <folder>` | Starts a terminal on another of your computers. Relative paths and `~` resolve against your home directory there. Use `kodosi devices list` to see which computers are online. |
| `kodosi agent list` | Lists installed agents, their accounts, and your last-used settings. |
| `kodosi agent usage` | Shows current plan usage and reset times for signed-in Claude Code, Codex, and Copilot CLI accounts. |
| `kodosi terminal list` | Lists terminals with their names and IDs (pass either as `<terminal>`). |
| `kodosi terminal read <terminal>` | Prints the current screen contents. Add `--history` to include scrollback. |
| `kodosi terminal message <terminal> "Can you review my branch?"` | Sends a message to the agent in a terminal, tagged with sender and origin. |
| `kodosi terminal agent <terminal> --model sonnet` | Changes a running agent's model, effort, fast mode, or permissions. |
| `kodosi terminal input <terminal> --enter "make test"` | Types text into a terminal. |
| `kodosi terminal keys <terminal> ctrl-c` | Sends keystrokes to a terminal. |
| `kodosi terminal wait <terminal> --until done,failed` | Blocks until the terminal enters one of the given states. |
| `kodosi terminal show <terminal>` | Focuses the terminal in the app. |
| `kodosi dosi say "What needs me?"` | Sends a message to Dosi. Use `kodosi dosi show` to print the conversation and `kodosi dosi stop` to cancel a turn. |
| `kodosi show <file>` | Opens a file on this terminal's canvas, which opens for the people viewing the terminal. Use `kodosi show <address>` to open a URL in its shared browser, `kodosi show browser` to bring up the browser as is, `kodosi show screen` or `kodosi show window <words>` to show the computer's screen to its owner's devices, `kodosi canvas` to list canvas items, and `kodosi canvas remove <file>` to remove one. |
| `kodosi browser open <address>` | Opens a URL in this terminal's shared browser (all other agent-browser commands work too). |
| `kodosi skill` | Prints the agent skill for collaborating in Kodosi; `kodosi skill canvas` prints the canvas skill. |
| `kodosi dosi skill` | Prints the skill that lets another agent act as Dosi. |
| `kodosi host` | Runs the host independently so terminals keep running after you quit the app. Use `kodosi host stop` to stop a background host. |

`message`, `input`, and `keys` refuse to run while you're actively typing in that terminal.
`message` also refuses while the agent is waiting on a human response.

## If something doesn't connect

- Check that the terminal's host computer is online with Kodosi running, and that the terminal is
  still shared.
- A newly added device may still be waiting for approval, or a friend whose identity changed may
  need re-verification in **Friends**.
- After a lost connection, Kodosi shows the current state again. If a remote terminal shows
  **Offline**, this device can't reach the server; check your network connection. Kodosi
  reconnects automatically, or click **Try again** to retry immediately.
- When a host computer goes offline, its terminals reconnect automatically once it returns (unless
  they closed in the meantime). To remove an offline terminal from your list, click its `×`:
  **Hide until it's back** hides it temporarily, and **Close forever** closes it permanently once
  the computer reconnects.
- If Kodosi warns that delivery of your input was uncertain, check the terminal screen before
  re-running the command.
