# Sign-in pages, account page and e-mails

Kodosi signs people in with Keycloak. This package is the theme that Keycloak shows them, with the
name `kodosi`. It has the three kinds of a Keycloak theme:

- **The sign-in pages:** the code of the Kodosi app, the sign-in, a new account, the
  confirmation of an e-mail, a new password, a first sign-in with GitHub, Google or Apple, passkeys,
  authenticator apps, the deletion of an account, and each message of Keycloak.
- **The account page:** how a person signs in, where they are signed in, the apps with access, the
  services joined to the account, their details, and the deletion of the account. It is one page in
  place of Keycloak's account console.
- **The e-mails:** the confirmation of an address, a link for a new password, the steps of an
  account, the link of a service, and the notes about an account.

The pages are React, packed as one Keycloak theme by [Keycloakify](https://www.keycloakify.dev).
They have the look and the motion of the apps ([DESIGN.md](../clients/macos/DESIGN.md)): the warm
ground, one capsule that holds the step, the copper cursor of the mark, and the springs of the apps.
The words are English; in a different language, Keycloak's own words show.

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

A page shows the code only when that page sent it to Keycloak itself. When the app opens the
address with the code in the query (`?user_code=`), Keycloak takes the code and no page sees it.
The pages then ask "Connect Kodosi?" with no code.

The words never say whether an account exists: a wrong name and a wrong password get the same
sentence, and so does each forgotten password.

## The check against programs

The check is on the form that makes an account and on the form that sends a link for a new
password, never on the sign-in: a person with an account always reaches the sign-in. Keycloak does
the check. The page draws the widget of the service, and the widget asks the person only when it
must. When the check says no, its row says so and offers a new check. A realm with no check shows
nothing. The pages support these Keycloak steps:

- Keycloak's own reCAPTCHA step.
- The Turnstile step of
  [keycloak-cloudflare-turnstile-provider](https://github.com/zymlabs/keycloak-cloudflare-turnstile-provider).
  The page draws the widget itself and does not load the extension's scripts.
- The Turnstile step of [keycloak-turnstile](https://github.com/panpaul/keycloak-turnstile).

The widget comes from `https://challenges.cloudflare.com` or `https://www.google.com`, so the
realm's `frame-src` must allow that address.

## The account page

The account page uses Keycloak's account service, as Keycloak's own account console does, and it
signs in with Keycloak's own client for the account pages. A group that the realm has no use for is
not on the page. A change of a way of sign-in (a password, a passkey, an authenticator app, a
removal), a link to a sign-in service and the deletion of the account are steps of the sign-in
pages, where Keycloak asks for the person's proof; the person then comes back to the page.

## The e-mails

Each e-mail is one card with the mark: what it is, one sentence, one action, and a small note, in
HTML and in plain text. Keycloak sends the notes about an account only when the realm has the event
listener `email`, and the notes about a locked account only when the server's option
`spi-events-listener--email--include-events` names them (see `keycloak.sh`).

## Work on the pages

The tools are [Bun](https://bun.sh) 1.4 and Docker. `bun install` installs the packages in
`node_modules/`.

- `bun run dev` shows each page with sample data at http://localhost:3184 (`KODOSI_SIGN_IN_PORT`
  moves it). `?scene=<name>` selects a page, and `?sheet=0,12` shows twelve pages side by side in
  the size of a phone. The scenes `account`, `account-new` and `account-down` are the account page
  with a sample account service.
- `bun run theme` builds the theme in `dist_keycloak/`. It needs Node.js, because Keycloakify runs
  Vite through `npx`, and Maven with a JDK, which packs the JAR.
- `./keycloak.sh` runs a disposable Keycloak in Docker with the theme, a realm with the settings
  of the hosted realm, and a mailbox that takes its e-mails. It prints their addresses.
  - `./keycloak.sh code` starts a sign-in as the app does, and prints the address with the code.
  - `./keycloak.sh check pass|block|ask|refuse|off` sets the check of the register and reset forms.
    The check is a stand-in with the values of the Turnstile extension, and it uses Cloudflare's
    test keys only.
  - GitHub is a stand-in too: a second realm of the same Keycloak, so a first sign-in with a
    service goes on to the pages that follow it. Google and Apple have sample keys.
  - `dev/people.txt` has the sample people and their passwords.
  - `./keycloak.sh down` removes the containers.
- `just sign-in-check` (in the root of the repository) checks the types, the lint and the format,
  and builds the pages.
- After an update of Keycloak, set its version in `keycloak.sh` and go through the pages there.

Each form keeps the field names and the ids of Keycloak's own page, because Keycloak's scripts and
password managers find the fields by them. In the words of the e-mails (`src/email/messages/`),
Keycloak reads each line as a message format, so an apostrophe is written two times. Keycloakify builds a one-page account theme only when the
package of Keycloak's own account console is installed, and it reads only its version:
`tools/keycloak-account-ui` stands in for it.
