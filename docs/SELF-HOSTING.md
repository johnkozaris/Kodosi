# Self-hosting

Run the backend with PostgreSQL and an OIDC sign-in service, behind HTTPS.

## Sign-in

Create a public OIDC client `kodosi-app` with the device authorization and refresh-token
flows and the scopes `openid profile offline_access`. The issuer's discovery document
must expose `device_authorization_endpoint` and `token_endpoint` on the issuer's origin,
over HTTPS (loopback HTTP works for development). Access tokens use RS256 or ES256, name
the issuer and audience, and include a non-empty `sub` and an expiry. Accounts are keyed
by issuer and subject; `preferred_username` and `name` give display names.

## Backend

Build with the pinned .NET SDK, or build the image (below). Configure:

| Variable | Value |
| --- | --- |
| `ConnectionStrings__Kodosi` | PostgreSQL connection string |
| `Auth__Authority` | OIDC issuer URL |
| `Auth__Audience` | `kodosi-app` |
| `ASPNETCORE_URLS` | Listening address |
| `Proxy__Addresses__0`, `__1`, … | Trusted reverse proxies for forwarded headers |

Run the host once with the `migrate` argument, then start it normally (`just backend-run`
does both). Run one serving instance, back up the database before upgrades, and keep the
ASP.NET Data Protection keys across replacements. Terminate HTTPS at a reverse proxy
with WebSocket support. `/health/live` checks the process and `/health/ready` readiness.

## Account deletion

With Keycloak, the backend follows account deletions from Keycloak's events every 15
seconds. Give it a confidential client with only a service account and the
`realm-management` role `view-events`:

| Variable | Value |
| --- | --- |
| `Auth__AccountDeletion__ClientId` | Client ID |
| `Auth__AccountDeletion__ClientSecret` | Client secret |
| `Auth__AccountDeletion__AdminUrl` | Optional Keycloak admin address (HTTPS or loopback HTTP) when the public one refuses `/admin` |

## Image

```sh
just backend-package linux/amd64 registry.example.com/kodosi/backend:1
```

Builds the image for a platform with an optional tag (default `kodosi-backend`) into
your local Docker images.
