# Getting started

**English** · [Deutsch](de/GETTING_STARTED.md) · [Ελληνικά](el/GETTING_STARTED.md) · [Español](es/GETTING_STARTED.md) · [Français](fr/GETTING_STARTED.md) · [Italiano](it/GETTING_STARTED.md) · [日本語](ja/GETTING_STARTED.md) · [한국어](ko/GETTING_STARTED.md) · [简体中文](zh-Hans/GETTING_STARTED.md)

[Get Kodosi](../README.md#get-kodosi) for your computer. This guide uses the Mac keys; on
Linux and Windows, use Ctrl+Shift with the same letter.

## 1. Sign in

Open Kodosi and select Sign in at the bottom of the sidebar. Kodosi opens your browser,
where you sign in. In a terminal, `kodosi auth login` shows a code to enter in any
browser.

Without an account, Kodosi runs terminals on this computer. Sign in to use your other
computers, friends and projects.

## 2. Start an agent

Select New terminal (⌘N). Choose an agent and a folder, and if you want, an account, a
model and an effort. Type a first prompt and press Return. Kodosi types the command into
the shell of the new terminal.

<p align="center"><img src="media/launcher.webp" width="580" alt="Claude Code chosen in the launcher, with a folder and a first prompt."></p>

- For a plain terminal, press ⌘D (Ctrl+Shift+O on Linux and Windows). It opens at once in
  the folder of the selected terminal. ⌘T (Ctrl+Shift+T) opens it with the agent of the
  selected terminal, with the same account and choices.
- The keys follow Ghostty. ⌘1 to ⌘9 select a terminal (Alt+1 to Alt+9 on Linux and
  Windows), ⌥⌘ with an arrow selects the one beside it (Ctrl+Alt with an arrow), and ⌘W
  closes the selected terminal. Settings › Shortcuts lists every key and changes it.
- If an agent is missing, the launcher shows its install command.
- Agents start unrestricted: they change files and run commands without asking you. For an
  agent that asks first, choose Ask on the button after the model. There you also choose
  how much of their work Claude Code and Codex show; they start at Quiet.
- A new account gets its own sign-in folder, such as `~/.claude-work`. Kodosi also finds
  the accounts that your shell sets up, such as
  `alias cw='CLAUDE_CONFIG_DIR=~/.claude-work claude'`.
- In a terminal at its prompt, the actions key (two sliders) in its header lists the
  installed agents. Select one to start it with your last choice.
- While an agent runs, the same actions key changes its model and effort, and for some
  agents its permissions, with the agent's own commands, so it doesn't start again. A
  change waits for the end of the agent's turn, and Claude Code also keeps a model and
  effort for its next sessions.
- In zsh and bash (not the bash 3.2 of macOS), ⌘↑ and ⌘↓ jump between prompts.
- ⌘-click a link to open it (Ctrl-click on Linux and Windows). In a program that uses the
  mouse, also hold Shift. On the iPhone, tap the link; a tap does not open a link in a
  program that uses the mouse. The link opens in your own browser on the device that you
  use. Kodosi asks before it opens a link in another app. It does not open a file that
  can run a program, or a file or a `localhost` address of a terminal on another computer.
- To give an agent a picture, such as a screenshot, paste it with ⌘V (Ctrl+Shift+V on
  Linux and Windows, Paste on the iPhone). Kodosi saves it on the computer of the terminal
  and pastes the path of its file, and the agent attaches it. A terminal on another
  computer shows how much of the picture arrived. The agent's own key for pictures, such
  as Ctrl+V in Claude Code, reads the clipboard of the computer where the agent runs.
- To give an agent its own copy of a repository, hover the repository folder in the
  sidebar, select the branch button and name a branch. Kodosi starts the terminal in a new
  Git worktree. When the terminal ends, Kodosi removes the worktree if nothing changed.
  Settings › Terminal sets where these folders go.

A terminal is a real shell on the computer that runs it. Minimize hides a terminal and
keeps it running; Close ends it and its programs. When you close the window, Kodosi and
your terminals keep running, and Kodosi's icon stays in the menu bar or the system tray;
on a desktop without a system tray, closing the window quits Kodosi. When you quit
Kodosi, your terminals stop. Kodosi saves what they show on your computer and nowhere
else. When you open Kodosi again, they start again in their folders, projects and places on
the stage, with their earlier screen and history above a new prompt (on Windows, scroll
up to see it). Claude Code, Codex and Copilot CLI continue their conversations, and on a
Mac or on Linux they keep the model, effort, permissions and output that you chose.
When Kodosi cannot tell which conversation an agent showed, the agent lists its
conversations so that you can continue one. In Settings, you can choose what closing
the window does.

On a Mac, the programs in your terminals have the permissions that you give Kodosi, as in
any terminal app. When macOS says that Kodosi was prevented from modifying apps, a program
in a terminal tried to change another app. To let it, turn on Kodosi in App Management;
Settings › General opens it. There you also allow Device Control and Data Access
(Accessibility before macOS 27) and Screen & System Audio Recording, for programs that
control apps or see the screen.

## 3. See which agent needs you

The mark of each terminal shows what its program does.

| You see | It means |
| --- | --- |
| A warm light that goes around the mark | Working. The edge fills to show a percent. |
| A hand, a question mark or a key | Waiting for your approval, answer or sign-in. |
| A green check | Done. |
| A yellow warning sign | Failed. |

The check and the warning go away when you open the terminal. While Kodosi is in the
background, a notification tells you when one of your terminals starts to wait, finishes
or fails. Programs report their state with the
[Program Status Protocol](https://www.superlogical.com/rex/docs/build/program-status) or a
progress bar. A program that reports nothing shows no state.

## 4. See what an agent shows

An agent can show you a page, Markdown, a picture, a PDF, a document, a sheet, a
presentation or the app that it runs on the canvas of its terminal, and use a browser that
you share with it. The canvas opens with what the agent shows while you look at its
terminal, and when a file on it or the browser changes, unless you closed it. To open it
only yourself, turn off Open the canvas by itself in Settings › General. Claude Code and
Copilot CLI know how, also when you start them yourself, and so does a Codex that Kodosi
starts; tell another agent to read `kodosi skill canvas`. When the canvas has something,
the terminal's header shows a key for its files and one for its browser, with a copper dot
for news. Select one to open the canvas beside the terminal; More room gives the canvas
the stage and keeps the terminal small in its corner. Close a part with the × on its key,
and open it again with +. Open Browser in the actions key opens the browser before the
agent uses it, and Add a File puts a file on the canvas and into the terminal's folder;
you can also drop a file on the canvas. On the iPhone, the canvas key is in the terminal's
bar, and a banner names what the agent shows or changes, with Open.

- A document, sheet, presentation, PDF or text file is live: you, the people in the
  terminal and the agent edit it at the same time, and it stays a normal file in its
  folder. When a document has tracked changes, its bar has a key that accepts or rejects
  them. Settings › Documents turns each viewer off or on, shows documents as pages or as
  text that fits the canvas, and makes pages dark when Kodosi is dark.

- A page has no network. Links in pages, documents, sheets, presentations and PDFs open in
  your own browser by the same rules as terminal links. Where you can edit, ⌘-click a link
  (Ctrl-click on Linux and Windows); elsewhere a click opens it, and on the iPhone a tap. A
  link to a slide or a cell of the same file goes there.
- Select words on a page or in a document, then Ask about this. Kodosi types the file and
  the words into the terminal, and you write the question.
- Click and type in the shared browser as in your own. The agent sees the same pages and
  sign-ins, and its next command tells it that you used the browser. After 10 minutes with
  no use the browser sleeps, and it opens its last page again when you look at it or the
  agent uses it. The first browser on a computer gets Kodosi Browser, so it takes a moment.
- To use your sign-ins in your private terminals, select Copy beside a browser in
  Settings › Browser. Remove takes away what you copied, and Remove All removes every
  sign-in and all site data of your private terminals. On a Mac, when macOS keeps a
  browser's sign-ins from Kodosi, as it does Safari's, select Allow… beside the browser to
  give Kodosi Full Disk Access.
- On one network, the browser's picture goes straight between your devices. macOS and iOS
  ask once for local network access; without it, the picture goes through the service.
- Everyone who can open the terminal sees its canvas and uses its browser. When the
  terminal closes, its canvas and the browser's sign-ins go.

## 5. Ask Dosi

Dosi is your agent for your other agents. Select Dosi's face at the bottom of the sidebar
(⌘J). Dosi runs on Claude Code or Codex; Settings › Dosi chooses which, with its account,
model and effort.

<p align="center">
  <picture>
    <source media="(prefers-reduced-motion: reduce)" srcset="media/dosi-still.webp">
    <img src="media/dosi.webp" width="900" alt="Asked what needs you, Dosi reads the terminals, says that checkout waits for an approval and asks whether to allow it.">
  </picture>
</p>

Ask Dosi what needs you, or ask it to do something. Dosi reads your terminals and projects,
answers, and types, starts and closes things for you. When it needs a choice, it asks with
buttons, and you can also answer in your own words. Your conversation with Dosi is the
same on all your devices. On an iPhone or iPad, the Dosi tab writes to Dosi on the computer
that you choose above the message, at first the one that answered last. A message for a
computer that is offline waits until it runs Kodosi again, and Do not send takes it back.

The line beside Dosi's face tells you when Dosi works, when it answered and when it has a
question. On a computer with no agent, it reads Set up Dosi and opens Settings › Dosi. To
remove Dosi from a computer, turn off Use Dosi in Settings › Dosi.

## 6. Add your other computers

Sign in on the new computer with the same account. It shows a code, and the computers
that you already use ask you to approve it. Choose Approve on one of them and enter the
code. An iPhone or iPad joins the same way. Then each computer shows the terminals of all
your computers.

To start a terminal on another of your computers, choose that computer at the right end
of the launcher's band. The folder key then lists its recent folders, and Choose a
folder… opens the folders in its home. On an iPhone or iPad, select + in Terminals, or
the + beside one of your computers. That computer needs Kodosi open, or `kodosi host`
running in a terminal.

Make a recovery key there too, and keep it safe. If you lose all your devices, sign in
on a new one and choose "Use my recovery key" to keep your friends and projects. Remove a
lost device from another device. If a device was stolen, also make a new recovery key.

## 7. Work with friends

Open Friends from the menu beside your account. Send your invite, or paste the invite or
username of a friend. An invite lets Kodosi verify your friend. On a phone, show your
invite as a code: your friend scans it with their camera or in Kodosi, and confirms that
they trust you before the request goes out.

<p align="center"><img src="media/friends.webp" width="900" alt="The Friends page with two verified friends."></p>

To share a terminal, select Share in the actions key of its header and choose friends or
projects. The people you share with get full control, with your privileges on that
computer. Your other terminals stay private.

A project gathers terminals from any folder and computer, with a conversation and tasks.
Select + beside Projects, then start a terminal in the project or share one into it, and
invite friends when you want.

<p align="center"><img src="media/project.webp" width="900" alt="A project with a shared terminal and a conversation of people and agents."></p>

- All members can use the shared terminals and share their own. People who join later
  get the terminals and the full conversation. Files stay on the computer of each
  terminal.
- Put tasks up for grabs, claim them, give them back, close them with a result or open
  them again.
- Link GitHub or Gitea repositories to turn issues into tasks; when you change such a
  task, Kodosi changes its issue. GitHub uses your `gh` sign-in or Git credential helper,
  and Gitea the credential helper or `KODOSI_GITEA_TOKEN` for the server in
  `KODOSI_GITEA_URL`. Your credentials stay on your computer.
- Write `@` and the name of a terminal in the chat, and Kodosi gives the agent in that
  terminal your message with your name and the project; the agent decides what to do with it.
  Claude Code and Copilot CLI in a Kodosi terminal, and a Codex that Kodosi starts, know
  that they run in Kodosi, and `kodosi skill` prints the
  [skill](../runtime/skills/kodosi/SKILL.md) that lets an agent take part. In a Kodosi
  terminal, the command finds the project; elsewhere, add `--project <name>`.

## 8. Use the command line

The `kodosi` command works on the same terminals as the app, and the app shows what it
does.

| Command | What it does |
| --- | --- |
| `kodosi terminal start --agent claude --prompt "fix the failing test"` | Starts an agent with your last choice. `--account`, `--model`, `--effort`, `--permissions` and `--output` change it. |
| `kodosi terminal start --directory <repository> --branch <name>` | Starts a terminal on a new branch, in its own worktree. |
| `kodosi terminal start --computer <computer> --directory <folder>` | Starts a terminal on another of your computers. A relative folder, or one that starts with `~`, is in your home there. `kodosi devices list` shows which computers are online. |
| `kodosi agent list` | Shows the agents, their accounts and your last choice. |
| `kodosi agent usage` | Shows how much of the plan limits each signed-in Claude Code, Codex and Copilot CLI account has used. |
| `kodosi terminal list` | Shows your terminals with their names and IDs. Give either one as `<terminal>`. |
| `kodosi terminal read <terminal>` | Prints the screen. `--history` adds the lines that scrolled away. |
| `kodosi terminal message <terminal> "Can you review my branch?"` | Gives the agent in a terminal a message, with who sends it and from where. |
| `kodosi terminal agent <terminal> --model sonnet` | Changes the model, effort or permissions of the agent in a terminal while it runs. |
| `kodosi terminal input <terminal> --enter "make test"` | Types a line. |
| `kodosi terminal keys <terminal> ctrl-c` | Presses keys. |
| `kodosi terminal wait <terminal> --until done,failed` | Waits for a state. |
| `kodosi terminal show <terminal>` | Shows the terminal in the app. |
| `kodosi dosi say "What needs me?"` | Asks Dosi. `kodosi dosi show` prints the conversation, and `kodosi dosi stop` stops Dosi. |
| `kodosi show <file>` | Shows a file on the canvas of this terminal, which opens for the people who look at it; `kodosi show <address>` shows a web address in its browser. `kodosi canvas` lists what it shows, and `kodosi canvas remove <file>` takes a file off. |
| `kodosi browser open <address>` | Opens a page in the shared browser of this terminal. The other agent-browser commands work too. |
| `kodosi skill` | Prints the skill that lets an agent work with the people and agents in Kodosi. `kodosi skill canvas` prints the skill for the canvas. |
| `kodosi dosi skill` | Prints the skill that lets another agent act as Dosi. |
| `kodosi host` | Keeps your terminals running after you quit the app. `kodosi host stop` stops a host that runs in the background. |

`message`, `input` and `keys` stop with a message while you type in that terminal.
`message` also stops while the agent waits for a person's answer.

## If something does not connect

Make sure that the computer of the terminal is online, with Kodosi open, and that the
terminal is still shared. A new device can need approval, and a friend with a changed
identity can need verification in Friends. After a lost connection, Kodosi shows the
current state again. When a terminal of another computer says Offline, this device cannot
reach the server: check its network. Kodosi connects again by itself, and Try again
connects at once. When Kodosi says that a computer is offline, its terminals open again
when it is back, unless they ended there. To take such a terminal out of your list,
select its X. Hide until it's back shows it again when the computer is back, and Close
forever closes it then. If Kodosi says that it is not sure your input arrived, look at
the terminal before you type the command again.
