#!/usr/bin/env python3
"""Deploy the signed update source from main and verify the live snapshot."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import time
import urllib.request

BASE_URL = "https://sauucee.github.io/saucefx/flatpak/"


def gh(*args):
    return subprocess.check_output(["gh", *args], text=True)


def deploy(archive):
    if os.environ.get("GITHUB_ACTIONS") != "true" or not os.environ.get("GITHUB_REF", "").startswith("refs/tags/v"):
        raise SystemExit("Update deployment runs only for tagged GitHub releases.")
    repository = os.environ["GITHUB_REPOSITORY"]
    request_id = f"{os.environ['GITHUB_RUN_ID']}-{os.environ['GITHUB_RUN_ATTEMPT']}"
    expected = {}
    with tarfile.open(archive) as bundle:
        for name in ("summary", "summary.sig", "SauceFX.flatpakref", "app-commit.txt"):
            member = next((item for item in bundle.getmembers() if item.name.removeprefix("./") == name), None)
            if member is None or not member.isfile():
                raise SystemExit(f"The signed update artifact has no {name}.")
            expected[name] = bundle.extractfile(member).read()
    commit = expected["app-commit.txt"].decode().strip()
    if len(commit) != 64 or any(char not in "0123456789abcdef" for char in commit):
        raise SystemExit("Invalid application commit in signed update artifact.")
    gh("workflow", "run", "pages.yml", "--repo", repository, "--ref", "main",
       "-f", f"request_id={request_id}", "-f", f"update_commit={commit}")
    deadline = time.monotonic() + 900
    run = None
    while time.monotonic() < deadline:
        runs = json.loads(gh("api", f"repos/{repository}/actions/workflows/pages.yml/runs?event=workflow_dispatch&per_page=30"))["workflow_runs"]
        run = next((item for item in runs if item["display_title"] == f"Deploy site and updates {request_id}"), None)
        if run and run["status"] == "completed":
            if run["conclusion"] != "success":
                raise SystemExit(f"Update-source deployment failed: {run['html_url']}")
            break
        time.sleep(10)
    else:
        raise SystemExit("Timed out waiting for update-source deployment.")
    print(f"Update-source deployment passed: {run['html_url']}")
    # Pages/CDN propagation can lag the successful deployment for a short time.
    for attempt in range(30):
        try:
            for name, data in expected.items():
                request = urllib.request.Request(BASE_URL + name + f"?release={request_id}",
                                                 headers={"Cache-Control": "no-cache"})
                with urllib.request.urlopen(request, timeout=20) as response:
                    received = response.read(len(data) + 1)
                if hashlib.sha256(received).digest() != hashlib.sha256(data).digest():
                    raise ValueError(f"Live {name} differs from the verified artifact")
            print(f"Live signed update source matches release commit {commit}.")
            return
        except (OSError, ValueError) as error:
            if attempt == 29:
                raise SystemExit(f"The deployed update source could not be verified: {error}")
            time.sleep(10)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: deploy-repository.py SIGNED_REPOSITORY.tar.gz")
    deploy(sys.argv[1])
