# Encryption design

No independent party has audited this design or implementation. See [Security](SECURITY.md) for
the user-facing overview; the code and [`protocol/`](../protocol/) define the exact wire formats
and validation rules.

## Threat model

| Actor | Capabilities | What Kodosi prevents |
| --- | --- | --- |
| Network attacker | Read, modify, drop, and replay traffic | Reading or tampering with terminal traffic or project content |
| Backend operator or database leak | Same as above, plus read all stored records | Reading plaintext content; silently adding devices or swapping a member's identity |
| Identity provider (OIDC) | Sign in as any account | Approving a new device from sign-in alone |
| Unrelated account | Call public API endpoints | Accessing anyone else's terminals or projects |
| Removed device or member | Retain data it already decrypted | Receiving future project keys or new terminal access |
| Friend or project member | Full access to shared terminals and projects | Nothing: sharing intentionally grants full control |

Confidentiality should hold as long as either ML-KEM-768 or X25519 remains unbroken. Kodosi does
not hide metadata, provide forward secrecy for project content, protect against a compromised
member device, or prevent denial of service by the backend operator.

## Cryptographic primitives

Kodosi uses AWS-LC (via `aws-lc-rs`) and `rustls` for:

- ML-DSA-65 signatures
- Hybrid ML-KEM-768 + X25519 key agreement (always combined)
- AES-256-GCM with random 96-bit nonces
- HKDF-SHA256 and SHA-256

All signed and authenticated payloads begin with a domain-separation label, and all variable-length
fields are length-prefixed.

## Mechanisms

### Device keys
Each device generates an ML-DSA-65 signing key pair and a static ML-KEM-768 + X25519 project key
pair. Private keys are stored in the platform secret store: Apple Keychain, Linux Secret Service
(falling back to user-only `0600` files if no Secret Service is running), Windows DPAPI-sealed
files, or app storage sealed with AES-GCM under an Android Keystore key.

### Identity root and device list
The first device's self-signed certificate establishes the account's identity root. Existing
approved devices certify new devices and sign a device list renewed every 24 hours. Clients pin
every account identity they verify: an account's root only changes when explicitly accepted by the
user, device list versions never roll back, and removed devices can never be re-added.

Starting fresh creates a new identity root. If the account still has active devices, the backend
only registers the new root for the requesting device key after a 24-hour delay, preventing a
compromised sign-in alone from silently replacing the root.

### Friend list
An account's friend list is a signed hash chain of friends' identity roots (also carried in
invites). Each update embeds the SHA-256 hash of the previous list, and devices only accept lists
that extend their current list and are validly signed by an approved device. If the latest list was
signed by a device that has since been removed, an approved device re-signs its own list.

Sharing with a friend whose root changes pauses until you accept the new root (one click if the
friend wasn't verified, or a fresh invite if they were). A device that starts fresh reloads the
account's friends as unverified.

### Device linking codes
Typed link codes carry 60 bits of entropy stretched with PBKDF2-HMAC-SHA512 (2^20 iterations),
with HMAC-SHA256 proofs binding the request, approval, and identity root.

QR codes shown by an approved computer for a new mobile device carry 256 bits of entropy with no
typed fallback. HKDF-SHA256 derives the authentication key, and the approving computer accepts a
single valid proof within the code's 10-minute window.

### Recovery key
Recovery keys have 160 bits of entropy and are shown once. HKDF bound to the account and identity
root derives both the signing key and ID of a recovery device-list entry (which cannot open device
sessions or view terminals) and an AES key sealing that entry's project key pair. A recovering
device uses the recovery key to sign its own certificate and the next device list, and temporarily
uses the recovery project key pair until it receives wrapped keys of its own in every project.

### Terminal channels
Terminal channels use TLS 1.3 end to end from viewer to host (relayed through the backend), with
ML-DSA-65 raw public keys, X25519MLKEM768 key exchange, and `TLS_AES_256_GCM_SHA384`.

Between two devices of the same user that can connect directly, those TLS records travel over a
direct QUIC connection whose own TLS 1.3 layer uses the same keys and cipher suites. The host only
accepts direct QUIC connections from a device whose relayed channel is expecting one. Because
records share a single sequence across both transports, switching between relay and direct QUIC
requires no new handshake.

On a direct connection, the video frames of the canvas browser and screens travel as QUIC datagrams
of that connection, outside the channel. Its TLS 1.3 layer, which ends at the two devices and
authenticates them with their device keys, is then their only encryption.

Traffic keys rekey after 2^24 frames or one hour, and reconnections perform a fresh key exchange.
The viewer verifies the host device's approved public key. The host only admits verified,
non-recovery devices belonging to authorized accounts, and re-checks authorization whenever sharing
or device lists change. For project terminals, the host only admits friends of the terminal owner
whose friend record matches the identity recorded in the project's key state. Live document edits
and shared browser input travel inside this channel, as does canvas video on the relay, leaving the
backend with only timing and frame sizes.

### Remote host calls
A device opens the same encrypted channel to another computer on its own account to query agents
and folders, launch a terminal, or change who a terminal on that computer is shared with. The
caller pins the host's approved key, and the host accepts only verified, non-recovery devices from
its own account and restricts requests to those operations. A sharing change still runs the host's
own checks and is written by the host, so the backend never learns of a person before the host has
chosen them.

### Terminal project assignments
Adding a terminal to a project or removing it from one is signed by the acting device with a
monotonic sequence number and the hash of the latest project key state it has seen. The host only
applies an assignment signed by an approved device of its owner, with a strictly newer sequence
number, and whose referenced key state appears in the project's verified history.

### Project key states
Project membership and epochs form a hash chain signed by member devices. Only the project owner
can add or remove members; any member can leave. Devices persist the latest state they accepted so
they detect rollbacks or forks.

### Project keys
Each epoch uses a random 32-byte symmetric key, wrapped per member device with AES-256-GCM under a
key derived via HKDF from a fresh ML-KEM-768 encapsulation plus X25519 agreement against that
device's signed project public key. Every epoch key also encrypts the previous epoch key, allowing
the latest key to decrypt full history.

A new epoch starts whenever a member or device leaves, when the member who created the current key
has left, or when the owner trusts a member's new identity root (until then, that member receives
no new keys).

### Project content
Messages, tasks, and repository links are encrypted by the author's device under the current epoch
key, authenticated with associated data binding the project and item ID, and signed. Deleting a
message writes a new empty version from the author; any device that sees the deletion retains it.

### Terminal status
Each terminal's agent type and reported state are sent to the backend unencrypted, accepted only
from that terminal's host. Terminal titles and contents remain encrypted.

## Known limitations

1. Project content has no forward secrecy. Compromising one member device's static project private
   key along with stored ciphertext reveals the project's entire history.
2. Signatures rely solely on ML-DSA-65 (not hybrid classical + PQ).
3. Project members trust the project owner's key state for the identity of members they aren't
   friends with directly.
4. Friends added by username (and all friends reloaded after starting fresh) rely on the backend
   for initial identity verification (TOFU).
5. Typed link codes don't use a PAKE; a malicious backend can attempt an offline brute-force
   attack with roughly 2^80 work within the 10-minute window.
6. The backend can withhold or reorder project items (their ordering isn't signed), withhold or
   forge terminal status reports, withhold terminal-project updates or newer key states, or
   substitute another terminal on the same host as the replacement target after a restart.
7. Project keys don't auto-rotate by message volume; a single epoch key should encrypt fewer than
   2^32 records.
8. Key-state timestamps are set by their author; devices only reject timestamps in the future.
9. A recovery key plus account sign-in is sufficient to approve a new device. If a recovered
   device is removed before it obtains its own per-project keys everywhere, it still holds the
   recovery project key pair; generate a new recovery key in that case.
10. A recovering device trusts the backend's latest device list, which could still include a
    removed device. Kodosi tells you to make a new recovery key after you remove a lost or stolen
    device.
11. A project owner who starts fresh loses decryption access to projects they previously created.
12. The backend observes all metadata documented in [Security](SECURITY.md).
13. A removed device still possesses its signing key. Devices or friends that haven't yet received
    the updated device list without it can still be served lists it signed. Once a device receives
    the removal, it rejects any device approved by the removed one.
14. The backend can withhold newer friend-list updates. A device with no prior friend list accepts
    the newest validly signed one the server offers, which could be stale.
15. The backend retains the 1,024 most recent friend-list updates. A device that falls further
    behind (or diverges from retained history) can't verify the chain; removing and re-approving
    it resets it to an empty list, subject to limitation 14.
16. Deleting a message does not erase it from devices that already synced it, and anyone with an
    archived copy of the old ciphertext can still decrypt it with the project keys.
