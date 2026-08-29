#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/rustProcess/src"
FAIL=0

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }

EXPECTED_ROOTS=(
    agent_intel
    discovery
    host_protocol
    identity_core
    local_sessions
    remote_sessions
    sharing
)

if [ ! -d "$SRC" ]; then
    red "FAIL: expected Rust source root is missing: $SRC"
    exit 1
fi
for sub in "${EXPECTED_ROOTS[@]}"; do
    if [ ! -d "$SRC/$sub" ]; then
        red "FAIL: expected architecture root is missing: rustProcess/src/$sub"
        FAIL=1
    fi
done


check() {
    local rule="$1" path="$2" pattern="$3"
    local hits
    hits=$(grep -rn -E "$pattern" "$SRC/$path" 2>/dev/null | grep -v '^[^:]*:[0-9]*://' || true)
    if [ -n "$hits" ]; then
        red "FAIL: $rule"
        echo "$hits" | sed 's|^|  |'
        FAIL=1
    else
        green "PASS: $rule"
    fi
}




check \
    "local_sessions/ must NOT import from remote_sessions/" \
    "local_sessions" \
    'use[[:space:]]+(crate::)?remote_sessions(::|;|$)'

check \
    "remote_sessions/ must NOT import from local_sessions/" \
    "remote_sessions" \
    'use[[:space:]]+(crate::)?local_sessions(::|;|$)'


check \
    "sharing/ must NOT import from remote_sessions/" \
    "sharing" \
    'use[[:space:]]+(crate::)?remote_sessions(::|;|$)'


check \
    "discovery/ must NOT import from local_sessions/" \
    "discovery" \
    'use[[:space:]]+(crate::)?local_sessions(::|;|$)'

check \
    "discovery/ must NOT import from remote_sessions/" \
    "discovery" \
    'use[[:space:]]+(crate::)?remote_sessions(::|;|$)'

check \
    "agent_intel/ must NOT import from remote_sessions/" \
    "agent_intel" \
    'use[[:space:]]+(crate::)?remote_sessions(::|;|$)'





REQUIRED_SESSION_DISPATCHES=(
    sessions_dispatch.rs
    sessions_focus_dispatch.rs
    sessions_terminal_dispatch.rs
)
for name in "${REQUIRED_SESSION_DISPATCHES[@]}"; do
    dispatch="$SRC/host_protocol/$name"
    if [ ! -f "$dispatch" ]; then
        red "FAIL: expected session dispatch authority is missing: rustProcess/src/host_protocol/$name"
        FAIL=1
        continue
    fi
    LOC=$(wc -l < "$dispatch" | tr -d ' ')
    relative="${dispatch#"$SRC/"}"
    if [ "$LOC" -gt 150 ]; then
        red "FAIL: $relative ($LOC LoC) exceeds 150-LoC ceiling — kind-aware logic is leaking in"
        FAIL=1
    else
        green "PASS: $relative is $LOC LoC (≤ 150)"
    fi
done




for sub in agent_intel discovery identity_core local_sessions remote_sessions sharing; do
    check \
        "$sub/ must NOT contain \`impl Runtime\` blocks (composition root only)" \
        "$sub" \
        '^impl[[:space:]]+Runtime[[:space:]]*\{'
done



for sub in agent_intel discovery identity_core local_sessions remote_sessions sharing; do
    check \
        "$sub/ must NOT take \`&[mut] Runtime\` parameters (use precise borrows or composition root)" \
        "$sub" \
        '\bapp:[[:space:]]*&(mut[[:space:]]+)?Runtime\b'
done



if [ "$FAIL" -ne 0 ]; then
    red ""
    exit 1
fi
green ""
green "All architecture gates passed."
