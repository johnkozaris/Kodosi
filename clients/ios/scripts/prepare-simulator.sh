#!/usr/bin/env bash
set -euo pipefail

name=${1:?Usage: prepare-simulator.sh <simulator name>}

id=$(xcrun simctl list devices available -j | python3 -c '
import json
import sys
name = sys.argv[1]
devices = json.load(sys.stdin)["devices"]
for runtime, entries in devices.items():
    if "iOS" not in runtime:
        continue
    for device in entries:
        if device["name"] == name and device["isAvailable"]:
            print(device["udid"])
            raise SystemExit
' "$name")

if [[ -z "$id" ]]; then
    runtime=$(xcrun simctl list runtimes available -j | python3 -c '
import json
import sys
runtimes = [entry for entry in json.load(sys.stdin)["runtimes"] if entry["platform"] == "iOS" and entry["isAvailable"]]
if not runtimes:
    raise SystemExit("No iOS simulator runtime is installed.")
runtimes.sort(key=lambda entry: [int(part) for part in entry["version"].split(".")])
print(runtimes[-1]["identifier"])
')
    id=$(xcrun simctl create "$name" com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro "$runtime")
fi

printf '%s\n' "$id"
