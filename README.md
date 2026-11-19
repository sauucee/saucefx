<p align="center"><img src="assets/Images/saucefx-logo.svg" width="76" alt="SauceFX logo"></p>

# SauceFX

**Give Linux the sound it deserves — native PipeWire processing, a 31-band EQ,
live spectrum, rich effects, presets, and a fully native desktop UI.**

SauceFX runs the open-source FxSound DSP engine (`DfxDsp`) natively on Linux,
routes system audio through a PipeWire virtual sink, and presents a frameless
desktop interface built with [Slint](https://slint.dev/) and Rust. There is no
browser, no WebKit and no JavaScript in the app: the window, controls and text
are rendered natively (Skia), while the same proven DSP engine, preset library
and PipeWire bridge from the original release run behind the scenes.

SauceFX is an independent Linux application based on the FxSound project. The
upstream Windows application, installer and build tooling are not included. The
FxSound DSP engine and interface artwork/design retain their upstream
attribution.

## Features

- 5-effect rack: Clarity, Ambience, Surround Sound, Dynamic Boost, Bass Boost.
- 31-band graphic EQ with a live spectrum visualizer fed by the native engine.
- Factory preset library plus user **Save preset...**, import and export.
- Dark and soft light themes with six accent presets or a custom colour.
- Native PipeWire virtual sink with automatic output routing.
- Session restore for preset, effect values and EQ across restarts.
- Close-to-background, single-instance restore, and login autostart (on by
  default; toggle it in Settings).
- Frameless window with a custom title bar and native window dragging.
- Crisp native text: fractional glyph positioning with slight hinting,
  verified against the original interface.

## Download and install

Ready-to-install packages are attached to tagged
[GitHub Releases](https://github.com/sauucee/saucefx/releases). Every format
is built and smoke-tested on GitHub CI (`.github/workflows/native-packages.yml`).

### AppImage (any distro)

Download `SauceFX-<version>-x86_64.AppImage`, make it executable, and run it:

```sh
chmod +x SauceFX-*-x86_64.AppImage
./SauceFX-*-x86_64.AppImage
```

The image bundles the app, presets and the version-matched PipeWire client
libraries. It uses the host's PipeWire/WirePlumber audio services and graphics
drivers. Requires glibc 2.35+ (Ubuntu 22.04+, Debian 12+, Fedora 36+, Arch,
openSUSE and derivatives). If FUSE mounting is unavailable, use
`--appimage-extract-and-run`. Keep the file in a permanent location before
enabling login startup.

### Flatpak (any distro, sandboxed)

Download the bundle and `install-flatpak.sh` from the same release, then run:

```sh
bash ./install-flatpak.sh ./SauceFX-<version>-x86_64.flatpak
flatpak run io.github.sauucee.SauceFX
```

The installer updates the existing user or system installation in place. New
installations default to your user account. If both copies are installed, pass
`--user` or `--system` before the bundle path to choose which one to update.
Quit SauceFX from Settings and reopen it after upgrading to run the new version;
closing its window only hides it. Your presets and settings are kept.

Some graphical installers show a downloaded bundle as already installed instead
of offering an upgrade. Use the command above; uninstalling the old version is
not required. Without the helper, use `flatpak install --user --bundle --or-update`
followed by the bundle path (replace `--user` with `--system` if needed). An
identical bundle can report “already installed”; that means it is already current.

The bundle uses the `org.gnome.Platform//51` runtime; Flatpak offers to install
it from Flathub if missing. The sandbox has access to PipeWire, your audio
output, and the Downloads folder for preset import/export. Bundles have no
online SauceFX update repository: download a newer bundle to upgrade. The
application ID remains the same as the 0.2.x releases.

### Fedora / openSUSE (RPM)

```sh
sudo dnf install ./saucefx-<version>-1.fc42.x86_64.rpm
```

DNF resolves all runtime dependencies automatically. The RPM installs the app
under `/usr/bin/SauceFX`, shared data under `/usr/share/saucefx`, and a launcher
plus AppStream metadata for the desktop.

### Debian / Ubuntu / Mint / Pop!_OS (DEB)

```sh
sudo apt install ./saucefx_<version>_amd64.deb
```

APT resolves all runtime dependencies automatically. Built on Ubuntu 22.04, so
it runs on Debian 12+ and any newer Ubuntu-family release.

### Arch / Manjaro / EndeavourOS (pacman)

```sh
sudo pacman -U ./saucefx-<version>-1-x86_64.pkg.tar.zst
```

The package is compiled from source against Arch's current libraries. An AUR
package may follow later.

## Source build requirements

These are only needed when building from source; package users install nothing
extra.

- Linux with PipeWire and WirePlumber, plus the PipeWire Pulse server and
  `pactl` (Fedora: `pulseaudio-utils`) for the virtual sink.
- Rust (stable, via [rustup](https://rustup.rs/)), `make`, a C++17 compiler,
  `pkg-config` and `ninja`.
- Clang/LLVM with `libclang` (Skia's bindings generate code at build time).
- PipeWire and PulseAudio client development headers.
- FreeType, Fontconfig, libxkbcommon (with X11 support), OpenGL and EGL
  development headers.

On Fedora:

```sh
sudo dnf install make gcc-c++ clang llvm-devel pkgconf-pkg-config ninja-build \
  pipewire-devel pulseaudio-libs-devel freetype-devel fontconfig-devel \
  libxkbcommon-devel libxkbcommon-x11-devel mesa-libGL-devel mesa-libEGL-devel \
  pulseaudio-utils pipewire-pulseaudio
```

On Debian/Ubuntu:

```sh
sudo apt install build-essential clang libclang-dev pkg-config ninja-build \
  libpipewire-0.3-dev libpulse-dev libfreetype-dev libfontconfig1-dev \
  libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev libegl1-mesa-dev \
  pulseaudio-utils pipewire-pulse
```

On Arch:

```sh
sudo pacman -S --needed base-devel rust clang ninja pipewire libpulse \
  fontconfig freetype2 libxkbcommon libglvnd
```

No JUCE, Node.js, npm or WebKit is required.

## Build from source

Build the DSP engine archives first, then the Rust host with the Skia
renderer (the same configuration every package ships):

```sh
make -C src rt
cargo build --release -p saucefx-host --no-default-features --features renderer-skia
```

Run it:

```sh
./target/release/saucefx
```

The default `cargo build` feature set uses the femtovg renderer, kept only as
a lightweight fallback; release builds always use Skia.

Tests and formatting:

```sh
cargo test --workspace -- --test-threads=1
cargo fmt --all -- --check
```

The workspace covers settings/session persistence, preset catalog and
save/import/export logic, the engine binding (including first-launch Default
loading), single-instance and autostart behaviour. The realtime CLI bridge
(`src/build/bin/saucefxrt`) can still be built with `make -C src rt` for
headless engine work; `make -C src -j2 test` runs the native C++ regressions
with a private PipeWire session that never touches the desktop's audio routing.

## Packaging (maintainers)

All five formats build on GitHub CI on every push to the development branch
and on release tags:

| Format | Job | Script |
| --- | --- | --- |
| AppImage | ubuntu-22.04 + LLVM 18 | `packaging/native/appimage/build-appimage.sh` |
| Flatpak | runner-built host + GNOME 51 sandbox | `packaging/native/flatpak/build-flatpak.sh` |
| Arch | `archlinux:base-devel` container, makepkg | `packaging/native/arch/build-pkg.sh` |
| RPM | `fedora:42` container, rpmbuild | `packaging/native/rpm/build-rpm.sh` |
| DEB | ubuntu-22.04, dpkg-deb | `packaging/native/deb/build-deb.sh` |

Each job installs or extracts its artifact and smoke-tests it before upload:
the AppImage test relocates the image, restores a session and pushes audio
through the DSP into the default output; the Flatpak test launches inside the
sandbox and checks the SauceFX sink appears; the native packages verify
linkage, the desktop entry and the factory presets.

The Flatpak manifest installs a runner-built host because the GNOME 51 SDK
ships no clang; zenity is built from source inside the sandbox so the
import/export choosers work. The legacy JUCE packaging under `packaging/`
(appimage/flatpak/rpm) remains for the 0.2.x release line.

## Run

Installed packages launch **SauceFX** from the desktop menu. From a source
build, run `./target/release/saucefx`. The app finds `presets/` and `assets/`
relative to the executable and the working directory; installed packages place
them in `/usr/share/saucefx` (RPM/DEB/pacman) or `/app/share/saucefx` (Flatpak).
Set `SAUCEFX_ROOT` to point at a repository checkout when a copied binary needs
an override.

Play audio into the `SauceFX` virtual sink, or use the output list in the app to
route system audio into it.

### Audio outputs and headsets

The **Output** menu lists the devices and ports reported by PipeWire, including
**Speakers**, wired **Headphones**, USB audio interfaces and connected Bluetooth
headsets. Wired headphones commonly share the built-in sound card's sink and
appear as a separate port. Unavailable ports are marked **Not connected**.

- **Automatic** processes system audio and follows physical output changes,
  including newly connected USB/Bluetooth devices.
- Selecting a named device processes audio through that output and remembers
  the choice. If it disconnects, SauceFX temporarily uses another available
  output and returns to the selected device when it reconnects.
- **Direct playback**, under **Without effects**, uses the system output
  without SauceFX effects. The power switch can also bypass the effects while
  keeping the selected routing.

Device changes refresh the menu without restarting the app or resetting EQ and
effects. Pair and connect wireless headsets in the desktop's Bluetooth settings
first; SauceFX lists outputs provided by Linux rather than pairing devices.

## Presets and theme

The gear in the title bar opens Settings: switch between the dark and soft
light themes, choose an accent swatch or a custom colour, import `.fac` presets
into `~/.local/share/saucefx/presets`, save the current sound as a named custom
preset, or export the live settings as a `.fac` file. Imported and saved
presets appear in the preset list alongside the factory library. The theme and
accent are saved in `~/.local/state/saucefx/settings.txt`.

**Save preset...** opens a name dialog. It suggests a free `<current name>
Custom` variant, adding a numeric suffix when that name is already in use.
Duplicate factory or custom names are rejected inline. Factory `.fac` files are
never modified. Saving adds the preset to the user library and selects it.

The app remembers the selected preset and the current effect/EQ values across a
full quit and relaunch (including login/reboot). It loads the saved `.fac`, then
restores the live session snapshot from
`~/.local/state/saucefx/preset-session.txt`, so unsaved slider and EQ tweaks
survive without modifying the factory or imported preset file. A first launch
with no session loads the **Default** preset, matching the original app.

## Closing and background

Closing the window hides it and keeps the DSP engine and virtual sink running.
Launch SauceFX again to bring the existing window back (a second launch
requests the hidden window instead of starting a new instance). Use
**Settings -> Quit SauceFX** when you want to stop the engine and exit
completely.

## Autostart

**Settings -> Start at login** controls the XDG autostart entry at
`~/.config/autostart/saucefx.desktop`. It is **enabled by default**; turning it
off removes the entry immediately. Login starts SauceFX with `--autostart`,
hidden in the background, so the engine and virtual sink are ready without
opening a window. Launching SauceFX normally attaches and shows that same
instance. The hidden login instance restores the same selected preset and live
effect/EQ session as a normal launch, and re-applies the remembered output
routing.

The Flatpak build writes the host autostart entry with
`flatpak run io.github.sauucee.SauceFX --autostart`; the manifest grants access
to `~/.config/autostart` for that purpose. The AppImage records its own path, so
keep the file in a permanent location.

## Layout

```text
crates/
  saucefx-core/    settings, session, preset catalog and save/import/export
  saucefx-ui/      Slint interface (app window, settings, dialogs) + binding
  saucefx-host/    native window host: engine FFI, PipeWire, autostart,
                   single instance, tray-less background lifecycle
third-party/
  i-slint-renderer-skia/  vendored Slint 1.18.1 Skia renderer with the
                   verified text-rendering defaults
dsp/               original DfxDsp engine sources (compiled as-is, LP64 fixes)
audiopassthru/     helper modules the engine links against
src/
  include/         Win32 stub headers + POSIX shims
  shims/           registry / misc POSIX implementations
  pwbridge/        PipeWire bridge (fxpw) + saucefxrt CLI
  harness/         diagnostic tools (WAV harness, route/EQ probes)
  gui/             legacy JUCE desktop host (0.2.x release line)
frontend/          legacy React interface (0.2.x release line)
packaging/
  native/          appimage/ flatpak/ arch/ rpm/ deb/ builders + smoke tests
  linux/           desktop entry and AppStream metadata
  appimage/ flatpak/ rpm/  legacy JUCE packaging (0.2.x)
presets/           factory preset library (Factsoft .fac files)
assets/            Figtree UI fonts, title-bar mark and application icon
```

The Rust host owns the engine and preset model directly: `saucefx-core`
implements the settings/session/preset logic, `saucefx-ui` binds it to the
Slint interface, and `saucefx-host` links the same `fxpw` PipeWire bridge and
DSP archives the released app uses. The audio path is byte-for-byte the
released 0.2.4 engine — the migration replaces the interface, not the sound.

## License and attribution

SauceFX is licensed under the **GNU Affero General Public License, version 3
or later** (AGPL-3.0-or-later); see [`LICENSE`](LICENSE).

The DSP engine, preset library and legacy interface artwork are derived from
the [FxSound open-source project](https://github.com/fxsound2/fxsound-app) and
remain under their original AGPL-3.0-or-later terms. The SauceFX name, new
logo and interface design are original to this project. SauceFX is not
affiliated with or endorsed by FxSound.

The native interface is built with **Slint** (GPLv3, compatible with
AGPL-3.0-or-later), **Skia** (BSD-style) and **winit**; the legacy web
interface used React, Base UI, Vite and Lucide. Their licenses are compatible
with AGPL-3.0-or-later.

The interface bundles **Figtree** under the SIL Open Font License 1.1; see
[`assets/Fonts/OFL.txt`](assets/Fonts/OFL.txt).