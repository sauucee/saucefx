# SauceFX — FxSound DSP integration and engineering notes

SauceFX is a Linux sound EQ and audio-enhancement app built on the FxSound DSP
engine (the core of `DfxDsp.dll`). This document records the engine's Linux
integration with GCC, the headless WAV → `DfxDsp::processAudio()` → WAV harness
used for correctness verification, and the PipeWire/desktop-app architecture.

## Layout

```
src/
  Makefile            builds everything; run as: make -C src
  sources.txt         the 121-file source list (the DSP engine plus the
                      audiopassthru helper modules it links against)
  include/            Win32 header shims (windows.h, io.h, mmsystem.h, ...)
  shims/              POSIX implementations of the Win32-only layers
  harness/fxdspharness.cpp   headless WAV → DSP → WAV test harness
  harness/probe.cpp          minimal float-path probe (kept as a tool)
  build/              build products: libdfxdsp_linux.a, bin/fxdspharness
```

## Build / usage

```sh
make -C src                                  # build
./src/build/bin/fxdspharness in.wav out.wav [preset.fac] # run
```

Preprocessor state mirrors the MSVC project's software-DSP target:
`-DDSPSOFT_TARGET -DPT_DSP_BUILD=PT_DSP_DFX -DPT_NON_MFC -DNDEBUG`
(`WIN32` is deliberately **not** defined, so the code's existing
`#ifndef WIN32` / `platform.h` POSIX paths are active). Also:
`-funsigned-char` (MSVC default char signedness), `-fno-strict-aliasing`
(legacy C predates aliasing rules), `-MMD` deps.
The 94-file DSP set plus audiopassthru helpers (pstr/MTH/FILE/SLOUT) and the
shims compile as a static library; harness and probe link against it.

## Shim inventory

`src/include/` — header-only shims providing the Win32 API surface the
original sources include: `windows.h` (holds `_stat64i32` struct,
`_wcserror`, `Sleep`, `_wtol`, wide-path helpers, `wsprintf`), `io.h`,
`direct.h`, `share.h`, `conio.h`, `crtdbg.h`, `dos.h`, `mmsystem.h`
(`PWAV` → no-op `wav*` stubs), `winbase.h`, `wtypes.h`, `Shellapi.h`,
`shlobj.h`, `Shlwapi.h`.

`src/shims/`:
- `reg_linux.cpp` — the entire `reg*` API from `audiopassthru/src/reg/`
  backed by a flat file (`$FXSOUND_REG_FILE`, else
  `$XDG_STATE_HOME/saucefx/reg.txt`). The harness `mkstemp`s a fresh file
  per run and `setenv("FXSOUND_REG_FILE", ...)`, which makes every run
  byte-deterministic (previously state leaked across runs at
  `$XDG_STATE_HOME`).
- `misc_linux.cpp` — `operatingSystem`, `ptime`, `FILE/fileWin32*`, most of
  `MRY`, and the `fgetws` shim. `fgetws` is a narrow read + `mbstowcs`
  because glibc forbids mixing wide/narrow ops on one `FILE*`; without it,
  `.fac` preset loads came back garbage.
- Other notes: `stat` shim fills a locally defined `_stat64i32` struct with
  explicit-width members; wide-path helpers convert via `wcstombs`.

## Source-tree changes (in-tree, kept minimal)

1. **32-bit `long` in DSP parameter structs** — the engine's parameters are
   exchanged through a float array overlaid by structs whose `long` members
   must be 4 bytes (they were written as floats via `comIntWrite` /
   `comSftwrWriteParam`). Windows is LLP64 (`long` = 32 bits even in x64
   builds); Linux is LP64 (`long` = 64 bits), which shifted *every* struct
   member into the wrong parameter slot. The symptom: the aural engine read
   `a0=1.00322` (a stale unrelated value) instead of `-0.704778`, making its
   lowpass biquad unstable → state blew up to ±inf → `sin(-inf) = NaN` →
   poisoned everything downstream ("silence" in the int path).
   Fix: in the five "must occupy same 32 word locations" parameter structs —
   `dsp/ptutil/include/c_aural.h c_lex.h c_max.h c_play.h c_wid.h` — all
   `long` / `unsigned long` field declarations are now `int` /
   `unsigned int` (identical size/semantics on both LLP64 and LP64).
2. **`%s` → `%ls` in wide printf formats (`comSftwr`-independent, the
   original MSVCism)**: `swprintf(buf, L"%s", wide_string)` — under MSVC,
   `%s` inside a wide format means *wide* argument; under glibc it means
   *narrow*, so `L"valFidelity"` printed as `v` (bytes until the first zero).
   Every dfxp session-registry key collapsed to its first letter, all five
   effects' knob values shared one key, and the last writer won at every
   re-communicate (presets barely affected the output). 111 occurrences of
   `L"..."%s...` → `L"..."%ls...` across `dsp/` and `audiopassthru/`
   (`wcsftime` format strings left untouched; narrow `%s` formats don't
   apply since args are wide).
3. Misc: a couple of missing `#include <stdio.h>`/`<stdlib.h>` in shimmed
   paths (kept in-tree, minimal).

## Harness

`fxdspharness.cpp` mirrors the real app's audio and control paths:

- Audio: float 32-bit stereo, processed **in place**
  (`processAudio((short*)fp, (short*)fp, nss, dup_check)`), 1024-sample
  sets — same as `AudioPassthruPrivate.cpp:531-551` (`setSignalFormat(32,
  nch, srate, 32)` per capture iteration).
- Control: mirrors `FxController::selectPreset` — `loadPreset(path)` →
  `setEffectValue(e, getEffectValue(e)*10)` for every effect →
  `setEqBandFrequency/BoostCut` per band. No `isPowerOn` gate (its
  inverted semantics: true = bypassed; the shipped GUI never calls it).

DSP harness verification (5 s stereo 44.1 kHz input, RMS 5968):

| case            | out RMS | corr vs none |
|-----------------|---------|--------------|
| none (defaults) | 6876    | —            |
| Jazz            | 9546    | 0.703        |
| Classic Rock    | 7320    | 0.985        |
| extreme (maxed .fac) | 8235 | 0.899      |

Preset button/knob states verified at the engine (`Play32` trace):
none = `act=1 amb=1 wid=1 bass=1`, Jazz = `amb=0`, Classic Rock = `wid=0`.
Deterministic (byte-identical output across runs, md5-checked). Zero NaN/inf
in output; 1 kHz sine passes through at the expected level with low-level
harmonics added by the exciter.

## Known remaining quirks

- `getInfo`-style calls report "31 EQ bands" even for 10-band resource
  presets (resource `.fac` parsing edge; harmless for processing).
- `master_gain` (dsp param idx 12) and `DSP_SAMPLING_FREQ` (idx 3) are never
  written by host code in this build; engines get them zero/0 — on Windows
  these are seeded during `comSoftDspLoadAndRunNonShared`-time engine init
  and are not used in the default chain (Play32 multiplies by `master_gain`
  only for the demo/limiter path, which is inactive).
- The int (16-bit) processing path still assumes `sizeof(long)==4` at the
  `dutil*` macro level; the app always uses the 32-bit float path (GUI
  included), so this is latent only.

## PipeWire bridge (`saucefxrt`)

`src/pwbridge/saucefxrt.cpp` wires live audio through the same DSP path.
Build: `make -C src rt` (needs `pipewire-devel`; runtime
PipeWire 1.6.x + `pipewire-pulse` verified on Fedora 44).

### Architecture (mirrors Windows AudioPassthru)

- A virtual sink is created with `pactl load-module module-null-sink
  sink_name=FxSound` (pipewire-pulse; the C-API `support.null-audio-sink`
  factory is not registered on this daemon, so pactl is the transport).
  Any app playing into that sink is "the system mix" the DSP should hear.
- Capture stream: `PW_DIRECTION_INPUT`, autoconnect to the virtual sink's
  monitor ports, float32. Per dequeued buffer the code runs the exact
  AudioPassthru sequence: `setSignalFormat(32, ch, rate, 32)` then
  `processAudio((short*)fp, (short*)fp, frames, 0)` — in place, arbitrary
  chunk size, duplicate-check off.
- One-ring bridge: capture pushes processed frames, a render stream
  (`PW_DIRECTION_OUTPUT`, default sink or `--out NAME`) pops with a small
  pre-fill. Feedback is structurally impossible: the render never reaches
  the captured monitor unless a user manually loops.
- Presets run the GUI `selectPreset` sequence (loadPreset → per-effect
  `setEffectValue(v*10)` → per-EQ-band pushes) then `powerOn(true)`.
- The engine uses the real persistent registry store by default (any
  `FXSOUND_REG_FILE` override still works the same as in the DSP harness).

### CLI

```
saucefxrt [--preset FILE] [--out NODE] [--sink-name SauceFX]
          [--default] [--bypass] [--stats] [--quiet]
```

`--default` makes the virtual sink the system default (WirePlumber routes
every new app into the DSP automatically) and restores the previous
default on exit. Ctrl-C/SIGTERM exit cleanly, unlink the sink module, and
restore the default sink; a stale-sink reaper unpicks leftovers from
killed runs on the next start.

### Verification (live A/B on the desktop graph)

Methodology note: two independent `parecord` monitors cannot be aligned
sample-exactly (unknown relative start offsets), so the decisive numbers
below come from in-process `--stats` prints (RMS of the buffer exactly as
pushed into the ring, post-DSP) plus FFT of the recorded device monitor.

- 48 kHz float32 stereo negotiation confirmed; 256-frame capture chunks;
  ring occupancy ~2–12 k frames (0.05–0.25 s; pw chunk-size jitter).
- Bypass vs default state, same music window (testin.wav through the
  virtual sink): chunk-RMS goes 0.145/0.138/0.129 → 0.169/0.162/0.168 —
  the ~1.15–1.3× lift of the all-effects-on state, same direction and
  magnitude as the WAV-harness lift (6876/5968 = 1.152).
- extreme preset (maxed .fac) on the 1 kHz sine through the sink: 1 kHz
  collapses to 0.00024 amp (vs 0.167 bypass) while exciter/reverb
  harmonics appear (3 k = 0.00049, 6 k = 0.00044, 12 k = 0.00140) —
  unmistakable live processing signature.
- Default-state 1 kHz sine ≈ unity through the engine: plausible (the
  default EQ/ambience settings are flat near 1 kHz for steady tones);
  music, not sine, discriminates the default state.
- `--target FxSound` playback from `pw-cat` lands in the DSP (verified by
  both `--stats` RMS changes and the monitor-FFT signature).

### Known limitations

- ~0.1–0.25 s variable latency ( pw quantum + pre-fill ring ); fine for
  FxSound's non-listening use cases, not for lip-sync-critical paths.
- Stereo only (2-ch DSP path), like the Windows app in practice.
- Rates are whatever the graph runs (48 kHz here); capture and render
  rates are assumed equal (both resample to the shared graph rate).
- The DSP init still happens lazily inside the first captured buffer
  (no preset load outside the format-locked callback path).
- If the bridge is SIGKILLed (not INT/TERM) the sink node can outlive its
  pulse module; the module reaper clears normal leftovers, but a zombie
  `FxSound` sink node needs a manual `pw-cli destroy <node-id>`.

## Desktop app: JUCE host + React UI (`src/gui/`, `frontend/`)

The visible SauceFX interface is a React/TypeScript app built with Base UI and
rendered by WebKitGTK through JUCE's `WebBrowserComponent`. It draws the
frameless window's own title bar. A hidden JUCE panel (`SauceFXPanel`) remains
alive as the engine/model backend, so the same engine instance the audio
callback uses is still controlled from the message thread — the same threading
model as the FxSound app (FxController mutates DfxDsp on the message thread
while AudioPassthru processes; no locks).

The implementation is split into:

- `src/pwbridge/fxpw.{h,cpp}` — library form of the bridge (pipeline on
  its own worker thread; `fxpw_start/stop` + engine control calls
  `fxpw_set_power`, `fxpw_load_preset`, `fxpw_set_effect_value`,
  `fxpw_set_eq_band`, `fxpw_route_default`, `fxpw_get_stats`...). The
  CLI (`saucefxrt.cpp`) is a thin wrapper; behavior is unchanged
  (same state machine, same error surfacing, same pactl sink handling).
- `src/gui/Main.cpp` — JUCE 9 host:
  - `SauceFXPanel` keeps the existing native control surface (power, preset
    list from `Installer/Resources/Factsoft/*.fac`, five effect sliders,
    31-band EQ, routing) and all engine/preset wiring, but is never painted
    in web mode.
  - `WebInterface` provides the resource provider, injects
    `window.saucefxHost`, and maps commands to the panel. It pushes
    `saucefxState` (presets, live values, power, output, theme, accent),
    30 Hz `saucefxSpectrum` and `saucefxToast` events.
  - The frameless window uses X11 `_NET_WM_MOVERESIZE` for dragging and
    XShape for 14 px rounded corners; minimize, close-to-background and quit
    are routed through the web header.
- `frontend/` — Vite + React + TypeScript interface. Base UI supplies the
  headless controls; `saucefx-mark.svg` is the cropped title-bar mark and
  `assets/Images/saucefx-logo-128.png` remains the window icon.

Build:

```
sudo dnf install cmake gcc-c++ nodejs npm pipewire-devel \
                 webkit2gtk4.1-devel gtk3-devel libsoup3-devel \
                 libX11-devel libXext-devel libXinerama-devel \
                 libXrandr-devel libXcursor-devel freetype-devel \
                 fontconfig-devel
make -C src                       # DSP lib (linked by the GUI)
# JUCE 9.x is not vendored; point at your checkout if it is not at
# ../deps/JUCE relative to the repository root.
cmake -S src/gui -B src/gui/build -DJUCE_DIR=/path/to/JUCE
cmake --build src/gui/build -j
src/gui/build/saucefxgui_artefacts/SauceFX
```

The CMake build runs `npm install && npm run build` when frontend sources
change and copies `frontend/dist` to `saucefxgui_artefacts/web`. Node.js must
satisfy Vite 8's `^20.19.0 || >=22.12.0` engine range. Set
`SAUCEFX_WEB_DEV=http://127.0.0.1:5173` to load a running Vite dev server
instead of the embedded assets; the native bridge is still injected.

Design notes:

- The upstream `fxsound/FxSound.jucer` JUCE project (Windows) is not reused
  as the app shell — its controller glue is WASAPI/Win32/.MessageBox-heavy.
  SauceFX implements the control surface against the FxSound DSP.
- Engine control from the GUI thread races the audio thread exactly like
  FxSound does (no locks in the SauceFX panel either). DO NOT add locks in
  the GUI layer only.
- `fxpw_load_preset` pushes the selectPreset sequence immediately once the
  DSP is initialized and clears its pending flag; only a call before the
  first captured buffer defers the push to `dsp_setup`. (Leaving the pending
  flag set after an immediate push made the next buffer reload the `.fac`,
  silently overwriting a restored live session.)
- Settings -> **Save preset...** names the current live effect/EQ state and
  calls `fxpw_save_preset(name, userPresetDir)`. It writes only under
  `~/.local/share/saucefx/presets`, never the factory library. Names are
  sanitized for filesystem safety. The web dialog rejects factory/custom
  duplicates inline; the native save path also routes through a unique-name
  helper so a stale request cannot overwrite either a factory or an existing
  custom `.fac`. There is no Replace action. Saving refreshes and selects the
  new preset, then queues the normal session snapshot.
- Initial theme/accent values are injected before the page loads to avoid a
  flash. On startup the web UI sends `ready`; the host replies with the full
  native state, and re-sends it after a hidden/background window is restored.

## Build phases

The engine and app were brought up in three phases: the headless DSP harness,
the PipeWire bridge, and the JUCE desktop app. The current tree ships all
three; the desktop app drives `setEffectValue`/`loadPreset` on the running
engine through the bridge.

## Runtime field fixes (root-caused in the running session)

These live on top of the sections above — they're what actually made
the routed pipeline sound right on Fedora:

1. **WirePlumber defeats stream target pins unless asked not to.**
   Fedora's WirePlumber 0.5 `linking/find-defined-target.lua` consults
   the *default metadata* BEFORE the stream's own `target.object`:
   pipewire writes `target.node = -1` ("follow current default") per
   stream node, which silently replaced our render pin — route ON meant
   the render stream got moved onto the `FxSound` virtual sink
   (DSP-in → DSP-out self loop: +15%/pass gain creep → saturated
   static; route "no sound at speakers"). Fix: `"node.dont-move", "true"`
   on BOTH stream property sets in `fxpw.cpp`'s `connect_streams` —
   WP then honors `target.object` and never re-homes the stream.

2. **Sink modules are server-side and outlive their process; instance
   tags prevent cross-kills.** `pactl load-module module-null-sink`
   allocates a pulse module in pipewire-pulse that survives the client.
   Indexes are REUSED: a naive `pactl unload-module <idx>` on exit once
   unloaded a live sibling instance's sink (its capture then silently
   re-homed to the ALSA monitor = another feedback path). Now every
   sink is spawned with
   `sink_properties='device.description=FxSound (fxpw-<pid>)'`, the
   stale-sink reaper only reaps FxSound modules whose tag pid is dead
   (checked via `/proc/<pid>/stat` so unreaped zombies count as dead —
   `kill(pid,0)` alone can't tell), and `fxpw_stop` re-verifies the
   module row still carries our tag before unloading.
   Additionally the registry `global_remove` handler treats "our
   virtual sink disappeared" as a hard engine error instead of letting
   the capture ride defaults.

3. **LEGACY `long` = 32-bit param indexing (the right-channel killer).**
   The whole Power-Technology DSP core addresses its 32-bit param/state
   spaces with `((long *)ptr)[index]`. MSVC `long` is 4 bytes; LP64
   `long` is 8 — every such index lands ONE SLOT LATE (double-byte
   stride). In `Play32.c` the per-block stereo-mode transfer
   `((long*)params)[DSP_PLAY_STEREO_MODE_INDEX=4]` wrote float slot 8
   and the maximizer's `stereo_in_flag` (slot 4) stayed 0 → the
   maximizer ran its MONO path forever: right output zeroed (measured:
   2100 Hz tone 0.50 → 0.0007 at the render) and mono-rate buffer
   walking → the "buggy"/gating artifacts users heard. Fix everywhere
   it participates in an index: `((int *)ptr)[index]` in
   `Play32.c`/`Play16.c` (read + 5 transfers; verified: 440 Hz and
   2100 Hz both survive file harness AND live route captures with
   normal DFX cross-feed ≈10–15%). Static lpp/fpp state-array views in
   Dly8/Lex/Aural are commented out upstream — inert. Keep this rule
   for any future `long`-indexed param math: on LP64 it must be `int`
   (or `int32_t`), NOT `long`.

4. **Spectrum meter scaling.** Engine analyzer bands are linear
   magnitudes ~1e-5 (quiet) .. 5e-1 (peaks); a linear divisor shows
   nothing useful. The panel maps dB: `v = (20*log10(b) + 70) / 55`
   (-70 dBFS floor, 55 dB span), smoothed 0.45/0.55.

5. **Effect sliders were silently range-rejected (the "sliders do
   nothing" bug).** `DfxDsp::setEffectValue(effect, value)` takes the
   UI's 0..10 scale and does its own `/10` before feeding
   `dfxpSetKnobValue()`, whose knob domain is 0..1
   (`DFX_UI_MIN_VALUE/DFX_UI_MAX_VALUE`). `fxpw_set_effect_value()`
   pre-multiplied the UI value by 10 (mirroring what looked like the
   Windows app's `value * 10` at `FxController.cpp:1088` — but that
   caller feeds 0..1, not 0..10), so every push above 1.0 UI became
   `f_value > 1.0` → `dfxpSetKnobValue` returns `NOT_OKAY` **without
   any error surface**: session `val*` keys never written, knobs never
   communicated. Discovered only by instrumenting — silently-OK is the
   codebase's failure style (see `dfxp_SessionWriteIntegerValue`'s
   `fully_initialized`/`vendor_code` gates too). Fix: pass the raw
   0..10 value through (`fxpw_set_effect_value`), multiply ×10 back in
   `fxpw_get_effect_value` for the panel. `push_preset`'s
   `getEffectValue()*10 → setEffectValue` round-trip follows the same
   contract and was correct once the pair aligned; presets audibly
   apply *because* of that re-push loop (`loadPreset` itself only
   fills the DfxDspPrivate structs — Windows' FxController does the
   same 5-value re-push after a preset load). Verified end-to-end with
   the engine harness: fidelity's exciter harmonics (3rd harmonic of
   2.1 kHz, absent at knob 0 → present at knob 10 → absent after zero
   push), ambience Lex crosstalk, surround widening, boost lift, and
   the session `val*` keys tracking every push exactly.

6. **Route-off restore could strand the system on the virtual sink.**
   `fxpw_route_default(on)` captured the restore point *only inside the
   ON branch and only when the default wasn't already ours*. In the
   field the default can already point at the virtual sink (leftover
   routing from a killed instance, WirePlumber persistence, or a
   previous session): ON then returned without setting `did_default`,
   and the OFF toggle became a silent no-op — the system stayed routed
   through FxSound permanently. Fix (three parts): capture
   `orig_default` at engine start unconditionally (post-reap, so it
   reflects the real device); OFF restores whenever this session owns
   the takeover **and additionally heals** the stray case where the
   default is still our sink though we never routed; ON while already
   routed marks `did_default=1` (restore owed) instead of no-op'ing.
   `fxpw_is_routed_default()` reports reality to the checkbox at panel
   construction. Note: the default change settles asynchronously in
   pipewire-pulse — verify with a poll, not an instant read
   (`harness/routetest.cpp` drives ON/OFF/leftover/idempotent cases
   against the live API; PASSED with the poll).
