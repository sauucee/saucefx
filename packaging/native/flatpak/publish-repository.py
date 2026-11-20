#!/usr/bin/env python3
"""Publish a signed Flatpak repository from a verified release build."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile


def git(directory, *args, capture=False):
    return subprocess.run(["git", "-C", str(directory), *args], check=True,
                          stdout=subprocess.PIPE if capture else None, text=True)


def publish(archive):
    if os.environ.get("GITHUB_ACTIONS") != "true" or not os.environ.get("GITHUB_REF", "").startswith("refs/tags/v"):
        raise SystemExit("Repository publication is allowed only for tagged GitHub releases.")
    repository = os.environ["GITHUB_REPOSITORY"]
    with tempfile.TemporaryDirectory(prefix="saucefx-update-publication-") as temp:
        root = Path(temp)
        extracted = root / "incoming"
        extracted.mkdir()
        with tarfile.open(archive) as bundle:
            bundle.extractall(extracted, filter="data")
        for name in ("config", "summary", "summary.sig", "SauceFX.flatpakref", "SauceFX.flatpakrepo"):
            if not (extracted / name).is_file():
                raise SystemExit(f"Signed repository is incomplete: {name}")
        target = root / "repository"
        target.mkdir()
        git(target, "init", "-b", "flatpak-repository")
        git(target, "remote", "add", "origin", f"https://github.com/{repository}.git")
        git(target, "config", "user.name", "SauceFX release automation")
        git(target, "config", "user.email", "sauucee@users.noreply.github.com")
        result = subprocess.run(["git", "-C", str(target), "ls-remote", "--exit-code", "origin",
                                 "refs/heads/flatpak-repository"], capture_output=True, text=True)
        if result.returncode == 0:
            git(target, "fetch", "--depth=1", "origin", "flatpak-repository")
            git(target, "checkout", "-B", "flatpak-repository", "FETCH_HEAD")
        elif result.returncode != 2:
            raise SystemExit("Could not inspect the existing update repository branch.")
        # Preserve old objects so clients can finish downloads across a release.
        # Replace current refs and signed summaries with the new tested snapshot.
        for child in extracted.iterdir():
            if child.name in (".git", "tmp", "cache", "state", "lock"):
                continue
            destination = target / child.name
            if child.is_dir():
                shutil.copytree(child, destination, dirs_exist_ok=True)
            else:
                shutil.copy2(child, destination)
        git(target, "add", "--all")
        if subprocess.run(["git", "-C", str(target), "diff", "--cached", "--quiet"]).returncode == 0:
            print("Update repository already contains this release.")
            return
        git(target, "commit", "-m", f"Publish signed Flatpak {os.environ['GITHUB_REF_NAME']}")
        git(target, "push", "origin", "HEAD:refs/heads/flatpak-repository")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: publish-repository.py SIGNED_REPOSITORY.tar.gz")
    publish(sys.argv[1])
