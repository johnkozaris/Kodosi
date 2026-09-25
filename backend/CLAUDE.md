# Kodosi backend

One ASP.NET application stores account, device, friendship, Mission, and explicit
terminal-sharing metadata and routes encrypted terminal traffic.

- Approved own devices can use owner terminals. Friends require an explicit share for
  each terminal. Mission membership never grants access.
- Identity changes, sharing changes, and revocation must take effect before more
  traffic is admitted.
- Terminal connection state is bounded and temporary. It is not a terminal archive or durable input
  queue, and removing expired metadata must never end the host's local process.
- Feature services own short EF transactions directly. Do not add repository or port
  layers around them.
- Run one serving backend process.
- Use the root `justfile` and fresh test storage. Unknown schemas must fail without
  resetting live data.
