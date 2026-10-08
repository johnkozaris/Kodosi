#!/usr/bin/env python3

from __future__ import annotations

import argparse
from pathlib import Path

from release_common import checkout_identity, commit_epoch


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", required=True, type=Path)
    parser.add_argument("--require-clean", action="store_true")
    arguments = parser.parse_args()
    try:
        epoch = commit_epoch(arguments.source_root)
    except (OSError, ValueError) as error:
        raise SystemExit(f"SOURCE_DATE_EPOCH error: {error}") from error
    if arguments.require_clean and checkout_identity(arguments.source_root)["dirty"]:
        raise SystemExit("SOURCE_DATE_EPOCH error: client checkout is dirty")
    print(epoch)


if __name__ == "__main__":
    main()
