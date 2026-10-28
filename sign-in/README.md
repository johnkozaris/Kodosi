# Sign-in pages, account page and e-mails

This folder is the Keycloak theme `kodosi`, built with
[Keycloakify](https://www.keycloakify.dev) and React in the look of the apps. It has the
sign-in pages, one account page in place of Keycloak's account console, and the e-mails.

## Use it in Keycloak

`bun run theme` builds the theme in `dist_keycloak/` (it needs Node.js and Maven). Put
the jar in Keycloak's `providers/` folder and set the realm's login, account and e-mail
themes to `kodosi`. Then:

- `KODOSI_TERMS_URL` and `KODOSI_PRIVACY_URL` on the Keycloak server add terms and
  privacy links.
- Account notices need the realm's `email` event listener.
- Registration and password reset work with Keycloak's reCAPTCHA or the Turnstile
  providers
  [keycloak-cloudflare-turnstile-provider](https://github.com/zymlabs/keycloak-cloudflare-turnstile-provider)
  and [keycloak-turnstile](https://github.com/panpaul/keycloak-turnstile); the realm's
  `frame-src` must allow `https://www.google.com` or `https://challenges.cloudflare.com`.

## Work on the pages

You need [Bun](https://bun.sh) 1.4 and Docker.

- `bun run dev` serves every page with sample data at http://localhost:3184;
  `?scene=<name>` picks a page and `?sheet=0,12` shows twelve side by side.
- `./keycloak.sh` runs a disposable Keycloak with the theme and a mailbox, and prints
  their addresses. `./keycloak.sh code` starts a device sign-in, as the command line
  does, `./keycloak.sh check pass|block|ask|refuse|off` sets the stand-in captcha, and
  `./keycloak.sh down` removes it all.
- `just sign-in-check` from the repository root checks and builds.

Keep Keycloak's field names and ids, because its scripts and password managers rely on
them.
Wording never reveals whether an account exists. In `src/email/messages/`, write an
apostrophe twice. Keycloak caches e-mail pictures for 30 days, so give a changed picture
a new file name. After a Keycloak update, set its version in `keycloak.sh` and check the
pages.
