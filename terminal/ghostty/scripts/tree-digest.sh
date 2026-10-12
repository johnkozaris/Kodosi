#!/bin/bash

set -euo pipefail

ROOT=${1:-}
if [ -z "$ROOT" ] || [ ! -d "$ROOT" ]; then
    echo "Usage: $0 <tree_root>" >&2
    exit 1
fi

python3 - "$ROOT" <<'PY'
import hashlib
from pathlib import Path
import sys

root = Path(sys.argv[1])
digest = hashlib.sha256()
for path in sorted(item for item in root.rglob("*") if item.is_file()):
    relative = path.relative_to(root).as_posix().encode()
    digest.update(len(relative).to_bytes(8, "big"))
    digest.update(relative)
    data = path.read_bytes()
    digest.update(len(data).to_bytes(8, "big"))
    digest.update(data)
print(digest.hexdigest())
PY
