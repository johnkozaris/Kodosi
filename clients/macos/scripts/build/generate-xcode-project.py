#!/usr/bin/env python3

import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def project_value(path: Path):
    result = subprocess.run(
        ["plutil", "-convert", "json", "-o", "-", str(path)],
        check=True,
        capture_output=True,
    )
    return json.loads(result.stdout)


def generate(root: Path) -> None:
    project = root / "KodosiDesktop.xcodeproj"
    with tempfile.TemporaryDirectory(prefix="kodosi-xcodegen-") as temporary:
        saved = Path(temporary) / project.name
        if project.exists():
            shutil.copytree(project, saved, symlinks=True)
        try:
            subprocess.run(["xcodegen", "generate"], cwd=root, check=True)
            generated = project / "project.pbxproj"
            if not generated.is_file():
                raise RuntimeError("XcodeGen did not produce project.pbxproj")
            generated_value = project_value(generated)
            previous = saved / "project.pbxproj"
            if previous.is_file() and project_value(previous) == generated_value:
                shutil.copy2(previous, generated)
            if saved.exists():
                for original in saved.rglob("*"):
                    current = project / original.relative_to(saved)
                    if (original.is_file() and not original.is_symlink()
                            and current.is_file() and not current.is_symlink()
                            and original.read_bytes() == current.read_bytes()):
                        shutil.copystat(original, current)
        except BaseException:
            if project.exists():
                shutil.rmtree(project)
            if saved.exists():
                shutil.copytree(saved, project, symlinks=True)
            raise


if __name__ == "__main__":
    try:
        generate(Path(sys.argv[1]).resolve())
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Xcode project generation failed: {error}") from error
