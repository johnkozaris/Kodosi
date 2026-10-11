#!/usr/bin/env python3

from __future__ import annotations

import argparse
import os
from pathlib import Path

from release_common import canonical_json, source_identity


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()

    try:
        identity = source_identity(arguments.source_root)
    except (OSError, ValueError) as error:
        raise SystemExit(f"source identity error: {error}") from error
    output = arguments.output
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(f".{output.name}.part")
    try:
        with temporary.open("wb") as destination:
            destination.write(canonical_json(identity))
            destination.flush()
            os.fsync(destination.fileno())
        os.replace(temporary, output)
    finally:
        temporary.unlink(missing_ok=True)
    print(output)


if __name__ == "__main__":
    main()
