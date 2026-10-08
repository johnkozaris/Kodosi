# Self-hosting

Kodosi can use your own backend and OIDC sign-in service through environment
configuration. There is no server selector in the apps yet. All participants in a
room use the same service.

You need PostgreSQL, one serving Kodosi backend process, an OIDC provider supporting
device authorization, and HTTPS endpoints reachable from participant machines.

## Configure sign-in

Create a public OIDC client named `kodosi-app` in your provider. Enable the device
authorization and refresh-token flows, with the scopes `openid profile offline_access`.
The client ID is currently fixed in the runtime.

The issuer's discovery document must expose `device_authorization_endpoint` and
`token_endpoint` on the issuer's own origin. Use HTTPS; loopback HTTP is accepted for
local development. Access tokens must use RS256 or ES256, identify the configured
issuer and audience, and include a nonempty `sub` and valid expiry. Accounts are
identified by issuer and subject. `preferred_username` and `name` supply display names.

Configure a dedicated issuer/client for your deployment. Use the same issuer URL in
the backend and client settings below.

## Start the backend

Build from the repository root with the pinned .NET SDK, or use
[backend/Dockerfile](../backend/Dockerfile) with the repository root as its build
context. Set these environment variables through your deployment's configuration:

| Variable | Value |
| --- | --- |
| `ConnectionStrings__Kodosi` | Your PostgreSQL connection string |
| `Auth__Authority` | Your OIDC issuer URL |
| `Auth__Audience` | `kodosi-app` |
| `ASPNETCORE_URLS` | The backend's listening address |

Run the development host after configuring those values:

```sh
just backend-run
```

This recipe applies migrations before serving. For a published backend deployment,
run the host once with the `migrate` argument, then start it normally. A serving
start refuses an outdated or unknown schema without changing it.

Run one serving instance: live device sessions and relay routing are held in memory,
and the backend takes a database lease. PostgreSQL stores durable shared records.
Back up the database before upgrades and retain ASP.NET Data Protection keys across
backend replacements.

Terminate HTTPS at a reverse proxy with WebSocket support. Configure trusted proxy
addresses through `Proxy__Addresses__0` (and successive indices as needed) when using
forwarded headers. `/health/live` checks the process; `/health/ready` reports readiness.

The repository's [Compose setup](../compose.yaml) is for local development: it binds
only to loopback and includes a development database password. See
[Development](DEVELOPMENT.md#run-the-backend) for its volume selection and startup.
Supply deployment credentials through your own environment configuration.

## Point the apps at your service

Launch the app process with both settings:

```sh
export KODOSI__BACKEND__API=https://kodosi.example.com
export KODOSI__AUTH__ISSUER=https://identity.example.com/realms/kodosi
```

These example domains are placeholders. The backend URL is the service root, without
an added `/api` suffix. On macOS, launch the app executable from that environment;
Finder does not inherit a terminal's exported variables. On Linux, launch `kodosi-qt`
from that environment. The CLI talks to the running host,
so configure and restart that host too. Mac Debug builds have a local backend
override; see the [Mac build guide](../clients/macos/README.md#build-and-run).

Use a fresh installation or separate OS profile for a different deployment. Current
clients do not manage several server profiles or migrate account identity between
servers. Sign in to your provider, approve devices, and invite participants normally.

## Check the deployment

Confirm readiness, then sign in from a client and create a terminal. Connect a second
approved device, share a terminal with a room, and exchange a room message. Check the
backend and client logs while doing it. A health response alone does not establish
that sign-in, WebSocket proxying, and device-to-device encryption work together.

[All documentation](README.md) · [Security](SECURITY.md)
