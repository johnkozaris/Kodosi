"""The checks of the evidence that each Kodosi package ships: the source
identity and the Rust license inventory. The Linux and the Windows package
checks use them."""

from __future__ import annotations

import json
from pathlib import Path

from release_common import canonical_json


def verify_source_identity(packaged: bytes, checkout: dict[str, object]) -> None:
    """The package must record the clean checkout that this check runs in."""
    if checkout["dirty"]:
        raise ValueError(
            "the checkout has changes. Commit all the changes, then build the package again"
        )
    if packaged == canonical_json(checkout):
        return
    try:
        recorded = json.loads(packaged)
    except ValueError:
        recorded = None
    if not isinstance(recorded, dict) or set(recorded) != set(checkout):
        raise ValueError("the source identity of the package is not a source identity record")
    differences = ", ".join(
        f"{key}={recorded[key]!r} (checkout: {value!r})"
        for key, value in checkout.items()
        if recorded[key] != value
    )
    raise ValueError(
        "the source identity of the package differs from the checkout: "
        + (differences or "the bytes are not canonical JSON")
    )


def verify_rust_licenses(packaged: dict[str, bytes], generated: Path) -> None:
    """The package must hold the Rust license inventory that the build generated."""
    expected = {
        path.relative_to(generated).as_posix(): path
        for path in generated.rglob("*")
        if path.is_file()
    }
    if not expected:
        raise ValueError(f"{generated} contains no Rust license inventory")
    missing = sorted(set(expected) - set(packaged))
    extra = sorted(set(packaged) - set(expected))
    if missing or extra:
        raise ValueError(
            f"the packaged Rust licenses differ from {generated}: "
            f"{len(missing)} missing {missing[:3]}, {len(extra)} extra {extra[:3]}"
        )
    for name, path in sorted(expected.items()):
        if packaged[name] != path.read_bytes():
            raise ValueError(f"the packaged Rust license file differs from {path}")
