# Kodosi protocol 16: state and remaining work

Protocol 16 replaces the shared session key of protocol 15 with one encrypted
channel for each viewer device. This file holds only what the code and
`protocol/*.json` do not hold yet: the reasons, the measured results, and the
work that remains. When a part is built, move its contract to `protocol/*.json`
and delete it here.

Words: **Host** is the device that runs the terminal. **Viewer** is a device
that shows and controls it. **Owner** is the account of the host. **Relay** is
the server part that copies bytes between a host and a viewer.

## 1. Goals

1. The server cannot read terminal traffic and cannot send input.
2. An open view stays open through a sharing change, a slow link and a short
   loss of connection.
3. Typing has no delay that Kodosi adds. Output is not more than about half a
   second behind on a normal link; when a link cannot carry the stream, the
   viewer gets the current screen and not the backlog.
4. The server does little work for each terminal.
5. **The user does nothing extra.** A security step is acceptable only when it
   is rare (one time for each device or friend), short, and has a clear reason
   on the screen. No step can leave the user with no way forward.

Design choices:

- **Terminal state, not video.** Each viewer has a real terminal, so selection,
  copy, search and fonts are local and exact. An active terminal needs kilobytes
  each second. A video stream (GeForce NOW, Selkies) needs a video encoder on
  the host and megabits each second.
- **Taken from the video systems:** the sender adapts to each receiver; the
  newest state replaces a backlog; input does not wait for output.
- **Taken from Signal:** the server only moves encrypted data; each pair of
  devices has its own keys from a new exchange; a removed person gets nothing
  after the removal and the other people are not disturbed.
- **No own cryptography:** the channel is standard TLS 1.3 from `rustls`.
- **No new keys and no new enrolment.** The channel proves each device with the
  ML-DSA-65 signing key that its certificate has today.

## 2. Built: the channel (contract in `protocol/terminal-connections.json`)

- One TLS 1.3 channel for each viewer device, inside a relay pipe. Key exchange
  `X25519MLKEM768` (classic plus post-quantum), new keys for each channel, no
  resumption. A later theft of a device key does not open recorded traffic.
- The host is the authority for access: it admits a viewer only when the viewer
  account is its own account or a person that the host shared this terminal
  with, and the TLS key is the key of that device in the verified device list.
- The host keeps a stream for each viewer: window of half a second at the speed
  that the viewer acknowledges, the current screen in place of a backlog, input
  and results before waiting output.
- Input is one ordered byte stream. The view sends without waiting; the host
  confirms in order; nothing is sent again after a lost connection; the host
  refuses input that was held back.
- The server copies bytes between two sockets and reads none of them. It holds
  no terminal key. The key tables, key routes, order checks and snapshot proofs
  are removed.
- A sharing change closes only the channels of removed people, at once, on the
  host. Other views see nothing.

Measured in the local end-to-end setup (release build, one computer):

| Case | Protocol 15 at the start of the review | Protocol 16 |
|---|---|---|
| Hold a key, 150 ms round trip: last echo after release | 14.5 s | 0.1 s |
| Ctrl-C by a friend in an endless flood, host uplink 1 Mbit/s | 5.2 s | 0.5 s |
| The same, viewer link 1 Mbit/s and 100 ms round trip | not measured | 0.7 s |
| The same, 128 kbit/s on either side | view closed | 2.6 s to 3.6 s |
| Paste of 1 MB through a remote view | stopped at 355 KB | identical file |
| Sharing change while a second friend watches | all views get a new key and a new snapshot | the friend's view gets nothing |
| Server stops for 3 s | views closed | views stay open and continue |
| Join of a terminal with one line of 300,000 characters | reconnect loop | joins |

Known limit: at 128 kbit/s under an endless flood the delay is the time to send
one full snapshot (screen plus 1,024 history lines). A snapshot of the visible
screen only would make it about half a second. The terminal library snapshot
has no history limit today, so this needs a library change.

An open view of a remote terminal reports `connectionState: connected` with
`status: reconnecting` while its link is lost, and `running` again when the new
channel has its first keyframe. The Mac app and the Qt app keep the terminal on
screen in that state and show "Reconnecting" in the tile header.

## 3. Built: a device link with one typed code (contract in `protocol/backend-api.json`, `deviceLink`)

What the user does: on the new device, sign in; it shows a code of 12
characters at once. On a device that is already approved, type that code and
select "Approve". Nothing else. This is one time for each device.

Why a typed code and not a button: with a button, the server selects which
keys are approved, and the new device takes the account identity that the
server gives. With the code, each side proves itself to the other side and the
server cannot take the place of one of them.

- The new device makes the code at random. The server never gets it.
- The new device sends a proof that binds its keys to the code. The approving
  device signs only the request whose proof holds for the typed code. A request
  whose keys the server changed has no such proof.
- The approving device sends back a proof that binds the account identity to
  the code. The new device takes the account identity only with that proof.
- A device takes its own account identity from three sources only: it made the
  first device itself, or an approval proof, or the identity that it has on
  disk. It never takes that identity from the server on first use.
- The code has 60 bits and each try costs 2^20 rounds of PBKDF2-HMAC-SHA512, so
  a server that wants to find the code from a proof needs about 2^80 hash
  operations in the 10 minutes of the request. The new device counts the
  10 minutes itself.
- Input ignores case, spaces and hyphens, and reads O as 0 and I or L as 1.
- One proof costs about 50 ms on an Apple M-series computer.

An exchange that cannot be attacked offline at all (a PAKE) needs a new
cryptography library and more messages, and the new device could not finish
alone. The cost of 2^80 for each request makes that unnecessary.

No dead end: when no approved device is reachable, the new device offers
"Start fresh" as before (new identity; friends see one question, see 4).

## 4. Remaining: friends that the server cannot change

Today a friend's identity is taken from the server at first use. This is the
remaining way for a hostile server to get terminal access. The fix must follow
goal 5.

What the user does: nothing new. "Add friend" takes a username as today, or an
**invite text** that a friend copied from their app ("Copy my invite") and sent
in any chat. An invite text holds the username and a fingerprint of the
friend's identity, so a friend added by invite is verified with no more steps.
A friend added by username only is not verified; the app shows this as a small
mark and offers "Verify" (compare the fingerprint, 30 digits, one time).

- The device list gets the hash of the previous list, and each device keeps the
  newest list of each account that it reads and never accepts an older one.
- Friend records (account, identity fingerprint, verified) are a signed list
  that the user's own devices share; the server stores it and cannot change it.
- Sharing and connecting never take a friend's identity on first use. A friend
  from the server with no record in the signed list cannot be shared with.
- When a friend starts fresh, the app asks one time: "<name> set up Kodosi
  again. Share with them as before?" Until the user answers, that friend has no
  access. The answer is one click.

A short code made from both identities together is not used: the server could
select two false identities that give the same short code. A fingerprint is of
one identity only.

Surfaces: `friends.snapshot` items get `verified` and `identityState`
(`fixed`, `changed`); new `friends.invite` (own invite text),
`friends.verify {userId}`, `friends.approveIdentity {userId}`; `friends.add`
accepts an invite text.

## 5. Remaining: one connection for each device

Each approved device keeps one authenticated WebSocket for requests, events and
token renewal. This removes the challenge table and the signature on each
request (two HTTP requests for each call today), and it lets the server use one
lock for each account in place of the global lock. HTTP stays for health, first
enrolment and the start of a device link. Close codes carry a reason; the
device sends the protocol range that it supports.

Also in this part: each refused connection writes one log line with its reason;
one `System.Diagnostics.Metrics` meter (device connections, relay pipes, relay
bytes, refused connections by reason, seconds of database fault); the migration
is a separate command; an account can be deleted with one request.

## 6. Remaining: direct path

Inside an open channel the two sides try a direct QUIC connection (`iroh`, with
relay fallback). When it works, the viewer makes a new channel on it and closes
the relay pipe after the first keyframe. The channel is the same on each
transport. One setting, "Use direct connections", default on: a person who
shares a terminal with you already gives you full control of a program on
their computer, so hiding addresses between such people by default costs speed
and buys little.

## 7. Remaining: client marks

- After a lost connection the view says "Some of your last input was not sent"
  when the host did not confirm all input. Today this is only in the reason
  text of the lost connection.
- Qt: remove the limit of 20 tries for a view that is attached.

## 8. Order of work

| Step | Content | Proof |
|---|---|---|
| done | Channel, relay pipe, per-viewer stream, input stream | section 2 |
| done | Device link with one typed code in all three clients | section 3 |
| next | Friend list, invite text, no first use (4) | a friend that the server adds cannot be shared with |
| then | Device connection and server items (5) | database pause, restart with many devices |
| last | Direct path (6); screen-only keyframe (2); client marks (7) | two computers on different networks |

Each step keeps `just check-all` green, is tested on Linux x86-64 (`ssh lenovo`),
and changes the command-line app, the Qt app and the Mac app together when a
user surface changes.

## 9. Risks that stay

- The server reads terminal names, host names, device labels and handles, and
  sees who connects to whom, when, and padded sizes.
- Until section 4 is built, the server can replace a friend at first contact.
- A removed device that works with the server can show a reader that did not
  see the removal an older device list, until section 4 is built.
- A secret and attacker text in the same output frame leak a little through the
  frame size, also with padding and with each frame packed alone.
- The server can stop or delay service. It cannot deliver old input.
- A stolen device has the access of that device until an owner removes it.
