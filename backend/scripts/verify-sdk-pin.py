#!/usr/bin/env python3

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GLOBAL_JSON = "global.json"
DOCKERFILE = "Dockerfile"
SDK_IMAGE = re.compile(
    r"^FROM\s+(?:--platform=\S+\s+)?mcr\.microsoft\.com/dotnet/sdk:([^@\s]+)@sha256:[0-9a-f]{64}\s",
    re.MULTILINE,
)


def fail(message: str) -> None:
    raise SystemExit(f"[!] .NET SDK pin: {message}")


def main() -> None:
    if len(sys.argv) != 1:
        fail("this verifier accepts no arguments")
    try:
        sdk = json.loads((ROOT / GLOBAL_JSON).read_text(encoding="utf-8"))["sdk"]["version"]
        dockerfile = (ROOT / DOCKERFILE).read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, KeyError, TypeError) as error:
        fail(f"cannot read the SDK version: {error}")
    tags = SDK_IMAGE.findall(dockerfile)
    if tags != [sdk]:
        found = ", ".join(tags) or "no SDK image with a digest"
        fail(f"backend/{DOCKERFILE} uses {found}, but backend/{GLOBAL_JSON} names the SDK {sdk}")
    print(f"[*] backend/{DOCKERFILE} uses the .NET SDK {sdk} of backend/{GLOBAL_JSON}")


if __name__ == "__main__":
    main()
