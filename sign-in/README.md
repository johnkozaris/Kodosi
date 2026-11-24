# Sign-in pages, account page and e-mails

This folder is the Keycloak theme `kodosi`, built with
[Keycloakify](https://www.keycloakify.dev) and React in the look of the apps. It has the
sign-in pages, one account page in place of Keycloak's account console, and the e-mails.
`extension/` is a Keycloak extension that checks a mail address with a typed code: before a
new account exists, when a password is forgotten, and for an account whose address is not
confirmed. Its mails have no links. It also gives the account page Keycloak's time, which the
page needs to bind its tokens to the browser.

## Use it in Keycloak

`bun run theme` builds the theme in `dist_keycloak/` (it needs Node.js and Maven), and
`extension/maven.sh package` builds the extension in `extension/target/` (it needs Docker;
Maven 3.10 or later with Java 21 builds it too). Put both jars in Keycloak's `providers/`
folder and set the realm's login, account and e-mail themes to `kodosi`. Then:

- In the registration flow, the step `kodosi-mail-code` goes before the registration form,
  and in the form `kodosi-confirmed-email` goes after the step that makes the account.
- In the reset flow, `kodosi-mail-code` takes the place of the reset e-mail, after the step
  that chooses the account.
- The required action `kodosi-mail-code` takes the place of Verify Email: enable it,
  disable Verify Email, and keep the realm's Verify email on, so that the password comes
  after the code. A code works as long as the realm's User-initiated action lifespan.
- `KODOSI_TERMS_URL` and `KODOSI_PRIVACY_URL` on the Keycloak server add terms and
  privacy links.
- Account notices need the realm's `email` event listener and Keycloak's option
  `--spi-events-listener--email--include-events` with the events that `keycloak.sh` names.
  The notice of a locked account asks the person to reply, so send the realm's mail from an
  address that someone reads.
- With brute force detection on, set the realm's Maximum Secondary Authentication Failures
  as `keycloak.sh` does, so that many wrong authenticator codes lock an account for good;
  the notice of that lockout needs `USER_DISABLED_BY_PERMANENT_LOCKOUT` among the events of
  the option above.
- The pages show the person check of Keycloak's reCAPTCHA or of the Turnstile providers
  [keycloak-cloudflare-turnstile-provider](https://github.com/zymlabs/keycloak-cloudflare-turnstile-provider)
  and [keycloak-turnstile](https://github.com/panpaul/keycloak-turnstile); the realm's
  `frame-src` must allow `https://www.google.com` or `https://challenges.cloudflare.com`.
  In the registration flow the check goes before `kodosi-mail-code`, so that no step sends
  mail before it; the authenticator of keycloak-cloudflare-turnstile-provider does that on
  its own page.

## Work on the pages

You need [Node.js](https://nodejs.org) 22.12 or later and Docker. `python3 ../scripts/bun.py`
prints the path of the Bun of [.bun-version](../.bun-version), and installs it in
`.tools/bun/` first; the recipes and `./keycloak.sh` use that Bun.

- `"$(python3 ../scripts/bun.py)" run dev` serves every page with sample data at http://localhost:3184;
  `?scene=<name>` picks a page, `?sheet=0,12` shows twelve side by side, and `?lang=de`
  shows a language (Keycloak's tag: `zh-CN` for Chinese).
- `./keycloak.sh` runs a disposable Keycloak with the theme, the extension and a mailbox,
  and prints their addresses. `./keycloak.sh code` starts a device sign-in, as the command
  line does, `./keycloak.sh check pass|block|ask|refuse|off` sets the stand-in captcha, and
  `./keycloak.sh down` removes it all. `KODOSI_SIGN_IN_RIG` names its containers, and
  `KODOSI_SIGN_IN_KEYCLOAK_PORT` and `KODOSI_SIGN_IN_MAILBOX_PORT` set its ports, so that a
  second one can run beside the first.
- `just sign-in-check` from the repository root checks and builds the pages, and builds
  and tests the extension.

Keep Keycloak's field names and ids, because its scripts and password managers rely on
them.
Wording never reveals whether an account exists, and neither does a page of the code
step: an address that has an account gets the same pages, and its mail says so in place
of a code. In `src/email/messages/`, write an apostrophe twice. Keycloak caches e-mail
pictures for 30 days, so give a changed picture a new file name. After a Keycloak update,
set its version and image digest in `keycloak.sh` and its version in `extension/pom.xml`,
and check the pages and the code step.
