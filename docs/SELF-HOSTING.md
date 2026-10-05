# Self-hosting (not built)

A person can operate the backend today, but the apps cannot use it without environment
variables. This note records the gap and the options. Nothing here is decided.

## The gap

The apps and the command line use one fixed server and one fixed sign-in service. Only
`KODOSI__BACKEND__API` and `KODOSI__AUTH__ISSUER` change them.

## One rule

A token from the Kodosi sign-in service is valid at the Kodosi service. An app must
never send such a token to another server, because the owner of that server could use
it at the Kodosi service as that user. A self-hosted server must have its own sign-in,
and the app must refuse a self-hosted server that names the Kodosi sign-in service.

## The app side

The same for each option:

- One "Server" setting in both apps and in the command line. A change signs the user
  out.
- The server tells the app where its sign-in is, so the user types only one address.
- Sign-in, device approval, friends and sharing do not change. Friends must be on the
  same server.

## Where a self-hosted server gets its sign-in

| Option | What the self-hoster does | Cost |
|---|---|---|
| Approval on the server console | Starts the server. The app shows a sign-in code. The admin approves it with one command on the server and gives the account name. | The server issues and renews its own tokens. No passwords and no web page. |
| Own sign-in provider | Operates an OpenID Connect provider that supports sign-in with a device code, and sets `Auth:Authority` and `Auth:Audience`. | Almost no new code. Hard for a homelab. |
| Password page in the server | Makes accounts with passwords on a web page of the server. | The server stores passwords and has a web page to keep secure. |

"No sign-in, only a server address" is not an option. Each person who can reach the
address would get the account, and a friend would have no identity.

## Recommendation at the time of writing

Approval on the server console, with an own provider also possible through the server
settings. It is easy for a homelab, the app uses the sign-in steps that it has today,
and the admin of the server is the trust anchor. The cost is a second token issuer.
