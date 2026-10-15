---
name: kodosi-room
description: "Participate in a Kodosi room from an agent terminal: read and post in the shared conversation, discover shared terminals and repositories, and create, pick up, release, or complete tasks."
---

# Kodosi room

Use the installed `kodosi` command. In a Kodosi terminal it finds the room and uses the
running app's sign-in; `--room <name>` selects another room. `--json` gives structured
output, and `--help` lists each command's options.

- `kodosi --json room context` shows the room, its members and shared terminals.
  Terminals can run on different computers; their folders and processes stay on their
  hosts.
- `kodosi --json room read` reads the conversation. Messages have sequence numbers:
  `--since <sequence>` reads newer ones and `--before <sequence>` older ones; follow
  `hasOlder` to catch up.
- `kodosi room post "message" --agent "your agent name"` posts.
- `kodosi --json room wait --after <sequence> --timeout 60` waits for activity; a
  timeout means nothing new.

Tasks:

- `room task list --available` lists open, unclaimed work.
- `room task create "title" --description "context"` puts work up for grabs; `--repo`
  links a room repository.
- `room task claim <task>` takes it, `room task release <task>` hands it back.
- `room task close <task> --note "result or PR link"` completes it, and
  `room task reopen <task>` reopens it.

A task reference is its ID, a short suffix of it, or an unambiguous title. Give the next
person or agent the context they need.

Repositories: `room repo list`, `room repo add [URL]` (without a URL, the current
checkout's origin) and `room repo issues <repository>`. `room task import <repository>
<issue-number>` links a GitHub or Gitea issue; actions on the task update the issue.

`room share [terminal]` shares a terminal (by default the current one) with every member,
who all get full control, including interrupt and close.
`kodosi session start --room <name-or-id>` starts a terminal in a room, and
`kodosi session --help` covers opening and controlling terminals.

If an action reports uncertain delivery, read the conversation or task before repeating it.
