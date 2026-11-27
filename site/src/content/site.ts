/**
 * Everything the site asserts about SauceFX lives here, and every string in
 * this file comes from the project README. Marketing copy elsewhere describes
 * the page, never an unverified product capability.
 *
 * The release version is not hardcoded: the root Cargo.toml is inlined at build
 * time (`?raw`), so the site always reports the version the workspace ships.
 */
import workspaceManifest from '../../../Cargo.toml?raw';

export const REPO_URL = 'https://github.com/sauucee/saucefx';
export const RELEASES_URL = `${REPO_URL}/releases`;
export const RELEASES_LATEST_URL = `${REPO_URL}/releases/latest`;
export const LICENSE_URL = 'https://www.gnu.org/licenses/agpl-3.0.html';

/** `[workspace.package]` in the root Cargo.toml is the single source of truth. */
const workspacePackage = workspaceManifest
  .split(/^\[/m)
  .find((section) => section.startsWith('workspace.package]'));

const versionMatch = workspacePackage?.match(/^version\s*=\s*"([^"]+)"/m);

if (!versionMatch) {
  throw new Error('SauceFX site: could not read the workspace version from Cargo.toml.');
}

export const VERSION = versionMatch[1];

export const EFFECTS = [
  'Clarity',
  'Ambience',
  'Surround Sound',
  'Dynamic Boost',
  'Bass Boost',
] as const;

export type Effect = (typeof EFFECTS)[number];

/** One line per fader. Describes the control, never a DSP claim. */
export const EFFECT_NOTES: Record<Effect, string> = {
  Clarity: 'Top of the rack. Nudged first when a mix feels fogged in.',
  Ambience: 'Room around the sound, from dry desk to wide hall.',
  'Surround Sound': 'Pushes the image outward, past the two speakers.',
  'Dynamic Boost': 'Lifts the quiet parts without shoving the loud ones.',
  'Bass Boost': 'Weight under everything. The last fader on the right.',
};

export const STATS = [
  { value: 5, label: 'effects in the rack', detail: 'Clarity to Bass Boost' },
  { value: 31, label: 'EQ bands', detail: 'graphic, with live spectrum' },
  { value: 6, label: 'accent presets', detail: 'or a custom colour' },
  { value: 5, label: 'install formats', detail: 'AppImage, Flatpak, Arch, RPM or DEB' },
] as const;

export const FLATPAK_COMMANDS = [
  'flatpak install --user --from https://sauucee.github.io/saucefx/flatpak/SauceFX.flatpakref',
  'flatpak run io.github.sauucee.SauceFX',
];

/** CI builds the native RPM from this workspace, so the name follows VERSION. */
export const RPM_COMMANDS = [`sudo dnf install ./saucefx-${VERSION}-1.fc42.x86_64.rpm`];

export const REQUIREMENTS = [
  'Linux with PipeWire and WirePlumber, plus the PipeWire Pulse server and pactl (Fedora: pulseaudio-utils) for the virtual sink.',
  'Rust (stable, via rustup), make, a C++17 compiler, pkg-config and ninja.',
  'Clang/LLVM with libclang — Skia generates its bindings at build time.',
  'PipeWire and PulseAudio client development headers.',
  'FreeType, Fontconfig, libxkbcommon (with X11 support), OpenGL and EGL development headers.',
];

export const FEDORA_SETUP = [
  'sudo dnf install make gcc-c++ clang llvm-devel pkgconf-pkg-config ninja-build \\',
  '  pipewire-devel pulseaudio-libs-devel freetype-devel fontconfig-devel \\',
  '  libxkbcommon-devel libxkbcommon-x11-devel mesa-libGL-devel mesa-libEGL-devel \\',
  '  pulseaudio-utils pipewire-pulseaudio',
];

/** Framing straight from the README: these are source-build requirements. */
export const REQUIREMENTS_INTRO =
  'Only needed when building SauceFX from source — packaged installs skip this step. Either way, SauceFX needs a working PipeWire session (PipeWire and WirePlumber) at runtime.';

export const FEATURES = [
  {
    title: 'Presets',
    body: 'A factory library, plus Save preset… to keep the sound you just made. Import and export plain .fac files.',
    detail: 'Saved presets land beside the factory library and are selected immediately.',
  },
  {
    title: 'PipeWire virtual sink',
    body: 'A native virtual sink with automatic output routing. Play audio into SauceFX, or pick it from the output list in the app.',
    detail: 'Built on the native PipeWire bridge, not a shell script around it.',
  },
  {
    title: 'Session restore',
    body: 'The preset, the effect values and the whole EQ curve come back after a full quit and relaunch — unsaved tweaks included.',
    detail: 'Live state is restored from a session snapshot, so preset files are never rewritten.',
  },
  {
    title: 'Background mode and autostart',
    body: 'Closing the window hides it and keeps the engine running. Settings → Start at login is on by default.',
    detail: 'Autostart launches hidden with --autostart; a later launch attaches to that instance.',
  },
  {
    title: 'Frameless window',
    body: 'A custom title bar and native window dragging. The window, controls and text are rendered natively with Slint and Skia — no browser, no WebKit, no JavaScript.',
    detail:
      'Crisp native text: fractional glyph positioning with slight hinting, verified against the original interface.',
  },
] as const;

export const MARQUEE_WORDS = EFFECTS;
