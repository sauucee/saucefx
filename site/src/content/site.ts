/**
 * Everything the site asserts about SauceFX lives here, and every string in
 * this file comes from the project README. Marketing copy elsewhere describes
 * the page, never an unverified product capability.
 */

export const REPO_URL = 'https://github.com/sauucee/saucefx';
export const RELEASES_URL = `${REPO_URL}/releases`;
export const LICENSE_URL = 'https://www.gnu.org/licenses/agpl-3.0.html';
export const VERSION = '0.2.1';

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
  { value: 2, label: 'install formats', detail: 'Flatpak and Fedora RPM' },
] as const;

export const FLATPAK_COMMANDS = [
  `flatpak install --user ./SauceFX-${VERSION}-x86_64.flatpak`,
  'flatpak run io.github.sauucee.SauceFX',
];

export const RPM_COMMANDS = [
  'sudo dnf install --setopt=install_weak_deps=False https://github.com/sauucee/saucefx/releases/latest/download/SauceFX-x86_64.rpm',
];

export const REQUIREMENTS = [
  'Linux with PipeWire and WirePlumber, plus the PipeWire Pulse server and pactl (Fedora: pulseaudio-utils) for the virtual sink.',
  'CMake 3.22+, a C++17 compiler, make, and pkg-config.',
  'PipeWire development headers (pipewire-devel on Fedora).',
  'X11, Xext, Xrandr, Xinerama, Xcursor, FreeType and Fontconfig development headers.',
  'Node.js ^20.19.0 or >=22.12.0 together with npm.',
  'WebKitGTK 4.1, GTK3 and libsoup3 development headers.',
  'JUCE 9.x (not vendored).',
];

export const FEDORA_SETUP = [
  'sudo dnf install cmake gcc-c++ make pkg-config pipewire-devel \\',
  '  libX11-devel libXext-devel libXinerama-devel libXrandr-devel \\',
  '  libXcursor-devel libXcomposite-devel mesa-libGL-devel \\',
  '  freetype-devel fontconfig-devel \\',
  '  nodejs npm webkit2gtk4.1-devel gtk3-devel libsoup3-devel \\',
  '  pulseaudio-utils pipewire-pulseaudio',
];

/** Framing straight from the README: these are source-build requirements. */
export const REQUIREMENTS_INTRO =
  'Only needed when building SauceFX from source — Flatpak and RPM packages bring every runtime dependency with them.';

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
    body: 'A custom title bar, native window dragging, and rounded corners on X11. React and Base UI render the whole interface.',
    detail: 'The visible UI is WebKitGTK through JUCE’s WebBrowserComponent.',
  },
] as const;

export const MARQUEE_WORDS = EFFECTS;
