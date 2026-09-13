# Kodosi backend

One ASP.NET application owns accounts/devices, friendship, explicit session sharing,
Mission directory metadata, and a bounded ciphertext relay. Feature services own short
EF transactions directly. No repository/port framework or durable terminal-input queue.

- Approved own devices can use owner sessions. Friends require explicit per-session sharing.
- Mission membership never grants terminal access.
- Native terminal traffic is checkpoint plus ordered raw output. Provider permissions stay in the native CLI.
- API admission holds one short gate across proof checking and the feature operation, released before response serialization or writes; WebSocket admission uses the same gate. Raw/control forwarding uses per-session locks, not that gate.
- Device proof, current membership, host identity, and key generation are revalidated before admitting remote traffic.
- Initial and requested checkpoints carry a host signature bound to the viewer's fresh challenge, authenticated recipient, publication, key generation, and encrypted frame hash. The native checkpoint codec is unchanged.
- Offline publications expire after a two-minute reconnect grace; cleanup removes only metadata, keys, and members, never the host's local process. An expired publication can be re-created by its active host device; friend sharing requires fresh explicit consent.
- Relay presence is process-local. Run exactly one serving backend; loss of its PostgreSQL lease stops all sockets.
- Use a fresh test database. Unknown old schemas are rejected; do not reset live databases during development.
- Build and test through the repository root `justfile`; dependencies and SDK installs stay project-local.
