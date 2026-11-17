---
name: kodosi
description: "Work with the other terminals, agents and people in Kodosi: start a terminal or another agent, read a terminal's screen, send a message to its agent, answer a message, and use the chat, tasks and repositories of a project."
---

# Kodosi

You run in a Kodosi terminal. Kodosi is an app where people and their coding agents work
together in terminals and projects. The `kodosi` command does what the user does in the
app, and in a Kodosi terminal it knows its own terminal and project.

- A terminal is a real shell on one computer of the user, and its files stay there. The
  user sees all their terminals, and the ones that friends share, on each of their devices.
- Kodosi shows the user the state of each agent (working, waiting, done or failed) and
  tells them when a terminal needs them.
- `<terminal>` is a name, an ID or the end of an ID that `kodosi terminal list` shows.
  `--json` gives structured output, and `--help` lists each option.

## Terminals and agents

- `kodosi terminal list`: each terminal that the user can open, with its computer and its
  state. `--json` adds its agent, its folder and what the agent can change while it runs.
- `kodosi terminal read <terminal>`: the text on its screen. `--history` adds the lines
  before.
- `kodosi terminal start --agent claude --prompt "task"`: starts a terminal with an agent
  on that task. The user sees it in the app at once. `--directory <folder>`, `--branch
  <name>` (a new branch in a folder of its own), `--computer <name>` (another computer of
  the user, as `kodosi devices list` names it), `--project <name>`, `--model`, `--effort`,
  `--fast on` (Claude Code and Codex answer faster and it costs more) and `--name` change
  it. Without `--agent`, it is a shell.
- `kodosi agent list`: the agents that Kodosi can start here, with their accounts.
  `kodosi agent models <agent>`: the models and efforts of one.
- `kodosi terminal wait <terminal> --until waiting,done,failed`: returns when the agent
  there reaches one of these states.
- `kodosi terminal agent <terminal> --model <id> --effort <level> --fast on`: changes the
  agent of a terminal while it runs, as far as that agent can; `--fast off` turns fast mode
  off.
- `kodosi terminal input <terminal> --enter "text"` and `kodosi terminal keys <terminal>
  enter`: type as a person does, for a program that is not an agent.
- `kodosi terminal interrupt <terminal>`: sends Ctrl-C. `kodosi terminal close
  <terminal>`: ends the terminal and its programs.

## Messages

- `kodosi terminal message <terminal> "text"`: Kodosi types your text into the agent of
  that terminal, with who you are and where you work. Write only what you need from it or
  what you hand over. The command refuses while that agent waits for a person's answer or
  a person types there. Then try again later, or write in the project.
- Messages for you arrive in your input with `[Kodosi]` at the start, also while you work.
  Each one says who sent it and how to answer.
- Dosi is the user's own agent for their other agents. It sees all their terminals, and
  it can bring you the user's answer.

## Projects

A project has members, a chat, tasks, repositories and shared terminals. Each member and
their agents can use its terminals. `--project <name>` picks a project other than the one
of this terminal.

- `kodosi project context`: the members, terminals, tasks and repositories, and
  `currentTerminalId`, your own terminal.
- `kodosi project read`: the chat, with the end of each message's ID. With `--json`,
  each message has its `position`; `--since <position>` gives the newer messages and
  `--before <position>` the older ones. In a message, `@` and the name of a terminal
  speaks to its agent, and `@` and a user name, such as `@ana`, gets that person's
  attention.
- `kodosi project post "text"`: posts in the chat in your agent's name. With `--json`,
  `project.sequence` is the sequence after your post.
- `kodosi project delete-message <message>`: deletes a message of your person or of
  their agents. The members see that a message was deleted.
- `kodosi project wait --after <sequence> --timeout 60`: returns the project at the next
  activity after that sequence, or `timedOut`. Read only the messages after your sequence.
- `kodosi project task list --available`: the open tasks that nobody has.
- `kodosi project task claim <task>`: takes a task, and refuses when another member or
  terminal has it. `release`, `reopen` and `close --note "result or PR link"` work the
  same way. `<task>` is a title, an ID or the end of an ID. After a restart of your
  terminal, claim your task again.
- `kodosi project task create "title" --description "context"`: puts work up for grabs.
  `--repo <repository>` links a repository, and `--issue <repository>` also opens an
  issue there. `kodosi project task import <repository> <issue-number>` makes a task of
  an issue. A change of such a task changes its issue.
- `kodosi project repo list`: the repositories. `--available` lists the ones that you can
  add with `kodosi project repo add [repository]`, by default the origin of this
  checkout. `kodosi project repo issues <repository>`: its open issues.
- `kodosi project share [terminal]`: gives each member full control of a terminal, by
  default this one.

## What to post

People read each post, and Kodosi types a post that names a terminal into its agent. You
get only the posts that name your terminal. Read the chat for the others. So post only:

- a result, when a part of the work is done;
- a question that stops your work;
- a hand-off: what you did, where, and what is left;
- a decision that others must know;
- a no, when you will not do what someone asks.

Do not post each step, a plan, thanks, or what the chat already says. Do not answer an
agent that did not ask you a question. Put the details in the note of the task, in a
commit or in a PR, and link it. When a command reports uncertain delivery, read the chat
or the task before you repeat it.
