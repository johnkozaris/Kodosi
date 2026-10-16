# Security

Terminal traffic and room content are end-to-end encrypted. Private keys stay on
devices. The backend routes connections and stores encrypted room data; it never runs
commands or sees their plaintext. The [encryption design](CRYPTOGRAPHY.md) gives the
threat model, mechanisms and known limits; no independent party has audited it yet.

## What sharing grants

Sharing a terminal grants full control with the host user's operating-system privileges.
Everyone in a room can use the room's shared terminals, including members invited
later. Sharing one terminal shares nothing else. People who share are trusted, and so
are the programs and agents in their terminals.

## What the service sees

Account and device records, room membership, room and terminal metadata, each
terminal's agent and state (such as working or waiting for an approval, never its
text), routing fields, timing and ciphertext sizes. Encryption does not hide this
metadata or prevent an outage. A removed device loses future access; a new recovery key
after removing a lost or stolen device keeps it out. GitHub and Gitea credentials stay
on your computer.

Deleting an account removes its devices, recovery data, friends, sharing and owned
rooms; messages in other people's rooms stay without an account. For 30 days the service
keeps the issuer and subject of the deleted sign-in to refuse older tokens.

## Report a vulnerability

Use [private vulnerability reporting](https://github.com/johnkozaris/Kodosi/security/advisories/new)
or write to [security@kodosi.com](mailto:security@kodosi.com) with the revision, steps,
impact and redacted logs. Keep details out of public issues.
