#!/usr/bin/env python3
"""Include distro copyright/source records for every bundled ELF dependency."""
import subprocess
import sys
from pathlib import Path

appdir = Path(sys.argv[1])
packages = set()
for library in (appdir / "usr").rglob("*"):
    if not library.is_file() or library.is_symlink():
        continue
    with library.open("rb") as stream:
        if stream.read(4) != b"\x7fELF":
            continue
    owner = subprocess.run(
        ["dpkg-query", "-S", f"*/{library.name}"], text=True, capture_output=True,
    )
    for row in owner.stdout.splitlines():
        if ": " in row:
            packages.update(name.strip() for name in row.rsplit(": ", 1)[0].split(","))

destination = appdir / "usr/share/licenses/saucefx/dependencies"
destination.mkdir(parents=True, exist_ok=True)
records = []
for package in sorted(packages):
    name = package.split(":", 1)[0]
    copyright_file = Path("/usr/share/doc") / name / "copyright"
    if not copyright_file.is_file():
        raise SystemExit(f"Missing dependency copyright file: {package}")
    (destination / f"{name}.copyright").write_bytes(copyright_file.read_bytes())
    records.append(subprocess.check_output(
        ["dpkg-query", "-W", "-f=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n", package],
        text=True,
    ))

# Distro copyright files often refer to these complete license texts.
common = Path("/usr/share/common-licenses")
for path in common.iterdir():
    if path.is_file():
        (destination / path.name).write_bytes(path.read_bytes())
(destination / "packages.tsv").write_text("".join(records))
