# Kodosi

**Local-first mission control for coding agents and terminal sessions.**

Kodosi lets a developer run coding agents on one Mac and supervise those sessions from
the same Mac or another enrolled Mac. It brings live terminals, agent activity,
permission decisions, steering, session continuity, people, and tasks into one cockpit.
Authorized teammates can join with deliberately limited control.

The agent, shell, working tree, credentials, and terminal authority remain on the host
Mac. Kodosi's backend coordinates product state and relays end-to-end-encrypted content;
it does not run the agent or become the source of truth for the session.

## The Product Promise

Kodosi should let a developer answer three questions from one place:

1. **What are my agents doing?** See local and remote sessions, live terminal output,
   current activity, pending permissions, failures, and work that needs attention.
2. **Where do I need to intervene?** Approve or deny a proposed action; send
   terminal input; queue or steer an instruction; interrupt or stop a session.
3. **Who should be involved?** Bring another person into a session with scoped access,
   or coordinate people and agents together in a Mission.

This is active supervision, not passive observability. Live terminal output, current
activity, transcripts, and pending requests provide context for a decision; the
decision and its safe execution are the product.

## Core Experience

### My Agents

My Agents is the fleet cockpit. It combines sessions running locally with sessions
running on another enrolled host and sessions shared by another person.

A developer can:

- start, resume, inspect, focus, and stop agent-backed terminal sessions;
- watch several live terminals and focus the session that needs intervention;
- attach to a remote terminal, recover its current state, and continue following live
  output after reconnect;
- send terminal input when authorized, rather than falling back to a separate chat
  approximation;
- see current agent activity and pending requests;
- act on permission requests and other attention items without searching across
  terminal windows.

Claude Code and GitHub Copilot CLI are the currently supported coding-agent surfaces.
Kodosi supervises the CLI the developer chose; it does not replace that agent or require
the agent to execute in Kodosi's cloud.

### Supervision and Steering

Kodosi exposes the actions required to keep autonomous work safe and useful:

- approve or deny a requested operation;
- queue guidance for the next turn;
- steer at a supported tool boundary;
- stop current work and send a replacement instruction;
- interrupt or stop a session.

Actions are bound to the exact account, session incarnation, request, and authority that
made them valid. A timeout, disconnect, or uncertain delivery must be reconciled with
the runtime rather than presented as success.

### Remote Agents and Terminal Sessions

Remote control is a first-class product capability, not a secondary screen-sharing
mode. From the controller's perspective, the agent and terminal may be running on
another enrolled Mac. From the system's perspective, execution is always local to the
host that owns the session.

An authorized remote controller can, according to capability:

- observe the live terminal and agent state;
- suggest an instruction;
- inject terminal input;
- approve or deny supervised operations;
- steer, resize, focus, interrupt, or stop where the granted authority permits.

The host runtime remains authoritative for the PTY, process, terminal ordering, and
supervision policy. A remote client receives a recoverable encrypted
projection and sends authenticated intent back to that runtime. It never becomes a
second execution or terminal authority.

Access must be legible in the product. A session that is read-only, reconnecting, stale,
or no longer authorized must not look fully controllable. Commands that no longer match
the current session incarnation or permission set fail closed.

### Missions

A Mission is a persistent collaboration space for people and agents. It combines:

- membership and invitations;
- encrypted conversation;
- a task queue with assignment and review state;
- agent sessions attached to the Mission;
- directed messages to people, agents, or the whole Mission;
- terminal inspection and intervention without leaving the shared context.

Missions are not merely folders around terminals. They let a team coordinate work where
some participants are human and some are coding agents. A teammate can watch, suggest,
inject, or approve only to the extent explicitly granted by the session owner.

Mission is the product and UI name. The CLI, protocol, and backend retain `room` as the
internal transport and persistence term for the same collaboration space.

## Local-First Security and Trust

- Agent execution, source access, shell credentials, and private device keys stay on
  endpoint devices.
- Terminal, pending-approval, room, and control content is encrypted between authorized
  endpoints. The backend relays ciphertext and does not receive plaintext content keys.
- The backend necessarily sees product and routing metadata such as accounts, devices,
  room membership, session membership, timestamps, access levels, and frame sizes.
- Device lists, room rosters, session incarnations, and sensitive actions are signed and
  generation-bound so stale or replayed authority can be rejected.
- Cross-user identity bootstrap is trust-on-first-use. Changes fail closed after the
  first pin, but an actively malicious backend substituting identity material before
  that first pin remains outside the current threat model.

Local-first does not mean single-device. It means the machine running the agent remains
the execution authority while authorized devices collaborate through encrypted,
capability-gated connections.

## Product Principles

1. **Execution stays with the owner.** Remote control must not require moving source,
   secrets, or agent execution into Kodosi's backend.
2. **Remote is as real as local.** A remote terminal is the live session, not a delayed
   screenshot or a disconnected chat replica.
3. **Act, do not merely observe.** Attention, permission, steering, and interruption
   belong on the shortest path through the product.
4. **Authority is explicit.** Ownership, access level, connection state, and action
   availability must remain visible and fail closed.
5. **One cockpit above multiple CLIs.** Vendor-specific intelligence stays behind
   adapters; supervision remains coherent across supported agents.
6. **People and agents share the workflow.** Missions coordinate a mixed team rather
   than treating collaboration as a link to an isolated terminal.
7. **Continuity is honest.** Reconnect, recovery, uncertain outcomes, and stale
   incarnations are product states, not errors to hide behind optimistic UI.

## What Kodosi Is Not

- It is not a cloud coding-agent runner.
- It is not generic SSH or screen sharing.
- It is not only a terminal transport.
- It is not a single-vendor agent dashboard.
- It does not make the backend authoritative for terminal state or decrypted content.

Terminal streaming is an enabling capability. The product is the ability to supervise
and coordinate local or remote coding work safely from one place.

## Shipping Boundary

- `rustProcess/` owns the arm64 macOS runtime, PTY and process lifecycle, terminal
  semantics, supervision, identity, sharing, CLI, and C ABI.
- `dotnetBackend/` owns durable product metadata, HTTP APIs, PostgreSQL persistence, and
  the bounded ciphertext relay.
- `protocol/*.json` owns normative cross-stack contracts.
- `../kodosiSwift` is the shipping native macOS client and presentation layer.
- `../kodosi-ghostty` supplies the pinned native renderer and headless terminal engine
  used across the runtime and client.

The SwiftUI macOS client and arm64 macOS runtime are the supported local product.
Electron and Windows/Linux desktop and local-runtime surfaces are retired. Linux remains
a backend service deployment target.

Market positioning and strategic direction live in
[`docs/review-2026-06/PRODUCT-DIRECTION.md`](docs/review-2026-06/PRODUCT-DIRECTION.md).
