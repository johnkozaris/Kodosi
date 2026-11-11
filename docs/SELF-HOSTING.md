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
recent sign-in. The backend keys accounts by issuer and subject. Each
account must also have a `preferred_username` of 3 to 64 characters from `a` to `z`, `0` to
`9`, `_` and `-` (capital letters become small) that belongs to that account only and never
changes: it is the person's user name in Kodosi. The backend refuses a sign-in without such
a name, or with a name that another account has. `name` gives the display name.

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

With Keycloak, the backend follows account deletions in Keycloak's events. Give it a
confidential client with only a service account and the `realm-management` role
`view-events`:

| Variable | Value |
| --- | --- |
| `Auth__AccountDeletion__ClientId` | Client ID |
| `Auth__AccountDeletion__ClientSecret` | Client secret |
| `Auth__AccountDeletion__AdminUrl` | Optional Keycloak admin address (HTTPS or loopback HTTP) when the public one refuses `/admin` |

## Push notifications

The backend sends a push notification to a person's iPhone, iPad and Android devices when
one of their terminals starts to wait, finishes or fails. Without these settings it sends
none. Set those of Apple, of Google or both. Apple delivers only to the apps of the team
that owns the key, and Google only to the apps of the Firebase project:

| Variable | Value |
| --- | --- |
| `Push__Apns__TeamId` | Apple team ID |
| `Push__Apns__KeyId` | Key ID of the APNs authentication key |
| `Push__Apns__PrivateKey` | Text of the key's `.p8` file |
| `Push__Apns__Topic` | Bundle ID of the iPhone and iPad app, `com.kodosi.mobile` |
| `Push__Fcm__ProjectId` | Firebase project ID |
| `Push__Fcm__ServiceAccount` | JSON text of a Google service account that may send Firebase Cloud Messaging messages |

A part of either set, or a key that does not load, stops the backend at start.

## Plans

The setting `Plans` limits what an account can have, and the column `users.Plan` names
an account's plan (the default plan otherwise):

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

The limits are `Devices`, `OwnedProjects`, `ProjectPeople` in an owned project (with the
owner and open invitations), `SharePeople` who can see one of the account's terminals,
`ProjectBytes` of encrypted data in an owned project, `Friends`, `SentFriendRequests` and
`ReceivedFriendRequests`. An unset limit keeps the backend's own limit, and the owner's
plan applies to a project or terminal. Past `ProjectBytes` a project drops its oldest
messages. A smaller plan removes nothing, recovery is never limited, and a changed
`users.Plan` applies at once. The backend does not start with a setting that is not
valid, and names the value that it refuses.

## Bans

A user name tells whether its account exists, so the backend limits how many user names
of strangers an account can try. It bans an account that keeps trying for hours: it ends
the account's connections and refuses all its requests. The column `users.BannedAt` holds
the time of the ban; set it to null to remove the ban.

## Image

```sh
just backend-package linux/amd64 registry.example.com/kodosi/backend:1
```

This builds the image for one platform, with an optional tag, into your local Docker
images.
