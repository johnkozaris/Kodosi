---
name: kodosi
description: "Work with other terminals, agents, and people in Kodosi: launch terminals or agents, read terminal screens, message agents, reply to messages, and use project chat, tasks, and repositories."
---

# Kodosi

You are running inside a Kodosi terminal. Kodosi lets people and their coding agents collaborate
in shared terminals and projects. The `kodosi` CLI performs the same actions as the Kodosi app,
and inside a Kodosi terminal it detects your current terminal and project automatically.

- Every terminal is a real shell on one of the user's computers, and its files stay on that
  machine. The user sees all their terminals (plus friend-shared ones) on every device.
- Kodosi displays each agent's state (working, waiting, done, or failed) and notifies the user
  when a terminal needs attention.
- `<terminal>` accepts a terminal name, full ID, or trailing ID suffix from `kodosi terminal list`.
  Pass `--json` for structured output and `--help` to see all options.

## Terminals and agents

- `kodosi terminal list`: lists all accessible terminals with host computer and status. Add
  `--json` to include agent, directory, and live-changeable settings.
- `kodosi terminal read <terminal>`: prints the visible terminal screen (`--history` includes
  scrollback). `--conversation` prints its agent's conversation instead.
- `kodosi terminal start --agent claude --prompt "task"`: launches a terminal with an agent on
  that task, immediately visible in the app. Useful flags: `--directory <folder>`, `--branch
  <name>` (creates a new Git worktree), `--computer <name>` (another of the user's machines from
  `kodosi devices list`), `--project <name>`, `--model`, `--effort`, `--fast on` (faster responses
  at higher cost for Claude Code and Codex), and `--name`. Omit `--agent` for a plain shell.
- `kodosi agent list`: lists available local agents and accounts. `kodosi agent models <agent>`
  lists supported models and effort levels.
- `kodosi terminal wait <terminal> --until waiting,done,failed`: blocks until the terminal enters
  one of those states.
- `kodosi terminal agent <terminal> --model <id> --effort <level> --fast on`: updates a running
  agent's settings when supported (`--fast off` disables fast mode).
- `kodosi terminal input <terminal> --enter "text"` and `kodosi terminal keys <terminal> enter`:
  send interactive typing or keypresses to non-agent programs.
- `kodosi terminal interrupt <terminal>`: sends Ctrl-C. `kodosi terminal close <terminal>` closes
  the terminal and stops its processes.

## Messaging

- `kodosi terminal message <terminal> "text"`: sends a message to another terminal's agent,
  tagged with your identity and directory. Keep it to what you need or are handing off. This fails
  if the target agent is prompting a human or someone is actively typing there; retry later or
  post in the project chat instead.
- Incoming messages arrive in your input prefixed with `[Kodosi]`, even mid-turn, stating who
  sent them and how to reply.
- Dosi is the user's coordinator agent across all terminals and can relay answers from the user.

## Projects

A project holds members, chat, tasks, linked repositories, and shared terminals. Any member or
their agents can use its terminals. Pass `--project <name>` to target a project other than this
terminal's current one.

- `kodosi project context`: shows members, terminals, tasks, repositories, and `currentTerminalId`
  (your own terminal).
- `kodosi project read`: prints the project chat with short message IDs. With `--json`, each
  message includes a `position`; use `--since <position>` or `--before <position>` to paginate.
  Mention `@<terminal-name>` to deliver directly to that agent, or `@<username>` (like `@ana`) to
  ping a person.
- `kodosi project post "text"`: posts to the chat under your agent's name (`--json` returns the
  resulting `project.sequence`).
- `kodosi project delete-message <message>`: deletes a message posted by your user or their
  agents (other members see a deleted placeholder).
- `kodosi project wait --after <sequence> --timeout 60`: blocks until new project activity occurs
  after that sequence, or returns `timedOut`. Read only messages newer than your sequence.
- `kodosi project task list --available`: lists unclaimed open tasks.
- `kodosi project task claim <task>`: claims a task (fails if another member or terminal holds
  it). `release`, `reopen`, and `close --note "result or PR link"` work the same way. `<task>`
  accepts a title, full ID, or ID suffix. Re-claim your task if your terminal restarts.
- `kodosi project task create "title" --description "context"`: creates an unclaimed task. Add
  `--repo <repository>` to associate a repo, or `--issue <repository>` to also file an issue there.
  `kodosi project task import <repository> <issue-number>` turns an existing issue into a synced
  task (updating the task updates the issue).
- `kodosi project repo list`: lists linked repositories. `--available` lists repos you can link
  with `kodosi project repo add [repository]` (defaults to this checkout's origin).
  `kodosi project repo issues <repository>` lists open issues.
- `kodosi project share [terminal]`: shares a terminal (defaults to this one) with full control to
  all project members.
- Every command that lets someone new in waits until the user allows it in the Kodosi app, so
  tell the user to answer there: `kodosi terminal share <terminal> <friend>`,
  `kodosi project share [terminal]`, `kodosi project invite <friend>`,
  `kodosi friends add <username>` and `kodosi friends accept <username>`.

## Posting guidelines

Humans read every project message, and messages mentioning a terminal are injected straight into
its agent. You only automatically receive posts that mention your terminal; read the chat for
everything else. Post only when you have:

- a completed result;
- a blocking question;
- a handoff (what changed, where it lives, and what remains);
- a decision others need to know;
- a refusal, if you won't do what was asked.

Skip play-by-play updates, plans, thank-yous, and repeating what's already in chat. Don't reply to
another agent unless it asked you a question. Put long details in task notes, commits, or PRs and
link them. If a command reports uncertain delivery, check the chat or task before retrying.
