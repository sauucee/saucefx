<p align="center"><img src="assets/Images/saucefx-logo.svg" width="76" alt="SauceFX logo"></p>

# SauceFX

**Give Linux the sound it deserves — native PipeWire processing, a 31-band EQ,
live spectrum, rich effects, presets, and a slick desktop UI.**

SauceFX runs the open-source FxSound DSP engine (`DfxDsp`) natively on Linux,
routes system audio through a PipeWire virtual sink, and presents a frameless
desktop interface built with React and [Base UI](https://base-ui.com/). The JUCE
application keeps the native engine, preset library and PipeWire bridge behind
the scenes; the visible interface is rendered by WebKitGTK through JUCE's
`WebBrowserComponent`.

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
- Close-to-background, single-instance restore, and optional login autostart.
- Frameless window with a custom title bar, native window dragging and rounded
  X11 corners.

## Download and install

Ready-to-install packages are attached to tagged
[GitHub Releases](https://github.com/sauucee/saucefx/releases).

### Flatpak

```sh
flatpak install --user ./SauceFX-0.2.3-x86_64.flatpak
flatpak run io.github.sauucee.SauceFX
```

Version 0.2.1 changed the application ID to match the GitHub namespace. If
upgrading from the 0.2.0 bundle, export any custom presets you want to keep,
then remove the old ID with
`flatpak uninstall --user com.saucefx.SauceFX` before installing 0.2.1 or later.

The bundle uses the `org.gnome.Platform//51` runtime. Flatpak will offer to
install it from Flathub if it is not already present. The Flatpak has access to
PipeWire, your audio output, and the usual desktop portals; the Downloads
folder is available for preset import and export.

### Fedora RPM

Install the latest x86_64 release directly from GitHub with one command:

```sh
sudo dnf install --setopt=install_weak_deps=False \
  https://github.com/sauucee/saucefx/releases/latest/download/SauceFX-x86_64.rpm
```

Alternatively, download `SauceFX-x86_64.rpm` from the latest release and open
it in Software, or install the downloaded file with
`sudo dnf install ./SauceFX-x86_64.rpm`.

The RPM installs the app under `/usr/bin/SauceFX`, shared data under
`/usr/share/saucefx`, and a launcher and AppStream metadata for the desktop.
DNF resolves and installs all runtime dependencies automatically; users do not
need any compiler, development headers, Node.js, or JUCE. Disabling weak
dependencies avoids optional WebKit multimedia and device packages that
SauceFX does not use.

## Source build requirements

These packages are only needed when building SauceFX from source. Flatpak and
RPM users do not need to install them manually.

- Linux with PipeWire and WirePlumber, plus the PipeWire Pulse server and
  `pactl` (Fedora: `pulseaudio-utils`) for the virtual sink.
- CMake 3.22+, a C++17 compiler, `make`, and `pkg-config`.
- PipeWire and PulseAudio client development headers (`pipewire-devel` and
  `pulseaudio-libs-devel` on Fedora).
- X11, Xext, Xrandr, Xinerama, Xcursor, FreeType and Fontconfig development
  headers.
- Node.js `^20.19.0` or `>=22.12.0` together with npm.
- WebKitGTK 4.1, GTK3 and libsoup3 development headers.
- [JUCE](https://juce.com/) 9.x (not vendored).

On Fedora:

```sh
sudo dnf install cmake gcc-c++ make pkg-config pipewire-devel pulseaudio-libs-devel \
  libX11-devel libXext-devel libXinerama-devel libXrandr-devel \
  libXcursor-devel libXcomposite-devel mesa-libGL-devel \
  freetype-devel fontconfig-devel \
  nodejs npm webkit2gtk4.1-devel gtk3-devel libsoup3-devel \
  pulseaudio-utils pipewire-pulseaudio
```

## Build from source

Build the DSP engine first:

```sh
make -C src
```

Then configure and build the desktop app. The frontend is built automatically
with npm and copied next to the executable as `web/`; the first build installs
the frontend dependencies from `package-lock.json`. If JUCE is at the default
sibling path (`../deps/JUCE`), the `JUCE_DIR` argument can be omitted:

```sh
cmake -S src/gui -B src/gui/build -DJUCE_DIR=/path/to/JUCE
cmake --build src/gui/build -j
```

The realtime CLI bridge is optional and can be built with:

```sh
make -C src rt
```

## Packaging (maintainers)

The packaging builds use the existing `frontend/dist` instead of running npm
inside the sandbox. Build it once when needed:

```sh
cd frontend
npm ci
npm run build
```

### Flatpak bundle

```sh
packaging/flatpak/build-flatpak.sh
```

This writes `release/SauceFX-<version>-x86_64.flatpak`. Install it with
`flatpak install --user release/SauceFX-*.flatpak`. The manifest uses
`org.gnome.Platform//51` / `org.gnome.Sdk//51`; install those runtimes and
`flatpak-builder` (or `org.flatpak.Builder`) first. The script supports both a
host `flatpak-builder` and the Flatpak Builder app. If you install the SDK
per-user, install `org.freedesktop.Sdk//26.08` in the user installation too;
the script points `flatpak-builder` at the user runtimes when the system SDK is
not available.

### RPM

On Fedora, install `rpm-build` and the development packages listed above, then:

```sh
JUCE_DIR=/path/to/JUCE packaging/rpm/build-rpm.sh
```

The script builds the native app, stages the install tree, and writes both
`release/SauceFX-<version>-<arch>.rpm` and the stable release asset
`release/SauceFX-<arch>.rpm`. It also works inside a Fedora container. The RPM
is a binary package generated from the staged tree; it is not a source RPM.

Both package paths use `cmake --install --component Runtime`, so JUCE's
headers, CMake package files and helper tools are not included in the
application package.

Pushing a `v*` tag runs `.github/workflows/release.yml`, builds both packages,
and attaches them to the GitHub release for that tag.

## Run

```sh
./src/gui/build/saucefxgui_artefacts/SauceFX
```

The executable needs its `web/` directory. From a repository checkout the app
also finds `frontend/dist`; when copying a build elsewhere, copy the `web/`
directory next to the binary. The app searches for `assets/` and `presets/`
relative to the executable and the current working directory. Installed
packages place them in `/usr/share/saucefx` (RPM) or `/app/share/saucefx`
(Flatpak), and the build records that location automatically. Set
`SAUCEFX_ROOT` to point at the repository (or another data tree containing
`assets/` and `presets/`) when a copied binary needs an override.

Play audio into the `SauceFX` virtual sink, or use the output list in the app to
route system audio into it.

### Frontend development

Run the Vite dev server and point the native host at it:

```sh
cd frontend
npm install
npm run dev -- --host 127.0.0.1
```

```sh
SAUCEFX_WEB_DEV=http://127.0.0.1:5173 \
  ./src/gui/build/saucefxgui_artefacts/SauceFX
```

`SAUCEFX_WEB_DEV` bypasses the embedded resource provider for live reloading;
the native `saucefxHost` bridge still works. Set `SAUCEFX_WEB_LOG=1` to log
resource requests if the page does not load. Production builds always use the
`web/` assets copied by CMake.

## Layout

```text
dsp/             original DfxDsp engine sources (compiled as-is, LP64 fixes)
audiopassthru/   helper modules the engine links against
src/             SauceFX application source
  include/         Win32 stub headers + POSIX shims
  shims/           registry / misc POSIX implementations
  pwbridge/        PipeWire bridge (fxpw) + saucefxrt CLI
  harness/         diagnostic tools (WAV harness, route/EQ probes)
  gui/             JUCE desktop host, native engine/model backend and bridge
frontend/        React + Base UI interface (Vite, TypeScript)
  src/             components, bridge, styles and preset data
  dist/            production build copied to web/ by CMake (git ignored)
packaging/
  flatpak/         Flatpak manifest and bundle builder
  linux/           desktop entry and AppStream metadata
  rpm/             staged binary RPM spec and builder
presets/         factory preset library (Factsoft .fac files)
assets/          Figtree UI fonts, title-bar mark and application icon
```

The hidden JUCE panel still owns the engine and preset model. `Main.cpp`
exposes power, preset, output, effect, EQ, save/import/export, autostart,
theme and accent operations to the web UI, and pushes native state, spectrum
and toast events back over the bridge.

## Presets and theme

The gear in the title bar opens Settings: switch between the dark and soft
light themes, choose an accent swatch or a custom colour, import `.fac` presets
into `~/.local/share/saucefx/presets`, save the current sound as a named custom
preset, or export the live settings as a `.fac` file. Imported and saved
presets appear in the preset list alongside the factory library. The theme and
accent are saved in `~/.local/state/saucefx/settings.txt`.

**Save preset...** opens a name dialog. It suggests a free `<current name>
Custom` variant, adding a numeric suffix when that name is already in use.
Duplicate factory or custom names are rejected inline with **A preset with this
name already exists.** Factory `.fac` files are never modified; the native save
path also enforces a unique file name as a final safeguard. Saving adds the
preset to the user library and selects it.

The app remembers the selected preset and the current effect/EQ values across a
full quit and relaunch (including login/reboot). It loads the saved `.fac`, then
restores the live session snapshot from
`~/.local/state/saucefx/preset-session.txt`, so unsaved slider and EQ tweaks
survive without modifying the factory or imported preset file. If the saved
preset is no longer available, startup falls back to **Default**.

## Motion

Settings and save dialogs fade and lift 160 ms; preset and output menus settle
in 120 ms; toasts enter in 160 ms. The visible UI has no page-level entrance
animation, and the frameless window never fades or resizes. The system
`prefers-reduced-motion` setting collapses all of these transitions.

## Closing and background

Closing the window hides it and keeps the DSP engine and virtual sink running.
Launch SauceFX again to bring the existing window back. Use **Settings -> Quit
SauceFX** when you want to stop the engine and exit completely.

## Autostart

**Settings -> Start at login** controls the XDG autostart entry at
`~/.config/autostart/saucefx.desktop`. It is disabled by default and can be
enabled explicitly in Settings. Login then starts SauceFX with `--autostart`,
hidden in the background, so the engine and virtual sink are ready without
opening a window. Launching SauceFX normally attaches and shows that same
instance. The hidden login instance restores the same selected preset and live
effect/EQ session as a normal launch.

The last output choice is remembered: if **SauceFX** was selected before exit,
the login instance routes the default output through the virtual sink again.
The button can be turned off at any time; the entry is removed immediately.

The Flatpak build keeps this behavior by writing the host autostart entry with
`flatpak run io.github.sauucee.SauceFX --autostart`; the manifest grants access to
`~/.config/autostart` for that purpose.

## License and attribution

SauceFX is licensed under the **GNU Affero General Public License, version 3
or later** (AGPL-3.0-or-later); see [`LICENSE`](LICENSE).

The DSP engine, preset library and legacy interface artwork are derived from
the [FxSound open-source project](https://github.com/fxsound2/fxsound-app) and
remain under their original AGPL-3.0-or-later terms. The SauceFX name, new
logo and React interface are original to this project. SauceFX is not
affiliated with or endorsed by FxSound.

The React interface is built with Base UI, React, Vite and Lucide; their
licenses are compatible with AGPL-3.0-or-later and are listed in the frontend
npm dependency tree.

The native and web interfaces bundle **Figtree** under the SIL Open Font
License 1.1; see [`assets/Fonts/OFL.txt`](assets/Fonts/OFL.txt). The same
notice is included in the built web bundle as `OFL-Figtree.txt`.

---

## Marketing site (this folder)

The page in this directory is the SauceFX marketing site. It is a static
Vite + React + TypeScript build: GSAP + ScrollTrigger and Lenis for motion,
Tailwind for layout, Lucide for icons, Figtree self-hosted.

### Run

```sh
npm install
npm run dev          # http://localhost:5173
```

### Build

```sh
npm run build        # typecheck + static build into dist/
npm run preview      # serve dist/ at http://localhost:4173
```

`vite.config.ts` sets `base: './'`, so every asset is relative and `dist/` works
unchanged from a GitHub Pages project subpath or from a Vercel root.

### Deploy

- **GitHub Actions (this folder as a repository)** —
  `.github/workflows/pages.yml` builds the site on every push to `main` and
  publishes `dist/` with `actions/deploy-pages`. Push this folder as its own
  repository, then enable **Settings → Pages → Source: GitHub Actions**. If
  the site lives in a subdirectory of an existing repo instead, give the
  workflow's build steps a `working-directory` and point the artifact path at
  `<dir>/dist`.
- **GitHub Pages, branch style** — publish `dist/` with
  `peaceiris/actions-gh-pages` (`publish_dir: ./dist`) or by pushing `dist/` to
  a `gh-pages` branch. No rewrite rules are needed; the site is one HTML
  document plus hashed assets.
- **Vercel** — framework preset *Vite*, build command `npm run build`, output
  directory `dist`.

The metadata ships with the project's assumed Pages URL
(`https://sauucee.github.io/saucefx/`), which matches the `saucefx` repo
name. If the site lands on a custom domain or under another path, update
`index.html` (`canonical`, `og:url`, `og:image`, `twitter:image`),
`public/robots.txt` and `public/sitemap.xml` together. Social images are PNG
1200×630; regenerate with `npm run og` while `npm run preview` is running.

### Checks

```sh
npm run typecheck    # TypeScript, no emit
npm i -D playwright  # one-time, for the browser checks only
npm run verify -- http://localhost:4173
```

`scripts/verify.mjs` drives the built page in Chromium and asserts: the curtain
unmounts, the headline sits behind and wider than the window, no horizontal
overflow on desktop or mobile, counters land on real values, the pinned rack
advances, an EQ fader responds to a drag, presets change the curve, install
tabs and copy work, the feature rail advances, the mobile menu closes on
Escape, the theme and accent switch, reduced motion skips the curtain,
CLS < 0.02, accessible names and slider semantics, and text/accent contrast in
both themes.

Lighthouse 12 against the production build (mobile emulation, Chromium):
Performance 94, Accessibility 97, Best Practices 100, SEO 100, LCP 2.4 s,
CLS 0. The remaining accessibility finding is `target-size` on the 31 EQ
sliders: they are a continuous mixing surface, the whole plot is draggable, and
each knob keeps a larger invisible hit area plus full keyboard control.

### Where things live

```text
src/components/     one file per section, plus Preloader and AppScreenshot
src/content/site.ts every product claim and install command
src/styles/theme.css  the design tokens (colour, type, spacing, motion)
src/styles/global.css base + component CSS
src/theme/          theme + accent state
scripts/og.mjs      renders public/og-image.png from the site's own tokens
scripts/verify.mjs  browser checks for the built site
```

Design decisions and the asset note are in [`DESIGN.md`](DESIGN.md); audience
and constraints in [`PRODUCT.md`](PRODUCT.md).
