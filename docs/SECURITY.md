# Security direction

Kodosi keeps terminal contents and private device keys on endpoint devices. The
service stores the shared metadata needed to connect people and routes encrypted
terminal traffic. Access to a terminal requires an approved own device or an explicit
share from its owner.

Devices remember identities after first contact and reject unexpected replacements.
First contact between different users is not independently verifiable today.

## Priorities

- Bind device approval to the joining device's keys without trusting the service to
  provide the binding.
- Give people a simple way to verify a friend's identity and understand identity
  changes.
- Use a reviewed design for forward secrecy and recovery after key compromise rather
  than inventing a custom ratchet.
- Reduce service-readable metadata and retention where the product does not need it.
- Keep focused tests for identity, authorization, revocation, replay, and terminal
  lifecycle, and seek independent cryptographic review before making stronger claims.

Security changes must preserve explicit sharing, endpoint ownership of plaintext,
ordered live terminal behavior, native provider permissions, and existing user data.
