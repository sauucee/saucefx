#!/usr/bin/env python3
"""Check bundle upgrade selection without changing installed applications."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


DIRECTORY = Path(__file__).resolve().parent


class InstallTests(unittest.TestCase):
    def run_installer(self, user=False, system=False, scope=None):
        with tempfile.TemporaryDirectory(prefix="saucefx-install-test-") as temp:
            root = Path(temp)
            bundle = root / "SauceFX download.flatpak"
            bundle.touch()
            log = root / "calls.jsonl"
            flatpak = root / "flatpak"
            flatpak.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "with open(os.environ['CALL_LOG'], 'a') as log:\n"
                "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if sys.argv[1] == 'info':\n"
                "    scope = 'TEST_USER_INSTALLED' if '--user' in sys.argv else 'TEST_SYSTEM_INSTALLED'\n"
                "    sys.exit(0 if os.environ[scope] == '1' else 1)\n"
            )
            flatpak.chmod(0o755)
            environment = dict(
                os.environ,
                PATH=str(root) + os.pathsep + os.environ["PATH"],
                CALL_LOG=str(log),
                TEST_USER_INSTALLED=str(int(user)),
                TEST_SYSTEM_INSTALLED=str(int(system)),
            )
            args = ["bash", str(DIRECTORY / "install-flatpak.sh")]
            if scope:
                args.append(scope)
            args.append(str(bundle))
            result = subprocess.run(args, env=environment, capture_output=True, text=True)
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            installs = [call for call in calls if call[0] == "install"]
            return result, installs, str(bundle)

    def test_updates_existing_user_installation(self):
        result, installs, bundle = self.run_installer(user=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(installs, [["install", "--user", "--bundle", "--or-update", bundle]])

    def test_updates_existing_system_installation(self):
        result, installs, bundle = self.run_installer(system=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(installs, [["install", "--system", "--bundle", "--or-update", bundle]])

    def test_new_installation_defaults_to_user_scope(self):
        result, installs, bundle = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(installs, [["install", "--user", "--bundle", "--or-update", bundle]])

    def test_two_installed_copies_require_an_explicit_scope(self):
        result, installs, _ = self.run_installer(user=True, system=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--user or --system", result.stderr)
        self.assertEqual(installs, [])

    def test_explicit_scope_resolves_two_installed_copies(self):
        result, installs, bundle = self.run_installer(user=True, system=True, scope="--system")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(installs, [["install", "--system", "--bundle", "--or-update", bundle]])


if __name__ == "__main__":
    unittest.main()
