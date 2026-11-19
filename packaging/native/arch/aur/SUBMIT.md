# Submitting saucefx to the AUR

The package name `saucefx` is unclaimed on the AUR (checked via the RPC API
on 2026-11-19). Submission must be done by the maintainer personally, from
their own AUR account — the files here are ready to copy.

## One-time setup

1. Create an account at https://aur.archlinux.org (if you do not have one).
2. Add your SSH public key under "My Account" → SSH Pubkey.

## First submission

```sh
git clone ssh://aur@aur.archlinux.org/saucefx.git
cd saucefx
cp /path/to/saucefx/packaging/native/arch/aur/PKGBUILD .
cp /path/to/saucefx/packaging/native/arch/aur/.SRCINFO .
# Optional but recommended on an Arch machine: test the build
makepkg -si
git add PKGBUILD .SRCINFO
git commit -m "saucefx 0.3.0"
git push
```

The recipe is the same one GitHub CI builds and smoke-tests in an Arch
container on every release, so the build itself is proven; `makepkg -si`
on a real Arch install is the final end-to-end check.

## Updating for a new release

1. Bump `pkgver` (and reset `pkgrel=1`) in `PKGBUILD`.
2. Refresh the checksum: `makepkg -g` (or download the new tag tarball and
   `sha256sum` it).
3. Regenerate `.SRCINFO`: `makepkg --printsrcinfo > .SRCINFO`.
4. Commit and push both files.

## Notes

- This is a source package: it compiles the DSP engine and the Rust/Slint
  host from the tagged release tarball. A `saucefx-bin` variant repackaging
  the release `.pkg.tar.zst` is possible later if there is demand.
- The AUR has no Flathub-style restriction on AI-assisted packaging, but
  review the files yourself before submitting — they are published under
  your account and you are the maintainer of record.