#!/usr/bin/env python3

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


TASKS = Path(__file__).resolve().parents[2] / "scripts/build"


class GenerationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="kodosi-project-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "checkout"
        tasks = self.root / "scripts/build"
        tasks.mkdir(parents=True)
        for name in ("xcode-gen.sh", "generate-xcode-project.py", "deployment-target.sh"):
            shutil.copy2(TASKS / name, tasks / name)
        (self.root / "Config").mkdir()
        (self.root / "Config/macOSDeploymentTarget").write_text("26.4\n")
        self.project = self.root / "KodosiDesktop.xcodeproj"
        self.project.mkdir()
        self.value = {
            "objects": {"target": {"name": "Kodosi", "shellScript":
                'printf "%s" "https://example.test/a/*literal*/"; # // not a comment'}},
            "rootObject": "target",
        }
        (self.root / "project.yml").write_text(json.dumps(self.value))
        self.pbxproj = self.project / "project.pbxproj"
        self.pbxproj.write_text(json.dumps(self.value, separators=(",", ":")))
        self.scheme = self.project / "xcshareddata/xcschemes/KodosiDesktop.xcscheme"
        self.scheme.parent.mkdir(parents=True)
        self.scheme.write_text("<Scheme />\n")
        self.user_state = self.project / "xcuserdata/local.pbxuser"
        self.user_state.parent.mkdir()
        self.user_state.write_text("untouched user state")
        self.bin = Path(self.temporary.name) / "bin"
        self.bin.mkdir()
        self.tool("xcodegen", r'''import json, os
from pathlib import Path
root = Path.cwd()
assert os.environ['KODOSI_MACOS_DEPLOYMENT_TARGET'] == '26.4'
assert os.environ['MACOSX_DEPLOYMENT_TARGET'] == '26.4'
project = root / 'KodosiDesktop.xcodeproj'
project.mkdir(exist_ok=True)
mode = os.environ.get('GENERATOR_TEST_MODE')
if mode == 'fail':
    (project / 'project.pbxproj').write_text('partial')
    raise SystemExit(19)
if mode == 'invalid':
    (project / 'project.pbxproj').write_text('not a property list')
else:
    value = json.loads((root / 'project.yml').read_text())
    (project / 'project.pbxproj').write_text(json.dumps(value, indent=4, sort_keys=True) + '\n')
scheme = project / 'xcshareddata/xcschemes/KodosiDesktop.xcscheme'
scheme.parent.mkdir(parents=True, exist_ok=True)
scheme.write_text('<Scheme changed="true" />\n' if mode == 'scheme' else '<Scheme />\n')
''')


        self.tool("plutil", r'''import json, sys
from pathlib import Path
assert sys.argv[1:5] == ['-convert', 'json', '-o', '-']
print(json.dumps(json.loads(Path(sys.argv[5]).read_text())))
''')
        self.env = {**os.environ, "PATH": f"{self.bin}{os.pathsep}{os.environ['PATH']}"}
        self.env.pop("KODOSI_MACOS_DEPLOYMENT_TARGET_FILE", None)

    def tool(self, name, body):
        path = self.bin / name
        path.write_text("#!/usr/bin/env python3\n" + body)
        path.chmod(0o755)

    def run_generator(self, mode="", success=True):
        result = subprocess.run(["bash", str(self.root / "scripts/build/xcode-gen.sh")],
            env={**self.env, "GENERATOR_TEST_MODE": mode}, capture_output=True, text=True)
        if success:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0)

    def test_unchanged_semantics_preserve_exact_bytes_and_timestamps(self):
        os.utime(self.pbxproj, ns=(1_700_000_000_000_000_000,) * 2)
        os.utime(self.scheme, ns=(1_700_000_000_000_000_000,) * 2)
        before = self.pbxproj.read_bytes()
        self.run_generator()
        self.run_generator()
        self.assertEqual(self.pbxproj.read_bytes(), before)
        self.assertEqual(self.pbxproj.stat().st_mtime_ns, 1_700_000_000_000_000_000)
        self.assertEqual(self.scheme.stat().st_mtime_ns, 1_700_000_000_000_000_000)
        self.assertEqual(self.user_state.read_text(), "untouched user state")

    def test_real_spec_drift_updates_project_without_corrupting_script_literals(self):
        self.value["objects"]["target"]["name"] = "Renamed target"
        (self.root / "project.yml").write_text(json.dumps(self.value))
        self.run_generator()
        self.assertEqual(json.loads(self.pbxproj.read_text()), self.value)
        generated = self.pbxproj.read_bytes()
        self.run_generator()
        self.assertEqual(self.pbxproj.read_bytes(), generated)

    def test_real_scheme_drift_remains_visible(self):
        original = self.pbxproj.read_bytes()
        self.run_generator("scheme")
        self.assertEqual(self.pbxproj.read_bytes(), original)
        self.assertEqual(self.scheme.read_text(), '<Scheme changed="true" />\n')

    def test_failure_restores_the_complete_previous_project(self):
        original = {str(path.relative_to(self.project)): path.read_bytes()
                    for path in self.project.rglob("*") if path.is_file()}
        for mode in ("fail", "invalid"):
            with self.subTest(mode=mode):
                self.run_generator(mode, success=False)
                self.assertEqual({str(path.relative_to(self.project)): path.read_bytes()
                                  for path in self.project.rglob("*") if path.is_file()}, original)

    def test_first_generation_creates_project(self):
        shutil.rmtree(self.project)
        self.run_generator()
        self.assertEqual(json.loads(self.pbxproj.read_text()), self.value)


if __name__ == "__main__":
    unittest.main()
