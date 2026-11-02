#!/usr/bin/env python3
"""Small packaging tests; no compiler, graphics session or audio server needed."""
import importlib.util
import os
import subprocess
import tempfile
from pathlib import Path

directory = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("relocate_webkit", directory / "relocate-webkit.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
with tempfile.TemporaryDirectory(prefix="saucefx-appimage-") as temp:
    library = Path(temp) / "libwebkit.so"
    original = b"\x7fELF\x00/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1\x00/usr/share/locale\x00/usr/bin/bwrap\x00/usr/bin/xdg-dbus-proxy\x00"
    library.write_bytes(original)
    assert module.relocate(library) == 2
    result = library.read_bytes()
    assert len(result) == len(original)
    assert b"/usr/bin/bwrap" in result and b"/usr/bin/xdg-dbus-proxy" in result
    assert b"././/lib/x86_64-linux-gnu/webkit2gtk-4.1" in result
    assert module.relocate(library) == 0
    appdir = Path(temp) / "Portable App"
    (appdir / "usr/bin").mkdir(parents=True)
    executable = appdir / "usr/bin/SauceFX"
    executable.write_text('#!/usr/bin/env bash\nprintf "%s\\n" "$APPDIR" "$SAUCEFX_ROOT" "$PWD" "$1"\n')
    executable.chmod(0o755)
    environment = dict(os.environ, APPDIR=str(appdir))
    environment.pop("XDG_DATA_DIRS", None)
    if Path("/usr/bin/bwrap").is_file() and Path("/usr/bin/xdg-dbus-proxy").is_file():
        result = subprocess.check_output(
            ["bash", str(directory / "AppRun"), "argument with spaces"],
            env=environment, cwd=temp, text=True,
        ).splitlines()
        assert result == [str(appdir), str(appdir / "usr/share/saucefx"), str(appdir / "usr"), "argument with spaces"]
        print("PASS: AppRun relocates data and preserves paths and arguments with spaces")
print("PASS: WebKit resource relocation preserves ELF size and sandbox helpers")
