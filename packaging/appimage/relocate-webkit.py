#!/usr/bin/env python3
"""Relocate WebKit's resource/libexec prefix without changing its sandbox flags.

The distro WebKit ignores WEBKIT_EXEC_PATH in non-developer builds. Equal-length
prefix edits are the same technique used by Tauri's AppImage bundler. Limit the
edits to library/resource paths; /usr/bin/bwrap and xdg-dbus-proxy stay untouched.
AppRun launches from AppDir/usr, and LD_LIBRARY_PATH exposes the bundled paths
to WebKit's sandbox as well.
"""
import mmap
import sys
from pathlib import Path


def relocate(path: Path) -> int:
    with path.open("r+b") as stream, mmap.mmap(stream.fileno(), 0) as image:
        if image[:4] != b"\x7fELF":
            raise ValueError(f"not an ELF library: {path}")
        changed = 0
        for prefix in (b"/usr/lib", b"/usr/share"):
            offset = 0
            while (position := image.find(prefix, offset)) != -1:
                image[position:position + 4] = b"././"
                changed += 1
                offset = position + len(prefix)
        image.flush()
        return changed


if __name__ == "__main__":
    for argument in sys.argv[1:]:
        path = Path(argument)
        print(f"Relocated {relocate(path)} WebKit resource paths in {path.name}")
