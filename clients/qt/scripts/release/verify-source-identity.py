#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from pathlib import Path

from release_common import (
    canonical_json,
    source_identity,
    validate_source_identity,
)


def fail(message: str) -> None:
    raise SystemExit(f"source identity verification error: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--identity", required=True, type=Path)
    parser.add_argument("--expected", type=Path)
    parser.add_argument("--source-root", type=Path)
    parser.add_argument("--require-clean", action="store_true")
    arguments = parser.parse_args()
    try:
        payload = arguments.identity.read_bytes()
        value = json.loads(payload)
        identity = validate_source_identity(value)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        fail(str(error))
    if payload != canonical_json(identity):
        fail("identity is not canonical JSON")
    if arguments.expected is not None:
        try:
            expected = validate_source_identity(
                json.loads(arguments.expected.read_text(encoding="utf-8"))
            )
        except (OSError, ValueError, json.JSONDecodeError) as error:
            fail(f"expected identity is invalid: {error}")
        if identity != expected:
            fail("identity differs from the expected package build identity")
    if arguments.source_root is not None:
        try:
            measured = source_identity(arguments.source_root)
        except (OSError, ValueError) as error:
            fail(str(error))
        if identity != measured:
            fail("identity differs from the measured checkout")
    if arguments.require_clean and identity["dirty"]:
        fail("identity records dirty source")
    summary = (
        f"{identity['repository']}@{identity['commit']} "
        f"dirty={str(identity['dirty']).lower()}"
    )
    print(f"source identity verified: {summary}")


if __name__ == "__main__":
    main()
