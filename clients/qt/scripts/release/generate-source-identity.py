#!/usr/bin/env python3

from __future__ import annotations

import argparse
from pathlib import Path

from canonical_json import write_canonical_json
from release_source import source_identity


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
    write_canonical_json(output, identity)
    print(output)


if __name__ == "__main__":
    main()
