# Encryption design and threat model

This document specifies how Kodosi protects terminal traffic, room content, and
identities, and which attacks it does and does not stop. It describes the code at this
revision. No independent party has audited the design or the code; the last section
says what a reviewer should examine.

[Security](SECURITY.md) is the short summary for users. The contract files in
[`protocol/`](../protocol/) give the wire formats.

## Threat model

| Party | What it can do | What Kodosi must prevent |
| --- | --- | --- |
| Network attacker | Read, change, drop, and replay traffic | Reading or changing terminal traffic and room content |
| Backend operator, or a person with a copy of its database | The same, and read all stored records | Reading terminal traffic and room content; adding a device to an account or changing a member's identity without detection |
| Sign-in provider | Sign in as any account | Getting an approved device from sign-in alone |
| Person who is not a friend | Use the public API with an account | Any access to another person's terminals or rooms |
| Removed device or removed member | Keep what it already read | New room keys and new terminal access |
| Friend or room member | Everything the user shared | Nothing: sharing grants full control. The person who shares is responsible for this choice. |

Kodosi also aims to keep confidentiality when one of ML-KEM-768 and X25519 is broken.

Kodosi does not try to hide metadata, to give forward secrecy for room content, to
protect a person from a compromised device of a room member, or to keep the service
available against its operator.

## Building blocks

All algorithms come from AWS-LC (through `aws-lc-rs`) and `rustls`.

- Signatures: ML-DSA-65.
- Key agreement: ML-KEM-768 and X25519, always used together.
- Encryption: AES-256-GCM with a random 96-bit nonce.
- Derivation: HKDF-SHA256. Hash: SHA-256.
- Signed and authenticated data is a domain name followed by fields, each with a 4-byte
  big-endian length before it.

## Identity

**Device keys.** Each device makes an ML-DSA-65 signing key and a room key pair (an
ML-KEM-768 key and an X25519 key). They stay in the macOS Keychain or the Linux Secret
Service.

**Device certificate.** A certificate holds the account, the device, a label, the
signing device, the signing public key, and the issue and expiry times. The signing
device signs it with the domain `kodosi-device-cert-v3`. The first device of an account
signs its own certificate. The identity root of an account is
SHA-256(`kodosi-identity-root-v1` ‖ body of that first certificate).

**Device list.** A list holds the account, a generation number, each device with its
signing device, and the issue and expiry times, signed by an approved device
(`kodosi-device-list-v1`). A list is valid for 24 hours; a running device renews it. A
device that verifies an account keeps the root, the generation, the certificates, and
the removed devices of that account, and applies these rules:

- The root does not change without a decision of the user.
- The generation does not go down, and a list of the same generation does not change.
- A new list needs a signer whose certificate chain reaches a device that is already
  trusted.
- A removed device does not come back. The key of a device does not change.

**Friends.** The signed friend list of an account (`kodosi-friend-list-v1`) records the
root of each friend. An invite text carries the handle and the root, so a friend added
by invite is verified at once. A friend added by username gets the root that the
backend returns at that time. If a friend later has a different root, sharing with that
friend stops until the user trusts the new identity.

**Link code.** A new device shows 12 random symbols (60 bits). Both devices derive a
key with PBKDF2-HMAC-SHA512 (2^20 rounds, salt `kodosi-device-link-v2` and a 16-byte
nonce). The new device proves its request, and the approving device proves the
approval and the identity root, with HMAC-SHA256. The approving device then signs the
certificate of the new device and the next list.

**Recovery key.** An approved device makes 32 random symbols (160 bits) and shows them
one time. HKDF-SHA256, with the salt (`kodosi-recovery-v1`, account, identity root),
gives an ML-DSA-65 seed, a device identifier, and an AES-256-GCM key. The approved
device signs a certificate for that key; the result is one more entry in the device
list. Because the identifier and the key depend on the root, a device that has the text
can check that the identity from the backend is the right one. The entry has a room
key pair; the backend stores its secret part encrypted with the AES-256-GCM key. The
entry cannot open a device session, and a host refuses it as a viewer. To recover, a
signed-in device proves possession of its own key and sends its certificate and the
next list, both signed with the recovery key.

**Start fresh.** A person with no approved device and no recovery key makes a new
identity. Friends must trust it again.

## Terminal connections

A viewer and a host run TLS 1.3 between their two devices. The records travel in
WebSocket messages through the backend, which copies them.

- Each side presents its device signing key as a raw public key and signs the
  handshake with ML-DSA-65.
- The key exchange is X25519MLKEM768 and the cipher suite is
  TLS_AES_256_GCM_SHA384. No other choice is offered.
- A channel changes its traffic keys after 2^24 frames or one hour. A later channel
  between the same two devices can resume and makes a new key exchange.
- The viewer accepts the host only when the key is the approved key of the hosting
  device.
- The host admits a viewer only when the account may use the terminal, the device is
  in the verified device list of that account with the same key, and the device is not
  a recovery entry. When the host has its own friend record of a room member, that
  record must agree with the identity in the room. The host checks again when sharing
  or devices change.

## Rooms

**Key state.** A room has a chain of signed key states (`kodosi-room-state-v1`). A
state holds the room, the owner, the author and its device, a version, an epoch, a
time, the SHA-256 of the previous state, the identity record of each member, the
wrapped room key for each recipient device, and the previous room key encrypted with
the current one. A device accepts a state only when:

- The version is the previous version plus one and the hash agrees.
- The owner is a member. Only the owner adds a member or removes a different member. A
  member can remove itself.
- A removal increases the epoch by one. The epoch never increases by more than one.
- The identity record of a member that does not change is taken from the previous
  state. A changed record must continue the earlier one and be current at the time of
  the state.
- Only the owner gives a member a different root, and only with a new epoch.
- The signature is from a current device of the author, and each recipient is a device
  of a member.

Each device keeps the version and hash of the newest state that it accepted, and the
root of the owner, to detect an older or different history.

**Room key.** Each epoch has a random 32-byte key. A new epoch encrypts the key of the
previous epoch with its own key, so a member with the current key can read the full
history.

**Wrap.** For each recipient device the author encapsulates to the ML-KEM-768 key and
makes an X25519 agreement with a new ephemeral key. The wrapping key is
HKDF-SHA256(salt `kodosi-room-kem-v2`, the two shared secrets, info) where info is the
context, the ML-KEM ciphertext, the ephemeral public key, and the X25519 key of the
recipient. The context is (`kodosi-room-key-wrap-v2`, room, version, epoch, account,
device). AES-256-GCM encrypts the room key with the context as additional data. Each
recipient key is signed by its device (`kodosi-room-recipient-v1`).

**Content.** A message, task, or repository record is encrypted with the room key of
its epoch. The additional data is (`kodosi-room-content-aead-v1`, room, item, kind,
version, key state version, epoch, author, device). The author's device signs the
record (`kodosi-room-content-v1`).

**Members who are not available.** A room continues when the backend cannot give the
current identity of a member: the state keeps the last record. When a member has a
different root, the other members stop wrapping new room keys for the earlier devices.
The owner puts the member in again after the owner trusts the new identity.

## Known limits

1. Room content has no forward secrecy. A person who gets the room key pair of one
   member device, and the stored records, can read the history of that room.
2. Signatures use ML-DSA-65 alone. A break of ML-DSA-65 or of its implementation
   breaks identity.
3. A member with no friend record of another member relies on the room owner for the
   identity of that member.
4. A friend added by username relies on the backend for the first identity.
5. The link code is not a password-authenticated key exchange. The backend can try to
   guess the code; the work is about 2^80 operations in the 10 minutes that a code is
   valid.
6. The backend can hold back or reorder room items. Their order is not signed.
7. A room key encrypts many records with random nonces. Change the epoch before 2^32
   records.
8. The time in a key state comes from its author. Devices refuse only a time in the
   future.
9. A person with the recovery key and the account sign-in can approve a device.
10. An owner who starts fresh cannot open the rooms that the owner made.
11. The backend sees the metadata that [Security](SECURITY.md) lists.

## Review

A reviewer should examine first:

- The rules for a key state and for a device list, and whether a backend with one
  dishonest member can break them.
- The recovery key: derivation, the binding to the identity root, and the recover
  request.
- The wrapping key derivation.
- The checks of a host before it admits a viewer, also on a resumed channel.
- The link code proofs.

Report a vulnerability as [Security](SECURITY.md) describes.

[All documentation](README.md)
