#!/usr/bin/env bash
# Builds the theme JAR for Keycloak in dist_keycloak/. Keycloakify packs it with Maven. A machine
# that has no Maven runs it in a container, so the build then needs only Docker.
set -euo pipefail
cd "$(dirname "$0")"
bun run build
command -v mvn >/dev/null || PATH="$PWD/tools:$PATH"
bunx keycloakify build
