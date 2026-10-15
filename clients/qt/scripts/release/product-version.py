#!/usr/bin/env python3

from __future__ import annotations

from pathlib import Path

from release_source import product_version


def main() -> None:
    try:
        print(product_version(Path(__file__).resolve().parents[2]))
    except (OSError, ValueError) as error:
        raise SystemExit(f"product version error: {error}") from error


if __name__ == "__main__":
    main()
