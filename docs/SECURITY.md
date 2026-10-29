# Security

Your devices encrypt terminal traffic and room content end to end. Private keys stay on
devices. The backend routes connections and stores encrypted room data; it never runs
commands or sees their plaintext. The [encryption design](CRYPTOGRAPHY.md) gives the
threat model, mechanisms and known limits; no independent party has audited it yet.

## What sharing grants

Sharing a terminal grants full control with the host user's operating-system privileges.
Everyone in a room can use the room's shared terminals, including members invited
later. Sharing one terminal shares nothing else. Kodosi trusts everyone who shares a
terminal or a room, and the programs and agents in their terminals, and does not limit
what they do. Each of your approved devices controls
all your terminals and can start new ones on your other computers that run Kodosi.

Everyone who can open a terminal also sees the files on its canvas and uses its shared
browser with the sign-ins in it, so sign in there only where all of them may act. A page
on the canvas runs in a closed frame with no network, no files and no storage that
stays. While a shared browser is awake, its Chrome and agent-browser's picture stream
listen on `127.0.0.1`, where other accounts of that computer can reach them. The Linux
app shows pages only where Chromium can sandbox them; on Ubuntu its AppArmor profile lets
the app and its page process make the user namespaces that the sandbox needs.

## What the service sees

The service sees account and device records, room membership, room and terminal
metadata, each terminal's agent and state (such as working or waiting for an approval,
never its text), routing fields, timing and ciphertext sizes. Encryption does not hide
this metadata or prevent an outage. A device that you remove loses access from then on.
After you remove a lost or stolen device, make a new recovery key to keep that device
out. GitHub and Gitea credentials stay
on your computer.

Deleting an account removes its devices, recovery data, friends, sharing and owned
rooms; messages in other people's rooms stay without an account. For 30 days the service
keeps the issuer and subject of the deleted sign-in to refuse older tokens.

## Report a vulnerability

Use [private vulnerability reporting](https://github.com/johnkozaris/Kodosi/security/advisories/new)
or write to [security@kodosi.com](mailto:security@kodosi.com) with the revision, steps,
impact and redacted logs. Keep details out of public issues.
