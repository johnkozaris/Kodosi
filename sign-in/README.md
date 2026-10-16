# Sign-in pages, account page and e-mails

The Keycloak theme `kodosi`, built with [Keycloakify](https://www.keycloakify.dev) and
React in the look of the apps ([DESIGN.md](../clients/macos/DESIGN.md)): the sign-in
pages (device code, sign-in, registration, e-mail confirmation, passwords, GitHub, Google
and Apple sign-in, passkeys, authenticator apps, account deletion and Keycloak's
messages), one account page in place of Keycloak's account console, and the e-mails in
HTML and plain text.

The apps sign in with the authorization code flow and PKCE; the command line uses the
device authorization grant. A page shows the device code only if it sent it to Keycloak
itself (`.../device#ABCD-EFGH`); with `?user_code=` the page asks "Connect Kodosi?"
without it. Wording never reveals whether an account exists.

Registration and password reset support Keycloak's reCAPTCHA step and the Turnstile
steps of
[keycloak-cloudflare-turnstile-provider](https://github.com/zymlabs/keycloak-cloudflare-turnstile-provider)
and [keycloak-turnstile](https://github.com/panpaul/keycloak-turnstile); the realm's
`frame-src` must allow `https://challenges.cloudflare.com` or `https://www.google.com`.
`KODOSI_TERMS_URL` and `KODOSI_PRIVACY_URL` on the Keycloak server add terms and privacy
links. Account notices need the realm's `email` event listener. Keycloak caches e-mail
pictures for 30 days, so a changed picture needs a new file name.

## Work on the pages

Use [Bun](https://bun.sh) 1.4 and Docker.

- `bun run dev` serves every page with sample data at http://localhost:3184;
  `?scene=<name>` picks a page and `?sheet=0,12` shows twelve side by side.
- `bun run theme` builds the theme in `dist_keycloak/` (needs Node.js and Maven).
- `./keycloak.sh` runs a disposable Keycloak with the theme and a mailbox;
  `./keycloak.sh code` starts a device sign-in, as the command line does,
  `./keycloak.sh check pass|block|ask|refuse|off` sets the stand-in captcha, and
  `./keycloak.sh down` removes it. Sample people are in `dev/people.txt`.
- `just sign-in-check` from the repository root checks and builds.

Keep Keycloak's field names and ids: its scripts and password managers rely on them. In
`src/email/messages/`, write an apostrophe twice. `tools/keycloak-account-ui` stands in
for the account console package that Keycloakify needs. After a Keycloak update, set its
version in `keycloak.sh` and check the pages.
