# Security

**English** · [Deutsch](de/SECURITY.md) · [Ελληνικά](el/SECURITY.md) · [Español](es/SECURITY.md) · [Français](fr/SECURITY.md) · [Italiano](it/SECURITY.md) · [日本語](ja/SECURITY.md) · [한국어](ko/SECURITY.md) · [简体中文](zh-Hans/SECURITY.md)

Your devices encrypt terminal traffic and room content end to end. Private keys stay on
devices. The backend routes connections and stores encrypted room data; it never runs
commands or sees their plaintext. The [encryption design](CRYPTOGRAPHY.md) gives the
threat model, mechanisms and known limits; no independent party has audited it yet.

## What sharing grants

Sharing a terminal grants full control with the host user's operating-system privileges.
A terminal in a room goes to the friends of its owner in that room, including friends
who join later. When you remove a friend, they lose your terminals, and each of you
leaves the rooms that the other owns. A shared terminal runs as you: the people in it
can also run `kodosi` as you and read and change your files on that computer, so share
it only with people whom you trust with that computer. Kodosi trusts everyone who shares
a terminal or a room, and the programs and agents in their terminals, and does not limit
what they do. Each of your approved devices controls all your terminals and can start
new ones on your other computers that run Kodosi. From any of your devices you can write
to Dosi on your computers, and Dosi acts there as it does when you write to it at the
computer. A device that you remove can no longer write to Dosi.

Everyone who can open a terminal also sees the files on its canvas and uses its shared
browser, so sign in there only where all of them may act. The shared browser of a shared
terminal never uses your kept sign-ins. The files on a canvas are normal files, and
everyone who can open the terminal can edit them. A page on the canvas and a live document
run in a closed frame with no network, no files and no storage that stays. While a shared
browser is awake, its debug port and the driver port of each window listen on `127.0.0.1`,
where other accounts of that computer can reach them. A site in the shared browser saves
one download into the terminal's folder for each action of a person or an agent, and it
reaches your clipboard only when you copy. When a device opens the browser of a terminal on
another computer, that computer listens on a TCP port for a few seconds, so that the two
can connect straight. Only that device gets through, with the encryption of the terminal.
The Linux app shows pages only where Chromium can sandbox them; on Ubuntu its AppArmor
profile lets the app, its page process and Kodosi Browser make the user namespaces that the
sandbox needs. For the same reason the shared browser does not run as root on Linux. The
apps do not contain Kodosi Browser. When a computer opens its first shared browser, its
host gets the browser from the feed on `updates.kodosi.com`, and it looks there for a new
version each day. The host uses a download only when the feed has the signature of the key
of Kodosi's updates and the download has the SHA-256 that the feed gives.

## What the service sees

The service sees account and device records, room membership, room and terminal
metadata, each terminal's agent and state (such as working or waiting for an approval,
never its text), routing fields, timing and ciphertext sizes. Encryption does not hide
this metadata or prevent an outage. A device that you remove loses access from then on.
After you remove a lost or stolen device, make a new recovery key to keep that device
out. When someone starts fresh on your account while you have devices, it waits a day
and your devices show it, so that you can stop it. GitHub and Gitea credentials stay on
your computer.

Deleting an account removes its devices, recovery data, friends, sharing and owned
rooms; messages in other people's rooms stay without an account. For 30 days the service
keeps the issuer and subject of the deleted sign-in to refuse older tokens.

## Check a download

Each release has `SHA256SUMS`, the SHA-256 of each of its files, and `SHA256SUMS.asc`, the
signature of that list by the key of Kodosi that also signs the apt repository. Put the two
files beside your download, then run:

```sh
curl -fsSLO https://updates.kodosi.com/apt/kodosi-archive-keyring.pgp
gpgv --keyring ./kodosi-archive-keyring.pgp SHA256SUMS.asc SHA256SUMS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

`gpgv`, from GnuPG, must report a good signature by the key
`E8007A36D199B6EE755B1EE9C612C00442E52299`, and `shasum` must say OK for your download.

## Report a vulnerability

Use [private vulnerability reporting](https://github.com/johnkozaris/Kodosi/security/advisories/new)
or write to [security@kodosi.com](mailto:security@kodosi.com) with the revision, steps,
impact and redacted logs. Keep details out of public issues.
