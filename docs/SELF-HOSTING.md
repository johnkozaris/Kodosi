# Self-hosting

Run the backend with PostgreSQL and an OIDC sign-in service, behind HTTPS. The
[sign-in theme](../sign-in/README.md) gives Keycloak the look of the apps.

## Sign-in

Create a public OIDC client `kodosi-app` with refresh tokens, the scopes
`openid profile offline_access`, authorization code with PKCE (S256) for the apps, and
device authorization for the command line. Allow the redirects
`http://127.0.0.1/callback` on any port (Mac, Linux, Windows) and `com.kodosi:/auth`
(iPhone and iPad).

The issuer's discovery document must give its authorization, device authorization and
token endpoints on the issuer's origin, over HTTPS (loopback HTTP for development).
Access tokens must be RS256 or ES256, name the issuer and the audience `kodosi-app`, and
carry `sub`, an expiry and `auth_time`; deleting an account or starting fresh needs a
sign-in at most 15 minutes old. The backend keys accounts by issuer and subject, and
`preferred_username` and `name` give display names.

## Backend

Build with the pinned .NET SDK, or build the image (below). Use PostgreSQL 14 or later.
Set:

| Variable | Value |
| --- | --- |
| `ConnectionStrings__Kodosi` | PostgreSQL connection string |
| `Auth__Authority` | OIDC issuer URL |
| `Auth__Audience` | `kodosi-app` |
| `ASPNETCORE_URLS` | Listening address |
| `Proxy__Addresses__0`, `__1`, … | Trusted reverse proxies for forwarded headers |

Run the host once with the argument `migrate`, then start it with no argument;
`just backend-run` does both. Run one serving instance, and back up the database before
upgrades. Terminate HTTPS at a reverse proxy that passes WebSockets. `/health/live`
checks that the process runs, and `/health/ready` checks that it can serve.

The backend uses up to 32 pooled database connections plus one more; `Maximum Pool Size`
in the connection string changes the 32. It logs its load each minute, and the meter
`Kodosi.Server` gives the same values.

## Apps

The apps and the `kodosi` command use `https://api.kodosi.com` and
`https://auth.kodosi.com/realms/kodosi` unless their environment sets
`KODOSI__BACKEND__API` to your backend URL and `KODOSI__AUTH__ISSUER` to your issuer
URL, each HTTPS (or loopback HTTP) without a query.

## Account deletion

With Keycloak, the backend follows account deletions in Keycloak's events every 15
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
unlimited. The setting `Plans` defines smaller limits, and the column `users.Plan` names
an account's plan (the default plan otherwise):

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

The limits are `Devices` (1 to 16), `OwnedRooms` (1 to 64), `RoomPeople` in an owned
room, counting the owner and open invitations (1 to 65), `SharePeople` who can see one of
the account's terminals (1 to 64), `RoomBytes` of encrypted data per owned room (at
least 131072), `Friends`, `SentFriendRequests` (1 to 64) and `ReceivedFriendRequests`. An unset limit
keeps its default, and the owner's plan applies to a room or terminal. Past `RoomBytes`
a room drops its oldest messages. A smaller plan removes nothing, recovery is never
limited, a changed `users.Plan` applies at once, and an invalid setting stops the
backend at start.

## Image

```sh
just backend-package linux/amd64 registry.example.com/kodosi/backend:1
```

This builds the image for one platform, with an optional tag, into your local Docker
images.
