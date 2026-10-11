from __future__ import annotations

import json
import shutil
import subprocess
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


ROOT = Path(__file__).resolve().parents[2]
VERIFIER = ROOT / "scripts/release/verify-native-license-evidence.py"
MANIFEST = ROOT / "packaging/licenses/native-license-evidence.json"


class NativeLicenseEvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        (ROOT / "build").mkdir(exist_ok=True)
        self.temporary = TemporaryDirectory(
            prefix="native-license-evidence-",
            dir=ROOT / "build",
        )
        self.root = Path(self.temporary.name)
        self.licenses = self.root / "licenses"
        shutil.copytree(ROOT / "packaging/licenses", self.licenses)
        self.qt_root = self.root / "qt"
        (self.qt_root / "sbom").mkdir(parents=True)
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        for row in manifest["qt"]["sboms"]:
            shutil.copyfile(
                ROOT / ".tools/Qt/6.11.2/gcc_64/sbom" / row["filename"],
                self.qt_root / "sbom" / row["filename"],
            )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def verify(self, expect_success: bool = True) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            [
                "python3",
                str(VERIFIER),
                "--manifest",
                str(self.licenses / "native-license-evidence.json"),
                "--license-root",
                str(self.licenses),
                "--qt-root",
                str(self.qt_root),
                "--dependencies",
                str(ROOT / "dependencies.lock.json"),
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if expect_success and result.returncode != 0:
            self.fail(result.stderr or result.stdout)
        if not expect_success and result.returncode == 0:
            self.fail("native evidence verification unexpectedly succeeded")
        return result

    def test_exact_evidence_is_deterministically_accepted(self) -> None:
        first = self.verify().stdout
        second = self.verify().stdout
        self.assertEqual(first, second)

    def test_license_sbom_and_source_metadata_tamper_are_rejected(self) -> None:
        license_path = self.licenses / "qt/6.11.2/LICENSES/LGPL-3.0-only.txt"
        original = license_path.read_bytes()
        license_path.write_bytes(original + b"\n")
        self.verify(expect_success=False)
        license_path.write_bytes(original)

        sbom = self.qt_root / "sbom/qtbase-6.11.2.spdx"
        original = sbom.read_bytes()
        sbom.write_bytes(original + b"\n")
        self.verify(expect_success=False)
        sbom.write_bytes(original)

        manifest_path = self.licenses / "native-license-evidence.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["sourceArchives"][0]["ref"] = "rolling"
        manifest_path.write_text(
            json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
        self.verify(expect_success=False)

    def test_source_archive_digest_tamper_is_rejected(self) -> None:
        manifest_path = self.licenses / "native-license-evidence.json"
        original_manifest = json.loads(
            manifest_path.read_text(encoding="utf-8")
        )
        for index, archive in enumerate(original_manifest["sourceArchives"]):
            with self.subTest(archive=archive["name"]):
                manifest = json.loads(json.dumps(original_manifest))
                original = manifest["sourceArchives"][index]["sha256"]
                manifest["sourceArchives"][index]["sha256"] = (
                    ("0" if original[0] != "0" else "1") + original[1:]
                )
                manifest_path.write_text(
                    json.dumps(
                        manifest,
                        sort_keys=True,
                        separators=(",", ":"),
                    )
                    + "\n",
                    encoding="utf-8",
                )
                result = self.verify(expect_success=False)
                self.assertIn(
                    "native license source archive metadata changed",
                    result.stderr,
                )


if __name__ == "__main__":
    unittest.main()
