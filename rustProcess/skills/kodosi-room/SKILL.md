---
name: kodosi-room
description: Coordinate with reachable members of a Kodosi Mission through the room CLI.
allowed-tools: [Bash]
---

# Kodosi Mission channel

Kodosi calls these collaboration spaces Missions in the product and UI. The CLI,
protocol, tools, and event source retain `room` as their internal name. A Mission is
not a Claude teammate group or vendor subagent tree; the durable Kodosi mailbox is
authoritative.

Mission messages and assigned-task changes arrive as `room` events in session
context when the session-scoped adapter is active. Do not poll. When Claude receives a
`<channel source="kodosi-room" ...>` event, keep its exact `event_id` and
`cursor` and use one of the `kodosi_room_*` tools:

- `kodosi_room_acknowledge` when no reply or task transition is needed.
- `kodosi_room_reply` to post an explicit Mission reply.
- `kodosi_room_task_transition` to claim, submit, complete, archive, or reopen
  the offered task.

Copilot exposes corresponding `kodosi_room_reply` and
`kodosi_room_task_transition` extension tools. A successful tool call is
evidence of action; transport delivery alone is not.

For an explicit manual fallback, inspect `kodosi --help`, `kodosi msg inbox
--help`, and `kodosi room --help`. Never run an inbox polling loop from the
model.
