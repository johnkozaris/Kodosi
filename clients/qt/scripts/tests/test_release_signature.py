from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

RELEASE = Path(__file__).resolve().parents[1] / "release"
sys.path.insert(0, str(RELEASE))

from release_common import canonical_json  # noqa: E402

SOURCE = {
    "schemaVersion": 4,
    "sourceDateEpoch": 1767225600,
    "repository": "example/kodosi",
    "commit": "1" * 40,
    "dirty": False,
}
MANIFEST = {
    "schemaVersion": 6,
    "productVersion": "0.1.0",
    "target": "linux-x86_64",
    "source": SOURCE,
    "pins": {
        "qtVersion": "6.12.0",
        "runtime": {"ffiAbiVersion": 1, "desktopProtocolVersion": 1},
        "ghostty": {
            "repository": "ghostty-org/ghostty",
            "commit": "2" * 40,
            "linuxVtArchiveSha256": "3" * 64,
        },
        "dependenciesLockSha256": "4" * 64,
    },
    "artifacts": [
        {"filename": name, "size": 1, "sha256": "5" * 64, "sourceIdentity": SOURCE}
        for name in sorted(
            (
                "kodosi_0.1.0_amd64.deb",
                "kodosi-0.1.0-linux-x86_64.tar.gz",
                "kodosi-bin-0.1.0-1-x86_64.pkg.tar.zst",
            )
        )
    ],
}


class ReleaseSignatureTests(unittest.TestCase):
    """No release recipe runs the signature check, and it accepts every good
    release. Only these tests show that it refuses a bad signature."""

    def setUp(self) -> None:
        folder = Path(tempfile.mkdtemp(prefix="kodosi-signature-"))
        self.addCleanup(shutil.rmtree, folder)
        gnupg = folder / "gnupg"
        gnupg.mkdir(mode=0o700)
        self.environment = {"GNUPGHOME": str(gnupg), "PATH": os.environ["PATH"]}
        self.addCleanup(self.run_command, "gpgconf", "--kill", "all")
        self.primary = self.generate_key("Kodosi Release Test <release@example.invalid>", "cert")
        self.subkey = self.add_signing_subkey(self.primary)
        self.other = self.generate_key("Other Signer <other@example.invalid>", "sign")
        self.manifest = folder / "release-manifest.json"
        self.manifest.write_bytes(canonical_json(MANIFEST))
        self.signature = folder / "release-manifest.json.asc"

    def run_command(self, *command: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            command,
            env=self.environment,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def assert_success(self, result: subprocess.CompletedProcess[str]) -> None:
        if result.returncode != 0:
            self.fail(result.stderr or result.stdout)

    def assert_rejected(self, result: subprocess.CompletedProcess[str], message: str) -> None:
        self.assertNotEqual(result.returncode, 0, "the signature check accepted bad input")
        self.assertIn(message, result.stderr)

    def fingerprints(self, key: str) -> list[str]:
        result = self.run_command("gpg", "--batch", "--with-colons", "--list-secret-keys", key)
        self.assert_success(result)
        return [line.split(":")[9] for line in result.stdout.splitlines() if line.startswith("fpr:")]

    def generate_key(self, user: str, usage: str) -> str:
        self.assert_success(
            self.run_command(
                "gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "",
                "--quick-generate-key", user, "ed25519", usage, "0",
            )
        )
        return self.fingerprints(user)[0]

    def add_signing_subkey(self, primary: str) -> str:
        self.assert_success(
            self.run_command(
                "gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "",
                "--quick-add-key", primary, "ed25519", "sign", "0",
            )
        )
        return self.fingerprints(primary)[-1]

    def sign(self, fingerprint: str) -> subprocess.CompletedProcess[str]:
        return self.run_command(
            "bash", str(RELEASE / "sign-release-manifest.sh"), str(self.manifest), fingerprint
        )

    def verify(self, trusted: str) -> subprocess.CompletedProcess[str]:
        return self.run_command(
            "bash",
            str(RELEASE / "verify-release-signature.sh"),
            str(self.manifest),
            str(self.signature),
            trusted,
        )

    def test_subkey_signature_is_accepted_for_the_subkey_and_the_primary_key(self) -> None:
        self.assert_success(self.sign(self.subkey))
        self.assert_success(self.verify(self.subkey))
        self.assert_success(self.verify(self.primary))

    def test_signature_of_another_key_is_refused(self) -> None:
        self.assert_success(self.sign(self.other))
        self.assert_rejected(
            self.verify(self.primary),
            "The detached signature does not match the trusted fingerprint.",
        )

    def test_changed_manifest_is_refused(self) -> None:
        self.assert_success(self.sign(self.subkey))
        self.manifest.write_bytes(canonical_json(dict(MANIFEST, productVersion="0.1.1")))
        self.assert_rejected(self.verify(self.primary), "The detached signature is invalid.")


if __name__ == "__main__":
    unittest.main()
