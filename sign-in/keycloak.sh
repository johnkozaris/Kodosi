#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

rig=${KODOSI_SIGN_IN_RIG:-kodosi-signin}
name=$rig-keycloak
mailbox=$rig-mailbox
network=$rig
port=${KODOSI_SIGN_IN_KEYCLOAK_PORT:-8284}
mailbox_port=${KODOSI_SIGN_IN_MAILBOX_PORT:-8285}
maven=$(<tools/maven/image)
mailpit=axllent/mailpit:v1.34.0@sha256:23a2ef85bc50853ab3093645d1d64679afd8887e89da5e48a88940c172591a4e
image=quay.io/keycloak/keycloak:26.8.2@sha256:7e66a96e4739239070aeaf72e4c5decf0f7f682cfc725b9f7ea2b336eaba38b3
jar=dist_keycloak/keycloak-theme-for-kc-all-other-versions.jar
check_jar=dev/turnstile/build/kodosi-turnstile-stand-in.jar
code_jar=extension/target/kodosi-mail-code.jar
realm=kodosi
join='registration%20with%20check'
form='registration%20with%20check%20registration%20form'
reset='reset%20with%20check'
broker=kodosi-first-broker-login
people=dev/people.txt
# Keycloak e-mails the lockout notes only when this option names them, and then only what it
# names. UPDATE_PASSWORD or UPDATE_TOTP too would send two e-mails for one change.
notes=UPDATE_CREDENTIAL,REMOVE_CREDENTIAL,USER_DISABLED_BY_TEMPORARY_LOCKOUT,USER_DISABLED_BY_PERMANENT_LOCKOUT

kc() { docker exec "$name" /opt/keycloak/bin/kcadm.sh "$@"; }
calm() { "$@" 2> >(grep -v -e '^Created new' -e '^Logging into' >&2) >/dev/null; }
signin() {
  docker exec "$name" sh -c '/opt/keycloak/bin/kcadm.sh config credentials --server http://localhost:8080 \
    --realm master --user admin --password "$KC_BOOTSTRAP_ADMIN_PASSWORD"' 2>/dev/null
}
step_of() {
  kc get "authentication/flows/$1/executions" -r $realm --format csv --noquotes \
    --fields id,providerId | sed -n "s/,$2\$//p"
}
require() {
  local step
  step=$(kc get "authentication/flows/$1/executions" -r $realm --format csv --noquotes \
    --fields id,priority,providerId | sed -n "s/,$2\$//p")
  calm kc update "authentication/flows/$1/executions" -r $realm \
    -b "{\"id\":\"${step%,*}\",\"priority\":${step#*,},\"requirement\":\"$3\"}"
}
add_step() {
  calm kc create "authentication/flows/$1/executions/execution" -r $realm -s "provider=$2"
  calm kc update "authentication/flows/$1/executions" -r $realm \
    -b "{\"id\":\"$(step_of "$1" "$2")\",\"requirement\":\"$3\",\"priority\":$4}"
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
  local on=REQUIRED off=DISABLED
  [ "$1" = off ] && on=DISABLED off=REQUIRED
  require "$join" kodosi-turnstile-check $on
  require "$reset" kodosi-turnstile-reset $on
  require "$reset" reset-credentials-choose-user $off
  [ "$1" = off ] && return
  local flow_of one config body
  for flow_of in "$join kodosi-turnstile-check" "$reset kodosi-turnstile-reset"; do
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
  challenge=$(openssl rand 32 | openssl dgst -sha256 -binary | openssl base64 -A | tr '+/' '-_' | tr -d '=')
  answer=$(curl -fsS "http://localhost:$port/realms/$realm/protocol/openid-connect/auth/device" \
    -d client_id=kodosi-app -d 'scope=openid profile offline_access' \
    -d "code_challenge=$challenge" -d code_challenge_method=S256)
  code=$(printf '%s' "$answer" | sed -E 's/.*"user_code":"([^"]+)".*/\1/')
  address=$(printf '%s' "$answer" | sed -E 's/.*"verification_uri":"([^"]+)".*/\1/')
  echo "The code:            $code"
  echo "With the code shown: $address#$code"
  echo "Without the code:    $address?user_code=$code"
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

# Keycloakify finds Maven only on PATH, and by default it keeps Maven's downloads in its cache,
# which the container does not see.
[ -f "$jar" ] && [ -z "$(find src index.html -newer "$jar" -print -quit)" ] ||
  PATH="$PWD/tools/maven:$PATH" KEYCLOAKIFY_USE_DEFAULT_MAVEN_REPO=true bun run theme

[ -f "$code_jar" ] && [ -z "$(find extension/src extension/pom.xml -newer "$code_jar" -print -quit)" ] ||
  extension/maven.sh -q package

if [ ! -f "$check_jar" ] || [ -n "$(find dev/turnstile/StandIn.java dev/turnstile/META-INF -newer "$check_jar" -print -quit)" ]; then
  libraries=$(mktemp -d)
  docker create --name "$rig-libraries" "$image" >/dev/null
  docker cp "$rig-libraries:/opt/keycloak/lib/lib/main" "$libraries" >/dev/null
  docker rm "$rig-libraries" >/dev/null
  mkdir -p "$(dirname "$check_jar")"
  docker run --rm -v "$PWD/dev/turnstile:/src" -v "$libraries:/libraries:ro" \
    "$maven" sh -c 'set -e; mkdir -p /tmp/classes
      javac --release 21 -cp "/libraries/main/*" -d /tmp/classes /src/StandIn.java
      cp -r /src/META-INF /tmp/classes/
      jar cf /src/build/kodosi-turnstile-stand-in.jar -C /tmp/classes .'
  rm -rf "$libraries"
fi

admin=$(openssl rand -hex 16)
docker network create "$network" >/dev/null
docker run -d --name "$mailbox" --network "$network" -p "127.0.0.1:$mailbox_port:8025" \
  "$mailpit" >/dev/null
docker run -d --name "$name" --network "$network" -p "127.0.0.1:$port:8080" \
  -e KC_BOOTSTRAP_ADMIN_USERNAME=admin -e KC_BOOTSTRAP_ADMIN_PASSWORD="$admin" \
  -e KODOSI_TERMS_URL=https://kodosi.com/terms -e KODOSI_PRIVACY_URL=https://kodosi.com/privacy \
  -v "$PWD/$jar:/opt/keycloak/providers/kodosi-sign-in.jar:ro" \
  -v "$PWD/$code_jar:/opt/keycloak/providers/kodosi-mail-code.jar:ro" \
  -v "$PWD/$check_jar:/opt/keycloak/providers/kodosi-turnstile-stand-in.jar:ro" \
  "$image" start-dev "--spi-events-listener--email--include-events=$notes" >/dev/null

printf 'Keycloak starts'
until curl -fsS "http://127.0.0.1:$port/realms/master" >/dev/null 2>&1; do
  printf '.'
  sleep 2
done
echo
signin

calm kc create realms -s realm=$realm -s enabled=true -s displayName=Kodosi \
  -s internationalizationEnabled=true -s defaultLocale=en \
  -s 'supportedLocales=["en","de","el","es","fr","it","ja","ko","zh-CN"]' \
  -s loginTheme=kodosi -s accountTheme=kodosi -s emailTheme=kodosi \
  -s registrationAllowed=true -s registrationEmailAsUsername=false \
  -s loginWithEmailAllowed=true -s duplicateEmailsAllowed=false -s resetPasswordAllowed=true \
  -s editUsernameAllowed=false -s verifyEmail=true -s rememberMe=true \
  -s bruteForceProtected=true -s permanentLockout=false -s failureFactor=10 \
  -s waitIncrementSeconds=60 -s maxFailureWaitSeconds=300 -s maxDeltaTimeSeconds=43200 \
  -s quickLoginCheckMilliSeconds=1000 -s minimumQuickLoginWaitSeconds=60 \
  -s maxSecondaryAuthFailures=100 \
  -s 'passwordPolicy=length(12) and maxLength(128) and notUsername and notEmail' \
  -s oauth2DeviceCodeLifespan=900 -s actionTokenGeneratedByUserLifespan=900 \
  -s accessCodeLifespanLogin=3600 -s accessCodeLifespanUserAction=1800 \
  -s oauth2DevicePollingInterval=5 -s eventsEnabled=true \
  -s webAuthnPolicyRpEntityName=Kodosi -s webAuthnPolicyPasswordlessRpEntityName=Kodosi \
  -s webAuthnPolicyUserVerificationRequirement=required \
  -s webAuthnPolicyPasswordlessUserVerificationRequirement=required \
  -s webAuthnPolicyPasswordlessResidentKey=required \
  -s webAuthnPolicyPasswordlessPasskeysEnabled=true \
  -s 'eventsListeners=["jboss-logging","email"]' \
  -s "browserSecurityHeaders.contentSecurityPolicy=frame-src 'self' https://challenges.cloudflare.com; frame-ancestors 'self'; object-src 'none';" \
  -s "smtpServer={\"host\":\"$mailbox\",\"port\":\"1025\",\"from\":\"hello@kodosi.example\",\"fromDisplayName\":\"Kodosi\"}"

calm kc update users/profile -r $realm -b '{
  "attributes": [
    {"name": "username", "displayName": "${username}", "multivalued": false,
     "validations": {"length": {"min": 3, "max": 40},
       "pattern": {"pattern": "^[a-z0-9_-]+$",
         "error-message": "kdsUsernameRule"},
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

calm kc create clients -r $realm -s clientId=kodosi-app -s name=Kodosi -s publicClient=true \
  -s standardFlowEnabled=true -s directAccessGrantsEnabled=false -s fullScopeAllowed=false \
  -s 'redirectUris=["http://127.0.0.1/callback","com.kodosi:/auth"]' \
  -s 'attributes={"oauth2.device.authorization.grant.enabled":"true","use.refresh.tokens":"true","pkce.code.challenge.method":"S256"}' \
  -s 'defaultClientScopes=["basic","profile"]' -s 'optionalClientScopes=["offline_access"]' \
  -s 'protocolMappers=[{"name":"kodosi-app audience","protocol":"openid-connect","protocolMapper":"oidc-audience-mapper","config":{"included.client.audience":"kodosi-app","id.token.claim":"false","access.token.claim":"true","introspection.token.claim":"true"}}]'

backend_secret=$(openssl rand -hex 16)
calm kc create clients -r $realm -s clientId=kodosi-backend -s publicClient=false \
  -s serviceAccountsEnabled=true -s standardFlowEnabled=false -s directAccessGrantsEnabled=false \
  -s "secret=$backend_secret" -s 'optionalClientScopes=["kc-admin-api"]'
calm kc add-roles -r $realm --uusername service-account-kodosi-backend --cclientid realm-management \
  --rolename view-events

# A built-in flow takes no new step, so the realm uses copies. A copy loses the place of the terms
# step after the step that makes the account.
calm kc create authentication/flows/registration/copy -r $realm -b '{"newName":"registration with check"}'
calm kc update "realms/$realm" -s 'registrationFlow=registration with check'
calm kc update "authentication/flows/$form/executions" -r $realm \
  -b "{\"id\":\"$(step_of "$form" registration-terms-and-conditions)\",\"requirement\":\"REQUIRED\",\"priority\":70}"
add_step "$form" kodosi-confirmed-email REQUIRED 25
add_step "$join" kodosi-turnstile-check REQUIRED 1
add_step "$join" kodosi-mail-code REQUIRED 2
calm kc update authentication/required-actions/TERMS_AND_CONDITIONS -r $realm -s enabled=true
calm kc create authentication/register-required-action -r $realm \
  -s providerId=kodosi-mail-code -s 'name=Confirm the email with a code'
calm kc update authentication/required-actions/kodosi-mail-code -r $realm -s "priority=$(kc get \
  authentication/required-actions/VERIFY_EMAIL -r $realm --fields priority --format csv --noquotes)"
calm kc update authentication/required-actions/VERIFY_EMAIL -r $realm -s enabled=false
while IFS='|' read -r locale terms privacy; do
  calm kc create "localization/$locale" -r $realm \
    -s "termsText=<a href=\"https://kodosi.com/terms\">$terms</a> · <a href=\"https://kodosi.com/privacy\">$privacy</a>"
done <<'WORDS'
en|Terms|Privacy
de|Bedingungen|Datenschutz
el|Όροι|Απόρρητο
es|Términos|Privacidad
fr|Conditions|Confidentialité
it|Termini|Privacy
ja|利用規約|プライバシー
ko|이용약관|개인정보 보호
zh-CN|条款|隐私
WORDS

calm kc update authentication/required-actions/delete_account -r $realm -s enabled=true
calm kc add-roles -r $realm --rname "default-roles-$realm" --cclientid account --rolename delete-account

calm kc create 'authentication/flows/first%20broker%20login/copy' -r $realm -b "{\"newName\":\"$broker\"}"
review=$(kc get "authentication/flows/$broker/executions" -r $realm --format csv \
  --noquotes --fields providerId,authenticationConfig | sed -n 's/^idp-review-profile,//p')
calm kc update "authentication/config/$review" -r $realm \
  -b '{"alias":"review profile config","config":{"update.profile.on.first.login":"on"}}'
calm kc create "authentication/flows/$broker%20Handle%20Existing%20Account/executions/execution" \
  -r $realm -s provider=idp-confirm-override-link
calm kc update "authentication/flows/$broker/executions" -r $realm \
  -b "{\"id\":\"$(step_of $broker idp-confirm-override-link)\",\"requirement\":\"REQUIRED\",\"priority\":30}"
calm kc update "authentication/flows/$broker/executions" -r $realm \
  -b "{\"id\":\"$(step_of $broker idp-email-verification)\",\"requirement\":\"DISABLED\"}"

calm kc create authentication/flows -r $realm -s 'alias=reset with check' -s providerId=basic-flow \
  -s topLevel=true -s builtIn=false
add_step "$reset" kodosi-turnstile-reset REQUIRED 10
add_step "$reset" reset-credentials-choose-user DISABLED 20
add_step "$reset" kodosi-mail-code REQUIRED 30
add_step "$reset" reset-password REQUIRED 40
calm kc update "realms/$realm" -s 'resetCredentialsFlow=reset with check'
check pass

keys='"clientId":"sample","clientSecret":"sample"'
calm kc create identity-provider/instances -r $realm -s alias=google -s providerId=google \
  -s displayName=Google -s enabled=true -s firstBrokerLoginFlowAlias=$broker -s "config={$keys}"
calm kc create identity-provider/instances -r $realm -s alias=apple -s providerId=oidc \
  -s displayName=Apple -s enabled=true \
  -s "config={$keys,\"authorizationUrl\":\"https://appleid.apple.com/auth/authorize\",\"tokenUrl\":\"https://appleid.apple.com/auth/token\"}"

# The GitHub stand-in is the realm "elsewhere". The browser reaches it at localhost and Keycloak at
# its name in the Docker network; frontendUrl makes its tokens name localhost in both cases.
calm kc create realms -s realm=elsewhere -s enabled=true -s 'displayName=Stand-in for GitHub' \
  -s "attributes={\"frontendUrl\":\"http://localhost:$port\"}"
secret=$(openssl rand -hex 16)
calm kc create clients -r elsewhere -s clientId=kodosi -s publicClient=false -s fullScopeAllowed=false \
  -s "secret=$secret" -s "redirectUris=[\"http://localhost:$port/realms/$realm/broker/github/endpoint\"]"
outside="http://localhost:$port/realms/elsewhere/protocol/openid-connect"
inside="http://$name:8080/realms/elsewhere/protocol/openid-connect"
calm kc create identity-provider/instances -r $realm -s alias=github -s providerId=oidc \
  -s displayName=GitHub -s enabled=true -s trustEmail=false \
  -s firstBrokerLoginFlowAlias=$broker \
  -s "config={\"clientId\":\"kodosi\",\"clientSecret\":\"$secret\",\"clientAuthMethod\":\"client_secret_post\",\"authorizationUrl\":\"$outside/auth\",\"tokenUrl\":\"$inside/token\",\"userInfoUrl\":\"$inside/userinfo\",\"jwksUrl\":\"$inside/certs\",\"validateSignature\":\"false\",\"defaultScope\":\"openid profile email\",\"syncMode\":\"IMPORT\",\"pkceEnabled\":\"true\",\"pkceMethod\":\"S256\"}"
calm kc create identity-provider/instances/github/mappers -r $realm \
  -s 'name=kodosi user name' -s identityProviderAlias=github \
  -s identityProviderMapper=oidc-username-idp-mapper \
  -s 'config={"syncMode":"IMPORT","template":"${CLAIM.preferred_username | lowercase}","target":"LOCAL"}'

# The required actions go on after the password, because a new password removes UPDATE_PASSWORD.
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
echo "The backend's client for account deletions: kodosi-backend, secret $backend_secret"
