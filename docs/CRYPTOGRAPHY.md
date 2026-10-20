# Encryption design

No independent party has audited this design or its code. [Security](SECURITY.md) is
the summary; the code and [`protocol/`](../protocol/) hold the exact formats and checks.

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
authenticated data starts with a domain label, and each field carries its length.

## Mechanisms

- **Device keys:** an ML-DSA-65 signing key and a fixed ML-KEM-768 and X25519 room key
  pair, in the Keychain (Apple), the Secret Service (Linux) or DPAPI-sealed files
  (Windows).
- **Identity root and device list:** the first device's self-signed certificate fixes
  the account's root. Approved devices certify new ones and sign a device list renewed
  every 24 hours. Devices pin each account they verify: the root changes only when the
  person accepts it, the list never goes back, and removed devices never return.
- **Friend list:** a signed list of friends' roots, which invites carry. Sharing with a
  friend whose root changed stops until the person accepts it.
- **Link code:** 60 bits, stretched with PBKDF2-HMAC-SHA512 (2^20 rounds), with
  HMAC-SHA256 proofs of the request, the approval and the identity root.
- **Recovery key:** 160 bits, shown once. HKDF, bound to the account and its root,
  derives the signing key and ID of a device-list entry that cannot open a device
  session or view a terminal, and an AES key that seals the entry's room key pair. A
  recovering device signs its certificate and the next list with it and uses that pair
  until it has its own keys in every room.
- **Terminal channels:** TLS 1.3 from viewer to host through the backend, with ML-DSA-65
  raw public keys, X25519MLKEM768 and TLS_AES_256_GCM_SHA384. Traffic keys change after
  2^24 frames or an hour, and resumption makes a new key exchange. The viewer accepts
  only the host device's approved key. The host admits only verified, non-recovery
  devices of accounts that may use the terminal, and checks again when sharing or
  devices change.
- **Terminal rooms:** the device that puts a terminal into a room, or takes it out, signs
  the change with a sequence. The host takes a room for its terminal only with the
  signature of an approved device of its owner and a sequence newer than the last one
  that it took.
- **Room key states:** a hash chain signed by member devices. Only the owner adds members
  or removes others; a member can leave. Devices remember the newest state they
  accepted, to detect an older or forked history.
- **Room keys:** a random 32-byte key per epoch, wrapped for each member device with
  AES-256-GCM under a key that HKDF derives from a new ML-KEM-768 encapsulation and
  X25519 agreement with the device's signed room public key. Each key encrypts the
  previous one, so the current key opens the whole history. A new epoch starts when a
  member or device leaves, when the member who made the current key has left, and when
  the owner trusts a member's new identity; that member gets no new keys until then.
- **Room content:** messages, tasks and repository links, encrypted under their epoch's
  key, bound to their room and item, and signed by the author's device.
- **Terminal status:** each terminal's agent and state, unencrypted, accepted only from
  its host; its title and text stay encrypted.

## Known limits

1. No forward secrecy for room content: one member device's room key pair and the
   stored records reveal the room's history.
2. Signatures use ML-DSA-65 alone.
3. A member relies on the owner for the identity of a member it has no friend record of.
4. A friend added by username relies on the backend for the first identity.
5. The link code is not a PAKE: the backend can guess it with about 2^80 work in its 10
   minutes.
6. The backend can withhold or reorder room items, whose order is unsigned, and withhold
   or change terminal status.
7. Room keys do not change with use; one key should encrypt fewer than 2^32 records.
8. A key state's time comes from its author; devices refuse only future times.
9. A recovery key plus the account's sign-in approves a device. A device removed before
   it has its own room keys everywhere keeps the recovery room key pair; make a new
   recovery key then.
10. A recovering device trusts the backend's newest device list, which may still name a
    removed device. Kodosi tells people to make a new recovery key after removing a lost
    or stolen device.
11. An owner who starts fresh cannot open the rooms they made.
12. The backend sees the metadata listed in [Security](SECURITY.md).
