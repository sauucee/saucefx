#!/usr/bin/env python3
"""Verify a signed repository with Flatpak before publishing or packaging it."""
import base64
import configparser
import os
from pathlib import Path
import subprocess
import sys
import tempfile

APP_ID = "io.github.sauucee.SauceFX"
REPO_URL = "https://sauucee.github.io/saucefx/flatpak/"


def verify(repository, public_key):
    repository = Path(repository).resolve()
    public_key = Path(public_key).resolve()
    for name in ("config", "summary", "summary.sig", "SauceFX.flatpakref", "SauceFX.flatpakrepo"):
        if not (repository / name).is_file():
            raise SystemExit(f"Signed update source is incomplete: {name}")
    for filename, section in (("SauceFX.flatpakref", "Flatpak Ref"), ("SauceFX.flatpakrepo", "Flatpak Repo")):
        descriptor = configparser.ConfigParser(interpolation=None)
        descriptor.read(repository / filename)
        values = descriptor[section]
        if values["Url"] != REPO_URL or base64.b64decode(values["GPGKey"], validate=True) != public_key.read_bytes():
            raise SystemExit("Update descriptor does not match the expected URL and public key.")
    with tempfile.TemporaryDirectory(prefix="saucefx-signature-check-") as temp:
        root = Path(temp)
        environment = dict(os.environ, FLATPAK_USER_DIR=str(root / "installation"),
                           XDG_DATA_HOME=str(root / "data"), XDG_CONFIG_HOME=str(root / "config"),
                           XDG_CACHE_HOME=str(root / "cache"))
        def flatpak(*args):
            return subprocess.run(["flatpak", *args], env=environment, check=True,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True).stdout.strip()
        flatpak("remote-add", "--user", f"--gpg-import={public_key}", "verified-source", repository.as_uri())
        # Installation without a runtime is sufficient to verify both signed
        # metadata and the actual application commit. Never launch this copy.
        flatpak("install", "--user", "--no-deps", "--no-related", "--noninteractive", "--assumeyes",
                "verified-source", f"{APP_ID}//master")
        commit = flatpak("info", "--user", "--show-commit", APP_ID)
        expected = subprocess.check_output(["ostree", f"--repo={repository}", "rev-parse",
                                            f"app/{APP_ID}/x86_64/master"], text=True).strip()
        if commit != expected:
            raise SystemExit("Verified installation differs from the published application ref.")
        print(f"Verified signed SauceFX source and application commit {commit}.")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("Usage: verify-repository.py REPOSITORY PUBLIC_KEY")
    verify(*sys.argv[1:])
