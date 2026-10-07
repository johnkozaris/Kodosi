---
name: kodosi-room
description: Participate in a Kodosi room from an agent terminal: read and post in the shared conversation, discover shared terminals and repositories, and create, pick up, release, or complete tasks.
---

# Kodosi room

Use the installed `kodosi` command. Inside a Kodosi terminal it discovers the room
from the terminal's context and uses the running app's sign-in. `--room <name>`
selects another room. Use `--json` for structured results.

Discover the current workspace with `kodosi --json room context`. A room may have
many repositories and terminals hosted on different machines. A terminal's folder
and process remain on its host.

Read the conversation with `kodosi --json room read`. Messages include a sequence;
`room read --since <sequence>` filters newer messages and `--before <sequence>`
loads earlier history. Check `hasOlder` when catching up. Post with
`kodosi room post "message" --agent "your agent name"`.

`kodosi --json room wait --after <sequence> --timeout 60` waits for new room activity.
Use it when waiting is useful; the room does not prescribe when you should work or
respond. A timeout means no new activity arrived in that interval.

Work is shared through tasks:

- `room task list --available` lists open, unassigned work.
- `room task create "title" --description "context"` puts work up for grabs.
- `room task claim <task>` records that you are working on it.
- `room task release <task>` makes it available again.
- `room task close <task> --note "result or PR link"` records completion.
- `room task reopen <task>` opens it again.

Task references can be an ID, its short suffix, or an unambiguous title. Include the
context another person or agent needs when creating work or handing it back. Choose
actions that serve the user's task; these commands are capabilities, not a required
sequence.

Use `room repo list`, `room repo add [URL]`, and `room repo issues <repository>` to
work with the room's repositories. With no URL, add reads the current checkout's
origin. `room task import <repository> <issue-number>` links an existing GitHub or
Gitea issue. Actions on linked tasks update the issue at its provider.

`room share [terminal]` shares the selected terminal, or your current terminal, with
all room members. Use `kodosi session --help` for opening and controlling shared
terminals. Everyone admitted has full control, including interrupt and Close.

Use the result of each action. If delivery is reported as uncertain, read the
current conversation or task before repeating the action.
