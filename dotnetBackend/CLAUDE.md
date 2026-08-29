# dotnetBackend

- Endpoints parse and map; policy stays in Application/Domain.
- Request work uses a scoped `DbContext`; socket work uses short factory contexts.
- Realtime state is singleton; terminal frames are memory-only; business code queues instead of writing sockets or doing per-frame database work.
- Relay state is process-local: deploy exactly one backend process.
- No SignalR, MediatR, AutoMapper, MassTransit, or Redis.
- Run `dotnet build` and `dotnet test` without `-q` on macOS (upstream SDK cache-file bug).
