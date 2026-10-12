#!/usr/bin/env python3

from __future__ import annotations

import argparse
from pathlib import Path

from release_common import verify_release_sources


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, default=Path(__file__).resolve().parents[2])
    arguments = parser.parse_args()
    try:
        source, ghostty = verify_release_sources(arguments.source_root, require_clean=True)
    except (OSError, ValueError) as error:
        raise SystemExit(f"release source verification error: {error}") from error
    print(
        f"Verified Kodosi {source['commit']}: "
        f"Ghostty {ghostty['linuxUpstreamCommit']} "
        f"archive={ghostty['linuxVtArchiveSha256']}"
    )


if __name__ == "__main__":
    main()
