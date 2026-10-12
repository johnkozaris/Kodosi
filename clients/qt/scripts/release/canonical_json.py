"""The canonical JSON encoding of the release records: sorted keys, no spaces,
UTF-8 text and a final line feed. Equal records give equal bytes."""

from __future__ import annotations

import json


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
