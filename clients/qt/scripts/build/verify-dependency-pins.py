#!/usr/bin/env python3

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEPENDENCIES = "dependencies.lock.json"
PRESETS = "CMakePresets.json"
QT_TEXTS = (
    "packaging/linux/NATIVE-DESKTOP-INTEGRATION-NOTICE.txt",
    "packaging/licenses/icu/73.2/README.txt",
)


def fail(message: str) -> None:
    raise SystemExit(f"[!] dependency pins: {message}")


def load_json(relative: str) -> dict:
    try:
        value = json.loads((ROOT / relative).read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"cannot read {relative}: {error}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain a JSON object")
    return value


def main() -> None:
    if len(sys.argv) != 1:
        fail("this verifier accepts no arguments")
    dependencies = load_json(DEPENDENCIES)
    try:
        cmake_version = dependencies["tools"]["cmake"]["version"]
        qt_version = dependencies["frameworks"]["qt"]["version"]
    except (KeyError, TypeError) as error:
        fail(f"{DEPENDENCIES} has no CMake or Qt version: {error}")

    minimum = load_json(PRESETS).get("cmakeMinimumRequired")
    if not isinstance(minimum, dict) or set(minimum) != {"major", "minor", "patch"}:
        fail(f"{PRESETS} must give the major, minor and patch CMake version")
    presets_version = f"{minimum['major']}.{minimum['minor']}.{minimum['patch']}"
    if presets_version != cmake_version:
        fail(f"{PRESETS} requires CMake {presets_version}, but {DEPENDENCIES} pins {cmake_version}")

    for relative in QT_TEXTS:
        try:
            text = (ROOT / relative).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as error:
            fail(f"cannot read {relative}: {error}")
        named = set(re.findall(r"\bQt ([0-9]+\.[0-9]+\.[0-9]+)\b", text))
        if named != {qt_version}:
            found = ", ".join(sorted(named)) or "no version"
            fail(f"{relative} names Qt {found}, but {DEPENDENCIES} pins {qt_version}")

    print(f"[*] the copies of CMake {cmake_version} and Qt {qt_version} agree with {DEPENDENCIES}")


if __name__ == "__main__":
    main()
