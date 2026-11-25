# Security

**English** · [Deutsch](de/SECURITY.md) · [Ελληνικά](el/SECURITY.md) · [Español](es/SECURITY.md) · [Français](fr/SECURITY.md) · [Italiano](it/SECURITY.md) · [日本語](ja/SECURITY.md) · [한국어](ko/SECURITY.md) · [简体中文](zh-Hans/SECURITY.md)

Terminal traffic and project content are end-to-end encrypted by your devices, and private keys
never leave your devices unencrypted. The backend routes connections and stores encrypted project
records; it never executes commands or sees their plaintext. For the full threat model,
cryptographic mechanisms, and known limitations, see the [encryption design](CRYPTOGRAPHY.md). No
independent third party has audited this design or implementation yet.

## What sharing grants

Sharing a terminal gives recipients full control with your OS privileges on that computer.
Sharing a terminal into a project makes it accessible to all of your friends in that project,
including friends who join later. Because a shared terminal runs as you, anyone in it can also run
`kodosi` as you and read or modify files on that machine. Only share terminals with people you
trust with that computer.

Kodosi does not restrict what participants or their agents can do inside a shared terminal or
project. Removing a friend immediately revokes access to your terminals and removes each of you
from projects owned by the other. Blocking a user does the same and prevents future friend
requests.

Each of your approved devices can control all your terminals, launch new ones on your other
computers running Kodosi, and message Dosi on those computers (where Dosi acts with the same local
access as when you use it directly). Removing a device revokes its access to terminals and Dosi.

### Canvas and shared browser

Everyone with access to a terminal can view and edit files on its canvas and use its shared
browser, so only sign in to accounts in a shared browser that every participant should be allowed
to use. Shared terminals never reuse your saved browser logins from private terminals. Canvas
files are regular files on the host computer.

HTML pages and live document views run inside an isolated frame with no network access, no file
access, and no persistent storage. While a shared browser is awake, its debug port and each
window's driver port bind to `127.0.0.1` or `::1`, which means other local OS user accounts on
that same machine can reach them. Sites in the shared browser can save at most one download into
the terminal's folder per user or agent action, and can only access your clipboard when you
explicitly copy.

On Linux, Kodosi only renders pages where Chromium's sandbox can run. On Ubuntu, Kodosi ships an
AppArmor profile permitting the app, its renderer process, and Kodosi Browser to create the user
namespaces Chromium's sandbox requires. For the same reason, the shared browser does not run as
root on Linux.

Kodosi apps don't bundle Kodosi Browser. The first time you open a shared browser on a computer,
Kodosi downloads it from `updates.kodosi.com` and checks daily for updates. Downloads are only
accepted if the update feed carries a valid signature from Kodosi's update key and the file's
SHA-256 matches the feed.

### A Mac's screen

Turning on screen sharing in a Mac's **Settings › General** lets each of your approved devices see
and control that Mac's whole screen, not only its terminals, from the canvas of its terminals.
Friends never see it, even in a terminal that you share with them, and the video and input travel
end to end encrypted like the terminals themselves. Kodosi Screen, a helper inside the app, holds
the Screen Recording and Accessibility permissions that this takes, so the programs in your
terminals do not gain them. While someone watches, the Mac's menu bar says who; turning the sharing
off, or **Stop sharing** there, ends every view.

## Direct connections

When two of your own devices can reach each other directly, on the same LAN or through NAT
traversal, terminal traffic travels directly between them with the same end-to-end encryption
instead of going through the relay server. Both devices see each other's IP addresses. When they
cannot, the traffic goes through the relay server over UDP with the same encryption, and neither
device sees the other's IP address.

To discover its public address, Kodosi asks the Kodosi server, which sees that IP anyway.
While searching for or holding either connection (and for one minute afterward), the device
keeps a single UDP port open and only accepts connections from your own approved devices.
Terminals shared with other people always go through the relay server, so other users never see
your IP addresses.

## What the service sees

The backend sees account and device records, project membership, project and terminal metadata,
each terminal's agent type and reported state (such as working or waiting for approval, never
terminal text), whether you have viewed that state yet, routing headers, timing, and ciphertext
sizes (except for traffic taking a direct peer-to-peer path). It also sees per-device activity
status (in Kodosi, in another desktop app, or idle) so notifications route only to an active
device.

Mobile push notifications travel through Apple's APNs or Google's FCM. Those providers see the IDs
of terminals that are waiting, finished, or failed along with their agent type and state, never
terminal names or contents.

Encryption does not hide this metadata or protect against server outages. Removing a device cuts
off access from that point on; if a device is lost or stolen, also generate a new recovery key to
keep that device out. If someone starts a fresh identity on your account while you still have
active devices, the backend delays it by 24 hours and notifies your existing devices so you can
cancel it. GitHub and Gitea credentials stay on your machine.

Deleting your account removes its devices, recovery data, friend list, active shares, and owned
projects. Messages you posted in other people's projects remain without an associated account. The
backend retains the deleted sign-in's issuer and subject for 30 days to reject previously issued
tokens.

## Verify a download

Every release includes `SHA256SUMS` (the SHA-256 digest of each artifact) and `SHA256SUMS.asc`,
signed with the same Kodosi key that signs the apt repository. Place both files next to your
download and run:

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv` (from GnuPG) must report a good signature from key
`E8007A36D199B6EE755B1EE9C612C00442E52299`, and `shasum` must output `OK` for your file.

## Report a vulnerability

Use [GitHub private vulnerability reporting](https://github.com/johnkozaris/Kodosi/security/advisories/new)
or email [security@kodosi.com](mailto:security@kodosi.com) with the commit or version, reproduction
steps, impact, and redacted logs. Please keep vulnerability details out of public issues.
