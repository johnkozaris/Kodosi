#!/usr/bin/env bash
# Runs a disposable Keycloak in Docker with the built theme and a sample realm, to use the pages
# on the real thing, and a mailbox that takes the e-mails that this Keycloak sends.
#
#   ./keycloak.sh              starts them and prints where to look
#   ./keycloak.sh code         starts a sign-in as the Kodosi app does, and prints the addresses
#                              that the app can open in the browser
#   ./keycloak.sh check MODE   sets the person check of the register and reset forms:
#                              pass, block, ask (a challenge for the person), refuse (the widget
#                              passes, the server refuses) or off
#   ./keycloak.sh down         removes them
#
# The realm has the settings of the hosted realm: people make their own account and confirm their
# e-mail before they choose a password, they choose a new password by e-mail, they sign in with a
# passkey, and they see the buttons of GitHub, Google and Apple. Google and Apple have sample keys:
# their buttons show and their sign-in does not work. GitHub here is a stand-in: a second realm of
# this Keycloak ("elsewhere"), so a first sign-in with a service goes on to the pages that follow
# it. The person check is a stand-in too (dev/turnstile): it uses Cloudflare's test keys only.
set -euo pipefail
cd "$(dirname "$0")"

name=kodosi-signin-keycloak
mailbox=kodosi-signin-mailbox
network=kodosi-signin
port=${KODOSI_SIGN_IN_KEYCLOAK_PORT:-8284}
mailbox_port=${KODOSI_SIGN_IN_MAILBOX_PORT:-8285}
image=quay.io/keycloak/keycloak:26.7.5
jar=dist_keycloak/keycloak-theme-for-kc-all-other-versions.jar
check_jar=dev/turnstile/build/kodosi-turnstile-stand-in.jar
realm=kodosi
# The flows of the realm that have the person check: Keycloak's own flows take no new step.
form='registration%20with%20check%20registration%20form'
reset='reset%20with%20check'
# The sample people of dev/people.txt: only this machine reaches this Keycloak.
people=dev/people.txt
# The notes about an account that Keycloak sends by e-mail. It sends the two notes about a locked
# account only when this option names them, and then it sends only what the option names. For a
# new password or authenticator app, Keycloak has an older event and UPDATE_CREDENTIAL: with both
# names here, one change gives two e-mails.
notes=LOGIN_ERROR,UPDATE_CREDENTIAL,REMOVE_CREDENTIAL
notes=$notes,USER_DISABLED_BY_TEMPORARY_LOCKOUT,USER_DISABLED_BY_PERMANENT_LOCKOUT

kc() { docker exec "$name" /opt/keycloak/bin/kcadm.sh "$@"; }
# kcadm says on its error stream where it signs in and what it made. This run shows only what
# goes wrong.
calm() { "$@" 2> >(grep -v -e '^Created new' -e '^Logging into' >&2) >/dev/null; }
signin() {
  # The admin password is in the environment of the container, and nowhere else.
  docker exec "$name" sh -c '/opt/keycloak/bin/kcadm.sh config credentials --server http://localhost:8080 \
    --realm master --user admin --password "$KC_BOOTSTRAP_ADMIN_PASSWORD"' 2>/dev/null
}
# The id of the step of a flow that a provider makes.
step_of() {
  kc get "authentication/flows/$1/executions" -r $realm --format csv --noquotes \
    --fields id,providerId | sed -n "s/,$2\$//p"
}

# Cloudflare's published test keys (developers.cloudflare.com/turnstile/troubleshooting/testing).
check() {
  local site secret
  case "$1" in
    pass) site=1x00000000000000000000AA secret=1x0000000000000000000000000000000AA ;;
    block) site=2x00000000000000000000AB secret=1x0000000000000000000000000000000AA ;;
    ask) site=3x00000000000000000000FF secret=1x0000000000000000000000000000000AA ;;
    refuse) site=1x00000000000000000000AA secret=2x0000000000000000000000000000000AA ;;
    off) ;;
    *) echo "check: pass, block, ask, refuse or off" >&2 && exit 2 ;;
  esac
  local requirement=REQUIRED flow="reset with check"
  [ "$1" = off ] && requirement=DISABLED flow="reset credentials"
  calm kc update "authentication/flows/$form/executions" -r $realm \
    -b "{\"id\":\"$(step_of "$form" kodosi-turnstile-registration)\",\"requirement\":\"$requirement\",\"priority\":65}"
  calm kc update "realms/$realm" -s "resetCredentialsFlow=$flow"
  [ "$1" = off ] && return
  local flow_of one config body
  for flow_of in "$form kodosi-turnstile-registration" "$reset kodosi-turnstile-reset"; do
    set -- $flow_of
    one=$(step_of "$1" "$2")
    config=$(kc get "authentication/flows/$1/executions" -r $realm --format csv --noquotes \
      --fields id,authenticationConfig | sed -n "s/^$one,//p")
    body="{\"alias\":\"check-$2\",\"config\":{\"site.key\":\"$site\",\"secret.key\":\"$secret\"}}"
    if [ -n "$config" ]; then
      calm kc update "authentication/config/$config" -r $realm -b "$body"
    else
      calm kc create "authentication/executions/$one/config" -r $realm -b "$body"
    fi
  done
}

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

if [ "${1:-}" = check ]; then
  signin
  check "${2:-}"
  echo "The person check is ${2}."
  exit
fi

docker rm -f "$name" "$mailbox" >/dev/null 2>&1 || true
docker network rm "$network" >/dev/null 2>&1 || true
[ "${1:-}" = down ] && exit

# The theme is built again when a source file is newer than the built file.
[ -f "$jar" ] && [ -z "$(find src index.html -newer "$jar" -print -quit)" ] || ./theme.sh

# The stand-in of the person check, compiled against the libraries of this Keycloak.
if [ ! -f "$check_jar" ] || [ dev/turnstile/StandIn.java -nt "$check_jar" ]; then
  libraries=$(mktemp -d)
  docker create --name kodosi-signin-libraries "$image" >/dev/null
  docker cp kodosi-signin-libraries:/opt/keycloak/lib/lib/main "$libraries" >/dev/null
  docker rm kodosi-signin-libraries >/dev/null
  mkdir -p "$(dirname "$check_jar")"
  docker run --rm -v "$PWD/dev/turnstile:/src" -v "$libraries:/libraries:ro" \
    maven:3.9-eclipse-temurin-21 sh -c 'set -e; mkdir -p /tmp/classes
      javac --release 21 -cp "/libraries/main/*" -d /tmp/classes /src/StandIn.java
      cp -r /src/META-INF /tmp/classes/
      jar cf /src/build/kodosi-turnstile-stand-in.jar -C /tmp/classes .'
  rm -rf "$libraries"
fi

admin=$(openssl rand -hex 16)
docker network create "$network" >/dev/null
docker run -d --name "$mailbox" --network "$network" -p "127.0.0.1:$mailbox_port:8025" \
  axllent/mailpit:v1.27 >/dev/null
docker run -d --name "$name" --network "$network" -p "127.0.0.1:$port:8080" \
  -e KC_BOOTSTRAP_ADMIN_USERNAME=admin -e KC_BOOTSTRAP_ADMIN_PASSWORD="$admin" \
  -v "$PWD/$jar:/opt/keycloak/providers/kodosi-sign-in.jar:ro" \
  -v "$PWD/$check_jar:/opt/keycloak/providers/kodosi-turnstile-stand-in.jar:ro" \
  "$image" start-dev "--spi-events-listener--email--include-events=$notes" >/dev/null

printf 'Keycloak starts'
until curl -fsS "http://127.0.0.1:$port/realms/master" >/dev/null 2>&1; do
  printf '.'
  sleep 2
done
echo
signin

# The realm, with the settings that the hosted realm has (ops/provision.py of its service). Its
# e-mails go to the mailbox, and the notes about an account are among them. The frames of the
# person check come from Cloudflare.
calm kc create realms -s realm=$realm -s enabled=true -s displayName=Kodosi \
  -s loginTheme=kodosi -s accountTheme=kodosi -s emailTheme=kodosi \
  -s registrationAllowed=true -s registrationEmailAsUsername=false \
  -s loginWithEmailAllowed=true -s duplicateEmailsAllowed=false -s resetPasswordAllowed=true \
  -s editUsernameAllowed=false -s verifyEmail=true -s rememberMe=true \
  -s bruteForceProtected=true -s permanentLockout=false -s failureFactor=10 \
  -s waitIncrementSeconds=60 -s maxFailureWaitSeconds=300 -s maxDeltaTimeSeconds=43200 \
  -s quickLoginCheckMilliSeconds=1000 -s minimumQuickLoginWaitSeconds=60 \
  -s 'passwordPolicy=length(12) and maxLength(128) and notUsername and notEmail' \
  -s oauth2DeviceCodeLifespan=900 -s actionTokenGeneratedByUserLifespan=900 \
  -s oauth2DevicePollingInterval=5 -s eventsEnabled=true \
  -s webAuthnPolicyRpEntityName=Kodosi -s webAuthnPolicyPasswordlessRpEntityName=Kodosi \
  -s webAuthnPolicyUserVerificationRequirement=required \
  -s webAuthnPolicyPasswordlessUserVerificationRequirement=required \
  -s webAuthnPolicyPasswordlessResidentKey=required \
  -s webAuthnPolicyPasswordlessPasskeysEnabled=true \
  -s 'eventsListeners=["jboss-logging","email"]' \
  -s "browserSecurityHeaders.contentSecurityPolicy=frame-src 'self' https://challenges.cloudflare.com; frame-ancestors 'self'; object-src 'none';" \
  -s "smtpServer={\"host\":\"$mailbox\",\"port\":\"1025\",\"from\":\"hello@kodosi.example\",\"fromDisplayName\":\"Kodosi\"}"

# A username is public, friends find each other by it, and it does not change. A first and a last
# name are optional.
calm kc update users/profile -r $realm -b '{
  "attributes": [
    {"name": "username", "displayName": "${username}", "multivalued": false,
     "validations": {"length": {"min": 3, "max": 40},
       "pattern": {"pattern": "^[a-z0-9_-]+$",
         "error-message": "Use 3 to 40 lowercase letters, digits, hyphens or underscores."},
       "up-username-not-idn-homograph": {}},
     "permissions": {"view": ["admin", "user"], "edit": ["admin", "user"]}},
    {"name": "email", "displayName": "${email}", "multivalued": false,
     "validations": {"email": {}, "length": {"max": 255}}, "required": {"roles": ["user"]},
     "permissions": {"view": ["admin", "user"], "edit": ["admin", "user"]}},
    {"name": "firstName", "displayName": "${firstName}", "multivalued": false,
     "validations": {"length": {"max": 255}, "person-name-prohibited-characters": {}},
     "permissions": {"view": ["admin", "user"], "edit": ["admin", "user"]}},
    {"name": "lastName", "displayName": "${lastName}", "multivalued": false,
     "validations": {"length": {"max": 255}, "person-name-prohibited-characters": {}},
     "permissions": {"view": ["admin", "user"], "edit": ["admin", "user"]}}
  ],
  "groups": [{"name": "user-metadata", "displayHeader": "User metadata",
    "displayDescription": "Attributes, which refer to user metadata"}]
}'

# The one client of Kodosi, as the hosted realm has it: the app asks for a code and a person
# confirms it in the browser.
calm kc create clients -r $realm -s clientId=kodosi-app -s name=Kodosi -s publicClient=true \
  -s standardFlowEnabled=false -s directAccessGrantsEnabled=false \
  -s 'attributes={"oauth2.device.authorization.grant.enabled":"true","use.refresh.tokens":"true"}' \
  -s 'defaultClientScopes=["basic","profile"]' -s 'optionalClientScopes=["offline_access"]'

# The registration of the realm is a copy of Keycloak's own, so it can take the person check. It
# asks the person to accept the terms. The terms step keeps its place after the step that makes
# the account, which a copy loses. The realm writes the words of its terms: here, the two
# addresses where the hosted service will have its own.
calm kc create authentication/flows/registration/copy -r $realm -b '{"newName":"registration with check"}'
calm kc update "realms/$realm" -s 'registrationFlow=registration with check'
calm kc update "authentication/flows/$form/executions" -r $realm \
  -b "{\"id\":\"$(step_of "$form" registration-terms-and-conditions)\",\"requirement\":\"REQUIRED\",\"priority\":70}"
calm kc update authentication/required-actions/TERMS_AND_CONDITIONS -r $realm -s enabled=true
calm kc create localization/en -r $realm \
  -s 'termsText=<a href="https://kodosi.com/terms">Terms</a> · <a href="https://kodosi.com/privacy">Privacy</a>'

# A person can delete their own account: the step is on, and each person has the role for it.
calm kc update authentication/required-actions/delete_account -r $realm -s enabled=true
calm kc add-roles -r $realm --rname "default-roles-$realm" --cclientid account --rolename delete-account

# A first sign-in with a service shows the profile, so the person picks their username.
review=$(kc get 'authentication/flows/first%20broker%20login/executions' -r $realm --format csv \
  --noquotes --fields providerId,authenticationConfig | sed -n 's/^idp-review-profile,//p')
calm kc update "authentication/config/$review" -r $realm \
  -b '{"alias":"review profile config","config":{"update.profile.on.first.login":"on"}}'

# The person check of the registration form, and of the form for a forgotten password: the stand-in
# takes the place of Keycloak's step that finds the person.
calm kc create "authentication/flows/$form/executions/execution" -r $realm \
  -s provider=kodosi-turnstile-registration
calm kc create authentication/flows -r $realm -s 'alias=reset with check' -s providerId=basic-flow \
  -s topLevel=true -s builtIn=false
priority=0
for provider in kodosi-turnstile-reset reset-credential-email reset-password; do
  priority=$((priority + 10))
  calm kc create "authentication/flows/$reset/executions/execution" -r $realm -s "provider=$provider"
  calm kc update "authentication/flows/$reset/executions" -r $realm \
    -b "{\"id\":\"$(step_of "$reset" $provider)\",\"requirement\":\"REQUIRED\",\"priority\":$priority}"
done
check "${KODOSI_SIGN_IN_CHECK:-pass}"

# The buttons of the three services. Google and Apple have sample keys.
keys='"clientId":"sample","clientSecret":"sample"'
calm kc create identity-provider/instances -r $realm -s alias=google -s providerId=google \
  -s displayName=Google -s enabled=true -s "config={$keys}"
calm kc create identity-provider/instances -r $realm -s alias=apple -s providerId=oidc \
  -s displayName=Apple -s enabled=true \
  -s "config={$keys,\"authorizationUrl\":\"https://appleid.apple.com/auth/authorize\",\"tokenUrl\":\"https://appleid.apple.com/auth/token\"}"

# The stand-in of GitHub: the realm "elsewhere" of this Keycloak, with Keycloak's own pages. Its
# sign-in is the sign-in of a service. The browser reaches it at localhost, and this Keycloak at its
# name in the network of the two containers. Its tokens name localhost in both cases.
calm kc create realms -s realm=elsewhere -s enabled=true -s 'displayName=Stand-in for GitHub' \
  -s "attributes={\"frontendUrl\":\"http://localhost:$port\"}"
secret=$(openssl rand -hex 16)
calm kc create clients -r elsewhere -s clientId=kodosi -s publicClient=false \
  -s "secret=$secret" -s "redirectUris=[\"http://localhost:$port/realms/$realm/broker/github/endpoint\"]"
outside="http://localhost:$port/realms/elsewhere/protocol/openid-connect"
inside="http://$name:8080/realms/elsewhere/protocol/openid-connect"
calm kc create identity-provider/instances -r $realm -s alias=github -s providerId=oidc \
  -s displayName=GitHub -s enabled=true -s trustEmail=false \
  -s 'firstBrokerLoginFlowAlias=first broker login' \
  -s "config={\"clientId\":\"kodosi\",\"clientSecret\":\"$secret\",\"clientAuthMethod\":\"client_secret_post\",\"authorizationUrl\":\"$outside/auth\",\"tokenUrl\":\"$inside/token\",\"userInfoUrl\":\"$inside/userinfo\",\"jwksUrl\":\"$inside/certs\",\"validateSignature\":\"false\",\"defaultScope\":\"openid profile email\",\"syncMode\":\"IMPORT\",\"pkceEnabled\":\"true\",\"pkceMethod\":\"S256\"}"
calm kc create identity-provider/instances/github/mappers -r $realm \
  -s 'name=kodosi user name' -s identityProviderAlias=github \
  -s identityProviderMapper=oidc-username-idp-mapper \
  -s 'config={"syncMode":"IMPORT","template":"${CLAIM.preferred_username | lowercase}","target":"LOCAL"}'

# Each line of the people file: realm, username, e-mail, first name, last name, password, and the
# steps that Keycloak asks of the person at the next sign-in. The steps go on after the password,
# because a new password takes the step "new password" away.
while IFS='|' read -r where username email first last password steps; do
  case "$where" in '' | '#'*) continue ;; esac
  id=$(kc create users -r "$where" -s "username=$username" -s "email=$email" \
    -s "firstName=$first" -s "lastName=$last" -s enabled=true -s emailVerified=true -i)
  kc set-password -r "$where" --username "$username" --new-password "$password"
  [ -z "$steps" ] || kc update "users/$id" -r "$where" -s "requiredActions=[$steps]"
done <"$people"

echo "The account page: http://localhost:$port/realms/$realm/account"
echo "A device sign-in: ./keycloak.sh code"
echo "The mailbox:      http://localhost:$mailbox_port"
echo "The people and their passwords: sign-in/$people"
