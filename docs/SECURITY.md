# Security

Kodosi encrypts terminal traffic and room content end to end. Private keys stay on
participant devices. The backend routes connections and stores encrypted room data;
it does not run terminal commands or receive their plaintext.

Sharing a terminal grants full control with the host user's operating-system
privileges, including typing, interrupting, and closing. Everyone admitted to its
room can use that shared terminal, including members invited later. This is the
product's collaboration model.

## Terminal connections

Each viewer has a TLS 1.3 channel to the hosting device, carried through the backend
relay. The channel authenticates device keys and uses hybrid X25519/ML-KEM-768 key
exchange. The service copies encrypted records and holds no terminal keys.

The host checks sharing against signed account/device identities and room membership.
When the host has its own friend record of a room member, that record must agree with
the identity in the room.
Sharing changes and device removal close affected views without disturbing other
viewers. Reconnection restores ordered terminal state; input with uncertain delivery
is not replayed.

## Room content

Messages, tasks, and repository links are encrypted with AES-256-GCM. Room keys are
wrapped for participant devices using ML-KEM-768 and X25519 together, with HKDF-SHA256.
ML-DSA-65 signatures authenticate room key state and content.

Membership changes produce signed key history. Removing a member advances the key
epoch; wrapped prior keys preserve the conversation history available to current
members and people invited later. Removal cannot erase content someone already read.

A room continues while a member is away. When a member starts fresh with a new
identity, the other members stop giving new room keys to the earlier devices. After the
room owner trusts the new identity in Friends, the member is in the room again with the
full history. An owner who starts fresh makes a new room.

## Identity and visible metadata

A new device is approved using a code from that device; the service never receives
the code. Account devices sign their device lists and friend identity records.
Invites carry a friend's identity for verification. Adding a friend by username
without an invite initially relies on the service's identity response; a changed
identity needs the user's decision to trust it again.

A person can make a recovery key in Settings. It approves a new device when no other
device can, and keeps friends and rooms. Kodosi shows it one time and the service never
gets it. A person with the recovery key and the account sign-in can approve a device.
After you remove a lost or stolen device, make a new recovery key.
Without a recovery key, a person who lost all devices starts fresh with a new identity.

The service can see account and device records, room membership, room and terminal
metadata, content routing fields, timing, and ciphertext sizes. Encryption does not
hide that metadata or prevent service interruption. A compromised participant device
can access what that device could access; removing it stops future authorized access.

GitHub and Gitea credentials stay on the participant's machine. Linked issues retain
the external provider's visibility and access rules.

[Encryption design and threat model](CRYPTOGRAPHY.md) specifies the keys, the rules,
and the known limits. The current contracts and source map are in
[Architecture and protocol](PROTOCOL.md). This describes the implementation; it is
not a claim of an independent security audit.

## Report a vulnerability

Use [GitHub's private vulnerability reporting](https://github.com/johnkozaris/Kodosi/security/advisories/new).
Include the affected revision, reproduction steps, impact, and any useful redacted
logs. Keep exploitable details and private data out of public issues.

For ordinary bugs or feature requests, use
[GitHub issues](https://github.com/johnkozaris/Kodosi/issues).

[All documentation](README.md)
