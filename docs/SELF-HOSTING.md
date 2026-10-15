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

### Plans and limits

Without plans, each account can have 10 devices, own 64 rooms with 64 members each,
share a terminal with 64 people, and send 64 friend requests that wait for an answer.
Rooms have no data limit, and accounts have no friend limit.

To set smaller limits, define plans in the setting `Plans`. The column `Plan` of the
table `users` gives the plan of each account. An account without a plan, or with a
plan that the setting does not define, gets the default plan:

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

A limit that a plan does not set gets the value of a server without plans.

| Limit | The most that an account can have | Values |
| --- | --- | --- |
| `Devices` | Approved devices | 1 to 16 |
| `OwnedRooms` | Rooms that it owns | 1 to 64 |
| `RoomPeople` | People in each room that it owns: the owner, the members and the open invitations | 1 to 65 |
| `SharePeople` | People that can see one of its terminals | 1 to 64 |
| `RoomBytes` | Encrypted bytes in each room that it owns | 131072 or more |
| `Friends` | Friends | 1 or more |
| `SentFriendRequests` | Friend requests that it sent and that wait for an answer | 1 to 64 |
| `ReceivedFriendRequests` | Friend requests to it that wait for an answer | 1 or more |

The plan of the owner sets the limits of a room or a terminal. When a new room item
goes past `RoomBytes`, the backend deletes the oldest messages of the room, and the
apps tell the members. Past another limit, the backend refuses the change with status
403 and the name of the limit in the field `limit`. A smaller plan removes nothing that
an account has. An account can always recover its devices with its recovery key.

To give a plan to an account, change its row:

```sql
UPDATE users SET "Plan" = 'team' WHERE "Handle" = 'ana';
```

The change has an effect at once. The backend reads the setting when it starts, and does
not start when the setting has an error.

### Build the backend image

To build the release image of the backend, run this command from the repository root:

```sh
just backend-package linux/amd64 registry.example.com/kodosi/backend:1
```

The first argument is the platform of the image, for example `linux/amd64` or
`linux/arm64`; the second is its tag (default `kodosi-backend`). The image goes to your
local Docker images.

[All documentation](README.md) · [Security](SECURITY.md)
