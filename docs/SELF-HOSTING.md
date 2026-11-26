# Self-hosting

Run the Kodosi backend with PostgreSQL and an OIDC identity provider behind HTTPS. The
[sign-in theme and Keycloak extension](../sign-in/README.md) match the apps' visual style and
replace email verification links with typed codes.

## OIDC sign-in

Configure a public OIDC client named `kodosi-app` with:

- Refresh tokens enabled and scopes `openid profile offline_access`
- Authorization code flow with PKCE (`S256`) for the apps
- Device authorization flow for the CLI
- Redirect URIs `http://127.0.0.1/callback` on any port (macOS, Linux, Windows) and
  `com.kodosi:/auth` (iOS and Android)

The issuer's OIDC discovery document must expose authorization, device authorization, and token
endpoints on the issuer's own origin over HTTPS (loopback HTTP is permitted in development).

Access tokens must be signed with `RS256` or `ES256`, include the issuer, audience `kodosi-app`,
`sub`, an expiration, and `auth_time` (recent authentication is required to delete an account or
start fresh). The backend identifies accounts by `(issuer, sub)`. Each account must also have a
unique, immutable `preferred_username` of 3 to 64 characters using `a-z`, `0-9`, `_`, and `-`
(uppercase is lowercased); logins missing a valid username or colliding with another account are
rejected. The `name` claim supplies the display name.

## Backend

Build with the pinned .NET SDK or use the Docker image below. On x86-64 (including VMs), the
backend requires `x86-64-v2` CPU instructions. Use PostgreSQL 15 or newer.

Set these environment variables:

| Variable | Value |
| --- | --- |
| `ConnectionStrings__Kodosi` | PostgreSQL connection string |
| `Auth__Authority` | OIDC issuer URL |
| `Auth__Audience` | `kodosi-app` |
| `ASPNETCORE_URLS` | Bind address |
| `Proxy__Addresses__0`, `__1`, … | Trusted reverse-proxy IPs for forwarded headers |
| `Relay__Address` | Host name or IP address at which devices reach the backend's UDP port |
| `Relay__Port` | That UDP port, `3478` unless set |

Run the binary once with `migrate`, then start it normally (`just backend-run` does both). Run a
single backend instance and back up PostgreSQL before upgrading. Terminate HTTPS at a reverse proxy
that forwards WebSockets. `/health/live` checks process liveness and `/health/ready` checks serving
readiness.

Open the UDP port to the backend itself, not through the proxy. Devices ask it for their public
addresses (STUN) to connect directly, and it relays QUIC between two devices that find no direct
path; QUIC encrypts that traffic end to end. Without `Relay__Address`, devices connect directly
only within one network.

The backend opens up to 32 pooled database connections plus one dedicated connection (override the
pool size via `Maximum Pool Size` in the connection string). It logs load metrics every minute and
exports the same counters under the `Kodosi.Server` meter.

## Pointing clients at your server

Clients and the `kodosi` CLI default to `https://api.kodosi.com` and
`https://auth.kodosi.com/realms/kodosi`. Override them by setting `KODOSI__BACKEND__API` and
`KODOSI__AUTH__ISSUER` to your HTTPS URLs (or loopback HTTP for dev) without query strings.

App updates and Kodosi Browser come from Kodosi's update feeds on `updates.kodosi.com`,
independent of your backend. The feeds point to Kodosi's releases on GitHub.

## Account deletion sync

When using Keycloak, the backend can follow account deletions through Keycloak's events. Create a
confidential client with only a service account enabled, grant it the `realm-management` role
`view-events` and the `kc-admin-api` client scope, and set:

| Variable | Value |
| --- | --- |
| `Auth__AccountDeletion__ClientId` | Client ID |
| `Auth__AccountDeletion__ClientSecret` | Client secret |
| `Auth__AccountDeletion__AdminUrl` | Optional Keycloak admin URL (HTTPS or loopback HTTP) if the public URL blocks `/admin` |

The backend requests `kc-admin-api` because Keycloak's Admin API rejects tokens without that scope.

## Push notifications

The backend sends push notifications to iOS and Android devices when a terminal starts waiting,
finishes, or fails while the user is inactive on all devices. Leave these unset to disable push, or
configure APNs, FCM, or both. APNs only delivers to apps signed by the team owning the key, and FCM
only delivers to apps registered in that Firebase project:

| Variable | Value |
| --- | --- |
| `Push__Apns__TeamId` | Apple Team ID |
| `Push__Apns__KeyId` | APNs auth key ID |
| `Push__Apns__PrivateKey` | Contents of the `.p8` private key file |
| `Push__Apns__Topic` | iOS app bundle ID (`com.kodosi.mobile`) |
| `Push__Fcm__ProjectId` | Firebase project ID |
| `Push__Fcm__ServiceAccount` | JSON credentials for a Google service account authorized to send FCM messages |

Partially configured credentials or an unreadable key will prevent the backend from starting.

## Account plans

Configure per-account quotas under `Plans`. The `users.Plan` column assigns an account's plan
(falling back to `Default`):

```json
"Plans": {
  "Default": "free",
  "Definitions": {
    "free": { "Devices": 4, "OwnedProjects": 3, "ProjectPeople": 3, "SharePeople": 2,
              "ProjectBytes": 10485760, "Friends": 50, "SentFriendRequests": 20,
              "ReceivedFriendRequests": 50 },
    "team": { "OwnedProjects": 20, "ProjectPeople": 20 }
  }
}
```

Supported limits: `Devices`, `OwnedProjects`, `ProjectPeople` (in an owned project, counting the
owner and pending invites), `SharePeople` (people who can see one of the user's terminals),
`ProjectBytes` (encrypted storage in an owned project), `Friends`, `SentFriendRequests`, and
`ReceivedFriendRequests`.

Any omitted field falls back to the backend's built-in limit, and a project or terminal uses its
owner's plan. Exceeding `ProjectBytes` evicts the project's oldest messages. Moving to a smaller
plan never deletes existing data, account recovery is never capped, and edits to `users.Plan` take
effect immediately. Invalid plan config stops startup and logs the offending value.

## Rate limits and bans

Because username lookups reveal whether an account exists, the backend rate-limits how many
unknown usernames an account can probe. Accounts that keep probing over several hours are banned
automatically: their connections are closed and all requests are rejected. `users.BannedAt` records
the ban timestamp; clear it to `NULL` to unban an account.

## Container image

```sh
just backend-package linux/amd64 registry.example.com/kodosi/backend:1
```

Builds and loads a single-platform Docker image (with an optional tag) into your local Docker
daemon.
