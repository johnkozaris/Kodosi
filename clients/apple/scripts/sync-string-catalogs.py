#!/usr/bin/env python3
"""Add the texts of the last Mac and iPhone builds to the string catalogs. A text that a build no
longer has stays: only a sync from the builds of both apps may remove it, with --remove-gone."""

import json
import subprocess
import sys
from pathlib import Path

CLIENTS = Path(__file__).resolve().parent.parent.parent
APPS = {
    "macos": ("KodosiDesktop.xcodeproj", "KodosiDesktop"),
    "ios": ("KodosiMobile.xcodeproj", "KodosiMobile"),
}


def intermediates(folder: str, project: str, scheme: str) -> Path | None:
    command = ["xcodebuild", "-project", project, "-scheme", scheme, "-showBuildSettings", "-json"]
    output = subprocess.run(command, cwd=CLIENTS / folder, capture_output=True, text=True, check=True)
    root = Path(json.loads(output.stdout)[0]["buildSettings"]["OBJROOT"])
    return root if root.is_dir() else None


def newest_texts(root: Path, project: str, target: str) -> list[Path]:
    """The extracted texts of the newest build of a target, so that an old configuration does not
    bring back texts that the code no longer has."""
    builds = [
        build
        for build in root.glob(f"{project}.build/*/{target}*.build")
        if build.name in (f"{target}.build", f"{target}-t.build")
    ]
    builds = [(max(path.stat().st_mtime for path in files), files) for build in builds if (files := list(build.rglob("*.stringsdata")))]
    return max(builds)[1] if builds else []


def main() -> int:
    remove_gone = "--remove-gone" in sys.argv[1:]
    roots = {folder: intermediates(folder, *app) for folder, app in APPS.items()}
    if remove_gone and None in roots.values():
        print("Build both apps before --remove-gone.", file=sys.stderr)
        return 1
    catalogs: dict[Path, list[Path]] = {}
    for folder, root in roots.items():
        if root is None:
            continue
        catalog = CLIENTS / folder / "Resources/Localizable.xcstrings"
        catalogs.setdefault(catalog, []).extend(newest_texts(root, APPS[folder][1], APPS[folder][1]))
        for module in (CLIENTS / "apple/Sources").glob("*/Resources/Localizable.xcstrings"):
            texts = newest_texts(root, "KodosiApple", module.parent.parent.name)
            catalogs.setdefault(module, []).extend(texts)
    if not catalogs:
        print("Build the Mac or the iPhone app first.", file=sys.stderr)
        return 1
    for catalog, texts in sorted(catalogs.items()):
        if not texts:
            continue
        command = ["xcrun", "xcstringstool", "sync", str(catalog), "--stringsdata", *map(str, texts)]
        subprocess.run(command if remove_gone else [*command, "--skip-marking-strings-stale"], check=True)
        print(f"{catalog.relative_to(CLIENTS.parent)}: {len(texts)} files of texts")
    return 0


if __name__ == "__main__":
    sys.exit(main())
