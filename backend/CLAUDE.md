# Kodosi backend

One ASP.NET application stores shared metadata and routes encrypted traffic.
Follow `../AGENTS.md` and `../PRODUCT.md` for product intent and implementation gaps.

- The backend does not run terminals or agents or receive terminal or room plaintext.
- Keep sharing consistent with room membership. The existing per-person share model
  is current implementation, not a requirement to preserve as room sharing is built.
- Terminal connection state is bounded and temporary. It is not a terminal archive or durable input
  queue. An offline host keeps its terminal record and sharing. Only ended records are removed, and
  removing them must never end the host's local process.
- Feature services own short EF transactions directly. Do not add repository or port
  layers around them.
- Run one serving backend process.
- Use the root `justfile` and fresh test storage. Unknown schemas must fail without
  resetting live data.
