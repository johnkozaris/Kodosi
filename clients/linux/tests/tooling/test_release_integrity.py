from __future__ import annotations

import hashlib
import gzip
import io
import json
import os
import re
import shutil
import subprocess
import tarfile
import unittest
from contextlib import contextmanager
from pathlib import Path
from tempfile import TemporaryDirectory


ROOT = Path(__file__).resolve().parents[2]
GENERATOR = ROOT / "scripts/release" / "generate-release-manifest.py"
VERIFIER = ROOT / "scripts/release" / "verify-release-manifest.py"
IDENTITY_GENERATOR = ROOT / "scripts/release" / "generate-source-identity.py"
SOURCE_VERIFIER = ROOT / "scripts/release" / "verify-release-sources.py"
SIGNER = ROOT / "scripts/release" / "sign-release-manifest.sh"
SIGNATURE_VERIFIER = ROOT / "scripts/release" / "verify-release-signature.sh"


class ReleaseIntegrityTests(unittest.TestCase):
    def setUp(self) -> None:
        build = ROOT / "build"
        build.mkdir(exist_ok=True)
        self.temporary = TemporaryDirectory(
            prefix="release-integrity-",
            dir=build,
        )
        temporary = Path(self.temporary.name)
        lock = dict(
            line.split("=", 1)
            for line in (ROOT.parents[1] / "Ghostty.lock").read_text().splitlines()
        )
        lock["linux_vt_archive_sha256"] = hashlib.sha256(b"archive\n").hexdigest()
        self.checkout = self.create_repo(
            temporary / "Kodosi",
            "johnkozaris/Kodosi",
            {
                ".gitignore": "**/build/\n",
                "Ghostty.lock": "".join(f"{key}={value}\n" for key, value in lock.items()),
                "clients/linux/CMakeLists.txt": "project(KodosiQT VERSION 0.1.0 LANGUAGES CXX)\n",
                "clients/linux/tracked.txt": "one\n",
                "runtime/runtime.txt": "runtime-one\n",
                "terminal/ghostty/LinuxGhostty.ref": lock["linux_upstream_commit"] + "\n",
                "terminal/ghostty/Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a": "archive\n",
                "terminal/ghostty/LICENSES/Ghostty.txt": "Ghostty license fixture\n",
            },
        )
        self.source = self.checkout / "clients/linux"
        self.runtime = self.checkout / "runtime"
        self.ghostty = self.checkout / "terminal/ghostty"
        (self.source / "tool.sh").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        (self.source / "tool.sh").chmod(0o755)
        os.symlink("tracked.txt", self.source / "tracked-link")
        self.run_command(["git", "add", "tool.sh", "tracked-link"], cwd=self.source)
        self.run_command(["git", "commit", "-m", "add mode fixtures"], cwd=self.source)
        dependencies = json.loads((ROOT / "dependencies.lock.json").read_text())
        self.dependencies = temporary / "dependencies.lock.json"
        self.write_json(self.dependencies, dependencies)
        self.output = self.source / "build" / "release"
        self.output.mkdir(parents=True)
        self.artifacts = [
            self.output / "kodosi_0.1.0_amd64.deb",
            self.output / "kodosi-0.1.0-linux-x86_64.tar.gz",
            self.output / "kodosi-bin-0.1.0-1-x86_64.pkg.tar.zst",
        ]
        self.identity = self.output / "identity.json"
        self.generate_identity(self.identity)
        self.build_artifacts(self.identity)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def create_repo(
        self,
        path: Path,
        repository: str,
        files: dict[str, str],
    ) -> Path:
        path.mkdir()
        self.run_command(["git", "init", "-b", "main"], cwd=path)
        self.run_command(["git", "config", "user.name", "Release Test"], cwd=path)
        self.run_command(
            ["git", "config", "user.email", "release@example.invalid"],
            cwd=path,
        )
        self.run_command(
            [
                "git",
                "remote",
                "add",
                "origin",
                f"https://github.com/{repository}.git",
            ],
            cwd=path,
        )
        for relative, contents in files.items():
            destination = path / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(contents, encoding="utf-8")
        self.run_command(["git", "add", "."], cwd=path)
        self.run_command(["git", "commit", "-m", "initial"], cwd=path)
        return path

    def write_json(self, path: Path, value: object) -> None:
        path.write_text(
            json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )

    def generate_identity(self, output: Path) -> None:
        self.run_command(
            [
                "python3",
                str(IDENTITY_GENERATOR),
                "--source-root",
                str(self.source),
                "--output",
                str(output),
            ]
        )

    def build_artifacts(self, identity: Path) -> None:
        source_date_epoch = json.loads(
            identity.read_text(encoding="utf-8")
        )["sourceDateEpoch"]
        stage = self.output / "package-stage"
        shutil.rmtree(stage, ignore_errors=True)
        identity_destination = (
            stage
            / "usr/share/doc/kodosi/provenance/source-identity.json"
        )
        identity_destination.parent.mkdir(parents=True)
        shutil.copyfile(identity, identity_destination)

        deb_root = self.output / "deb-root"
        shutil.rmtree(deb_root, ignore_errors=True)
        shutil.copytree(stage, deb_root)
        control = deb_root / "DEBIAN" / "control"
        control.parent.mkdir()
        control.write_text(
            "Package: kodosi-test\n"
            "Version: 0.1.0\n"
            "Architecture: amd64\n"
            "Maintainer: Test <release@example.invalid>\n"
            "Description: release identity fixture\n",
            encoding="utf-8",
        )
        for root in (stage, deb_root):
            root.chmod(0o755)
            for path in root.rglob("*"):
                if path.is_dir():
                    path.chmod(0o755)
                elif path.is_file():
                    path.chmod(0o644)
                os.utime(path, (source_date_epoch, source_date_epoch))
            os.utime(root, (source_date_epoch, source_date_epoch))
        old_umask = os.umask(0o022)
        try:
            self.run_command(
                [
                    "dpkg-deb",
                    "-Zxz",
                    "--build",
                    "--root-owner-group",
                    str(deb_root),
                    str(self.artifacts[0]),
                ],
                env={
                    **os.environ,
                    "SOURCE_DATE_EPOCH": str(source_date_epoch),
                },
            )
        finally:
            os.umask(old_umask)
        self.write_normalized_tgz(
            stage,
            self.artifacts[1],
            "kodosi-0.1.0-Linux",
            source_date_epoch,
        )
        self.run_command(
            [
                "bash",
                str(ROOT / "scripts/release/write-arch-package.sh"),
                str(self.artifacts[1]),
                str(self.output),
            ],
            env={
                **os.environ,
                "SOURCE_DATE_EPOCH": str(source_date_epoch),
            },
        )
        shutil.rmtree(stage)
        shutil.rmtree(deb_root)

    def write_normalized_tgz(
        self,
        source: Path,
        output: Path,
        root_name: str,
        epoch: int,
    ) -> None:
        uncompressed = io.BytesIO()
        with tarfile.open(
            fileobj=uncompressed,
            mode="w",
            format=tarfile.GNU_FORMAT,
        ) as archive:
            paths = [source, *sorted(source.rglob("*"))]
            for path in paths:
                relative = path.relative_to(source)
                name = root_name if relative == Path(".") else (
                    Path(root_name) / relative
                ).as_posix()
                member = archive.gettarinfo(str(path), arcname=name)
                member.uid = 0
                member.gid = 0
                member.uname = "root"
                member.gname = "root"
                member.mtime = epoch
                if member.isdir():
                    member.mode = 0o755
                elif member.isfile():
                    member.mode = 0o755 if os.access(path, os.X_OK) else 0o644
                if member.isfile():
                    with path.open("rb") as contents:
                        archive.addfile(member, contents)
                else:
                    archive.addfile(member)
        with output.open("wb") as destination:
            with gzip.GzipFile(
                filename="",
                mode="wb",
                fileobj=destination,
                mtime=epoch,
            ) as compressed:
                compressed.write(uncompressed.getvalue())

    def common_arguments(self) -> list[str]:
        return [
            "--source-root",
            str(self.source),
            "--dependencies",
            str(self.dependencies),
        ]

    def generate(
        self,
        *extra: str,
        expect_success: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        command = [
            "python3",
            str(GENERATOR),
            *self.common_arguments(),
            "--output-dir",
            str(self.output),
        ]
        for artifact in self.artifacts:
            command.extend(["--artifact", artifact.name])
        command.extend(extra)
        return self.run_command(command, expect_success=expect_success)

    def verify(
        self,
        *extra: str,
        expect_success: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return self.run_command(
            [
                "python3",
                str(VERIFIER),
                *self.common_arguments(),
                "--manifest",
                str(self.output / "release-manifest.json"),
                "--output-dir",
                str(self.output),
                *extra,
            ],
            expect_success=expect_success,
        )

    def verify_sources(self, expect_success: bool = True) -> subprocess.CompletedProcess[str]:
        return self.run_command(
            ["python3", str(SOURCE_VERIFIER), "--source-root", str(self.source)],
            expect_success=expect_success,
        )

    def run_command(
        self,
        command: list[str],
        expect_success: bool = True,
        *,
        cwd: Path = ROOT,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            command,
            cwd=cwd,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if expect_success and result.returncode != 0:
            self.fail(result.stderr or result.stdout)
        if not expect_success and result.returncode == 0:
            self.fail("command unexpectedly succeeded")
        return result

    @contextmanager
    def index_flag(self, repo: Path, relative: str, flag: str):
        self.run_command(
            ["git", "update-index", f"--{flag}", "--", relative],
            cwd=repo,
        )
        try:
            yield
        finally:
            self.run_command(
                ["git", "update-index", f"--no-{flag}", "--", relative],
                cwd=repo,
            )

    def rewrite_manifest(self, mutate) -> None:
        path = self.output / "release-manifest.json"
        value = json.loads(path.read_text(encoding="utf-8"))
        mutate(value)
        self.write_json(path, value)

    def test_clean_generation_is_deterministic_and_verifiable(self) -> None:
        self.generate()
        first = (self.output / "release-manifest.json").read_bytes()
        self.verify()
        self.generate()
        second = (self.output / "release-manifest.json").read_bytes()
        self.verify()
        self.assertEqual(first, second)
        manifest = json.loads(first)
        self.assertEqual(manifest["schemaVersion"], 6)
        self.assertEqual(manifest["productVersion"], "0.1.0")
        self.assertEqual(manifest["target"], "linux-x86_64")
        self.assertEqual(
            set(manifest["source"]),
            {
                "schemaVersion",
                "sourceDateEpoch",
                "repository",
                "commit",
                "dirty",
            },
        )
        self.assertEqual(
            manifest["source"]["sourceDateEpoch"],
            int(
                self.run_command(
                    ["git", "show", "-s", "--format=%ct", "HEAD"],
                    cwd=self.source,
                ).stdout
            ),
        )
        self.assertFalse(manifest["source"]["dirty"])
        self.assertEqual(len(manifest["artifacts"]), 3)

    def test_dirty_client_runtime_and_ghostty_are_rejected(self) -> None:
        for repo, relative in (
            (self.source, "tracked.txt"),
            (self.runtime, "runtime.txt"),
            (self.ghostty, "LinuxGhostty.ref"),
        ):
            original = (repo / relative).read_text(encoding="utf-8")
            (repo / relative).write_text("dirty\n", encoding="utf-8")
            self.generate(expect_success=False)
            (repo / relative).write_text(original, encoding="utf-8")
        (self.runtime / "untracked.txt").write_text("dirty\n", encoding="utf-8")
        self.generate(expect_success=False)

    def test_hidden_index_flags_and_modifications_are_rejected(self) -> None:
        cases = (
            (self.source, "tracked.txt", self.generate),
            (self.runtime, "runtime.txt", self.verify_sources),
            (self.ghostty, "LICENSES/Ghostty.txt", self.verify_sources),
        )
        for flag in ("assume-unchanged", "skip-worktree"):
            for repo, relative, verifier in cases:
                with self.subTest(flag=flag, repo=repo.name, path=relative):
                    path = repo / relative
                    original = path.read_bytes()
                    try:
                        with self.index_flag(repo, relative, flag):
                            path.write_bytes(b"hidden modification\n")
                            verifier(expect_success=False)
                    finally:
                        path.write_bytes(original)

    def test_symlink_target_and_executable_mode_changes_are_rejected(self) -> None:
        link = self.source / "tracked-link"
        try:
            link.unlink()
            os.symlink("tool.sh", link)
            self.generate(expect_success=False)
        finally:
            link.unlink(missing_ok=True)
            os.symlink("tracked.txt", link)

        executable = self.source / "tool.sh"
        executable.chmod(0o644)
        try:
            self.generate(expect_success=False)
        finally:
            executable.chmod(0o755)
        self.generate()

    def test_staged_changes_are_rejected(self) -> None:
        tracked = self.source / "tracked.txt"
        original = tracked.read_bytes()
        try:
            tracked.write_bytes(b"staged modification\n")
            self.run_command(["git", "add", "--", "tracked.txt"], cwd=self.source)
            self.generate(expect_success=False)
        finally:
            self.run_command(
                ["git", "reset", "--quiet", "HEAD", "--", "tracked.txt"],
                cwd=self.source,
            )
            tracked.write_bytes(original)
        self.generate()

    def test_source_verifier_rejects_dirty_checkout(self) -> None:
        self.verify_sources()
        (self.runtime / "runtime.txt").write_text("dirty\n", encoding="utf-8")
        self.verify_sources(expect_success=False)

    def test_lock_ref_and_archive_substitution_are_rejected(self) -> None:
        self.verify_sources()
        lock_path = self.checkout / "Ghostty.lock"
        original_lock = lock_path.read_text()
        original_ref = (self.ghostty / "LinuxGhostty.ref").read_bytes()
        archive = (
            self.ghostty
            / "Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a"
        )
        original_archive = archive.read_bytes()

        lock_path.write_text(re.sub(
            r"linux_upstream_commit=.*", "linux_upstream_commit=" + "f" * 40, original_lock,
        ))
        self.run_command(["git", "add", "Ghostty.lock"], cwd=self.checkout)
        self.run_command(["git", "commit", "-m", "change upstream pin"], cwd=self.checkout)
        self.verify_sources(expect_success=False)
        lock_path.write_text(original_lock)
        self.run_command(["git", "add", "Ghostty.lock"], cwd=self.checkout)
        self.run_command(["git", "commit", "-m", "restore upstream pin"], cwd=self.checkout)
        self.verify_sources()

        with self.index_flag(
            self.ghostty,
            "LinuxGhostty.ref",
            "assume-unchanged",
        ):
            (self.ghostty / "LinuxGhostty.ref").write_text("f" * 40 + "\n")
            self.verify_sources(expect_success=False)
            (self.ghostty / "LinuxGhostty.ref").write_bytes(original_ref)

        archive.write_bytes(b"substitute archive\n")
        archive_relative = str(archive.relative_to(self.ghostty))
        with self.index_flag(
            self.ghostty,
            archive_relative,
            "assume-unchanged",
        ):
            self.verify_sources(expect_success=False)
            archive.write_bytes(original_archive)

    def test_full_verifier_remeasures_ghostty_inputs(self) -> None:
        self.generate()
        ref = self.ghostty / "LinuxGhostty.ref"
        original_ref = ref.read_bytes()
        with self.index_flag(
            self.ghostty,
            "LinuxGhostty.ref",
            "assume-unchanged",
        ):
            ref.write_text("f" * 40 + "\n", encoding="ascii")
            self.verify(expect_success=False)
            ref.write_bytes(original_ref)

        archive = (
            self.ghostty
            / "Vendor/GhosttyVt/linux-x86_64/lib/libghostty-vt.a"
        )
        original_archive = archive.read_bytes()
        archive.write_bytes(b"substitute archive\n")
        archive_relative = str(archive.relative_to(self.ghostty))
        with self.index_flag(
            self.ghostty,
            archive_relative,
            "assume-unchanged",
        ):
            self.verify(expect_success=False)
            archive.write_bytes(original_archive)

    def commit_change(self, repo: Path, relative: str) -> None:
        (repo / relative).write_text("two\n", encoding="utf-8")
        self.run_command(["git", "add", relative], cwd=repo)
        self.run_command(["git", "commit", "-m", "second"], cwd=repo)

    def test_stale_artifacts_after_client_change_are_rejected(self) -> None:
        self.commit_change(self.source, "tracked.txt")
        self.generate(expect_success=False)

    def test_stale_artifacts_after_runtime_change_are_rejected(self) -> None:
        self.commit_change(self.runtime, "runtime.txt")
        self.generate(expect_success=False)

    def test_stale_artifacts_after_ghostty_change_are_rejected(self) -> None:
        self.commit_change(self.ghostty, "LICENSES/Ghostty.txt")
        self.generate(expect_success=False)

    def test_mismatched_and_dirty_artifact_identities_are_rejected(self) -> None:
        mismatch = json.loads(self.identity.read_text(encoding="utf-8"))
        mismatch["commit"] = "f" * 40
        mismatch_path = self.output / "mismatch.json"
        self.write_json(mismatch_path, mismatch)
        self.build_artifacts(mismatch_path)
        self.generate(expect_success=False)

        dirty = json.loads(self.identity.read_text(encoding="utf-8"))
        dirty["dirty"] = True
        dirty_path = self.output / "dirty-identity.json"
        self.write_json(dirty_path, dirty)
        self.build_artifacts(dirty_path)
        self.generate(expect_success=False)

    def test_tamper_duplicate_extra_path_and_pin_drift_are_rejected(self) -> None:
        self.build_artifacts(self.identity)
        self.generate()
        self.artifacts[0].write_bytes(b"tampered")
        self.verify(expect_success=False)
        self.build_artifacts(self.identity)

        duplicate = self.artifacts[0]
        original = self.artifacts[2]
        self.artifacts[2] = duplicate
        self.generate(expect_success=False)
        self.artifacts[2] = original
        self.generate()
        extra = self.output / "old_0.0.1_amd64.deb"
        extra.write_bytes(b"stale")
        self.verify(expect_success=False)
        extra.unlink()

        self.generate()
        self.rewrite_manifest(
            lambda value: value["artifacts"][0].update(
                filename="../kodosi_0.1.0_amd64.deb"
            )
        )
        self.verify(expect_success=False)
        self.generate()
        drift = json.loads(self.dependencies.read_text(encoding="utf-8"))
        drift["frameworks"]["qt"]["version"] = "0.0.0"
        drift_path = self.output / "dependencies.lock.json"
        self.write_json(drift_path, drift)
        self.verify("--dependencies", str(drift_path), expect_success=False)

    def test_signing_subkey_verifies_offline_by_subkey_or_primary(self) -> None:
        self.generate()
        signing_home = TemporaryDirectory(prefix="kodosi-signing-")
        self.addCleanup(signing_home.cleanup)
        gnupg = Path(signing_home.name)
        env = os.environ.copy()
        env.update(
            {
                "GNUPGHOME": str(gnupg),
                "KODOSI_SOURCE_ROOT": str(self.source),
                "KODOSI_DEPENDENCIES": str(self.dependencies),
            }
        )
        self.run_command(
            [
                "gpg",
                "--batch",
                "--no-tty",
                "--pinentry-mode",
                "loopback",
                "--passphrase",
                "",
                "--quick-generate-key",
                "Kodosi Release Test <release@example.invalid>",
                "ed25519",
                "cert",
                "0",
            ],
            env=env,
        )
        listing = self.run_command(
            ["gpg", "--batch", "--with-colons", "--list-secret-keys"],
            env=env,
        ).stdout
        primary = next(
            line.split(":")[9]
            for line in listing.splitlines()
            if line.startswith("fpr:")
        )
        self.run_command(
            [
                "gpg",
                "--batch",
                "--no-tty",
                "--pinentry-mode",
                "loopback",
                "--passphrase",
                "",
                "--quick-add-key",
                primary,
                "ed25519",
                "sign",
                "0",
            ],
            env=env,
        )
        listing = self.run_command(
            ["gpg", "--batch", "--with-colons", "--list-secret-keys"],
            env=env,
        ).stdout
        fingerprints = [
            line.split(":")[9]
            for line in listing.splitlines()
            if line.startswith("fpr:")
        ]
        signing = fingerprints[-1]
        manifest = self.output / "release-manifest.json"
        original = manifest.read_bytes()
        self.run_command(["bash", str(SIGNER), str(manifest), signing], env=env)
        signature = Path(f"{manifest}.asc")

        outside = Path(self.temporary.name) / "offline"
        outside.mkdir()
        offline_manifest = outside / "release-manifest.json"
        offline_signature = outside / "release-manifest.json.asc"
        shutil.copyfile(manifest, offline_manifest)
        shutil.copyfile(signature, offline_signature)
        for trusted in (signing, primary):
            self.run_command(
                [
                    "bash",
                    str(SIGNATURE_VERIFIER),
                    str(offline_manifest),
                    str(offline_signature),
                    trusted,
                ],
                env={"GNUPGHOME": str(gnupg), "PATH": env["PATH"]},
            )
        self.run_command(
            [
                "bash",
                str(SIGNATURE_VERIFIER),
                str(offline_manifest),
                str(offline_signature),
                "0" * 40,
            ],
            expect_success=False,
            env={"GNUPGHOME": str(gnupg), "PATH": env["PATH"]},
        )

        offline_manifest.write_bytes(original + b" ")
        self.run_command(
            [
                "bash",
                str(SIGNATURE_VERIFIER),
                str(offline_manifest),
                str(offline_signature),
                primary,
            ],
            expect_success=False,
            env={"GNUPGHOME": str(gnupg), "PATH": env["PATH"]},
        )



if __name__ == "__main__":
    unittest.main()
