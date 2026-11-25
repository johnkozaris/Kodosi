# Sign-in pages, account page, and emails

This package builds the `kodosi` Keycloak theme using [Keycloakify](https://www.keycloakify.dev)
and React to match the native apps. It covers the sign-in flows, a single streamlined account page
replacing Keycloak's default account console, and transactional emails.

`extension/` is a Keycloak SPI extension that verifies email addresses using typed codes (during
registration, password reset, and unverified-email login) with zero links in the emails. It also
exposes Keycloak's server clock to the account page so DPoP proofs stay valid even when a user's
device clock is off.

## Deploying to Keycloak

Build the theme JAR into `dist_keycloak/` with `bun run theme` (requires Node.js and Maven), and
build the extension JAR into `extension/target/` with `extension/maven.sh package` (uses Docker;
a local Maven 3.10+ with Java 21 builds it too). Drop both JARs into Keycloak's `providers/`
directory and set the realm's login, account, and email themes to `kodosi`.

Realm configuration:

- In the registration flow, place `kodosi-mail-code` before the registration form, and place
  `kodosi-confirmed-email` inside the form right after user creation.
- In the reset credentials flow, replace the reset email step with `kodosi-mail-code`, placed
  after user selection.
- Enable the `kodosi-mail-code` required action, disable Keycloak's built-in Verify Email action,
  and keep the realm's "Verify email" setting on so password entry comes after code verification.
  Codes expire according to the realm's User-initiated action lifespan.
- Set `KODOSI_TERMS_URL` and `KODOSI_PRIVACY_URL` on Keycloak to show Terms and Privacy links.
- For account security notices, enable the realm's `email` event listener and pass
  `--spi-events-listener--email--include-events` with the events listed in `keycloak.sh`. The
  locked-account email invites users to reply, so send realm emails from a monitored inbox.
- With brute-force detection on, set Maximum Secondary Authentication Failures as configured in
  `keycloak.sh` so repeated bad 2FA codes permanently lock the account, and include
  `USER_DISABLED_BY_PERMANENT_LOCKOUT` in the included events list above so the lockout email
  fires.
- For CAPTCHA, the theme supports Keycloak's built-in reCAPTCHA as well as
  [keycloak-cloudflare-turnstile-provider](https://github.com/zymlabs/keycloak-cloudflare-turnstile-provider)
  and [keycloak-turnstile](https://github.com/panpaul/keycloak-turnstile). Allow
  `https://www.google.com` or `https://challenges.cloudflare.com` in the realm's `frame-src` CSP.
  In registration, place the CAPTCHA before `kodosi-mail-code` so emails aren't sent beforehand
  (`keycloak-cloudflare-turnstile-provider` handles this on its own step).

## Local development

Requires [Node.js](https://nodejs.org) 22.12+ and Docker. `python3 ../scripts/bun.py` prints the
path of the Bun pinned in [.bun-version](../.bun-version), installing it into `.tools/bun/` first;
the recipes and `./keycloak.sh` use that Bun.

- `"$(python3 ../scripts/bun.py)" run dev`: preview every page with mock data at
  `http://localhost:3184`. Use `?scene=<name>` for a single page, `?sheet=0,12` for a 12-up
  contact sheet, and `?lang=de` for localization (using Keycloak's language tags, e.g. `zh-CN` for
  Simplified Chinese).
- `./keycloak.sh`: starts a disposable Keycloak instance with the theme, extension, and a local
  mailbox, printing their URLs. Use `./keycloak.sh code` to trigger a CLI device login,
  `./keycloak.sh check pass|block|ask|refuse|off` to simulate CAPTCHA responses, and
  `./keycloak.sh down` to tear everything down. Override `KODOSI_SIGN_IN_RIG`,
  `KODOSI_SIGN_IN_KEYCLOAK_PORT`, and `KODOSI_SIGN_IN_MAILBOX_PORT` to run a second instance in
  parallel.
- `just sign-in-check` (from the repo root): lints, checks formatting, builds the theme, and builds
  and tests the Keycloak extension.

## Implementation notes

- Preserve Keycloak's original form field `name` and `id` attributes; Keycloak's scripts and
  password managers rely on them.
- Never leak whether an account exists in UI copy or the email-code flow: existing addresses see
  the same screens and receive an informational email instead of a verification code.
- Escape apostrophes by doubling them (`''`) in `src/email/messages/` (Java `MessageFormat` syntax).
- Keycloak caches email images for 30 days, so give updated images a new filename.
- When upgrading Keycloak, update the version and image digest in `keycloak.sh` plus the version
  in `extension/pom.xml`, then re-verify the theme pages and email-code flow.
