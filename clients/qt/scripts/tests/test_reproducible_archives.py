from __future__ import annotations

import gzip
import importlib.util
import io
import json
import subprocess
import sys
import tarfile
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


ROOT = Path(__file__).resolve().parents[2]
VERIFIER = ROOT / "scripts/release/verify-reproducible-archive.py"
EPOCH = 1_700_000_000
sys.path.insert(0, str(ROOT / "scripts/release"))
SPEC = importlib.util.spec_from_file_location("archive_verifier", VERIFIER)
assert SPEC is not None and SPEC.loader is not None
ARCHIVE_VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ARCHIVE_VERIFIER)


class ReproducibleArchiveTests(unittest.TestCase):
    def setUp(self) -> None:
        (ROOT / "build").mkdir(exist_ok=True)
        self.temporary = TemporaryDirectory(
            prefix="reproducible-archive-",
            dir=ROOT / "build",
        )
        self.root = Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def identity(self) -> bytes:
        return (
            json.dumps(
                {"sourceDateEpoch": EPOCH},
                sort_keys=True,
                separators=(",", ":"),
            )
            + "\n"
        ).encode()

    def tar_bytes(
        self,
        rows: list[tuple[str, bytes | None, int, int, int]],
    ) -> bytes:
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode="w", format=tarfile.GNU_FORMAT) as tar:
            for name, payload, mtime, uid, mode in rows:
                member = tarfile.TarInfo(name)
                member.mtime = mtime
                member.uid = uid
                member.gid = 0
                member.uname = "root"
                member.gname = "root"
                member.mode = mode
                if payload is None:
                    member.type = tarfile.DIRTYPE
                    tar.addfile(member)
                else:
                    member.size = len(payload)
                    tar.addfile(member, io.BytesIO(payload))
        return output.getvalue()

    def custom_tar_bytes(self, entries: list[dict[str, object]]) -> bytes:
        output = io.BytesIO()
        with tarfile.open(fileobj=output, mode="w", format=tarfile.GNU_FORMAT) as tar:
            for entry in entries:
                member = tarfile.TarInfo(str(entry["name"]))
                member.mtime = EPOCH
                member.uid = 0
                member.gid = 0
                member.uname = "root"
                member.gname = "root"
                member.type = entry.get("type", tarfile.REGTYPE)
                member.linkname = str(entry.get("linkname", ""))
                member.mode = int(
                    entry.get(
                        "mode",
                        0o755 if member.isdir() else (
                            0o777 if member.issym() else 0o644
                        ),
                    )
                )
                payload = entry.get("payload", b"")
                if member.isfile():
                    assert isinstance(payload, bytes)
                    member.size = len(payload)
                    tar.addfile(member, io.BytesIO(payload))
                else:
                    tar.addfile(member)
        return output.getvalue()

    def identity_entries(self) -> list[dict[str, object]]:
        return [
            {"name": "pkg/usr", "type": tarfile.DIRTYPE},
            {"name": "pkg/usr/share", "type": tarfile.DIRTYPE},
            {"name": "pkg/usr/share/doc", "type": tarfile.DIRTYPE},
            {"name": "pkg/usr/share/doc/kodosi", "type": tarfile.DIRTYPE},
            {
                "name": "pkg/usr/share/doc/kodosi/provenance",
                "type": tarfile.DIRTYPE,
            },
            {
                "name": (
                    "pkg/usr/share/doc/kodosi/provenance/"
                    "source-identity.json"
                ),
                "payload": self.identity(),
            },
        ]

    def write_custom_tgz(
        self,
        name: str,
        entries: list[dict[str, object]],
    ) -> tuple[Path, bytes]:
        entries.sort(key=lambda entry: str(entry["name"]))
        payload = self.custom_tar_bytes(entries)
        path = self.root / name
        with path.open("wb") as destination:
            with gzip.GzipFile(
                filename="",
                mode="wb",
                fileobj=destination,
                mtime=EPOCH,
            ) as compressed:
                compressed.write(payload)
        return path, payload

    def verify_memory_and_artifact(
        self,
        name: str,
        entries: list[dict[str, object]],
        *,
        expect_success: bool,
        expected_error: str | None = None,
    ) -> None:
        artifact, payload = self.write_custom_tgz(name, entries)
        try:
            with tarfile.open(fileobj=io.BytesIO(payload), mode="r:") as archive:
                ARCHIVE_VERIFIER.validate_tar(
                    archive,
                    EPOCH,
                    require_pkginfo=False,
                )
            with tarfile.open(fileobj=io.BytesIO(payload), mode="r:") as archive:
                ARCHIVE_VERIFIER.source_identity_payload(
                    archive,
                    allow_package_root=True,
                )
        except SystemExit as error:
            if expect_success:
                self.fail(str(error))
            if expected_error is not None:
                self.assertIn(expected_error, str(error))
        else:
            if not expect_success:
                self.fail("in-memory archive verification unexpectedly succeeded")
        result = self.verify(artifact, expect_success=expect_success)
        if expected_error is not None:
            self.assertIn(expected_error, result.stderr + result.stdout)

    def write_tgz(
        self,
        name: str,
        *,
        mtime: int = EPOCH,
        uid: int = 0,
        reverse: bool = False,
    ) -> Path:
        rows = [
            ("pkg/usr", None, mtime, uid, 0o755),
            ("pkg/usr/share", None, mtime, uid, 0o755),
            ("pkg/usr/share/doc", None, mtime, uid, 0o755),
            ("pkg/usr/share/doc/kodosi", None, mtime, uid, 0o755),
            ("pkg/usr/share/doc/kodosi/provenance", None, mtime, uid, 0o755),
            (
                "pkg/usr/share/doc/kodosi/provenance/source-identity.json",
                self.identity(),
                mtime,
                uid,
                0o644,
            ),
        ]
        if reverse:
            rows.reverse()
        path = self.root / name
        with path.open("wb") as destination:
            with gzip.GzipFile(
                filename="",
                mode="wb",
                fileobj=destination,
                mtime=EPOCH,
            ) as compressed:
                compressed.write(self.tar_bytes(rows))
        return path

    def write_arch(
        self,
        name: str,
        pkginfo: str,
        include_buildinfo: bool = False,
    ) -> Path:
        rows = [
            (".", None, EPOCH, 0, 0o755),
            ("./.PKGINFO", pkginfo.encode(), EPOCH, 0, 0o644),
        ]
        if include_buildinfo:
            rows.append(("./.BUILDINFO", b"buildenv = host\n", EPOCH, 0, 0o644))
        rows.extend(
            [
                ("./usr", None, EPOCH, 0, 0o755),
                ("./usr/share", None, EPOCH, 0, 0o755),
                ("./usr/share/doc", None, EPOCH, 0, 0o755),
                ("./usr/share/doc/kodosi", None, EPOCH, 0, 0o755),
                (
                    "./usr/share/doc/kodosi/provenance",
                    None,
                    EPOCH,
                    0,
                    0o755,
                ),
                (
                    "./usr/share/doc/kodosi/provenance/source-identity.json",
                    self.identity(),
                    EPOCH,
                    0,
                    0o644,
                ),
            ]
        )
        rows.sort(key=lambda row: row[0])
        uncompressed = self.root / f"{name}.tar"
        uncompressed.write_bytes(self.tar_bytes(rows))
        path = self.root / f"{name}.pkg.tar.zst"
        subprocess.run(
            [
                "zstd",
                "--quiet",
                "--threads=1",
                "-19",
                "-o",
                str(path),
                str(uncompressed),
            ],
            check=True,
        )
        return path

    def verify(
        self,
        artifact: Path,
        expect_success: bool,
    ) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            [
                "python3",
                str(VERIFIER),
                "--artifact",
                str(artifact),
                "--source-date-epoch",
                str(EPOCH),
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
            self.fail("archive verification unexpectedly succeeded")
        return result

    def test_sorted_root_owned_epoch_tgz_is_accepted(self) -> None:
        self.verify(self.write_tgz("valid.tar.gz"), expect_success=True)

    def test_wall_clock_owner_and_order_tamper_are_rejected(self) -> None:
        for artifact in (
            self.write_tgz("mtime.tar.gz", mtime=EPOCH + 1),
            self.write_tgz("owner.tar.gz", uid=1000),
            self.write_tgz("order.tar.gz", reverse=True),
        ):
            self.verify(artifact, expect_success=False)

    def test_arch_build_host_and_buildinfo_are_rejected(self) -> None:
        base = f"builddate = {EPOCH}\n"
        self.verify(
            self.write_arch("buildhost", base + "buildhost = workstation\n"),
            expect_success=False,
        )
        self.verify(
            self.write_arch("buildinfo", base, include_buildinfo=True),
            expect_success=False,
        )

    def test_unsafe_and_deceptive_paths_are_rejected(self) -> None:
        cases = (
            (
                "parent.tar.gz",
                [{"name": "../../outside", "payload": b"escape"}],
                "unsafe archive path",
            ),
            (
                "absolute.tar.gz",
                [{"name": "/absolute", "payload": b"escape"}],
                "unsafe archive path",
            ),
            (
                "duplicate.tar.gz",
                [
                    {"name": "pkg/x", "payload": b"one"},
                    {"name": "./pkg/x", "payload": b"two"},
                ],
                "duplicate normalized path",
            ),
            (
                "deceptive.tar.gz",
                [
                    {
                        "name": (
                            "evilusr/share/doc/kodosi/provenance/"
                            "source-identity.json"
                        ),
                        "payload": self.identity(),
                    }
                ],
                "source identity is missing",
            ),
        )
        for name, extra, error in cases:
            with self.subTest(name=name):
                entries = self.identity_entries()
                if name == "deceptive.tar.gz":
                    entries = extra
                else:
                    entries.extend(extra)
                self.verify_memory_and_artifact(
                    name,
                    entries,
                    expect_success=False,
                    expected_error=error,
                )

    def test_unsafe_links_and_special_files_are_rejected(self) -> None:
        cases = (
            (
                "symlink-escape.tar.gz",
                {
                    "name": "pkg/escape-link",
                    "type": tarfile.SYMTYPE,
                    "linkname": "../../../outside",
                },
                "unsafe symlink target",
            ),
            (
                "hardlink-escape.tar.gz",
                {
                    "name": "pkg/escape-hardlink",
                    "type": tarfile.LNKTYPE,
                    "linkname": "../../outside",
                },
                "unsafe hardlink target",
            ),
            (
                "hardlink-missing.tar.gz",
                {
                    "name": "pkg/missing-hardlink",
                    "type": tarfile.LNKTYPE,
                    "linkname": "pkg/missing",
                },
                "hardlink target is missing",
            ),
            (
                "symlink-cycle.tar.gz",
                {
                    "name": "pkg/cycle",
                    "type": tarfile.SYMTYPE,
                    "linkname": "cycle/child",
                },
                "link chain is cyclic",
            ),
            (
                "fifo.tar.gz",
                {
                    "name": "pkg/fifo",
                    "type": tarfile.FIFOTYPE,
                    "mode": 0o644,
                },
                "unsupported archive member type",
            ),
        )
        for name, entry, error in cases:
            with self.subTest(name=name):
                self.verify_memory_and_artifact(
                    name,
                    [*self.identity_entries(), entry],
                    expect_success=False,
                    expected_error=error,
                )

    def test_safe_internal_symlink_and_hardlink_are_accepted(self) -> None:
        self.verify_memory_and_artifact(
            "safe-links.tar.gz",
            [
                *self.identity_entries(),
                {"name": "pkg/data.txt", "payload": b"data\n"},
                {
                    "name": "pkg/data-hardlink",
                    "type": tarfile.LNKTYPE,
                    "linkname": "pkg/data.txt",
                },
                {
                    "name": "pkg/data-symlink",
                    "type": tarfile.SYMTYPE,
                    "linkname": "data.txt",
                },
            ],
            expect_success=True,
        )


if __name__ == "__main__":
    unittest.main()
