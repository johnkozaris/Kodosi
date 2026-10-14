#!/usr/bin/env python3

from __future__ import annotations

import argparse
import re
from pathlib import Path

OBJECT_ROW = re.compile(r"^\[\s*(\d+)\]\s+(.+)$")
SYMBOL_ROW = re.compile(r"^(\S+)\s+(\S+)\s+\[\s*(\d+)\]\s+(\S+)$")
PATH_ROW = re.compile(r"^# Path:\s+(.+)$")


def archive_path(object_path: str) -> str | None:
    marker = object_path.find(".a(")
    if marker < 0:
        return None
    return object_path[: marker + 2]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("link_map", type=Path)
    parser.add_argument("symbol_manifest", type=Path)
    parser.add_argument("expected_archive", type=Path)
    parser.add_argument("expected_binary", type=Path)
    args = parser.parse_args()

    authority_symbols = {
        line.strip()
        for line in args.symbol_manifest.read_text().splitlines()
        if line.strip()
    }
    if not authority_symbols:
        raise SystemExit("Ghostty strong-symbol manifest is empty")

    objects: dict[str, str] = {}
    linked_authority: list[tuple[str, str]] = []
    declared_binary: Path | None = None
    in_live_symbols = False
    for line in args.link_map.read_text(errors="surrogateescape").splitlines():
        path_match = PATH_ROW.match(line)
        if path_match:
            declared_binary = Path(path_match.group(1))
            continue
        if line == "# Symbols:":
            in_live_symbols = True
            continue
        if line == "# Dead Stripped Symbols:":
            in_live_symbols = False
            continue
        if not in_live_symbols:
            match = OBJECT_ROW.match(line)
            if match:
                objects[match.group(1)] = match.group(2)
            continue
        match = SYMBOL_ROW.match(line)
        if not match:
            continue
        size = int(match.group(2), 16)
        object_index = match.group(3)
        symbol = match.group(4)
        if size == 0 and object_index == "0":
            continue
        if symbol in authority_symbols or symbol.startswith("_ghostty_"):
            linked_authority.append((symbol, objects.get(object_index, "")))

    if declared_binary is None:
        raise SystemExit("Final link map does not declare its output path")
    if declared_binary.resolve() != args.expected_binary.resolve():
        raise SystemExit(
            f"Final link map describes {declared_binary}, not {args.expected_binary.resolve()}"
        )
    if not linked_authority:
        raise SystemExit("Final link map contains no live Ghostty authority symbols")

    expected = args.expected_archive.resolve()
    unexpected: list[tuple[str, str]] = []
    for symbol, object_path in linked_authority:
        contributor = archive_path(object_path)
        if contributor is None or Path(contributor).resolve() != expected:
            unexpected.append((symbol, object_path or "<missing object index>"))
    if unexpected:
        print("Final link contains Ghostty authority symbols from unexpected contributors:")
        for symbol, contributor in unexpected[:25]:
            print(f"{symbol}\t{contributor}")
        if len(unexpected) > 25:
            print(f"... and {len(unexpected) - 25} more")
        return 1

    print(
        f"Verified {len(linked_authority)} live Ghostty authority symbols "
        f"from {expected}."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
