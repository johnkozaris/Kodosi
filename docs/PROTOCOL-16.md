# Kodosi protocol 16: target specification

This is the working specification for the next protocol. It replaces protocol 15.
There is no compatibility with protocol 15. When the code and `protocol/*.json`
carry a part of this document, delete that part here.

Words: **must** is a requirement. **Host** is the device that runs the terminal.
**Viewer** is a device that shows and controls it. **Owner** is the account of the
host. **Relay** is the server part that moves bytes between a host and a viewer.

## 1. Goals and threat model

Goals, in priority order:

1. The server cannot read terminal traffic, cannot send input, and cannot add a
   device or a person to a terminal.
2. An open view stays open through a sharing change, a slow link and a short loss
   of connection.
3. Typing has no delay that Kodosi adds. Output is never more than about half a
   second behind on any link; when a link cannot carry the stream, the viewer gets
   the current screen and not the backlog.
4. The server does little work for each terminal and stays up when the database
   is not reachable.

Assume this attacker:

- The server and each party that ends TLS for it can read, drop, delay, reorder
  and replay all messages, and can lie in all its own data (names, lists, ids).
- A friend can be hostile. A removed friend or device must lose access to all
  later traffic.
- A stolen sign-in token alone must give no terminal access and must not block
  the devices of the account.
- A stolen device key file gives the access of that device until the owner
  removes the device. It must not open traffic recorded before the theft.

Not protected: the server can always refuse service, and it sees who connects to
whom, when, and how many bytes (padded, see 6.4).

### 1.1 Design choices

- **Terminal state, not video.** Kodosi sends the terminal's own data, and each
  viewer has a real terminal. Selection, copy, search and fonts are local and
  exact, with no round trip. An active terminal needs kilobytes each second,
  not megabits. A video stream (GeForce NOW, Selkies) needs a video encoder on
  the host and cannot go below about 1 Mbit/s.
- **Taken from the video systems:** the sender adapts to each receiver; the
  newest state replaces a backlog; input has priority over output; the data
  goes on a direct path when one exists.
- **Taken from Signal:** the server only moves encrypted data; identity is
  checked by the devices; each pair of devices has its own keys from a new
  exchange; a removed party gets nothing after the removal.
- **No own cryptography:** standard TLS 1.3 for the channel, standard signatures
  for identity.

## 2. Keys

Each device has one long-term key: a signature key with two parts. A signature
is valid only if both parts are valid.

| Part | Algorithm | Public key | Signature |
|---|---|---|---|
| Classic | Ed25519 | 32 bytes | 64 bytes |
| Post-quantum | ML-DSA-65 | 1,952 bytes | 3,309 bytes |

- A hybrid signature is the Ed25519 signature followed by the ML-DSA-65
  signature. Both are made over the same message (17.1).
- A device has no long-term agreement key. Each channel makes its keys from a
  one-time key exchange (4.1), so a later theft of the device key does not open
  old traffic.
- The server checks both parts for its own records: ML-DSA-65 as it does today,
  and Ed25519 with the BouncyCastle package that it already has. The server is
  not trusted, so devices always check both parts themselves.
- Protocol 15 devices have an ML-DSA-65 key and an ML-KEM-768 key. Protocol 16
  adds the Ed25519 part and removes the ML-KEM-768 key.
- All primitives come from `aws-lc-rs` and `rustls`, which the runtime already
  has. No new cryptography dependency is necessary.

Signatures are made only when identity or access changes, once for each device
connection, and once on each side for each channel. No signature is made for
each message, each request or each snapshot.

## 3. Identity and access

### 3.1 Device certificate and device list

- A **device certificate** binds: account id, identity incarnation id, device id,
  label, the public key (both parts), the signer device id, and the issue time.
  The first device of an account signs its own certificate.
- The signed **device list** has: account id, identity incarnation id, generation,
  entries (device id and SHA-256 of its certificate body), signer device id,
  issue time, and **the SHA-256 of the body of the previous list** (32 zero bytes
  for generation 1). A list has at most 32 devices.
- **Identity digest** of an account = SHA-256 of (account id, identity incarnation
  id, body of the first device certificate). It does not change when devices are
  added or removed.
- A reader that holds an identity digest checks a list in this way: the first
  certificate matches the digest and signs list 1; each list `n+1` has the hash
  of list `n` and a signer that is in list `n`; each device in a list has a
  certificate that a device in that list or an earlier list signed. The server
  gives all lists and certificates that the reader does not hold.
- **Never back:** each device stores, for each account that it reads, the highest
  generation and its hash. It refuses a lower generation, and it refuses a
  different list for a generation that it holds.
- Lists and certificates have no expiry. An expiry does not stop a removed
  device that still has its key, and it would stop sharing when no owner device
  is online to sign again.
- Issue times are for display. No security decision uses a clock of another
  party.
- **Start fresh** (all devices lost): the user makes a new identity incarnation
  on a new device. The identity digest changes. Each friend sees "identity
  changed" (3.3), and all sharing of the old identity ends.

Remaining risk, stated plainly: a removed device that works with the server can
show a reader that has not seen the removal a different list `n+1`. The reader
cannot tell which is true. The channel handshake (4.1) carries the list
generation and hash of each side, so such a reader cannot connect to an honest
device without an error. The error is the detection.

### 3.2 Linking a new device

The server must not be able to put its own keys into the approval. The approving
device must type a code that the new device shows, and the code must depend on
the keys.

1. New device N makes its keys and a random 32-byte value `rN`. It sends to the
   server: label and `commit = SHA-256("kodosi-link-commit-v1", device id,
   public keys, rN)`. The server gives a request id. No code exists yet.
2. Approving device A lists the pending requests (label, request id). The user
   selects one. A sends a random 32-byte value `rA`, its device id, and the hash
   of the current device list.
3. N receives these values and then reveals: device id, public keys, `rN`.
4. N and A each compute `code = first 40 bits of SHA-256("kodosi-link-code-v1",
   commit, A device id, rA, list hash, rN)`, shown as 8 characters `XXXX-XXXX`.
5. **N shows the code. The user types it into A.** A continues only if the typed
   code is equal to its own value, and only if the revealed keys match `commit`.
6. A signs the certificate for the revealed keys and the next device list.
7. N accepts the result only if the previous-list hash of the new list is the
   list hash from step 2, and its own certificate is in the list. N then fixes
   the account identity.

The server must send `commit` to A before it learns `rA`, and must give N a value
for `rA` before it learns `rN`. It then has one try in 2^40 to make the two codes
equal. One request permits one try.

`POST /api/devices/link/init` needs only a sign-in token, as now. It can show a
false request on an approved device; it cannot pass step 5. An account has at
most 5 open link requests, and a request ends after 10 minutes.

### 3.3 Friends

- A friend is fixed to an identity digest (3.1) on the device that sends or
  accepts the friend request.
- The **friend list** is a signed document like the device list (generation,
  previous hash, signer device). Each entry has: friend account id, identity
  digest, handle at that time, `verified`, and `directPath`. All devices of the
  account read it and accept it only from a device in their device list. The
  server stores it and cannot change it. A list has at most 512 friends.
- **Sharing and connecting never take an identity on first use.** A friend entry
  from the server that has no record in the signed friend list cannot be shared
  with and cannot connect.
- **Fingerprint:** each identity has a fingerprint of 30 digits made from its
  identity digest only (17.3). The **safety code** of two friends is the two
  fingerprints, the lower one first: 60 digits in 12 groups of 5. Two people
  compare it out of band. A match sets `verified`. The comparison is optional.
  The UI shows which friends are verified.
- A shorter code made from the two digests together is not safe: the server
  selects both false identities, so it can search for two that give the same
  short code. Each half must be the fingerprint of one identity.
- When the server gives a different identity for a fixed friend, the runtime
  shows "identity changed" and stops sharing with that friend until the user
  approves the new identity. Approval clears `verified`. The handle from the
  server is display text only.

First use is still the weak point for a friend that was never verified, as in
Signal. The safety code closes it for people who compare.

### 3.4 Share grant

- Access to one terminal is a **share grant**: terminal id, terminal incarnation
  id, host device id, revision, member account ids (at most 32), issue time,
  signer device. An owner device signs it.
- The host is the only authority. It accepts a grant only when the signer is in
  its own device list and the revision is higher than the one it holds. The
  server stores the newest grant and uses it only to refuse connections early.
- The server accepts revision `n+1` only when it holds revision `n`. When two
  owner devices change the sharing at the same time, the second gets "changed
  by another device", reads the new grant, and the user decides again.
- A member reads the grant with the terminal record and checks the signature,
  so a member knows the revision before it connects.
- When the host applies a grant, it closes the channels of removed members at
  once and sends `grantApplied(revision)` to the owner devices.
- The device that made the change shows it as **pending** until it receives
  `grantApplied`. The server can delay a removal but cannot hide the delay.
- Removing a friend makes a new friend list and new grants for all terminals of
  that owner. Removing a device makes a new device list; each host closes the
  channels of that device when it learns the list.
- A host that is offline applies the newest grant before it accepts a channel.
- A removal needs no server when the device that removes is the host: it
  changes its signed list or grant, closes the channels, and sends the document
  to the server when it can. Being offline never removes sharing by itself.
- When a device is removed, its own terminals are no longer in the catalog of
  the account, and views of them close with "no access".

## 4. Channel

A channel is one encrypted connection between one host device and one viewer
device for one terminal. There is no group key. A channel does not depend on the
transport (7).

### 4.1 Handshake

The channel is TLS 1.3 from `rustls`, the library that the runtime already uses
for HTTPS. Kodosi does not define its own key exchange or its own record
protection.

- The viewer is the TLS client. The host is the TLS server.
- Only TLS 1.3, only the key exchange group `X25519MLKEM768` (classic plus
  post-quantum), only the suite `TLS_AES_256_GCM_SHA384`. No session tickets, no
  resumption, no early data.
- Each side presents a raw public key (RFC 7250): the Ed25519 part of its device
  key. The keys are checked in this way:
  - the viewer accepts only the key of the host device that the terminal record
    names, and only if that device is in the owner's device list;
  - the host takes the key that the viewer presents and checks it against the
    device that `hello` names.
- After the TLS handshake both sides compute `binding` = the TLS exporter with
  the label `EXPORTER-kodosi-channel-v1`, 32 bytes.
- `V → H` **hello**, the first frame: the protocol version range, terminal id,
  terminal incarnation id, viewer account id and device id, generation and hash
  of the viewer's device list, and an ML-DSA-65 signature over `binding` and
  these fields.
- `H → V` **accept**: the selected version, host device id, generation and hash
  of the owner's device list, the grant revision, and an ML-DSA-65 signature over
  `binding`, the SHA-256 of hello, and these fields. The first keyframe follows.
- Or `H → V` **refuse** with a code (`version`, `access`, `busy`, `gone`) and the
  same signature. A refusal that the viewer cannot verify is a lost connection
  and not a refusal.

Rules:

- TLS proves the Ed25519 part of each device key. `hello` and `accept` prove the
  ML-DSA-65 part and bind the channel to the terminal, the device lists and the
  grant.
- The host sends no terminal data before a valid `hello`. It checks: the viewer
  device is in the current device list of its account; the key that TLS proved
  is the Ed25519 part of that device's key; the account is the owner or a member
  of the current grant; the terminal incarnation is the current one. If a check
  fails, the host sends `refuse` and closes. When the viewer names a newer list generation than the host holds, the host
  gets and checks the missing lists first (3.1).
- The viewer checks: the host device is the one in the terminal record; the
  owner's list generation is not lower than the highest that the viewer saw; the
  grant revision is not lower than the newest grant that the viewer holds.
- A reconnect makes a new channel with a new handshake. Nothing is resumed.
- The host accepts at most 8 handshakes each minute from one viewer device and
  has at most 32 handshakes in progress.

Measured with `rustls` 0.23.45 in a test program (both sides in one process,
Apple silicon): the TLS handshake is 1,393 + 209 bytes from the viewer and 1,459
bytes from the host; it uses 0.13 ms of processor time for both sides together;
a wrong key is refused on each side. `hello` and `accept` add about 3.4 KB each.
The first keyframe arrives after two round trips.

### 4.2 Frames

- TLS gives order, secrecy and integrity for each direction. A changed, dropped
  or reordered record ends the channel. Kodosi has no own counters or nonces.
- TLS adds 22 bytes to each record. A record holds at most 16 KB.
- Each side asks TLS for new traffic keys after 2^24 frames or one hour.
- Frame in the TLS stream: frame length (3 bytes), type (1 byte), body length
  (3 bytes), body, zero padding (6.4). A frame is at most 1 MiB. One frame is
  one write to TLS, so the frame size, not the write pattern, sets the record
  sizes.

| Type | Direction | Body |
|---|---|---|
| `hello` | V → H | see 4.1 |
| `accept`, `refuse` | H → V | see 4.1 |
| `keyframe` | H → V | next sequence, rows, columns, `more` flag, snapshot part, metadata (last part) |
| `output` | H → V | first sequence, packed flag, chunks |
| `resize` | H → V | rows, columns, at sequence |
| `metadata` | H → V | directory, title, program, people connected |
| `heartbeat` | H → V | heartbeat number, next sequence |
| `end` | H → V | final sequence, reason |
| `ack` | V → H | next sequence that the viewer needs |
| `input` | V → H | offset, bytes, last heartbeat number seen |
| `control` | V → H | request id, action (resize, focus, interrupt, close) |
| `inputAck` | H → V | input offset accepted |
| `controlResult` | H → V | request id, accepted, message |

Body layouts of the frames that are sent often (numbers are big-endian):

| Frame | Body |
|---|---|
| `output` | first sequence (8 bytes), flags (1 byte, bit 0 = packed), data |
| `ack` | next sequence (8 bytes) |
| `input` | offset (8 bytes), heartbeat number (4 bytes), data |
| `inputAck` | offset (8 bytes) |
| `heartbeat` | heartbeat number (4 bytes), next sequence (8 bytes) |
| `resize` | rows (2 bytes), columns (2 bytes), at sequence (8 bytes) |

The other frames have a JSON body. `protocol/terminal-connections.json` is the
contract for all layouts. One typed character costs 20 bytes of frame, padded to
64, plus 22 bytes of TLS.

A snapshot larger than 256 KB goes in more than one `keyframe` frame. All parts
but the last have `more` set. The viewer installs the snapshot when the last
part arrives. The host sends no `output` between the parts.

## 5. Output: the host adapts to each viewer

**Sequence** is the count of output bytes of the terminal since it started.

The host keeps this state for each channel: next sequence to send, sequence
acknowledged, bytes in transit, measured speed, and `live` or `behind`.

- **Start:** the first frame after the handshake is a `keyframe`. A view never
  starts in another way.
- **Live:** the host sends each terminal output chunk in order in `output` frames
  of at most 32 KB of plaintext.
- **Acknowledgement:** the viewer sends `ack` after each 4 KB of output and after
  each keyframe.
- **Speed:** the host measures acknowledged bytes each second while output waits
  for that viewer. It keeps at most half a second of output at that speed in
  transit, with a minimum of 16 KB.
- **Behind:** when the output that waits for one viewer is more than the larger
  of 64 KB and the size of the last keyframe, the host drops the waiting output
  for that viewer and marks it `behind`. Other viewers are not affected.
- **Catch up:** when the bytes in transit to a `behind` viewer are acknowledged,
  the host sends one `keyframe` for the current sequence and the viewer is `live`
  again. No request from the viewer is necessary.
- A viewer gets at most two keyframes each second. Viewers that need a keyframe
  at the same time share one snapshot.
- **Order on one channel:** `inputAck`, `controlResult`, `resize` and `heartbeat`
  go before output that waits. With at most half a second of output in transit,
  an answer to input is never more than about half a second plus one round trip
  behind.
- **Idle:** the host sends `heartbeat` each 15 s. A viewer that receives nothing
  for 45 s shows the view as not current and makes a new channel.
- **End:** only `end` from the host closes a view as "terminal ended". A closed
  transport is a lost connection, not an end.
- The local view of the host uses the same rules through the same per-view state.

The host sends output to nobody when no channel is open.

Possible later, not part of this protocol: a keyframe with the visible screen
only, for a link that needs more than two seconds for a full keyframe. The
terminal library snapshot has no history limit today, so this needs a library
change.

## 6. Input, controls, packing, padding

### 6.1 Input is one ordered byte stream

- `input.offset` is the count of input bytes sent before on this channel. The host
  accepts a frame only when the offset is the one it expects.
- The viewer does not wait for one frame before it sends the next. It keeps at
  most 64 KB without `inputAck` and joins waiting bytes into one frame.
- The host puts accepted bytes into the terminal's one ordered input queue
  (4 MiB) and sends `inputAck` with the new offset. `inputAck` means "in the
  queue in order", not "the program read it". When the queue is full the host
  stops reading input from that channel.
- The host writes the queue to the terminal when the terminal can take it. It
  never drops a part of a frame. It drops queued bytes of a channel only when
  that channel closes or loses access.
- **Never replayed:** when a channel closes, bytes without `inputAck` are not
  sent again on the next channel. The view tells the user how many bytes were
  not confirmed.
- **Not late:** each `input` carries the last heartbeat number that the viewer
  saw. The host refuses input when that number is more than 2 behind its own.
  The server cannot hold a key press and release it minutes later.

### 6.2 Controls

`control` carries a request id. The host answers each with `controlResult`. A
wrong or refused control never closes the channel of another viewer. A frame
that fails authentication closes only its own channel.

### 6.3 Packing

- An `output` body of 256 bytes or more is packed with raw deflate, level 3, **each
  frame alone**. No state is kept between frames. The frame is sent packed only
  when that is smaller.
- A keyframe snapshot is packed with raw deflate, level 6.
- Reason: one shared packing state lets the server learn text on the screen from
  frame sizes. Measured cost of no shared state: small interactive frames are
  about 2 times larger; frames of 64 KB do not change.

### 6.4 Padding

- Plaintext up to 1 KB is padded to the next multiple of 64 bytes.
- Larger plaintext is padded with the Padmé rule (at most 12 % more).
- This hides small size differences. It does not remove the leak for a secret
  and attacker text that are in the same frame; the residual risk is accepted.

## 7. Transport

The channel needs an ordered, reliable byte stream. Two transports give it.

### 7.1 Relay pipe (first)

- The viewer asks the server on its device connection (8) to connect to a
  terminal. The server checks the stored grant, makes a channel id and two
  tickets, and tells the host on its device connection.
- A ticket holds: channel id, terminal id, account, device, role, expiry (60 s),
  and an HMAC-SHA256 under a key that the server process makes when it starts.
  A ticket opens one socket, one time.
- Each side opens `wss://…/relay/{channel id}` with its ticket. The relay joins
  the two sockets and copies binary messages. It keeps at most 256 KB for each
  direction and stops reading from one side when the other side is slow.
- The relay does not read frames, keeps no order state, and does not use the
  database. One host has one relay socket for each viewer.
- A pipe closes when one side closes, or when it moved no bytes for 90 s. The
  server closes the pipes of a device or a member when it learns a removal; the
  host does the same without the server (3.4).

### 7.2 Direct path (second)

- Inside an open channel the two sides exchange `iroh` endpoint ids and try a
  direct QUIC connection. When it works, the viewer makes a new channel (4.1) on
  one QUIC stream and closes the relay pipe when the first keyframe arrives.
- The two directions of a QUIC stream are independent, so a lost output packet
  does not delay input.
- The relay pipe is the fallback when no direct path exists or when it fails.
- The channel is the same on each transport. QUIC is transport only; its own
  encryption is not what Kodosi relies on.
- **Address privacy:** a direct path shows each side the IP address of the other.
  Default: direct for the owner's own devices; relay only for friends, with a
  setting for each friend.

## 8. Device connection and server API

- Each signed-in, approved device keeps one WebSocket: `wss://…/device`.
- Open: the server sends a 32-byte challenge. The device sends the sign-in token,
  its device id, and a hybrid signature over the challenge, the SHA-256 of the
  token and the server origin. This is the only signature for the connection.
  The server checks both parts (2).
- On this connection: `request` and `response` (the same routes as the HTTP API,
  no proof for each request), `event` (changes), `token` (a renewed sign-in
  token), `ping` and `pong`.
- The connection does not end when the token that opened it expires; it ends
  when no newer token arrives before 60 s after expiry.
- HTTP stays for: health and version, first enrolment, and `link/init`.
- The challenge table and the proof for each request are removed.
- **Versions:** the device sends the range it supports. The server answers with
  the one it selects, or closes with code 4000 and the range it accepts. Close
  codes carry a reason: 4000 version, 4001 sign-in, 4003 access, 4008 rate.
  Unknown JSON fields are ignored.

### 8.1 Messages

Requests on the device connection (each has a response; `R` marks a route that
also stays on HTTP):

| Request | Purpose |
|---|---|
| `link.init` R | New device: label and commit (3.2 step 1) |
| `link.requests`, `link.select`, `link.reveal`, `link.approve`, `link.cancel` | Steps 2 to 6 of 3.2 |
| `identity.get {account, fromGeneration}` | Device lists from one generation to the newest, with certificates |
| `identity.putList` | A new signed device list |
| `friends.get`, `friends.putList` | The signed friend list |
| `friends.request.send`, `.accept`, `.reject`, `.cancel` | Friend requests (the server's own record; the signed list is the authority) |
| `terminal.publish`, `terminal.unpublish`, `terminal.rename` | The host's terminal record |
| `terminal.putGrant`, `terminal.grantApplied` | Share grant and the host's confirmation (3.4) |
| `terminal.list` | Terminals that this account owns or is a member of, each with its newest grant |
| `terminal.connect {terminal}` | Viewer: ask for a relay pipe; the answer has the channel id and ticket |
| `terminal.presence {terminal, accounts}` | Host: who is connected, for display only |
| `mission.*` | Unchanged |

Events from the server: `changed {what}` (devices, friends, terminals, missions),
`terminal.incoming {terminal, channel, ticket, account, device}` to the host,
`link.updated {request}`.

### 8.2 What the server stores for a terminal

Terminal id, incarnation id, owner, host device, name, host name, the newest
share grant, online state, and the presence list. It stores no keys and no
terminal content. Names are readable by the server; that does not change in
this protocol (see 15).

## 9. Server

- **Locks:** one lock for each account for writes. No global lock.
- **Database fault:** open device connections and relay pipes continue. Only
  requests that need the database fail, with "try again".
- **Removed:** key envelopes, key generations, stream order checks, snapshot
  bookkeeping, the challenge table.
- **Added:** link requests with commit and reveal, the signed friend list, the
  share grant, relay tickets.
- **Metrics:** one `System.Diagnostics.Metrics` meter named `Kodosi`: device
  connections, relay pipes, relay bytes, seconds that a pipe waited for a slow
  side, requests by route and result, refused connections by reason, seconds of
  database fault. No new dependency; the deployment selects the exporter.
- **Limits:** live terminals, connections for each device, and message sizes are
  settings with a metric for each. Each socket type has its own maximum message
  size. Removed devices that no certificate chain needs are deleted.
- **Operations:** the migration is a separate command and takes the lease first.
  Each refused connection writes one log line with the reason. Health reports
  the build and the version range and is not rate limited. An account can be
  deleted with one request.
- **Rate limits** use the client address from the proxy when `Proxy:Addresses` is
  set.

## 10. Runtime rules

- **Error scope.** Each error belongs to one of: request, view, connection,
  terminal. An error never closes more than its own scope. A request error does
  not close a view. A view error does not close a terminal. Only the end of the
  process, Close, or a terminal write error ends a terminal.
- **One ordered channel for each view** with the frames `data`, `resize`,
  `snapshot`, `closed`. The command-line view, the local socket and the FFI use
  the same cursor code.
- **Reconnect** is one state machine with these inputs: connect result, time
  connected, terminal catalog. Results are `retry`, `renew sign-in then retry`,
  or `gone`. Only "no access", "not found" and a changed terminal incarnation
  are `gone`. The delay grows from 250 ms to 32 s and resets after one minute
  connected.
- **A refresh always ends with a snapshot.** When a view asks for a refresh and
  the request fails, the runtime repeats it each second until a snapshot
  arrives. The view stays open.
- **Checks that get no answer** (time-out, 429, 5xx) do not cut a connection.
- **Loss of the device connection** does not close relay pipes, views or local
  terminals. Only new connections wait for it. Local terminals never need the
  server.
- **After a server restart** each device connects again after a random part
  (50 % to 100 %) of its reconnect delay, so that all devices do not arrive
  together.
- Resize ownership, focus and the Minimize and Close rules do not change.
- **Time limits:** handshake 10 s; first keyframe 20 s; control 12 s; heartbeat
  15 s; view not current after 45 s. There is no time limit for input that waits
  in the host queue.

## 11. Effect on the three clients

All three clients use the runtime, so the channel, the output adaptation, the
input stream, the device connection and the direct path need **no client code**.
The clients change only where the user sees something new.

### 11.1 What changes for the user

| Area | New behaviour | Runtime surface |
|---|---|---|
| Link a device | The approving device selects the request, then the user types the code that the **new** device shows. The new device shows the code only after the approving device selected it. | New-device event `devices.link.selfPending` gets `state` (`waiting`, `code`) and `code`. Request list entries get `requestId` and no code. `devices.link.approve` takes `requestId` and `code`. New `devices.link.select {requestId}`. |
| Friends | Each friend has `verified` and `identityState` (`fixed`, `changed`). The safety code can be shown. A changed identity needs approval. | `friends.snapshot` items get the two fields. New `friends.safetyCode {userId}` with a reply event, `friends.verify {userId}`, `friends.approveIdentity {userId}`. |
| Share a terminal | A change shows as pending until the host confirms it. | `sessions.snapshot` items get `sharingPending` (bool). |
| View state | A view that lost its connection stays open and says so. A view shows when its screen is not current. | `sessions.snapshot.connectionState` gets the value `reconnecting`. New view control `term.stale {since}` and `term.current`. |
| Unconfirmed input | After a lost connection the view says how many bytes were not confirmed. | New view control `term.inputUnconfirmed {bytes}`. |
| Direct path | A setting for each friend: relay only (default) or direct. | `friends.snapshot` items get `directPath`; new `friends.setDirectPath {userId, allowed}`. |

Version numbers that change: terminal connection protocol 16, backend API 17,
desktop protocol 45, FFI ABI 8 (the terminal control set grows).

### 11.2 Command-line app (`runtime/src/cli`)

- `devices link` prints "Approve this device on another device", then prints the
  code when it arrives, and waits for the result.
- `devices approve` lists requests when it has no argument. `devices approve
  <request>` asks for the code on standard input (or `--code`).
- `friends list` prints `verified` and `identity changed`. New: `friends verify
  <username>` prints the safety code and asks for confirmation; `friends
  approve-identity <username>`; `friends direct <username> on|off`.
- `session share` prints "pending" until the host confirms, and waits up to 10 s.
- `session attach` keeps the view through a lost connection, prints one status
  line while it is not current, and prints the unconfirmed byte count.
- The view loop uses the shared cursor code (10) in place of its own checks.

### 11.3 Qt app (`../KodosiUI`)

Facts from the code review of the app at b907794:

- It ignores unknown event types and unknown fields, so the new events do not
  break an old build. Two places are strict and must change with the runtime:
  the allowed values of `connectionState` (`SessionCatalogModel.cpp`) and the
  allowed terminal control types (`TerminalSessionRegistry.cpp`).
- Device link (`DeviceSettings.qml`, `DevicesModel.cpp`): the new-device row shows
  the code from `selfPending`; the approving side shows each request with its
  code and one text field. Change: request rows get a "Select" action, the code
  is removed from the rows, and the text field sends `requestId` and `code`. The
  new-device row shows "Approve this computer on another device" until the code
  arrives.
- Friends (`PeopleView.qml`, `PeopleModel.cpp`): add a verified mark, a "Verify"
  action that shows the safety code, and an "identity changed" row state with
  "Approve". Today no identity information is shown and an identity error is
  only a banner.
- Sharing (`SessionDetails.qml`): show "Pending" next to "Save" while
  `sharingPending` is true.
- Terminal tile (`TerminalTile.qml`, `TerminalSurfaceController.cpp`): today
  `connecting` and `offline` both detach the view and show a spinner, and the
  view retries 20 times and then fails. Change: for `reconnecting`, keep the
  view attached and draw a small "Reconnecting" mark; remove the 20-try limit
  for a view that is attached. Show `term.stale` and `term.inputUnconfirmed` in
  the existing tile message line.
- Input (`TerminalViewInput.cpp`): no change. The app sends one FFI call for each
  key and retries on `busy`; the runtime joins waiting bytes.
- Pins (`dependencies.lock.json`, `scripts/release`): `ghostty.packageCommit` and
  `ghostty.linuxVtArchiveSha256` change with each terminal library release;
  `kodosi.commit`, `ffiAbiVersion` 8 and `desktopProtocolVersion` 45 change with
  the runtime. The verifier needs clean, committed sibling checkouts.
- The server address has no setting in the app; it comes from
  `KODOSI__BACKEND__API`. A self-hosted server needs a setting (not part of this
  protocol; listed for step E).

### 11.4 Mac app (`../KodosiMac`)

Facts from the code review of the app at 75ed3ee:

- The app accepts exactly desktop protocol 44 and ABI 7, and it stops with
  "Kodosi couldn't start" on an unknown value of `kind`, `status` or
  `connectionState`. The app and the runtime are built together, so the new value
  `reconnecting` and the new version numbers must go into the app in the same
  change (`RuntimeHandle.swift`, `RuntimeSession.swift`, the tests that use 44).
  Unknown event types, unknown fields and unknown terminal control types are
  ignored.
- Device link (`DeviceTrustView.swift`, `DeviceSettingsView.swift`): the new Mac
  shows the code and "Waiting for approval…". The approving Mac shows each
  request with its code and an **Approve button that needs no typing**. Change:
  remove that button and the code from the request row; the row gets "Continue",
  then the user types the code that the new Mac shows. The new Mac shows
  "Approve this Mac on a device you already trust" until the code arrives. The
  "Start fresh" path does not change.
- Friends (`PeopleView.swift`, `DirectoryModels.swift`): `FriendEntry` gets
  `verified`, `identityState` and `directPath`. Add a verified mark, "Verify…"
  with the safety code, an "identity changed" state with "Approve…", and the
  direct path switch. Today an identity error is only the red banner.
- Sharing (`SessionSharingPopover.swift`): show "Waiting for the host" while
  `sharingPending` is true.
- Terminal view (`TerminalSessionManager.swift`): on `term.resize` the app asks
  for a refresh and drops data until the next snapshot. The runtime must
  therefore always answer a refresh with a snapshot, also after a failed try
  (see 10: the FFI repeats a failed refresh). Show `reconnecting` on a connected
  tile (today the label exists but is hidden while connected), and show
  `term.stale` and `term.inputUnconfirmed` in the strip under the terminal.
- Input (`TerminalInputQueue.swift`): no change. The app already joins waiting
  writes up to 64 KiB and repeats on `busy`.
- Pins: `Ghostty.lock` (must be byte-identical to `Kodosi/Ghostty.lock`) and
  `Kodosi.lock` change with each terminal library or runtime release; the linked
  image check and the Rust archive check (protocol 45, ABI 8) change with them.
- The server address has no setting in release builds. A self-hosted server
  needs one (listed for step E).

### 11.5 Rule for all three

A view never starts, and never continues after a gap, without a snapshot. The
runtime gives the snapshot; no client asks the host for missed output.

## 12. Order of work and proof for each step

| Step | Content | Proof |
|---|---|---|
| A | Hybrid keys, device list with previous hash, device link with typed code | Tests: a list fork is refused; a link with changed keys fails at the code. Real run: link a device with the command-line app and both apps. |
| B | Signed friend list, no first use on share or connect, safety code | Tests: a friend the server adds cannot be shared with. Real run: add, verify, share. |
| C | Channel (TLS 1.3 with fixed device keys, `hello` and `accept`) and frames; share grant; relay pipe; removal with pending state | Tests: a wrong device key, a changed record, a false end, late input and a second viewer's bad frame have no effect. Real run: the measurement set of protocol 15. |
| D | Host-side output adaptation and input stream | Real run: flood, Ctrl-C and paste at no limit, 1 Mbit/s and 128 kbit/s, on the viewer link and on the host link. |
| E | Device connection; server locks, limits, logs, migration command | Real run: database pause, restart with many devices, rate limit from one address. |
| F | Direct path | Real run: two computers on different networks; fall back to the relay. |

Each step keeps `just check-all` green and updates `protocol/*.json` and this file.
Each step is also tested on Linux x86-64: the runtime tests, then the Qt app.
Each step that changes a user surface (11.1) changes the command-line app, the
Qt app and the Mac app in the same step.

## 13. Constants

| Name | Value |
|---|---|
| Link code | 40 bits, 8 characters, one try for each request |
| Link requests | at most 5 open for one account; each ends after 10 minutes |
| Safety code | 60 digits: two fingerprints of 30 digits |
| Devices in one account | at most 32 |
| Friends in one account | at most 512 |
| Members of one grant | at most 32 |
| Handshakes | at most 8 each minute for one viewer device; 32 in progress on a host |
| Keyframe part | at most 256 KB |
| Channel frame plaintext | at most 1 MiB |
| Output frame plaintext | at most 32 KB |
| Output in transit | half a second at the measured speed, minimum 16 KB |
| Waiting output before `behind` | larger of 64 KB and the last keyframe |
| Keyframes for one viewer | at most 2 each second |
| Viewer acknowledgement | after each 4 KB and after each keyframe |
| Input without `inputAck` | at most 64 KB |
| Host input queue | 4 MiB for each terminal |
| Heartbeat | each 15 s; view not current after 45 s; input refused when the heartbeat number that it carries is more than 2 behind |
| New traffic keys | after 2^24 frames or 1 hour |
| Relay buffer | 256 KB for each direction |
| Relay ticket life | 60 s, one use |
| Relay pipe idle limit | 90 s |
| Viewers for one terminal | 32 |
| Reconnect delay | 250 ms to 32 s, reset after 1 minute connected |
| History in a snapshot | 1,024 lines (unchanged) |

## 14. Decisions with their defaults

The owner can change each default before the step that uses it.

| Decision | Default in this document | Other choice |
|---|---|---|
| Channel handshake | TLS 1.3 from `rustls` with fixed device keys and a signed binding (4.1); tested | An own key-exchange design: one round trip less at the start, but it needs its own security review |
| Packing | Each frame alone, with padding (6.3, 6.4) | One packing state for each channel: about half the bytes for interactive output, but frame sizes can show screen text to the server |
| Friend check | Fixed at accept; comparison of the safety code is optional | Comparison is mandatory before the first share |
| Direct path for friends | Off; a setting for each friend | On for all |
| Output copies | One stream for each viewer from the host | One shared stream with a group key (protocol 15); not recommended |
| Relay | Own relay pipe first, direct path second | `iroh` relays from the start |
| Input acknowledgement | "In the host queue in order" | "The terminal accepted it" (protocol 15); a busy program then causes input errors |

## 15. Risks that stay

- The server reads terminal names, host names, device labels and handles.
- A friend that was never verified can be replaced by the server at the first
  contact (3.3).
- A removed device that works with the server can show a different device list
  to a reader that did not see the removal; the handshake then fails and shows
  it (3.1).
- The server sees who connects to whom, when, and padded sizes. Typing rhythm is
  visible as frame times.
- A secret and attacker text in the same output frame leak a little through the
  frame size, also with padding (6.4).
- The server can stop or delay service. It cannot hide a delayed removal (3.4)
  and it cannot deliver old input (6.1).
- A stolen device has the access of that device until an owner removes it.

## 16. Start from zero

Protocol 16 has new key types, so a device that was enrolled under protocol 15
must enrol again, and each account starts with a new identity. The database
starts empty. This is acceptable because nothing is in production.

Sign out, Close, Minimize and host quit keep their product rules: sign out ends
sharing from that device (the host makes an empty grant and removes its
records); Close ends the terminal and sends `end`; Minimize closes only the
view; quitting the host ends its terminals.

## 17. Encodings and formulas

### 17.1 Signed and hashed messages

Each signed or hashed message is: a tag (ASCII, no length), then each field as a
32-bit big-endian byte length followed by the bytes. Text is UTF-8. Numbers are
decimal text. Lists are a count field followed by the fields of each entry.
This is the encoding that protocol 15 uses.

| Tag | Message |
|---|---|
| `kodosi-device-cert-v3` | Device certificate body |
| `kodosi-device-list-v2` | Device list body |
| `kodosi-friend-list-v1` | Friend list body |
| `kodosi-share-grant-v1` | Share grant body |
| `kodosi-identity-digest-v1` | Identity digest input |
| `kodosi-link-commit-v1`, `kodosi-link-code-v1` | Link commit and link code (3.2) |
| `kodosi-fingerprint-v1` | Fingerprint input (17.3) |
| `kodosi-device-connection-v2` | Device connection signature (8) |
| `kodosi-channel-hello-v1`, `kodosi-channel-accept-v1`, `kodosi-channel-refuse-v1` | Channel messages (4.1) |

### 17.2 Link code

The first 40 bits of the hash, as 8 characters of Crockford base 32
(`0123456789ABCDEFGHJKMNPQRSTVWXYZ`), shown as `XXXX-XXXX`. Input ignores case,
spaces and the hyphen, and reads `I` and `L` as `1` and `O` as `0`. The
approving device permits one try for each request.

### 17.3 Fingerprint and safety code

`h = SHA-512` applied 5,200 times, starting from (`kodosi-fingerprint-v1`,
identity digest), each time over the last result and the identity digest.
The fingerprint is 6 groups: group `i` is the 5 bytes `h[5i .. 5i+5]` as a
big-endian number modulo 100,000, shown as 5 digits. This is the Signal
fingerprint shape; each fingerprint has about 100 bits.

### 17.4 Padding

For a plaintext of `L` bytes: if `L <= 1024`, the padded length is the next
multiple of 64. Otherwise (Padmé): `E = floor(log2(L))`, `S = floor(log2(E)) + 1`,
`m = 2^(E-S) - 1`, padded length `= (L + m) AND NOT m`.

### 17.5 Test vectors

`protocol/crypto-domains.json` has one vector for each row of 17.1 and for 17.2,
17.3 and 17.4. The runtime tests and the server tests read the same file.
