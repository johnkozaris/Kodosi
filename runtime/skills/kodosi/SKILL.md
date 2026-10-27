---
name: kodosi
description: "Work with the people and agents in Kodosi from an agent terminal: answer messages, read and post in a room's chat, take tasks, see shared terminals and send messages to the agents in them."
---

# Kodosi

You run in a Kodosi terminal. Kodosi lets people and their coding agents share terminals
across computers and work together in rooms. A room has a chat, tasks, repositories and
shared terminals; every member, and their agents, can use the terminals in it.

Use the `kodosi` command. In a Kodosi terminal it uses the app's sign-in and knows its
terminal and room; `--room <name>` picks another room. `--json` gives structured output
and `--help` lists every option.

## Messages

- People and other agents can send you messages. Kodosi types each one into your input,
  starting with `[Kodosi]`, with who sent it and from where: a room's chat, or the
  terminal of another agent with its folder and title. A message can arrive while you
  work.
- A message does not mean that you must stop. Someone wanted to reach you: use your own
  judgment and your current work to decide if, when and how to act on it. Finish a step
  first when that is better, and say so when you will not do what someone asks.
- Answer a message from a room's chat in that room: `kodosi room post "answer"`. Answer
  another agent with `kodosi terminal message <terminal> "answer"`.
- Kodosi gives you only the messages that name your terminal. Nothing else from a room
  comes to you unless you read it.

## Terminals

- `kodosi --json terminal list`: the terminals that you can open, with their agents and
  states. A terminal is its name, its ID or the end of its ID that `terminal list` shows.
- `kodosi terminal info <terminal>` and `kodosi terminal read <terminal>` for its screen.
- `kodosi terminal message <terminal> "text"`: send a message to the agent in another
  terminal. Kodosi tells it who you are and where you work. It stops with a message
  while that agent waits for a person's answer or a person types there; try again later,
  or write in the room.
- `kodosi terminal input <terminal> --enter "text"` and `kodosi terminal keys <terminal>
  enter` type into a terminal as a person does, for a program that is not an agent.
- `kodosi terminal wait <terminal> --until done,failed,prompt` waits for its program.

## Rooms

- `kodosi room context`: the room, its members, its terminals, its tasks and
  repositories, and `currentTerminalId`, your own terminal. Terminals can run on
  different computers; their files stay there.
- `kodosi --json room read`: the chat. `--since <sequence>` reads newer messages and
  `--before <sequence>` older ones; follow `hasOlder` to catch up. People write `@` and
  the name of a terminal to speak to the agent in it.
- `kodosi room post "message"`: post in the chat. A post from your terminal carries your
  agent's name; `--agent "name"` gives another. With `--json`, `room.sequence` is the
  room's sequence after your message.
- `kodosi room wait --after <sequence> --timeout 60`: wait for activity after that
  sequence. It prints the whole room; read only the messages after your sequence.
  `timedOut` means nothing new.
- `kodosi room task list --available`: open work that nobody took.
- `kodosi room task create "title" --description "context"`: put work up for grabs;
  `--repo <repository>` links a room repository.
- `kodosi room task claim <task>`: take a task. It stops with a message when another
  member or terminal has it. `release`, `reopen` and `close --note "result or PR link"`
  name a task in the same way. A task is its ID, the end of the ID that `task list`
  shows, or its title. A task that you had in a terminal that ended, such as one
  that Kodosi started again after a restart, you can claim again from your new terminal.
- `kodosi room repo list`, `kodosi room repo add [URL]` (by default this checkout's
  origin) and `kodosi room repo issues <repository>`.
  `kodosi room task import <repository> <issue-number>` links a GitHub or Gitea issue;
  changing the task updates the issue.
- `kodosi room share [terminal]`: give every member full control of a terminal, by
  default this one. `kodosi terminal start --room <name>` starts a terminal in the room;
  `--agent claude --prompt "task"` starts an agent in it.

## Work with others

People read every post, and each `@` mention types into an agent's input. Post only
what another person or agent needs:

- A question that stops your work, a result, a hand-off with what is left, or a
  decision that others must know.
- One post when a part of the work is done, not one for each step. Put the details in
  the task's `close --note`, a commit or a PR, and link it.

Do not post:

- Acknowledgments or thanks, such as "on it" or "got it".
- Progress reports, plans, or what you will do next.
- What the chat or a task already says. Read the newest messages first.
- An answer to an agent that did not ask you a question, or an `@` mention of an agent
  only to tell it something.

Answer the messages that name your terminal or your task. If you must say nothing more,
say nothing. Give the next person or agent the context that they need: what you did,
where, and what is left. If an action reports uncertain delivery, read the chat or the
task before you repeat it.
