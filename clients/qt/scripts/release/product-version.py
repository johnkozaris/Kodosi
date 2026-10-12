#!/usr/bin/env python3

from __future__ import annotations

import argparse
from pathlib import Path

from release_common import product_version


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--source-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
    )
    arguments = parser.parse_args()
    try:
        print(product_version(arguments.source_root))
    except (OSError, ValueError) as error:
        raise SystemExit(f"product version error: {error}") from error


if __name__ == "__main__":
    main()
