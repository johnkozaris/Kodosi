from __future__ import annotations

import json
import os
from pathlib import Path


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(
            value,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n"
    ).encode("utf-8")


def write_canonical_json(output: Path, value: object) -> None:
    temporary = output.with_name(f".{output.name}.part")
    try:
        with temporary.open("wb") as destination:
            destination.write(canonical_json(value))
            destination.flush()
            os.fsync(destination.fileno())
        os.replace(temporary, output)
    finally:
        temporary.unlink(missing_ok=True)
