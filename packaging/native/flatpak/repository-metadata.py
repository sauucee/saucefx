#!/usr/bin/env python3
"""Write install descriptors with the public key used to sign this repository."""
import base64
from pathlib import Path
import sys
import subprocess


def metadata(repository, keyfile, ref_output):
    values = {}
    for line in (Path(__file__).with_name("repository.env")).read_text().splitlines():
        if line and not line.startswith("#"):
            key, value = line.split("=", 1)
            values[key] = value
    encoded = base64.b64encode(Path(keyfile).read_bytes()).decode("ascii")
    repo = Path(repository)
    repo.mkdir(parents=True, exist_ok=True)
    descriptor = (
        "[Flatpak Repo]\nTitle=SauceFX\n"
        f"Url={values['UPDATE_REPO_URL']}\nDefaultBranch={values['BRANCH']}\n"
        f"GPGKey={encoded}\n"
    )
    (repo / "SauceFX.flatpakrepo").write_text(descriptor)
    ref = (
        "[Flatpak Ref]\nVersion=1\nTitle=SauceFX\n"
        f"Name={values['APP_ID']}\nBranch={values['BRANCH']}\nIsRuntime=false\n"
        f"Url={values['UPDATE_REPO_URL']}\nSuggestRemoteName={values['REMOTE_NAME']}\n"
        f"RuntimeRepo={values['RUNTIME_REPO_URL']}\nGPGKey={encoded}\n"
    )
    Path(ref_output).write_text(ref)
    (repo / "SauceFX.flatpakref").write_text(ref)
    commit = subprocess.check_output([
        "ostree", f"--repo={repo}", "rev-parse", f"app/{values['APP_ID']}/x86_64/{values['BRANCH']}"
    ], text=True).strip()
    (repo / "app-commit.txt").write_text(commit + "\n")
    # GitHub Pages must serve OSTree's dotfiles and object tree verbatim.
    (repo / ".nojekyll").touch()


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("Usage: repository-metadata.py REPOSITORY PUBLIC_KEY REF_OUTPUT")
    metadata(*sys.argv[1:])
