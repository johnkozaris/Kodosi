# Agents and tasks

The `kodosi room` CLI lets people and their existing agents participate in a room.
It uses the running Kodosi host's sign-in. The agent keeps its own execution,
permissions, settings, memory, and conversation history.

## Connect the agent

Start a terminal in a room and run your agent there. Load the output of
`kodosi room skill` through the harness's normal skill or instruction mechanism.
The [bundled skill](../runtime/skills/kodosi-room/SKILL.md) is also available as a file.

Inside a Kodosi terminal, the CLI finds the room through `KODOSI_SESSION_ID`. Outside
that context, use `kodosi room --room "Room name" …`; it can also select the only room
when there is exactly one. Add `--json` for structured output.

First make the CLI available on your shell's PATH using the
[Mac](../clients/macos/README.md#command-line-access) or
[Linux](../clients/linux/README.md#command-line-access) guide.

## Read and contribute

```sh
kodosi --json room context
kodosi --json room read
kodosi room post "The parser change is ready for review." --agent "Codex"
```

Context identifies the room, members, and shared terminals. Read returns room
conversation and work state. Messages have sequence numbers: use `--since` for
newer messages, `--before` for older history, and follow `hasOlder` to catch up fully.

```sh
kodosi --json room read --since 120
kodosi --json room read --before 80
kodosi --json room wait --after 120 --timeout 60
```

Waiting returns when room activity changes or the timeout expires. Agents decide
when to read, respond, or work; these commands do not impose a workflow.

## Put work up for grabs

```sh
kodosi room task create "Review parser changes" --description "Check malformed input and attach findings."
kodosi room task list --available
kodosi room task claim "Review parser changes"
kodosi room task close "Review parser changes" --note "Reviewed; findings are in the room conversation."
```

Use `task release` to make a claimed task available again and `task reopen` to reopen
completed work. A task reference can be its ID, short suffix, or unambiguous title.
Descriptions and completion notes can include the context or pull request another
person or agent needs. Add `--repo` when creating a task to associate a room repository;
repeat it for several repositories.

## Connect existing issues

A room can link multiple GitHub or Gitea repositories. From a checkout, add its
origin and inspect its issues:

```sh
kodosi room repo add
kodosi room repo list
kodosi room repo issues "Repository name"
kodosi room task import "Repository name" 42
```

You can pass a URL to `repo add`, or use `--directory` to select a checkout. Imported
tasks stay linked to their issue: task actions update the provider, and refreshing
the room picks up provider changes. Native tasks need no Git provider.

GitHub actions use the participant's local `gh` sign-in or Git credential helper.
Gitea uses the Git credential helper, or `KODOSI_GITEA_TOKEN` scoped to the origin in
`KODOSI_GITEA_URL`. The account needs the relevant issue access. Credentials stay on
that participant's machine; the Kodosi backend does not receive them. Provider
issues retain their provider's visibility.

## Share a terminal

```sh
kodosi room share
kodosi session start --room "Room name"
```

`room share` shares the current terminal; pass a terminal reference to choose another.
Everyone admitted to the room has full terminal control. Terminals and folders stay
on their hosts, so a room can combine work from several machines without a shared
filesystem.

Use `kodosi room --help` and each subcommand's `--help` for the current options. If an
action reports uncertain delivery, read the current state before repeating it.

[All documentation](README.md) · [Getting started](GETTING_STARTED.md)
