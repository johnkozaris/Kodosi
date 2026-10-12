#!/usr/bin/env python3

from __future__ import annotations

import argparse
from pathlib import Path

from release_common import commit_epoch


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", required=True, type=Path)
    arguments = parser.parse_args()
    try:
        epoch = commit_epoch(arguments.source_root)
    except (OSError, ValueError) as error:
        raise SystemExit(f"SOURCE_DATE_EPOCH error: {error}") from error
    print(epoch)


if __name__ == "__main__":
    main()
