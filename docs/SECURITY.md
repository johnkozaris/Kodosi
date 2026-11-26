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

Kodosi asks you first, in the app of the device where it starts, before any change that lets someone
new into your terminals or projects, including changes the `kodosi` command asks for, such as an
agent's. This guards against mistakes and agents; someone typing in a terminal you shared acts as
you and can get around it.

Each of your approved devices can control all your terminals, choose who they're shared with, launch
new ones on your other computers running Kodosi, and message Dosi on those computers (where Dosi
acts with the same local access as when you use it directly). Removing a device revokes its access
to terminals and Dosi.

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
Kodosi downloads it from Kodosi's releases on GitHub, and it checks daily for updates in Kodosi's
update feed on `updates.kodosi.com`. Downloads are only accepted if the update feed carries a valid
signature from Kodosi's update key and the file's SHA-256 matches the feed.

### What Kodosi Browser sends to Google

Kodosi Browser is Chromium, which calls these services of Google by itself, none of them with your
cookies:

- Updates of its parts, such as the list of revoked certificates: about every five hours it asks
  `update.googleapis.com` for new versions, and downloads them from Google's download servers
  (such as `edgedl.me.gvt1.com`). It sends the ID and version of each part, its own version, and
  the operating system and processor, but no browsing data.
- Safe Browsing: it keeps lists of dangerous sites from `safebrowsing.googleapis.com`, sending only
  the state of its lists. When a page's address matches a list, it sends the first 4 bytes of a
  hash of the address, never the address. If its phishing check on your computer takes a page for
  phishing, it sends that page's address to `sb-ssl.google.com` to confirm.
- Network time: when it isn't sure of the time, it asks `clients2.google.com`, which tells it
  whether a site's certificate expired even if your clock is wrong. The request carries nothing
  about you.

**Form predictions from Google** stays off until you turn it on in **Settings › Browser**. Then
Kodosi Browser sends `content-autofill.googleapis.com` a hash of each form it shows, made from the
site's name and the names of the form's fields, and Google answers what each field is for. When you
send a form, it also tells which kind of data each field held, never what you typed. Kodosi Browser
never connects to `www.google.com` ahead of a search, and never asks Google, with your Google
cookies, whether Chrome's own address bar may offer AI Mode.

**Cross-site cookies** also stay off until you turn them on in **Settings › Browser**. While they're
off, a site shown inside another site's page, such as a YouTube video or a sign-in button, doesn't
get its cookies there, so it can't recognize you across the sites you visit.

### A computer's screen

Turning on screen sharing in a computer's Kodosi settings lets each of your approved devices see
and control that computer's whole screen, not only its terminals, from the canvas of its terminals.
Friends never see it, even in a terminal that you share with them, and the video, the input and the
clipboard text travel end to end encrypted like the terminals themselves. Kodosi Screen, a helper
of Kodosi's, does the capture and the input. On a Mac it holds the Screen Recording and
Accessibility permissions that this takes, so the programs in your terminals do not gain them. On
Linux the desktop grants the sharing to Kodosi through its screen sharing portal and shows its own
notice while the screen is shared. While someone watches, the computer's menu bar or system tray
says which devices watch; turning the sharing off, or **Stop sharing** there, ends every view.

While a device controls the screen, text copied on the computer goes to that device, and a paste on
it gives the computer that device's text. Text that a password manager marks as private stays on
the computer. While the person at the computer uses its mouse or keyboard, viewers' input waits a
moment, and a locked screen stays locked: viewers see that it is locked, not what it shows.

## Direct connections

When two of your own devices can reach each other directly, on the same LAN or through NAT
traversal, terminal traffic travels directly between them with the same end-to-end encryption
instead of going through the relay server. Both devices see each other's IP addresses. When they
cannot, the traffic goes through the relay server over UDP with the same encryption, and neither
device sees the other's IP address.

To discover its public address, Kodosi asks the Kodosi server, which sees that IP anyway.
While searching for or holding either connection (and for one minute afterward), the device
keeps a single UDP port open and only accepts connections from devices that view one of your
terminals.
Terminals shared with other people always go through the relay server, so other users never see
your IP addresses.

## What the service sees

The backend sees account and device records, project membership, project and terminal metadata,
each terminal's agent type and reported state (such as working or waiting for approval, never
terminal text), whether you have viewed that state yet, routing headers, timing, and ciphertext
sizes (except for traffic taking a direct peer-to-peer path). It also sees per-device activity
status (in Kodosi, in another desktop app, or idle) so notifications route only to an active
device. It does not see how your computers wake: their network addresses and wake settings travel
only between your own devices, encrypted end to end.

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
