# Kodosi protocol 16: reasons and results

Protocol 16 replaced the shared session key of protocol 15 with one encrypted
channel for each viewer device. All parts are built. This file holds only what
the code and `protocol/*.json` do not hold: the reasons, the measured results,
the parts that were decided against, and the risks that stay.

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
- **No new keys.** The channel proves each device with the ML-DSA-65 signing
  key of its certificate. A device has no other key: the ML-KEM key that
  received the shared session key of protocol 15 is removed from the device
  certificate (certificate format 3).

## 2. Built: the channel (contract in `protocol/terminal-connections.json`)

- One TLS 1.3 channel for each view, inside a relay pipe. Key exchange
  `X25519MLKEM768` (classic plus post-quantum) for each channel. A later theft
  of a device key does not open recorded traffic.
- **One connection for each device.** A device keeps one WebSocket to the
  server (`/ws/device`). It carries the change notices, the host registrations
  and all relay pipes of the device. A view is one pipe: a 16-byte identity in
  front of each message. Before, each view needed a new connection with its own
  handshake and device check, and the host opened a second connection for each
  viewer.
- The server still admits each pipe: it checks the terminal access before it
  tells the host, it closes the pipes of a removed person at once, and it holds
  a bounded quantity of data for each device.
- **A later view of the same host resumes.** The first channel between two
  devices is a full TLS handshake with both device keys. A later channel uses a
  TLS session ticket with a new key exchange, so its keys are new also. The
  host sends its answer and the first screen with its first flight. A later
  view thus starts in one round trip.
- The host is the authority for access: it admits a viewer only when the viewer
  account is its own account or a person that the host shared this terminal
  with, and the TLS key is the key of that device in the verified device list.
  The server tells the host which account and device opened the pipe; the host
  trusts only the key check.
- **Identity records only when they change.** A device keeps each verified
  identity in memory with the tag that the server gave. It uses the kept record
  for 10 minutes without a request. After that time, after a friend change
  notice, or when a device or a key is not in the record, it asks the server
  with the tag and gets "not changed" or the new record. Before, each view
  start read two records (46 KB).
- **The visible screen first.** A view starts with a snapshot that holds the
  exact visible rows, cursor, modes and styles of the two screens (normal and
  full-screen) and no history: about 0.5 KB. The host sends the snapshot with
  the history when the terminal made no output for 1 second. A viewer that
  falls behind gets the same: the visible screen at once, the history when the
  output is quiet. The terminal library got one option for this (a limit on
  the history rows of a snapshot). The screen below a full-screen program is
  thus always correct, which the earlier text repaint could not do.
- The history of a terminal has a limit of 1,024 lines on the host, so a
  snapshot with history is 20 KB to 40 KB for a full history.
- The host keeps a stream for each viewer: window of half a second at the speed
  that the viewer acknowledges, the current screen in place of a backlog, input
  and results before waiting output.
- The host learns the speed of a link from the first snapshot: data that is
  acknowledged in full shows a least speed. It also measures while output
  waits, while a snapshot is in transit and while the viewer is behind.
- Input is one ordered byte stream. The view sends without waiting; the host
  confirms in order; nothing is sent again after a lost connection; the host
  refuses input that was held back.
- The server copies bytes between two devices and reads none of them. It holds
  no terminal key. The key tables, key routes, order checks and snapshot proofs
  are removed.
- A sharing change closes only the channels of removed people, at once, on the
  host. Other views see nothing.
- A device that the owner removes loses its connection at once. Its runtime
  then reads its own identity, finds that it is not approved, and ends its
  views with the text that tells the user to approve the device.
- An account whose devices have a certificate of an earlier format signs in
  as "not approved" with a text that tells the way: start fresh on one device,
  approve the other devices again with a code, and each friend trusts the new
  identity one time.

Measured in the local end-to-end setup (release build, one computer):

| Case | Protocol 15 at the start of the review | Protocol 16 |
|---|---|---|
| Hold a key, 150 ms round trip: last echo after release | 14.5 s | 0.1 s |
| Ctrl-C by a friend in an endless flood, host uplink 1 Mbit/s | 5.2 s | 0.5 s |
| The same, viewer link 128 kbit/s | view closed | 0.5 s to 0.6 s |
| Paste of 1 MB through a remote view | stopped at 355 KB | identical file |
| Sharing change while a second friend watches | all views get a new key and a new snapshot | the friend's view gets nothing |
| Server stops for 3 s | views closed | views stay open and continue |
| Join of a terminal with one line of 300,000 characters | reconnect loop | joins |

View start by a friend, measured through a forwarder that counts bytes and adds
a delay:

| Case | One connection for each view (terminal connection 17) | One connection for each device (18) |
|---|---|---|
| Bytes to the viewer, later view | 57 KB | 8 KB with a short history, 24 KB with a full history; about 2 KB before the first screen |
| Bytes to the viewer, first view after the app starts | 57 KB | 59 KB (two identity records one time) |
| Time to the first screen, 100 ms round trip | 0.7 s | 0.13 s |
| Time to the first screen, 200 ms round trip | 1.4 s | 0.23 s |
| The same, first view after the app starts | 1.4 s | 0.64 s |
| Time to the first screen, 128 kbit/s, full history | not measured (57 KB need 3.6 s at this speed) | 0.25 s |
| A full-screen program ends on a 128 kbit/s link | wrong screen until the exact snapshot | correct screen after 0.2 s |

Known limits:

- The first view after the app starts reads the identity records again (they
  are kept in memory only). On a 128 kbit/s link that view needs 2.5 s.
- The snapshot with the history stops live output for the time of its
  transfer (about 1.5 s for a full history on a 128 kbit/s link). It is sent
  only when the output is quiet.
- All pipes of a device share one TCP connection. One lost packet delays all
  views of that device for a moment.

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

## 4. Built: friends that the server cannot change (contract in `protocol/backend-api.json`, `friendIdentity`)

What the user does: nothing new. "Add friend" takes a username as before, or
an **invite text** that a friend copied from their app ("Copy my invite") and
sent in any chat. A friend added by invite text is verified with no more
steps. A friend added by username is not verified; the app shows this as a
small mark and offers "Verify", which takes the invite text of that friend.

- An identity is named by its first device (the identity root). An invite text
  holds the username and the identity root.
- For each friend the user keeps a record: handle, identity root, verified.
  Only an action of the user makes a record: a request, an accepted request,
  "Trust", or "Verify".
- The records are one signed list that the user's own devices share. The
  server stores the list and cannot make or change it. A new own device reads
  the list and accepts it only with the signature of an approved device.
- A device reads the identity of a friend only against the record. Sharing and
  connecting are refused for a person with no record or with another identity
  root. A friend that the server adds cannot be shared with.
- When a friend sets up Kodosi again, the app shows one question for that
  friend ("Trust"). Until the user answers, that friend has no access.
- After "Start fresh" on a device that has no friend list on disk, each friend
  needs one "Trust" or "Verify". This is the price of a start with no device
  and no data.

Not built, by decision: the hash of the previous device list in each list.
Each reader already keeps the newest list of each account, refuses an older
one, and accepts a new list only from a device that it already trusts. The
hash would only make a split between two readers visible later; it would not
stop the first false list. See section 9.

A short code made from both identities together is not used: the server could
select two false identities that give the same short code. An identity root is
of one identity only.

## 5. Built: one proof for each device session (contract in `protocol/backend-api.json`, `deviceSession` and `operation`)

A device proves itself one time with one signature and gets a device session.
Each request and the device connection then carry the session. Before, each request
needed a challenge request and a signature, and each socket needed a challenge
and a signature.

- The server keeps the sessions in memory. After a server restart a device
  gets the answer "a device session is required", opens a new session and
  sends the request again. The user sees nothing.
- The challenge table is gone, and the server does not read and hash each
  request body.
- A change waits only for another change of the same account. A read does not
  wait. Before, one lock held all changes of all accounts, and a slow client in
  a socket handshake held a place in that lock.
- A device connection and each pipe are registered first and checked again
  after that. A removal that happens at the same time refuses the connection or
  closes it; no lock is necessary for this.
- Each refused device connection writes one log line with account, device and
  reason.
- The meter `Kodosi.Server` counts device connections, relay pipes, relay
  bytes, refused connections by reason, and seconds of database fault.
- The argument `migrate` applies the schema. A serving start refuses a
  database that is not current and changes nothing.
- An account can be deleted (`DELETE /api/me`, a recent sign-in is necessary).

Not built, by decision: API requests through the device connection. HTTP
requests with a session have the same cost for the server, keep their own time
limits and errors, and work when the device connection is down.

## 6. Decided against: direct path, QUIC, video

All terminal traffic goes through the relay on one TCP connection for each
device. Examined and not kept:

- **TCP between devices on one local network** was built and tested between a
  Mac and a Linux computer, then removed (the code is in commit `377819cb`).
  It helped only devices of one home or office network, it did not work
  without the server, and it gave each host an open port, a macOS question
  about the local network, and no result behind a Linux firewall that blocks
  unknown ports.
- **QUIC with hole punching (`iroh`) between different networks** needs
  discovery and relay servers from a third party or a second server process,
  and a large dependency.
- **QUIC or WebTransport to the server** removes the delay of all views after
  one lost packet. It needs UDP, which some networks block, so the TCP path
  must stay also, and the server support for WebTransport is experimental. The
  measured delays came from round trips and from bytes, not from lost packets.
- **Video** (GeForce NOW, Parsec) needs megabits each second. Terminal state
  needs kilobytes.

## 7. Built: client marks

- After a lost connection the view says "Some of your last input was not sent."
  when the host did not confirm all input. The text stays in the tile header
  until the next input.
- Qt: the limit of 20 tries for an attached view is removed.

## 8. Order of work

| Step | Content | Proof |
|---|---|---|
| done | Channel, relay pipe, per-viewer stream, input stream | section 2 |
| done | Device link with one typed code in all three clients | section 3 |
| done | Friend records, invite text, no first use in all three clients | section 4 |
| done | Device session and server items | section 5 |
| done | Client marks (7) | section 7 |
| done | One connection for each device, resumed channels, visible screen first, identity records only when changed | section 2 |

## 9. Risks that stay

- The server reads terminal names, host names, device labels and handles, and
  sees who connects to whom, when, and padded sizes.
- A friend added by username and not verified is taken from the server one
  time, when the friendship starts. The mark "not verified" shows this.
- A removed device that works with the server can show a reader that did not
  see the removal a false newer device list.
- A secret and attacker text in the same output frame leak a little through the
  frame size, also with padding and with each frame packed alone.
- The server can stop or delay service. It cannot deliver old input.
- A stolen device has the access of that device until an owner removes it.
