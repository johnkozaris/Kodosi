# Self-hosting

Run the Kodosi backend with PostgreSQL and an OIDC sign-in service, behind HTTPS.

## Configure sign-in

Create a public OIDC client named `kodosi-app` in your provider. Enable the device
authorization and refresh-token flows, with the scopes `openid profile offline_access`.

The issuer's discovery document exposes `device_authorization_endpoint` and
`token_endpoint` on the issuer's own origin. Use HTTPS; loopback HTTP works for local
development. Access tokens use RS256 or ES256, identify the issuer and the audience,
and include a nonempty `sub` and an expiry. Accounts are identified by issuer and
subject. `preferred_username` and `name` supply display names.

## Start the backend

Build from the repository root with the pinned .NET SDK, or use
[backend/Dockerfile](../backend/Dockerfile) with the repository root as its build
context, as [Build the backend image](#build-the-backend-image) tells. Set these
environment variables through your deployment's configuration:

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
run the host once with the `migrate` argument, then start it normally.

Run one serving instance. Back up the database before upgrades and keep the ASP.NET
Data Protection keys across backend replacements.

Terminate HTTPS at a reverse proxy with WebSocket support. Configure trusted proxy
addresses through `Proxy__Addresses__0` (and successive indices as needed) when using
forwarded headers. `/health/live` checks the process; `/health/ready` reports readiness.

### Account deletion

With Keycloak, the backend follows the account deletions in Keycloak's events. Give it
a confidential client that has only a service account, with the `realm-management`
role `view-events`:

| Variable | Value |
| --- | --- |
| `Auth__AccountDeletion__ClientId` | The client ID |
| `Auth__AccountDeletion__ClientSecret` | Its secret |
| `Auth__AccountDeletion__AdminUrl` | Optional: the Keycloak address for the admin API, when the public address refuses `/admin`. HTTPS, or loopback HTTP |

The backend reads the events every 15 seconds.

### Build the backend image

To build the release image of the backend, run this command from the repository root:

```sh
just backend-package linux/amd64 registry.example.com/kodosi/backend:1
```

The first argument is the platform of the image, for example `linux/amd64` or
`linux/arm64`; the second is its tag (default `kodosi-backend`). The image goes to your
local Docker images.

[All documentation](README.md) · [Security](SECURITY.md)
