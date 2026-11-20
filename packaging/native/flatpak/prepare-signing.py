#!/usr/bin/env python3
"""Prepare release signing, or a disposable key for unpublished CI artifacts."""
import os
from pathlib import Path
import subprocess
import tempfile


def run(*args, **kwargs):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs)


def main():
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise SystemExit("Signing preparation runs only in GitHub Actions.")
    directory = Path(tempfile.mkdtemp(prefix="saucefx-signing-", dir=os.environ.get("RUNNER_TEMP")))
    directory.chmod(0o700)
    environment = dict(os.environ, GNUPGHOME=str(directory))
    production = os.environ.get("GITHUB_REF", "").startswith("refs/tags/v")
    public = Path(__file__).resolve().parents[2] / "keys/saucefx-flatpak.gpg"
    if production:
        secret = os.environ.get("FLATPAK_SIGNING_KEY", "")
        if not secret:
            raise SystemExit("The release signing secret is missing; refusing unsigned publication.")
        run("gpg", "--batch", "--import", input=secret.encode(), env=environment)
    else:
        run("gpg", "--batch", "--pinentry-mode", "loopback", "--passphrase", "", "--quick-gen-key",
            "SauceFX unpublished CI <ci@saucefx.invalid>", "rsa2048", "sign", "1d", env=environment)
    listing = run("gpg", "--batch", "--with-colons", "--list-secret-keys", env=environment).stdout.decode()
    fingerprint = next(line.split(":")[9] for line in listing.splitlines() if line.startswith("fpr:"))
    if production:
        expected = run("gpg", "--batch", "--with-colons", "--show-keys", str(public), env=environment).stdout.decode()
        expected_fingerprint = next(line.split(":")[9] for line in expected.splitlines() if line.startswith("fpr:"))
        if fingerprint != expected_fingerprint:
            raise SystemExit("Release signing key does not match the committed public key.")
    else:
        public = directory / "ci-public.gpg"
        public.write_bytes(run("gpg", "--batch", "--export", fingerprint, env=environment).stdout)
    with open(os.environ["GITHUB_ENV"], "a") as output:
        output.write(f"GNUPGHOME={directory}\nSAUCEFX_SIGNING_KEY={fingerprint}\nSAUCEFX_PUBLIC_KEY={public}\n")
    print("Prepared production signing." if production else "Prepared disposable signing for unpublished CI artifacts.")


if __name__ == "__main__":
    main()
