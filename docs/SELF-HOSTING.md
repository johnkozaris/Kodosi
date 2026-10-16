# Self-hosting

Run the backend with PostgreSQL and an OIDC sign-in service, behind HTTPS.

## Sign-in

Create a public OIDC client `kodosi-app` with refresh tokens and the scopes `openid
profile offline_access`:

- Flows: authorization code with PKCE (S256) for the apps, device authorization for the
  command line.
- Redirects: `http://127.0.0.1/callback` on any port (Mac, Linux, Windows) and
  `com.kodosi:/auth` (iPhone and iPad).
- The discovery document exposes `authorization_endpoint`,
  `device_authorization_endpoint` and `token_endpoint` on the issuer's origin, over HTTPS
  (loopback HTTP for development).
- Access tokens use RS256 or ES256, name the issuer and audience, and carry `sub`, an
  expiry and `auth_time`. Deleting an account or starting fresh needs a sign-in at most
  15 minutes old. Accounts are keyed by issuer and subject; `preferred_username` and
  `name` give display names.

## Backend

Build with the pinned .NET SDK, or build the image (below). Use PostgreSQL 14 or later.
Configure:

| Variable | Value |
| --- | --- |
| `ConnectionStrings__Kodosi` | PostgreSQL connection string |
| `Auth__Authority` | OIDC issuer URL |
| `Auth__Audience` | `kodosi-app` |
| `ASPNETCORE_URLS` | Listening address |
| `Proxy__Addresses__0`, `__1`, … | Trusted reverse proxies for forwarded headers |

Run the host once with the `migrate` argument, then start it normally (`just backend-run`
does both). Run one serving instance and back up the database before upgrades. Terminate
HTTPS at a reverse proxy with WebSocket support. `/health/live` checks the process and
`/health/ready` readiness.

The backend uses at most 32 database connections plus one for its lease (`Maximum Pool
Size` in the connection string changes the 32). Each minute it logs its connections,
hosted terminals, device sessions, relayed data and refused or failed requests; the
meter `Kodosi.Server` gives the same values.

## Account deletion

With Keycloak, the backend follows account deletions from Keycloak's events every 15
seconds. Give it a confidential client with only a service account and the
`realm-management` role `view-events`:

| Variable | Value |
| --- | --- |
| `Auth__AccountDeletion__ClientId` | Client ID |
| `Auth__AccountDeletion__ClientSecret` | Client secret |
| `Auth__AccountDeletion__AdminUrl` | Optional Keycloak admin address (HTTPS or loopback HTTP) when the public one refuses `/admin` |

## Plans

Without plans, an account can have 10 devices, own 64 rooms of 64 members, share a
terminal with 64 people and send 64 open friend requests; room data and friends are
unlimited. Plans in the setting `Plans` set smaller limits, and the column `users.Plan`
names an account's plan (the default plan otherwise):

```json
"Plans": {
  "Default": "free",
  "Definitions": {
    "free": { "Devices": 4, "OwnedRooms": 3, "RoomPeople": 3, "SharePeople": 2,
              "RoomBytes": 10485760, "Friends": 50, "SentFriendRequests": 20,
              "ReceivedFriendRequests": 50 },
    "team": { "OwnedRooms": 20, "RoomPeople": 20 }
  }
}
```

Limits: `Devices` (1–16), `OwnedRooms` (1–64), `RoomPeople` per owned room with the owner
and open invitations (1–65), `SharePeople` who can see one of its terminals (1–64),
`RoomBytes` of encrypted data per owned room (at least 131072), `Friends`,
`SentFriendRequests` (1–64) and `ReceivedFriendRequests`. An unset limit keeps its
default, and the owner's plan applies to a room or terminal. Past `RoomBytes` a room
drops its oldest messages; past another limit the backend answers 403 with an error
that says what to do. A smaller plan removes nothing, recovery is never limited, a change in
the database applies at once, and an invalid setting stops the backend at start.

## Image

```sh
just backend-package linux/amd64 registry.example.com/kodosi/backend:1
```

This builds the image for a platform, with an optional tag, into your local Docker
images.
