# Security

Terminal traffic and room content are end-to-end encrypted. Private keys stay on
devices. The backend routes connections and stores encrypted room data; it never runs
commands or sees their plaintext. The [encryption design](CRYPTOGRAPHY.md) gives the
keys, rules and known limits; no independent party has audited it yet.

## What sharing grants

Sharing a terminal grants full control with the host user's operating-system privileges:
typing, interrupting and closing. Everyone in a room can use the room's shared
terminals, including members invited later. Sharing one terminal shares nothing else.

## How it works

- Each viewer has a TLS 1.3 channel to the hosting device, relayed by the backend, with
  device-key authentication and hybrid X25519/ML-KEM-768 key exchange.
- The host checks each viewer against signed device lists and room membership, and
  closes affected views when sharing or devices change.
- Room content uses AES-256-GCM with room keys wrapped for each member device with
  ML-KEM-768 and X25519. ML-DSA-65 signatures authenticate key history and content.
  Removing a member starts a new key epoch; what they already read stays read.
- A new device is approved with a code that only the two devices see. Invites carry a
  friend's identity; a friend added by username relies on the backend for the first
  identity, and a changed identity needs your decision.
- A recovery key approves a new device when no other can. The service never receives
  it. After you remove a lost or stolen device, make a new recovery key.

## What the service sees

Account and device records, room membership, room and terminal metadata, each
terminal's agent and state (such as working or waiting for an approval, never its
text), routing fields, timing and ciphertext sizes. Encryption does not hide this
metadata or prevent an outage. A compromised device can read what it could already
read; removing it stops future access. GitHub and Gitea credentials stay on your
computer.

Deleting an account removes its devices, recovery data, friends, sharing and owned
rooms. Messages in other people's rooms stay without an account. For 30 days the service
keeps the issuer and subject of the deleted sign-in to refuse older tokens.

## Report a vulnerability

Use [private vulnerability reporting](https://github.com/johnkozaris/Kodosi/security/advisories/new)
or write to [security@kodosi.com](mailto:security@kodosi.com) with the revision, steps,
impact and redacted logs. Keep details out of public issues.
