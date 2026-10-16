# Encryption design

No independent party has audited this design or its code. [Security](SECURITY.md) is
the summary; [`protocol/`](../protocol/) has the wire formats.

## Threat model

| Party | Can | Kodosi must prevent |
| --- | --- | --- |
| Network attacker | Read, change, drop and replay traffic | Reading or changing terminal traffic and room content |
| Backend operator or database copy | The same, and read all stored records | Reading content; adding a device or changing a member's identity unnoticed |
| Sign-in provider | Sign in as any account | Getting an approved device from sign-in alone |
| Stranger with an account | Use the public API | Any access to other people's terminals or rooms |
| Removed device or member | Keep what it already read | New room keys and new terminal access |
| Friend or room member | Everything shared with them | Nothing: sharing grants full control |

Confidentiality should hold if one of ML-KEM-768 and X25519 is broken. Kodosi does not
hide metadata, give forward secrecy for room content, protect against a compromised
member device, or keep the service available against its operator.

## Primitives

AWS-LC (through `aws-lc-rs`) and `rustls`: ML-DSA-65 signatures; ML-KEM-768 with X25519,
always together; AES-256-GCM with random 96-bit nonces; HKDF-SHA256; SHA-256. Signed and
authenticated data is a domain name followed by fields, each prefixed with a 4-byte
big-endian length.

## Identity

**Device keys.** Each device has an ML-DSA-65 signing key and a room key pair (ML-KEM-768
and X25519) in the macOS Keychain or the Linux Secret Service. The room key pair never
changes: the server refuses a new room key for a known device.

**Certificates.** A device certificate (`kodosi-device-cert-v3`) holds the account,
device, label, signing device, public key, issue and expiry times, signed by an approved
device; the first device signs its own. The account's identity root is
SHA-256(`kodosi-identity-root-v1` ‖ first certificate body).

**Device list.** A signed list (`kodosi-device-list-v1`) holds the account, a generation,
each device with its signer, and issue and expiry times. It is valid for 24 hours and
renewed by running devices. A device that verifies an account remembers its root,
generation, certificates and removed devices, and requires:

- the root changes only by the user's decision;
- the generation never goes down, and a generation never changes its list;
- the signer's certificate chain reaches an already trusted device;
- removed devices never return, and device keys never change.

**Friends.** A signed friend list (`kodosi-friend-list-v1`) records each friend's root.
An invite carries the handle and root, so the friend is verified at once; a friend added
by username gets the root the backend returns. A changed root stops sharing with that
friend until the user trusts it.

**Link code.** A new device shows 12 random symbols (60 bits). Both devices derive a key
with PBKDF2-HMAC-SHA512 (2^20 rounds, salt `kodosi-device-link-v2` and a 16-byte nonce)
and prove the request, the approval and the identity root with HMAC-SHA256. The approving
device then signs the new certificate and the next list.

**Recovery key.** An approved device makes 32 random symbols (160 bits) and shows them
once. HKDF-SHA256 with salt (`kodosi-recovery-v1`, account, identity root) derives an
ML-DSA-65 seed, a device ID (UUID v8) and an AES-256-GCM key. The recovery key becomes a
device-list entry with its own room key pair; its public part, signed by the recovery
key, and its secret part, encrypted with the AES key, are stored in one write. The entry
cannot open a device session, and hosts refuse it as a viewer. To recover, a signed-in
device proves its own key and sends its certificate and the next list, both signed with
the recovery key. It keeps the entry's room key pair until it has its own room keys in
each room, then deletes it.

**Starting fresh.** Without a device or recovery key, a person makes a new identity that
friends must trust again.

## Terminal connections

Viewer and host run TLS 1.3 device to device; the backend copies the records inside
WebSocket messages.

- Each side presents its device signing key as a raw public key and signs the handshake
  with ML-DSA-65. Key exchange is X25519MLKEM768 and the only cipher suite is
  TLS_AES_256_GCM_SHA384.
- Traffic keys change after 2^24 frames or one hour. A later channel between the same
  devices can resume, with a new key exchange.
- The viewer accepts only the approved key of the hosting device.
- The host admits a viewer only if the account may use the terminal, the device and key
  are in the account's verified device list, and the device is not a recovery entry. If
  the host has its own friend record of a room member, it must match the room's record.
  The host checks again when sharing or devices change.

## Rooms

**Key state.** A room has a chain of signed key states (`kodosi-room-state-v1`) holding
the room, owner, author and device, version, epoch, time, the previous state's SHA-256,
each member's identity record, the room key wrapped for each recipient device, and the
previous room key encrypted with the current one. A device accepts a state only if:

- the version is one higher and the hash matches;
- the owner is a member; only the owner adds members or removes others, and a member
  can remove itself;
- a removal raises the epoch by exactly one, and the epoch never jumps more;
- unchanged members keep their previous record; a changed record continues the old one
  and is current at the state's time; only the owner gives a member a new root, with a
  new epoch;
- the signer is a current device of the author, and every recipient is a member's device.

Each device remembers the newest accepted version and hash and the owner's root, to
detect an older or forked history.

**Room key.** Each epoch has a random 32-byte key. A new epoch encrypts the previous key,
so the current key opens the whole history. A member who leaves makes the key of the
epoch that removes them; the next member device that writes or checks the room then
starts another epoch, and no device writes with a key whose maker left.

**Wrapping.** For each recipient device the author encapsulates to its ML-KEM-768 key and
agrees with its X25519 key from a new ephemeral key. The wrapping key is
HKDF-SHA256(salt `kodosi-room-kem-v2`, both shared secrets, info = context, ML-KEM
ciphertext, ephemeral public key, recipient X25519 key), with context
(`kodosi-room-key-wrap-v2`, room, version, epoch, account, device), which is also the
AES-GCM additional data. Each recipient key is signed by its device
(`kodosi-room-recipient-v1`).

**Content.** Messages, tasks and repository records are encrypted with their epoch's
key. Additional data: (`kodosi-room-content-aead-v1`, room, item, kind, version, key
state version, epoch, author, device). The author's device signs the record
(`kodosi-room-content-v1`).

**Terminal status.** The host sends each terminal's agent and its state (for example
working, or waiting for an approval) without encryption, so that every device of the
people who can open the terminal sees it. The program's title and text stay in the
encrypted terminal connection. The backend accepts a status only from the host's
device connection.

**Unavailable members.** If the backend cannot give a member's current identity, the
state keeps the last record. When a member's root changes, others stop wrapping new keys
for the old devices until the owner trusts the new identity.

## Known limits

1. No forward secrecy for room content: one member device's room key pair plus the
   stored records reveal that room's history.
2. Signatures use ML-DSA-65 alone.
3. A member without a friend record of another member relies on the owner for that
   member's identity.
4. A friend added by username relies on the backend for the first identity.
5. The link code is not a PAKE: the backend can guess it, at about 2^80 work within the
   code's 10 minutes.
6. The backend can withhold or reorder room items (their order is unsigned). It reads
   each terminal status and can withhold or change it.
7. Change a room's epoch before 2^32 records under one key.
8. A key state's time comes from its author; devices only refuse future times.
9. Recovery key plus account sign-in can approve a device. A device removed before it
   has its own room keys everywhere keeps the recovery entry's room key pair: make a new
   recovery key then.
10. A recovering device has no history of the account and trusts the backend's newest
    device list, which may still contain a later-removed device. Kodosi tells people to
    make a new recovery key after removing a lost or stolen device.
11. An owner who starts fresh cannot open the rooms they made.
12. The backend sees the metadata listed in [Security](SECURITY.md).

Reviewers should start with the key-state and device-list rules against a dishonest
backend plus one dishonest member, the recovery key, the wrapping derivation, the host's
admission checks (also on resumed channels), and the link code proofs.
