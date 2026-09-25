import importlib.util
import io
import json
import multiprocessing
import tempfile
import sys
import unittest
from pathlib import Path
from unittest import mock

SPEC = importlib.util.spec_from_file_location("ffi_artifact", Path(__file__).with_name("build-ffi-artifact.py"))
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def concurrent_publish(output, identity):
    import fcntl
    root = Path(output)
    with (root / ".build.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        bundle = MODULE.publish(root, root / f"{identity}.a", root / f"{identity}.h", "test-target", "dev")
        MODULE.consume_and_prune(root, bundle, [sys.executable, "-c",
            "import sys,time;from pathlib import Path;p=Path(sys.argv[1]);"
            "a=(p/'libkodosi_ffi_c.a').read_text();time.sleep(.1);"
            "h=(p/'include/kodosi_runtime.h').read_text();assert a.split()[0]==h.split()[0]"])


class FfiArtifactTests(unittest.TestCase):
    def test_selects_same_package_header_and_rejects_missing_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            manifest = root / "Cargo.toml"
            manifest.touch()
            target = root / "cargo"
            out = target / "triple/release/build/ffi-hash/out"
            out.mkdir(parents=True)
            archive = target / "triple/release/libkodosi_ffi_c.a"
            archive.write_bytes(b"archive")
            header = out / "kodosi_runtime.h"
            header.write_bytes(b"header")
            messages = [
                {"reason": "build-script-executed", "package_id": "other", "out_dir": str(root)},
                {"reason": "build-script-executed", "package_id": "ffi", "out_dir": str(out)},
                {"reason": "compiler-artifact", "package_id": "ffi", "manifest_path": str(manifest),
                    "target": {"crate_types": ["staticlib"]}, "filenames": [str(archive)]},
            ]
            self.assertEqual(MODULE.resolve_outputs(messages, manifest, target), (archive, header))
            with self.assertRaises(ValueError):
                MODULE.resolve_outputs([messages[0], messages[2]], manifest, target)
            with self.assertRaises(ValueError):
                MODULE.resolve_outputs(messages + [messages[1]], manifest, target)
            for path in (archive, header):
                with self.subTest(symlink=path.name):
                    original = path.with_suffix(".real")
                    path.rename(original)
                    path.symlink_to(original)
                    with self.assertRaises(ValueError):
                        MODULE.resolve_outputs(messages, manifest, target)
                    path.unlink()
                    original.rename(path)
            outside = root / "outside.a"
            outside.write_bytes(b"archive")
            messages[2]["filenames"] = [str(outside)]
            with self.assertRaises(ValueError):
                MODULE.resolve_outputs(messages, manifest, target)

    def test_dot_targets_rejected_without_creating_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / "runtime/Cargo.toml"
            crate = root / "runtime/crates/kodosi-ffi-c/Cargo.toml"
            crate.parent.mkdir(parents=True)
            crate.touch()
            manifest.touch()
            output = root / "output"
            for target in (".", "..", "../target", "/target"):
                with self.subTest(target=target), self.assertRaisesRegex(ValueError, "safe identifiers"):
                    MODULE.build(manifest, output, "dev", target, True)
                self.assertFalse(output.exists())

    def test_publication_keeps_old_pair_and_rejects_tamper(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, header = root / "input.a", root / "input.h"
            archive.write_bytes(b"first archive")
            header.write_bytes(b"first header")
            first = MODULE.publish(root, archive, header, "target", "dev")
            archive.write_bytes(b"second archive")
            header.write_bytes(b"second header")
            second = MODULE.publish(root, archive, header, "target", "dev")
            self.assertNotEqual(first, second)
            self.assertEqual((root / "current").resolve(), second.resolve())
            self.assertEqual((first / "include/kodosi_runtime.h").read_bytes(), b"first header")
            self.assertEqual((first / "libkodosi_ffi_c.a").read_bytes(), b"first archive")
            (second / "include/kodosi_runtime.h").write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                MODULE.publish(root, archive, header, "target", "dev")

    def test_publication_replaces_only_empty_placeholder_directories(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, header = root / "input.a", root / "input.h"
            archive.write_bytes(b"archive")
            header.write_bytes(b"header")
            (root / "current/include").mkdir(parents=True)
            bundle = MODULE.publish(root, archive, header, "target", "dev")
            self.assertTrue((root / "current").is_symlink())
            self.assertEqual((root / "current").resolve(), bundle.resolve())
            (root / "current").unlink()
            (root / "current/include").mkdir(parents=True)
            (root / "current/include/kodosi_runtime.h").write_bytes(b"kept")
            with self.assertRaises(ValueError):
                MODULE.publish(root, archive, header, "target", "dev")
            self.assertEqual((root / "current/include/kodosi_runtime.h").read_bytes(), b"kept")

    def test_slot_rejects_another_source_before_cargo(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            manifest = root / "runtime/Cargo.toml"
            crate = root / "runtime/crates/kodosi-ffi-c/Cargo.toml"
            crate.parent.mkdir(parents=True)
            crate.touch()
            manifest.touch()
            slot = root / "output/target/dev"
            slot.mkdir(parents=True)
            (slot / "source-manifest").write_text("/other/Cargo.toml\n")
            with self.assertRaisesRegex(ValueError, "another source"):
                MODULE.build(manifest, root / "output", "dev", "target", True)

    def test_build_slots_separate_target_profile_and_preserve_on_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            manifest = root / "runtime/Cargo.toml"
            crate = root / "runtime/crates/kodosi-ffi-c/Cargo.toml"
            crate.parent.mkdir(parents=True)
            crate.touch()
            manifest.touch()
            output = root / "output"

            def cargo_build(command, **_):
                target_dir = Path(command[command.index("--target-dir") + 1])
                target = command[command.index("--target") + 1]
                profile = command[command.index("--profile") + 1]
                out = target_dir / target / profile / "build/ffi-hash/out"
                out.mkdir(parents=True)
                archive = target_dir / target / profile / "libkodosi_ffi_c.a"
                identity = f"{target} {profile}".encode()
                archive.write_bytes(identity + b" archive")
                (out / "kodosi_runtime.h").write_bytes(identity + b" header")
                messages = [
                    {"reason": "build-script-executed", "package_id": "ffi", "out_dir": str(out)},
                    {"reason": "compiler-artifact", "package_id": "ffi", "manifest_path": str(crate),
                        "target": {"crate_types": ["staticlib"]}, "filenames": [str(archive)]},
                    {"reason": "build-finished", "success": True},
                ]
                return mock.Mock(stdout=io.StringIO("".join(json.dumps(value) + "\n" for value in messages)),
                    wait=mock.Mock(return_value=0))

            selected = {}
            with mock.patch.object(MODULE.subprocess, "check_output", return_value="ffi\n"), \
                    mock.patch.object(MODULE.subprocess, "Popen", side_effect=cargo_build):
                for target, profile in (("target-one", "dev"), ("target-one", "release"), ("target-two", "dev")):
                    current = MODULE.build(manifest, output, profile, target, True)
                    self.assertEqual(current, output / target / profile / "current")
                    record = json.loads((current / "artifact.json").read_bytes())
                    self.assertEqual((record["target"], record["profile"]), (target, profile))
                    selected[(target, profile)] = current.resolve()
            self.assertEqual(len(set(selected.values())), 3)
            current = output / "target-one/dev/current"
            for exit_code in (0, 1):
                failure = mock.Mock(stdout=io.StringIO('{"reason":"build-finished","success":false}\n'),
                    wait=mock.Mock(return_value=exit_code))
                with mock.patch.object(MODULE.subprocess, "check_output", return_value="ffi\n"), \
                        mock.patch.object(MODULE.subprocess, "Popen", return_value=failure):
                    with self.assertRaises(ValueError):
                        MODULE.build(manifest, output, "dev", "target-one", True)
                self.assertEqual(current.resolve(), selected[("target-one", "dev")])
                self.assertEqual((current / "libkodosi_ffi_c.a").read_bytes(), b"target-one dev archive")
                self.assertEqual((current / "include/kodosi_runtime.h").read_bytes(), b"target-one dev header")

    def test_concurrent_publication_never_mixes_pairs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for identity in ("one", "two"):
                (root / f"{identity}.a").write_text(f"{identity} archive")
                (root / f"{identity}.h").write_text(f"{identity} header")
            context = multiprocessing.get_context("fork")
            workers = [context.Process(target=concurrent_publish, args=(temporary, identity)) for identity in ("one", "two")]
            for worker in workers:
                worker.start()
            for worker in workers:
                worker.join(10)
                self.assertEqual(worker.exitcode, 0)
            self.assertEqual(len(list((root / "bundles").iterdir())), 1)
            for bundle in (root / "bundles").iterdir():
                record = json.loads((bundle / "artifact.json").read_bytes())
                self.assertEqual(record["archiveSha256"], MODULE.digest(bundle / "libkodosi_ffi_c.a"))
                self.assertEqual(record["headerSha256"], MODULE.digest(bundle / "include/kodosi_runtime.h"))
                self.assertEqual((bundle / "libkodosi_ffi_c.a").read_text().split()[0], (bundle / "include/kodosi_runtime.h").read_text().split()[0])


if __name__ == "__main__":
    unittest.main()
