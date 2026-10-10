#!/usr/bin/env bash
# Runs a disposable Keycloak in Docker with the built theme and a sample realm, to use the pages
# on the real thing, and a mailbox that takes the e-mails that this Keycloak sends.
#
#   ./keycloak.sh        starts them and prints where to look
#   ./keycloak.sh code   starts a sign-in as the Kodosi app does, and prints the address that the
#                        app opens in the browser
#   ./keycloak.sh down   removes them
#
# The realm has what the hosted realm gets next: people make their own account, confirm their
# e-mail, choose a new password by e-mail, and see the buttons of GitHub, Google and Apple. The
# three services have sample keys, so their buttons show and their sign-in does not work.
set -euo pipefail
cd "$(dirname "$0")"

name=kodosi-signin-keycloak
mailbox=kodosi-signin-mailbox
network=kodosi-signin
port=${KODOSI_SIGN_IN_KEYCLOAK_PORT:-8284}
mailbox_port=${KODOSI_SIGN_IN_MAILBOX_PORT:-8285}
image=quay.io/keycloak/keycloak:26.7.5
jar=dist_keycloak/keycloak-theme-for-kc-all-other-versions.jar
realm=kodosi
# The sample people of dev/people.txt: only this machine reaches this Keycloak.
people=dev/people.txt
# The notes about an account that Keycloak sends by e-mail. It sends the two notes about a locked
# account only when this option names them, and then it sends only what the option names. For a
# new password or authenticator app, Keycloak has an older event and UPDATE_CREDENTIAL: with both
# names here, one change gives two e-mails.
notes=LOGIN_ERROR,UPDATE_CREDENTIAL,REMOVE_CREDENTIAL
notes=$notes,USER_DISABLED_BY_TEMPORARY_LOCKOUT,USER_DISABLED_BY_PERMANENT_LOCKOUT

if [ "${1:-}" = code ]; then
  # What the Kodosi app does first: it asks Keycloak for a code and shows the code. With the code
  # after the "#", the page shows it and sends it. With the code in the query, Keycloak takes it
  # and no page can show it: the pages then ask "Connect Kodosi?".
  answer=$(curl -fsS "http://localhost:$port/realms/$realm/protocol/openid-connect/auth/device" \
    -d client_id=kodosi-app -d 'scope=openid profile offline_access')
  code=$(printf '%s' "$answer" | sed -E 's/.*"user_code":"([^"]+)".*/\1/')
  address=$(printf '%s' "$answer" | sed -E 's/.*"verification_uri":"([^"]+)".*/\1/')
  echo "The code:            $code"
  echo "With the code shown: $address#$code"
  echo "As the app opens it: $address?user_code=$code"
  exit
fi

docker rm -f "$name" "$mailbox" >/dev/null 2>&1 || true
docker network rm "$network" >/dev/null 2>&1 || true
[ "${1:-}" = down ] && exit

# The theme is built again when a source file is newer than the built file.
[ -f "$jar" ] && [ -z "$(find src index.html -newer "$jar" -print -quit)" ] || ./theme.sh
admin=$(openssl rand -hex 16)
docker network create "$network" >/dev/null
docker run -d --name "$mailbox" --network "$network" -p "127.0.0.1:$mailbox_port:8025" \
  axllent/mailpit:v1.27 >/dev/null
docker run -d --name "$name" --network "$network" -p "127.0.0.1:$port:8080" \
  -e KC_BOOTSTRAP_ADMIN_USERNAME=admin -e KC_BOOTSTRAP_ADMIN_PASSWORD="$admin" \
  -v "$PWD/$jar:/opt/keycloak/providers/kodosi-sign-in.jar:ro" \
  "$image" start-dev "--spi-events-listener--email--include-events=$notes" >/dev/null

printf 'Keycloak starts'
until curl -fsS "http://127.0.0.1:$port/realms/master" >/dev/null 2>&1; do
  printf '.'
  sleep 2
done
echo

kc() { docker exec "$name" /opt/keycloak/bin/kcadm.sh "$@"; }
# kcadm says on its error stream where it signs in and what it made. This run shows only what
# goes wrong.
calm() { "$@" 2> >(grep -v -e '^Created new' -e '^Logging into' >&2) >/dev/null; }
calm kc config credentials --server http://localhost:8080 --realm master --user admin --password "$admin"

# The realm has the three kinds of pages of the theme. It sends its e-mails to the mailbox, and
# the notes about an account are among them.
calm kc create realms -s realm=$realm -s enabled=true -s displayName=Kodosi \
  -s loginTheme=kodosi -s accountTheme=kodosi -s emailTheme=kodosi \
  -s registrationAllowed=true -s verifyEmail=true -s resetPasswordAllowed=true \
  -s loginWithEmailAllowed=true -s rememberMe=true -s bruteForceProtected=true \
  -s webAuthnPolicyPasswordlessPasskeysEnabled=true \
  -s 'passwordPolicy=length(10) and notUsername' \
  -s 'eventsListeners=["jboss-logging","email"]' \
  -s "smtpServer={\"host\":\"$mailbox\",\"port\":\"1025\",\"from\":\"hello@kodosi.example\",\"fromDisplayName\":\"Kodosi\"}"

# The one client of Kodosi, as the hosted realm has it: the app asks for a code and a person
# confirms it in the browser.
calm kc create clients -r $realm -s clientId=kodosi-app -s name=Kodosi -s publicClient=true \
  -s standardFlowEnabled=false -s directAccessGrantsEnabled=false \
  -s 'attributes={"oauth2.device.authorization.grant.enabled":"true","use.refresh.tokens":"true"}' \
  -s 'defaultClientScopes=["basic","profile"]' -s 'optionalClientScopes=["offline_access"]'

# The buttons of the three services.
keys='"clientId":"sample","clientSecret":"sample"'
calm kc create identity-provider/instances -r $realm -s alias=github -s providerId=github \
  -s enabled=true -s "config={$keys}"
calm kc create identity-provider/instances -r $realm -s alias=google -s providerId=google \
  -s enabled=true -s "config={$keys}"
calm kc create identity-provider/instances -r $realm -s alias=apple -s providerId=oidc \
  -s displayName=Apple -s enabled=true \
  -s "config={$keys,\"authorizationUrl\":\"https://appleid.apple.com/auth/authorize\",\"tokenUrl\":\"https://appleid.apple.com/auth/token\"}"

# The steps "terms" and "delete my account" are off in a new realm. A realm writes its own terms:
# these are the terms of this sample.
calm kc update authentication/required-actions/TERMS_AND_CONDITIONS -r $realm -s enabled=true
calm kc update authentication/required-actions/delete_account -r $realm -s enabled=true
calm kc create localization/en -r $realm \
  -s 'termsText=<h3>Sample terms</h3><p>This Keycloak runs on your computer, for a look at the pages. It keeps the sample accounts until you remove it.</p><p>The hosted service has terms of its own.</p>'

# The check that a person makes the account, with Google reCAPTCHA: Keycloak has the step, and it
# is off. With the two keys of a reCAPTCHA site in the environment, this turns it on. The page
# then needs the frames of Google, so the realm allows them.
if [ -n "${KODOSI_SIGN_IN_RECAPTCHA_SITE_KEY:-}" ] && [ -n "${KODOSI_SIGN_IN_RECAPTCHA_SECRET:-}" ]; then
  step=$(kc get 'authentication/flows/registration form/executions' -r $realm \
    --format csv --noquotes --fields id,providerId | sed -n 's/,registration-recaptcha-action$//p')
  calm kc update 'authentication/flows/registration form/executions' -r $realm \
    -b "{\"id\":\"$step\",\"requirement\":\"REQUIRED\"}"
  calm kc create "authentication/executions/$step/config" -r $realm \
    -b "{\"alias\":\"person-check\",\"config\":{\"site.key\":\"$KODOSI_SIGN_IN_RECAPTCHA_SITE_KEY\",\"secret.key\":\"$KODOSI_SIGN_IN_RECAPTCHA_SECRET\"}}"
  calm kc update "realms/$realm" \
    -s "browserSecurityHeaders.contentSecurityPolicy=frame-src 'self' https://www.google.com; frame-ancestors 'self'; object-src 'none';"
fi

# Each line of the people file: username, e-mail, first name, last name, password, and the steps
# that Keycloak asks of the person at the next sign-in. The steps go on after the password,
# because a new password takes the step "new password" away.
while IFS='|' read -r username email first last password steps; do
  case "$username" in '' | '#'*) continue ;; esac
  id=$(kc create users -r $realm -s "username=$username" -s "email=$email" -s "firstName=$first" \
    -s "lastName=$last" -s enabled=true -s emailVerified=true -i)
  kc set-password -r $realm --username "$username" --new-password "$password"
  [ -z "$steps" ] || kc update "users/$id" -r $realm -s "requiredActions=[$steps]"
  # A person can delete their own account from the account page.
  kc add-roles -r $realm --uusername "$username" --cclientid account --rolename delete-account
done <"$people"

echo "The account page: http://localhost:$port/realms/$realm/account"
echo "A device sign-in: ./keycloak.sh code"
echo "The mailbox:      http://localhost:$mailbox_port"
echo "The people and their passwords: sign-in/$people"
