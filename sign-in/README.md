# Sign-in pages, account page and e-mails

The Keycloak theme `kodosi`, built with [Keycloakify](https://www.keycloakify.dev) and
React in the look of the apps ([DESIGN.md](../clients/macos/DESIGN.md)):

- **Sign-in pages:** the app's device code, sign-in, registration, e-mail confirmation,
  passwords, first sign-in with GitHub, Google or Apple, passkeys, authenticator apps,
  account deletion and Keycloak's messages.
- **Account page:** one page in place of Keycloak's account console, using Keycloak's
  account service. Changes that need proof (sign-in methods, linked services, deletion)
  go through the sign-in pages.
- **E-mails:** one card each, in HTML and plain text.

The apps sign in with the authorization code flow and PKCE: the Mac, Linux and Windows
apps through the system browser and `http://127.0.0.1/callback`, the iPhone and iPad app
in its sign-in window and `com.kodosi:/auth`. The command line uses the OAuth 2.0 device
authorization grant. A page shows the device code only if it sent the code to Keycloak
itself (`.../device#ABCD-EFGH`); with `?user_code=`
Keycloak takes the code and the page asks "Connect Kodosi?" without it. Wording never
reveals whether an account exists.

Registration and password reset support Keycloak's reCAPTCHA step and the Turnstile
steps of
[keycloak-cloudflare-turnstile-provider](https://github.com/zymlabs/keycloak-cloudflare-turnstile-provider)
and [keycloak-turnstile](https://github.com/panpaul/keycloak-turnstile). The realm's
`frame-src` must allow `https://challenges.cloudflare.com` or `https://www.google.com`.

`KODOSI_TERMS_URL` and `KODOSI_PRIVACY_URL` on the Keycloak server add the terms and
privacy links to each page's foot. Account notices need the realm's `email` event
listener (see `keycloak.sh`). Keycloak caches e-mail pictures for 30 days, so a changed
picture needs a new file name.

## Work on the pages

Use [Bun](https://bun.sh) 1.4 and Docker; `bun install` installs the packages.

- `bun run dev` serves every page with sample data at http://localhost:3184
  (`KODOSI_SIGN_IN_PORT` changes the port). `?scene=<name>` picks a page and
  `?sheet=0,12` shows twelve side by side.
- `bun run theme` builds the theme in `dist_keycloak/`; it needs Node.js and Maven with a
  JDK.
- `./keycloak.sh` runs a disposable Keycloak with the theme and a mailbox, and prints
  their addresses. `./keycloak.sh code` starts an app sign-in, `./keycloak.sh check
  pass|block|ask|refuse|off` sets the stand-in captcha (Cloudflare test keys only), and
  `./keycloak.sh down` removes the containers. Sample people are in `dev/people.txt`.
- `just sign-in-check` from the repository root checks types, lint and format, and
  builds.

Keep Keycloak's field names and ids: its scripts and password managers rely on them. In
`src/email/messages/`, write an apostrophe twice. `tools/keycloak-account-ui` stands in
for Keycloak's account console package, which Keycloakify needs for a one-page account
theme. After a Keycloak update, set its version in `keycloak.sh` and check the pages.
