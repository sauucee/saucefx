# AppImage packaging

`build-appimage.sh` creates an x86_64 AppImage from an Ubuntu build environment.
Release CI uses Ubuntu 22.04 so the system C/C++ runtime baseline is not tied to
the newest Fedora release. The image includes SauceFX, JUCE, the React assets,
GTK, WebKitGTK 4.1 and its matching helper processes, audio client libraries,
PipeWire client modules, and dependency license notices.

The host still supplies:

- PipeWire, WirePlumber, and pipewire-pulse for system audio.
- X11 or XWayland, and the appropriate graphics drivers.
- A glibc 2.35-or-newer system and a compatible C++ runtime (GCC 11 or newer).
- `/usr/bin/bwrap` and `/usr/bin/xdg-dbus-proxy`, from the `bubblewrap` and
  `xdg-dbus-proxy` packages, for WebKit's browser sandbox. User namespaces must
  be available; the launcher does not disable sandboxing as a workaround.

## Build

Install the Ubuntu development/tool packages listed in the `appimage` job in
`.github/workflows/release.yml`, then:

```sh
npm --prefix frontend ci
npm --prefix frontend run build
SAUCEFX_BUILD_JOBS=1 packaging/appimage/build-appimage.sh
```

Output: `release/SauceFX-<version>-x86_64.AppImage`. Build tooling and the JUCE
archive are version-pinned and SHA256-checked. Building and compression use no
more than two workers; one worker is the default.

## Verify

```sh
python3 packaging/appimage/test-packaging.py
packaging/appimage/smoke-test.sh release/SauceFX-<version>-x86_64.AppImage
```

The smoke test uses a private PipeWire/Pulse server, D-Bus session and Xvfb
display. It launches from a relocated directory containing spaces, checks the
actual web UI's ready event and bundled WebKit mapping, restores a preset
snapshot with a stale mount path, and verifies the login entry points to the
original AppImage. It does not install a launcher or alter desktop audio.

WebKit's distro builds do not always support `WEBKIT_EXEC_PATH` overrides.
`webkit-paths.c` is a small loader shim that redirects only WebKit's private
helper processes and injected bundle to the AppDir. System library paths and
the browser sandbox rules are unchanged. Source-package information and
copyright/license records for bundled packages are included under
`usr/share/licenses/saucefx`.

The Image is a portable alternative to the RPM or Flatpak, not an additional
audio processor to run simultaneously with an installed copy.
