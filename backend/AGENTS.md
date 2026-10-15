# Backend

One ASP.NET Core application stores shared metadata and relays encrypted traffic. It
never runs terminals or agents, or sees terminal or room plaintext.

- Terminal connection state is bounded and temporary: no terminal archive and no
  durable input queue. An offline host keeps its terminal records and sharing. Only
  ended records are removed, and removing them never ends a host's process.
- Feature services own short EF transactions directly; add no repository or port layers.
- Run one serving process. Unknown schemas fail without resetting data.
- Check with `just backend-check` and fresh test storage.
