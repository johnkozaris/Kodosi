# Sign-in pages, account page and e-mails

Kodosi signs people in with Keycloak. This package is the theme that Keycloak shows them, with the
name `kodosi`. It has the three kinds of a Keycloak theme:

- **The sign-in pages** (`src/login`): the code of the Kodosi app, the sign-in, a new account, the
  confirmation of an e-mail, a new password, a first sign-in with GitHub, Google or Apple, passkeys,
  authenticator apps, the deletion of an account, and each message of Keycloak.
- **The account page** (`src/account`): how a person signs in, where they are signed in, the apps
  with access, the services joined to the account, their details, and the deletion of the account.
  It is one page in place of Keycloak's account console.
- **The e-mails** (`src/email`): the confirmation of an address, a link for a new password, the
  steps of an account, the link of a service, and the notes about an account.

The pages are React. [Keycloakify](https://www.keycloakify.dev) packs them as one Keycloak theme.
They have the look and the motion of the apps (`clients/macos/DESIGN.md`): the warm ground, one
capsule that holds the step, the copper cursor of the mark, and the springs of the apps.

## The journey

The Kodosi app signs in with a code (OAuth 2.0 Device Authorization Grant). It shows a short code
and opens the browser. The pages then go this way:

1. The code page takes the code. When the app puts the code after the `#` of the address
   (`.../device#ABCD-EFGH`), the cells show it letter by letter and the page goes on by itself.
2. The sign-in, or a new account: the username, the details, the terms, then the confirmation of
   the e-mail and the first password. A first sign-in with a service asks for the username.
3. "Check the code": the code shows large, and the person connects the app only when the app shows
   the same code. A person who did not start a sign-in can stop here.
4. "You are in": the end of the journey in the browser.

A page shows the code only when that page sent it to Keycloak itself (`src/login/device.ts`). When
the app opens the address with the code in the query (`?user_code=`), Keycloak takes the code and
no page sees it. The pages then ask "Connect Kodosi?" with no code.

## How a page is made

- `index.html` draws the Kodosi mark, so the mark is there before the script loads and it has the
  same place on each page. Between two pages, it also draws the capsule and the cursor of the page
  before.
- `src/frame/` is what each page stands on: the capsule (`shell.tsx`), what one page hands to the
  next (`carry.ts`), the life of the cursor (`cursor.ts`), and the springs of the apps for the
  browser's own animations (`motion.ts`).
- `src/parts/` has the rows of a capsule, the code cells and tiles, the sliding pill, the notes,
  the marks of the sign-in services, and the check against programs (`captcha.tsx`).
- `src/styles.css` has the colours, the type, the depth and the motion of the apps.

### The sign-in pages

- `src/login/KcPage.tsx` gives each page of Keycloak its page here. A page of Keycloak that has no
  page here shows Keycloak's own fields in the same frame (`pages/other.tsx`).
- `src/login/stage.tsx` is the frame of a page, and `src/login/pages/` has the pages.
- `src/login/i18n.ts` has all the words of the pages, in English. Keycloakify reads them when it
  builds, so they are one literal there. In a different language, Keycloak's own words show. The
  words of the terms and of the deletion of an account are there too, each in one group.

Each form keeps the field names and the ids of Keycloak's own page, because Keycloak's scripts and
password managers find the fields by them. The words never say if an account exists: a wrong name
and a wrong password get the same sentence, and so does each forgotten password.

### The check against programs

The check is on the form that makes an account and on the form that sends a link for a new
password, never on the sign-in: a person with an account always reaches the sign-in. Keycloak does
the check. The page draws the widget of the service, and the widget asks the person only when it
must. When the check says no, its row says so and offers a new check. A realm with no check shows
nothing. The page reads these steps of Keycloak:

- Keycloak's own reCAPTCHA step.
- The Turnstile step of
  [keycloak-cloudflare-turnstile-provider](https://github.com/zymlabs/keycloak-cloudflare-turnstile-provider),
  with its custom-theme values on the registration form and its injection scripts on the reset
  form. The page draws the widget itself and does not load those scripts. Its pages before a form
  (`turnstile-form.ftl`, `turnstile-registration-form.ftl`) are pages of this theme too.
- The Turnstile step of [keycloak-turnstile](https://github.com/panpaul/keycloak-turnstile).

The widget comes from `https://challenges.cloudflare.com` or `https://www.google.com`, so the
realm's `frame-src` must allow that address.

### The account page

Keycloak has an account service, and its own account console is one program that uses it. The
account page here is a different program for the same service (`src/account/keycloak.ts`): it signs
in with Keycloak's own client for the account pages, and it shows what the service returns.

- `account.tsx` is the page, and `groups.tsx` has its groups. A group that the realm has no use for
  is not on the page.
- A change of a way of sign-in (a password, a passkey, an authenticator app, a removal), a link to
  a sign-in service and the deletion of the account are steps of the sign-in pages, where Keycloak
  asks for the person's proof. The page goes to that step and the person comes back.
- `words.ts` has the words of the page.

Keycloakify builds a one-page account theme only when the package of Keycloak's own account
console is installed, and it reads only its version. `tools/keycloak-account-ui` stands in for it.

### The e-mails

The e-mails are Keycloak's own kind of template (FreeMarker), with no script.

- `src/email/html/template.ftl` is the frame: one card on the ground, with the mark. An e-mail of
  Kodosi is the macro `message`: what it is, one sentence, one action, and a small note.
- The files `event-*.ftl` are the notes about an account. Keycloak sends them only when the realm
  has the event listener `email`, and the notes about a locked account only when the server's
  option `spi-events-listener--email--include-events` names them (see `keycloak.sh`).
- `src/email/text/` has the same e-mails as plain text, and `src/email/sentences.ftl` has the
  sentences that both forms share. `src/email/messages/` has the words: Keycloak reads each line as
  a message format, so an apostrophe is written two times.

## Work on the pages

The tools are [Bun](https://bun.sh) 1.4 and Docker. `bun install` installs the packages in
`node_modules/`.

- `bun run dev` shows each page with sample data at http://localhost:3184 (`KODOSI_SIGN_IN_PORT`
  moves it). `?scene=<name>` selects a page, and `?sheet=0,12` shows twelve pages side by side in
  the size of a phone. A send loads the next scene as a real page, so the capsule carries on as it
  does on Keycloak. The scenes `account`, `account-new` and `account-down` are the account page
  with a sample account service.
- `./theme.sh` builds the theme in `dist_keycloak/`.
- `./keycloak.sh` runs a disposable Keycloak in Docker with the theme, a realm with the settings
  of the hosted realm, and a mailbox that takes its e-mails. It prints their addresses.
  - `./keycloak.sh code` starts a sign-in as the app does, and prints the address with the code.
  - `./keycloak.sh check pass|block|ask|refuse|off` sets the check of the register and reset forms.
    The check is a stand-in (`dev/turnstile`) with the values of the Turnstile extension, and it
    uses Cloudflare's test keys only.
  - GitHub is a stand-in too: a second realm of the same Keycloak, so a first sign-in with a
    service goes on to the pages that follow it. Google and Apple have sample keys.
  - `dev/people.txt` has the sample people and their passwords.
  - `./keycloak.sh down` removes the containers.
- `just sign-in-check` (in the root of the repository) checks the types, the lint and the format,
  and builds the pages.

## Put the theme on a Keycloak

`dist_keycloak/keycloak-theme-for-kc-all-other-versions.jar` holds the theme for Keycloak 26. Put
it in Keycloak's `providers/` folder and start Keycloak again. Then set the realm:

- Login theme, account theme and email theme: `kodosi`.
- The client of the app: public, with the device grant, as `kodosi-app`.
- A new account: registration on, the e-mail confirmed (`verifyEmail`), and the step "Terms and
  conditions" in the registration form. The realm's localization key `termsText` holds the words
  or the links of its terms; the pages show them under the switch.
- A username that friends see: the realm's rule in its user profile (a `length` and a `pattern`
  with an `error-message`). The page shows the rule while the person types.
- A first sign-in with a service: "Review Profile" on (`update.profile.on.first.login`), so the
  person picks the username.
- The deletion of an account: the required action `delete_account` on, and the client role
  `account/delete-account` in the realm's default roles.
- A check against programs: its step on the registration form and on the reset form only, and the
  address of its frames in `frame-src` of the realm's Content-Security-Policy.

The theme replaces Keycloak's own pages. After an update of Keycloak, set its version in
`keycloak.sh` and go through the pages there.
