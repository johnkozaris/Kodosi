# Kodosi

Kodosi is a native terminal app for running sessions on your computer, reaching them
from your other approved devices, and sharing selected sessions with trusted friends.
The shared runtime supports the Swift macOS client, Linux Qt client, and CLI.

## Direction, not a finished specification

The September 2026 redesign moved Kodosi toward a smaller, terminal-first product.
This document captures its current intent, not a complete design or proof that every
workflow is implemented correctly. Some details, interactions, and architectural
choices are unfinished or may be wrong.

Use judgment and creative thinking. Look for missing workflows, question inherited
assumptions, and propose simpler or better ways to deliver the experience. Neither
this document nor the existing code should force a poor solution just because it is
already written down. The audit handoff contains leads, not predetermined answers.

Use `/code-cleanup:code-cleanup`, `/code-review`, and `/simplify` as appropriate for
the work, following their current instructions rather than a separate procedure in
these docs. Product direction and implementation details can be reassessed; genuine
changes to consent, trust, or user-facing guarantees need an explicit product decision.

## Terminals that stay understandable

The host owns a terminal's process, PTY, terminal state, and ordered output. Views
attach to that session; opening or reconnecting a view should show fresh state and
continue the output without disrupting other viewers. The current transport uses
native checkpoints followed by ordered output, with bounded buffering.

Minimize hides a terminal view and keeps its process running. Close ends the terminal.
Closing the app window leaves the host running; Quit ends its local processes, not
processes on other computers. There is no separate stopped-terminal archive.

Disconnected or stale views cannot control a replacement process. Input whose
delivery became uncertain is not automatically replayed. Runtime admission is not
proof that a shell command completed.

An expired remote listing does not kill its host's local terminal. Reconnection can
restore owner-device access with fresh keys; it must not silently restore potentially
revoked friend sharing.

## Explicit, trusted sharing

Approved own devices can use the owner's sessions. An owner selects which friends
may use each terminal. Admitted participants have full terminal control, including
input, resize, focus, interrupt, and closing the terminal. Sharing and identity administration remain
owner actions; current connection and focus/resize coordination still matter.

Friendship alone does not share a terminal. Removing a friend, device, or session
share removes its access and queued delivery. Subsequent traffic uses current
permissions and keys. Revocation cannot undo commands or retract data already received.

Shared shell access carries the host OS user's privileges. It is not a project
sandbox or a restricted agent role.

## Missions as useful grouping

A Mission currently has a name, invited members, and attached-terminal references.
Members accept invitations, but membership never grants terminal access. The UI and
CLI call these Missions; the backend/protocol currently use `room`.

The current scope is grouping, not Mission chat, task boards, or agent dispatch.
Those would be separate product exploration, not a reason to preserve dormant
implementations or empty controls from the previous app.

## Native provider workflows

Kodosi currently previews saved Claude Code and Copilot CLI conversations through
bounded, read-only discovery. Explicit Resume validates the saved identity and starts
a new process through the installed provider's native resume command; it does not
revive a stopped Kodosi shell.

Provider prompts and permissions stay in the native CLI. Ordinary shell launch does
not wrap executables, install hooks/plugins, inject telemetry, or add permission-bypass
flags. Provider settings are original local files to locate and open in the user's
editor, not a second configuration or memory-management system owned by Kodosi.

Live agent-intelligence dashboards, approval interception, semantic steering queues,
and global plugin/skill catalogs are outside the current focus. Better ideas are
welcome, but do not bring the old product back accidentally through retained machinery.

## Trust and data boundaries

Private device keys and terminal plaintext stay on endpoint devices. The backend
stores account/device/friend/Mission metadata, current session sharing, encrypted key
envelopes, and bounded encrypted relay traffic. It sees routing information and sizes,
not plaintext commands, and does not generate content keys or execute commands.

Endpoints verify approved identities and recipients. Current host-publication,
connection, and key identities bind traffic. Cross-user bootstrap currently uses
trust on first use; substitution before the first pin is outside that threat model.
Existing pins must not be silently reset or replaced through re-TOFU. An account owner
who has lost every trusted device may explicitly reset the account's device list from a
freshly signed-in device; that revokes every other device and issues a new identity
incarnation that friends must confirm again before reconnecting.

Development and validation must preserve live databases, provider configuration,
memory, conversations, credentials, and working files. Use disposable isolated data.
Do not mistake an incomplete design for permission to weaken these boundaries.
