# Kodosi

Kodosi is a native terminal app for running sessions on your computer, using them
from your other approved devices, and sharing selected sessions with trusted friends.
The same runtime supports the Swift macOS client, the Linux Qt client, and the CLI.

## Terminals and connections

The host owns each terminal's process, PTY, terminal state, and ordered output.
Opening a view attaches to that process. A new or reconnected view receives a fresh
terminal checkpoint followed by ordered output. It does not become another terminal
authority or reset other viewers.

Closing a view disconnects that view and leaves the session running. Stop ends the
process. Quitting the hosting app ends its local processes. Ended sessions leave the
live list; Kodosi does not reopen stopped shells or maintain a stopped-session shelf.

A disconnected view cannot send input. Commands carry the current session and
connection identities so stale input cannot affect a replacement process. Kodosi does
not replay input whose delivery became uncertain during a disconnect.

If a host stays offline long enough for its remote listing to expire, its local
terminal keeps running. Reconnection restores owner-device access with fresh keys;
the owner must select friends again rather than restore possibly revoked sharing.

## Devices and sharing

The owner's approved devices can use the owner's sessions. The owner can also select
which friends may use each session. Every admitted participant has full terminal
control: input, resize, focus, interrupt, and Stop. Sharing administration and device
approval remain owner actions.

Adding a friend never shares a session automatically. Removing a friend, device, or
session share removes the corresponding access and queued delivery. New traffic uses
current authorization and keys. Revocation cannot undo shell commands already run.

Shared shell access gives a trusted person the host user's shell privileges. It is
not per-project operating-system isolation or a restricted agent permission role.

## Missions

A Mission has a name, invited members, and attached-terminal references. Members must
accept invitations. Terminal access is still selected per session; Mission membership
never grants it. The protocol and backend use `room` for this directory metadata; the
CLI and user interface call it a Mission.

Missions have no chat, task board, agent dispatch, or encrypted content history.

## Provider conversations and configuration

Kodosi can discover and preview saved Claude Code and Copilot CLI conversations for a
chosen local directory. Reads are bounded and read-only. Resume validates the saved
native identity and starts a new process through the installed provider's native
resume command. It does not recover a stopped Kodosi process.

Provider prompts and permissions stay in the native CLI. Normal shells do not install
plugins, inject telemetry, wrap provider executables, or pass permission-bypass flags.
The configuration screen locates native settings files for opening in a local editor;
it does not own a separate settings tree or modify provider memory.

## Trust and storage

Private device keys and terminal plaintext stay on endpoint devices. The backend
stores accounts, device records, friendships, Mission metadata, current session/share
metadata, and encrypted session-key envelopes. It sees routing information and traffic
sizes but does not generate content keys or execute commands.

Endpoints verify approved device identities and current recipients before sharing
keys or accepting control. Session publication, key generation, and connection
identities bind traffic to the current host run. Cross-user identity bootstrap is
trust on first use. A backend substituting identity material before the first pin
remains outside the current threat model.

Kodosi does not provide live agent-intelligence dashboards, approval interception,
semantic instruction queues, global plugin or skill catalogs, memory management, or
agent orchestration. Provider-native terminal behavior remains the source of truth.
