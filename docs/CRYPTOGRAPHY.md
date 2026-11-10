# Encryption design

No independent party has audited this design or its code. [Security](SECURITY.md) is
the summary; the code and [`protocol/`](../protocol/) hold the exact formats and checks.

## Threat model

| Party | Can | Kodosi must prevent |
| --- | --- | --- |
| Network attacker | Read, change, drop and replay traffic | Reading or changing terminal traffic and project content |
| Backend operator or database copy | The same, and read all stored records | Reading content; adding a device or changing a member's identity unnoticed |
| Sign-in provider | Sign in as any account | Getting an approved device from sign-in alone |
| Stranger with an account | Use the public API | Any access to other people's terminals or projects |
| Removed device or member | Keep what it already read | New project keys and new terminal access |
| Friend or project member | Everything shared with them | Nothing, because sharing grants full control |

Confidentiality should hold if one of ML-KEM-768 and X25519 is broken. Kodosi does not
hide metadata, give forward secrecy for project content, protect against a compromised
member device, or keep the service available against its operator.

## Algorithms

Kodosi uses AWS-LC (through `aws-lc-rs`) and `rustls` for ML-DSA-65 signatures,
ML-KEM-768 with X25519 (always together), AES-256-GCM with random 96-bit nonces,
HKDF-SHA256 and SHA-256. Signed and authenticated data starts with a domain label, and
each field carries its length.

## Mechanisms

- **Device keys.** Each device has an ML-DSA-65 signing key and a fixed ML-KEM-768 and
  X25519 project key pair. It keeps them in the Keychain (Apple), the Secret Service (Linux)
  or DPAPI-sealed files (Windows). A Linux data folder chooses its store when it first
  keeps a secret: the Secret Service when the session can open one, or else files in
  `secrets/` that only the user can read (folder 0700, files 0600, read without following
  links). It keeps the choice in `secrets/location`.
- **Identity root and device list.** The first device's self-signed certificate fixes
  the account's root. Approved devices certify new ones and sign a device list that they
  renew every 24 hours. Devices pin each account they verify: the root changes only when the
  person accepts it, the list never goes back, and removed devices never return. Starting
  fresh makes a new root. While the account has devices, the backend registers the new
  root only for the device key that asked, a day later, so a sign-in alone does not
  replace the root unseen.
- **Friend list.** The friend list is a signed list of friends' roots, and invites carry
  it. Each list names the SHA-256 of the list before it, and a device takes only lists
  that continue the list it keeps and that an approved device validly signed. On a newest
  list of a device that it saw removed, it signs its own list again. Sharing with a friend
  whose root changed stops until the person accepts the new root: with one click for a
  friend who was not verified, and otherwise with a new invite. A device that starts fresh
  lists the account's friends again, not verified.
- **Link code.** The link code has 60 bits, stretched with PBKDF2-HMAC-SHA512 (2^20
  rounds), with HMAC-SHA256 proofs of the request, the approval and the identity root.
- **Recovery key.** The recovery key has 160 bits, and Kodosi shows it once. HKDF, bound to the account and its root,
  derives the signing key and ID of a device-list entry that cannot open a device
  session or view a terminal, and an AES key that seals the entry's project key pair. A
  recovering device signs its certificate and the next list with it and uses that pair
  until it has its own keys in every project.
- **Terminal channels.** A terminal channel is TLS 1.3 from viewer to host through the backend, with ML-DSA-65
  raw public keys, X25519MLKEM768 and TLS_AES_256_GCM_SHA384. Traffic keys change after
  2^24 frames or an hour, and resumption makes a new key exchange. The viewer accepts
  only the host device's approved key. The host admits only verified, non-recovery
  devices of accounts that may use the terminal, and checks again when sharing or
  devices change. In a project, it admits only friends of the terminal's owner whose friend
  record and the project's key state give the same identity. The edits of live documents and
  the video and input of the shared browser travel in this channel, so the backend sees
  only their sizes and timing.
- **Calls.** A device opens the same channel to another computer of its own account to
  list that computer's agents, its folders and the folders of its terminals, and to start
  a terminal there. The caller accepts only that device's approved key; the host admits
  only verified, non-recovery devices of its own account and answers only those requests.
- **Terminal projects.** The device that puts a terminal into a project, or takes it out,
  signs the change with a sequence and the hash of the newest key state of the project that
  it read. The host takes a project for its terminal only with the signature of an approved
  device of its owner, a sequence newer than the last one that it took, and a project
  history that holds that key state.
- **Project key states.** The project key states form a hash chain that member devices sign.
  Only the owner adds members
  or removes others; a member can leave. Devices remember the newest state they
  accepted, to detect an older or forked history.
- **Project keys.** Each epoch has a random 32-byte project key. The device that makes it
  wraps it for each member device with AES-256-GCM under a key that HKDF derives from a new
  ML-KEM-768 encapsulation and X25519 agreement with that device's signed project
  public key. Each key encrypts the
  previous one, so the current key opens the whole history. A new epoch starts when a
  member or device leaves, when the member who made the current key has left, and when
  the owner trusts a member's new identity; that member gets no new keys until then.
- **Project content.** The author's device encrypts project messages, tasks and
  repository links under their epoch's key, binds them to their project and item, and signs
  them.
- **Terminal status.** Each terminal's agent and state go to the backend unencrypted, and
  the backend accepts them only from the terminal's host. The title and text stay
  encrypted.

## Known limits

1. Project content has no forward secrecy. One member device's project key pair and the
   stored records reveal the project's history.
2. Signatures use ML-DSA-65 alone.
3. A member relies on the owner for the identity of a member it has no friend record of.
4. A friend added by username, and each friend of a person who starts fresh, relies on the
   backend for the first identity.
5. The link code is not a PAKE. The backend can guess it with about 2^80 work in its 10
   minutes.
6. The backend can withhold or reorder project items, whose order is unsigned, withhold or
   change terminal status, withhold a change of a terminal's project or the key states that
   are newer than the ones a device read, and name another terminal of the same computer
   as the one that a restarted terminal replaces.
7. Project keys do not change with use; one key should encrypt fewer than 2^32 records.
8. A key state's time comes from its author; devices refuse only future times.
9. A recovery key plus the account's sign-in approves a device. A device removed before
   it has its own project keys everywhere keeps the recovery project key pair; make a new
   recovery key then.
10. A recovering device trusts the backend's newest device list, which may still name a
    removed device. Kodosi tells people to make a new recovery key after removing a lost
    or stolen device.
11. An owner who starts fresh cannot open the projects they made.
12. The backend sees the metadata listed in [Security](SECURITY.md).
13. A removed device keeps its signing key. A device or friend that has not yet received
    the list without it still trusts it, so the backend can show them device lists that it
    signs. A device that received the removal refuses each device that the removed device
    approves.
14. The backend can withhold a newer friend list from a device. A device without a friend
    list takes the newest one that an approved device signed, so the backend can give it
    an older one.
15. The backend keeps the newest 1024 friend lists. A device that missed more changes, or
    whose list the backend does not continue, cannot follow the list. Removed and approved
    again, it starts without a list, as limit 14 says.
