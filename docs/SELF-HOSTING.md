# Self-hosting

Run the backend with PostgreSQL and an OIDC sign-in service, behind HTTPS.

## Sign-in

Create a public OIDC client `kodosi-app` with the device authorization and refresh-token
flows and the scopes `openid profile offline_access`. The issuer's discovery document
must expose `device_authorization_endpoint` and `token_endpoint` on the issuer's origin,
over HTTPS (loopback HTTP works for development). Access tokens use RS256 or ES256, name
the issuer and audience, and include a non-empty `sub`, an expiry and `auth_time`.
Deleting an account and starting fresh on a device need a sign-in of at most 15 minutes,
which the backend reads from `auth_time`. Accounts are keyed by issuer and subject;
`preferred_username` and `name` give display names.

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
HTTPS at a reverse proxy with WebSocket support. `/health/live` checks the process and `/health/ready` readiness.

The backend uses at most 32 database connections, and one more for its lease; set
`Maximum Pool Size` in the connection string to change the 32. A stopping backend closes
each device connection as a service restart, and the apps connect again. Each minute
the backend logs one line with its connections, hosted terminals and device sessions,
and with the data it relayed and the requests that it refused or that failed in that
minute. The meter `Kodosi.Server` gives the same values, for example to
`dotnet-counters monitor --counters Kodosi.Server`.

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

Without plans, each account can have 10 devices, own 64 rooms of 64 members, share a
terminal with 64 people and send 64 open friend requests; room data and friends have no
limit. To set smaller limits, define plans in the setting `Plans`. The column
`users.Plan` names the plan of an account; an account without a plan, or with an unknown
one, gets the default plan:

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

| Limit | The most that an account can have | Values |
| --- | --- | --- |
| `Devices` | Approved devices | 1 to 16 |
| `OwnedRooms` | Rooms that it owns | 1 to 64 |
| `RoomPeople` | People in each room that it owns, with the owner and open invitations | 1 to 65 |
| `SharePeople` | People who can see one of its terminals | 1 to 64 |
| `RoomBytes` | Encrypted bytes in each room that it owns | 131072 or more |
| `Friends` | Friends | 1 or more |
| `SentFriendRequests` | Open friend requests that it sent | 1 to 64 |
| `ReceivedFriendRequests` | Open friend requests to it | 1 or more |

A limit that a plan does not set keeps its value without plans. The owner's plan sets
the limits of a room or a terminal. A new room item past `RoomBytes` deletes the room's
oldest messages, and the apps say so; past another limit the backend refuses with status
403 and the limit's name in `limit`. A smaller plan removes nothing, and a recovery with
the recovery key is never limited. A plan change in the database applies at once, for
example `UPDATE users SET "Plan" = 'team' WHERE "Handle" = 'ana';`. An error in the
setting stops the backend at start.

## Image

```sh
just backend-package linux/amd64 registry.example.com/kodosi/backend:1
```

Builds the image for a platform with an optional tag (default `kodosi-backend`) into
your local Docker images.
