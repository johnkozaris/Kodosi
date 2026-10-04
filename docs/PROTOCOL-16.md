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

Not done in this part: the Mac app and the Qt app do not yet show
"reconnecting" on a connected tile (the view stays open and continues, with no
mark), and the unconfirmed input count is a text in the "connection lost"
message only.

## 3. Remaining: identity that the server cannot change

Today the server can add its own device when a device is linked, and a friend's
identity is taken from the server at first use. These are the two remaining
ways for a hostile server to get terminal access. The fixes must follow goal 5.

### 3.1 Linking a device: one typed code

What the user does: on the new device, sign in; it shows an 8-character code.
On a device that is already approved, select "Approve" on the request and type
that code. Nothing else. This is one time for each device.

Why a typed code and not a button: the code is made from the keys of the new
device, so the approved device signs only the device that the user holds. With
a button, the server selects which keys are approved.

How it works:

1. New device N makes a random value `rN` and sends a commitment to its keys
   and `rN`. No code exists yet.
2. Approved device A opens the request and sends a random value `rA`. The
   runtime does this when the user selects the request (or at once when there
   is only one request and the approval view is open).
3. N reveals its keys and `rN`. Both compute the code = 40 bits of
   SHA-256(commitment, A device id, `rA`, hash of the current device list,
   `rN`), shown as `XXXX-XXXX` (Crockford base 32; input ignores case, spaces
   and the hyphen).
4. N shows the code. The user types it into A. A signs only if the typed code
   is equal to its own value and the revealed keys match the commitment. One
   wrong code ends that request; N starts a new one with one click.

The server must commit before it sees the random value of the other side, so
it has one chance in 2^40 for each request. An account has at most 5 open
requests; a request ends after 10 minutes.

No dead end: when no approved device is reachable, the new device offers
"Start fresh" as today (new identity; friends see one question, see 3.2).

Surfaces: `devices.link.selfPending` gets `state` (`waiting`, `code`) and
`code`; request list entries get `requestId` and no code; new
`devices.link.select {requestId}`; `devices.link.approve {requestId, code}`.
Command-line app: `devices approve` selects the single request or lists them,
then asks for the code. Qt and Mac: the request row gets one text field and
"Approve"; the "Approve" button without a code is removed.

### 3.2 Friends: verified by how they were added

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

## 4. Remaining: one connection for each device

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

## 5. Remaining: direct path

Inside an open channel the two sides try a direct QUIC connection (`iroh`, with
relay fallback). When it works, the viewer makes a new channel on it and closes
the relay pipe after the first keyframe. The channel is the same on each
transport. One setting, "Use direct connections", default on: a person who
shares a terminal with you already gives you full control of a program on
their computer, so hiding addresses between such people by default costs speed
and buys little.

## 6. Remaining: client marks

- `sessions.snapshot.connectionState` gets `reconnecting`; the tile keeps its
  view and shows a small mark. (Mac: add the value in the same change; the app
  stops on an unknown value. Qt: add it to the allowed values.)
- `term.inputUnconfirmed {bytes}`: after a lost connection the view says
  "Some of your last input was not sent" when the count is not zero.
- Qt: remove the limit of 20 tries for a view that is attached.

## 7. Order of work

| Step | Content | Proof |
|---|---|---|
| done | Channel, relay pipe, per-viewer stream, input stream | section 2 |
| next | Client marks (6) in the command-line app, Qt app and Mac app | real run with a link loss |
| then | Device link with one typed code (3.1) in all three clients | a link with changed keys fails at the code; real link with each client |
| then | Friend list, invite text, no first use (3.2) | a friend that the server adds cannot be shared with |
| then | Device connection and server items (4) | database pause, restart with many devices |
| last | Direct path (5); screen-only keyframe (2) | two computers on different networks |

Each step keeps `just check-all` green, is tested on Linux x86-64 (`ssh lenovo`),
and changes the command-line app, the Qt app and the Mac app together when a
user surface changes.

## 8. Risks that stay

- The server reads terminal names, host names, device labels and handles, and
  sees who connects to whom, when, and padded sizes.
- Until 3.1 and 3.2 are built, the server can add a device at link time and can
  replace a friend at first contact.
- A removed device that works with the server can show a reader that did not
  see the removal an older device list, until 3.2 is built.
- A secret and attacker text in the same output frame leak a little through the
  frame size, also with padding and with each frame packed alone.
- The server can stop or delay service. It cannot deliver old input.
- A stolen device has the access of that device until an owner removes it.
