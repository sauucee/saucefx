//! View binding: real Slint callbacks <-> real `UiState`, plus local-only
//! menu/dialog state. Nothing here creates audio, files or devices.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Model};

use saucefx_core::settings::ThemeMode;
use saucefx_core::state::{BAND_COUNT, EFFECT_COUNT, EditError, UiState};
use saucefx_core::updates::UpdateStatus;

use crate::{AppWindow, MenuEntry, Palette};

/// The reference interface bundles Figtree; without registering it here the
/// Slint text falls back to a system font and every label renders
/// differently. Registered once, before the first paint.
fn register_ui_fonts() {
    use std::sync::Once;
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        const FONTS: [(&str, &[u8]); 3] = [
            (
                "Figtree-Regular.ttf",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../frontend/src/fonts/Figtree-Regular.ttf"
                )),
            ),
            (
                "Figtree-SemiBold.ttf",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../frontend/src/fonts/Figtree-SemiBold.ttf"
                )),
            ),
            (
                "Figtree-Bold.ttf",
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../frontend/src/fonts/Figtree-Bold.ttf"
                )),
            ),
        ];
        // Shared fontique storage: registering on the returned collection
        // applies process-wide, so "Figtree" resolves like the reference UI.
        let mut collection = slint::fontique_011::shared_collection();
        for (_file, bytes) in FONTS {
            let blob =
                slint::fontique_011::fontique::Blob::new(std::sync::Arc::new(bytes.to_vec()));
            collection.register_fonts(blob, None);
        }
    });
}

/// Literal effect labels from `frontend/src/components/EffectRack.tsx`.
pub const EFFECT_NAMES: [&str; EFFECT_COUNT] = [
    "Clarity",
    "Ambience",
    "Surround",
    "Dynamic Boost",
    "Bass Boost",
];

/// `EQ_LABEL_INDICES` from `frontend/src/data.ts:51`.
pub const EQ_LABEL_INDICES: [usize; 11] = [0, 4, 7, 10, 13, 16, 18, 21, 24, 27, 30];

/// The compact captions those indices produce (20 Hz .. 20 kHz).
/// Full unit captions for all 31 bands (frontend/src/data.ts EQ_FREQUENCIES +
/// formatFrequency). Equalizer.tsx uses these for the hover/focus readout; the
/// eleven-entry EQ_LABELS list is only the sparse axis captions.
pub const EQ_FREQUENCY_CAPTIONS: [&str; BAND_COUNT] = [
    "20 Hz", "25 Hz", "31.5 Hz", "40 Hz", "50 Hz", "63 Hz", "80 Hz", "100 Hz", "125 Hz", "160 Hz",
    "200 Hz", "250 Hz", "315 Hz", "400 Hz", "500 Hz", "630 Hz", "800 Hz", "1 kHz", "1.3 kHz",
    "1.6 kHz", "2 kHz", "2.5 kHz", "3.1 kHz", "4 kHz", "5 kHz", "6.3 kHz", "8 kHz", "10 kHz",
    "12.5 kHz", "16 kHz", "20 kHz",
];

/// frontend/src/data.ts formatGain: one decimal, '+' only for positives and
/// exactly "0" at zero.
pub fn format_gain(value: f32) -> String {
    // data.ts uses Math.round(value * 10) / 10. JS Math.round is
    // floor(x + 0.5), so negative halves (-1.25) round towards +infinity
    // (-1.2), unlike Rust's f32::round which rounds away from zero.
    //
    // Widen to f64 before the multiply: doing it in f32 double-rounds and
    // disagrees with JS at exact halves of the decimal value (1.15f32 * 10
    // becomes 11.5f32, which JS rounds down to 1.1, not up to 1.2).
    let rounded = ((f64::from(value) * 10.0) + 0.5).floor() / 10.0;
    if rounded == 0.0 {
        return "0".into();
    }
    let mut text = format!("{rounded:.1}");
    if text.ends_with(".0") {
        text.truncate(text.len() - 2);
    }
    if rounded > 0.0 {
        format!("+{text}")
    } else {
        text
    }
}

pub const EQ_LABELS: [&str; 11] = [
    "20", "50", "100", "200", "400", "800", "1.3k", "2.5k", "5k", "10k", "20k",
];

/// Normalised x positions of those captions: `(index + 0.5) / 31`.
pub const EQ_LABEL_POSITIONS: [f32; 11] = [
    0.5 / 31.0,
    4.5 / 31.0,
    7.5 / 31.0,
    10.5 / 31.0,
    13.5 / 31.0,
    16.5 / 31.0,
    18.5 / 31.0,
    21.5 / 31.0,
    24.5 / 31.0,
    27.5 / 31.0,
    30.5 / 31.0,
];

/// Spectrum bands (`Spectrum.tsx` BAND_COUNT).
pub const SPECTRUM_BANDS: usize = 10;

/// History slots kept per band (`Spectrum.tsx` HISTORY).
pub const SPECTRUM_HISTORY: usize = 10;

/// Bars drawn per frame: ten bands of ten history slots.
pub const SPECTRUM_SLOTS: usize = SPECTRUM_BANDS * SPECTRUM_HISTORY;

/// Idle bar alpha once the reference 1200 ms activity window lapses.
pub const SPECTRUM_IDLE_ALPHA: f32 = 0.78;

/// Activity window in milliseconds (`Spectrum.tsx` live check).
pub const SPECTRUM_ACTIVE_MS: u64 = 1200;

/// The Rose accent App.tsx falls back to when custom accents are off/invalid.
pub const ACCENT_ROSE: u32 = 0xffff_2e57;

/// The six accents shipped by `frontend/src/data.ts`.
pub const ACCENTS: [(u32, &str); 6] = [
    (ACCENT_ROSE, "Red"),
    (0xff3b_82f6, "Blue"),
    (0xff2f_a572, "Green"),
    (0xff12_a8a0, "Teal"),
    (0xffd4_922a, "Amber"),
    (0xff8b_7cf6, "Violet"),
];

/// Chart geometry, kept identical to `Layout` in `ui/palette.slint`; the layout
/// test compares both sides so the curve cannot drift from the drawn chart.
pub mod geometry {
    /// Width of the drawn chart, matching `Layout.eq-chart-width`.
    pub const EQ_CHART_WIDTH: f32 = 675.0;
    /// Height of the drawn chart, matching `Layout.eq-chart-height`.
    pub const EQ_CHART_HEIGHT: f32 = 186.0;
    /// Virtual coordinate space of the trace (`Equalizer.tsx` VIEW_WIDTH).
    pub const EQ_VIEW_WIDTH: f32 = 1000.0;
    /// Virtual height of the trace (`Equalizer.tsx` VIEW_HEIGHT).
    pub const EQ_VIEW_HEIGHT: f32 = 200.0;
    /// y of +12 dB (`Equalizer.tsx` TOP).
    pub const EQ_PLOT_TOP: f32 = 12.0;
    /// y of -12 dB (`Equalizer.tsx` BOTTOM).
    pub const EQ_PLOT_BOTTOM: f32 = 188.0;
}

/// Outcome of a view edit, so a disabled or invalid edit is never a silent
/// success.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOutcome {
    /// The bound value changed.
    Applied,
    /// Processing is off, so editing is disabled; the stored value is untouched.
    PoweredOff,
    /// The index is outside the bound control.
    OutOfRange,
    /// The value was NaN or infinite.
    NotFinite,
    /// The value is outside the control's range.
    Clamped,
}

impl EditOutcome {
    fn from_error(error: EditError) -> Self {
        match error {
            EditError::EffectIndex(_) | EditError::BandIndex(_) => Self::OutOfRange,
            EditError::NonFiniteEffect
            | EditError::NonFiniteGain
            | EditError::NonFiniteFrequency => Self::NotFinite,
            EditError::EffectOutOfRange
            | EditError::GainOutOfRange
            | EditError::FrequencyOutOfRange => Self::Clamped,
        }
    }
}

/// One preset fixture: the values a preset choice applies, plus the .fac
/// path for real library rows ("" for the synthetic preview fixtures).
#[derive(Clone, Debug, PartialEq)]
pub struct PresetFixture {
    /// Row label, e.g. `Default` or `Factsoft 3`.
    pub label: String,
    /// Factory library entry, as opposed to an in-memory saved preset.
    pub factory: bool,
    /// Five effect values on the 0..10 view scale.
    pub effects: [f32; EFFECT_COUNT],
    /// Thirty-one EQ gains in dB.
    pub eq: [f32; BAND_COUNT],
    /// Absolute .fac path the fixture was loaded from ("" for synthetic).
    pub path: String,
}

impl PresetFixture {
    /// Flat preset: every effect and band at zero.
    pub fn flat(label: &str, factory: bool) -> Self {
        Self {
            label: label.to_string(),
            factory,
            effects: [0.0; EFFECT_COUNT],
            eq: [0.0; BAND_COUNT],
            path: String::new(),
        }
    }
}

/// `factory_count` from `.hermes/operator/backend-baseline.json`: Default plus
/// the twelve Factsoft files.
pub const FACTORY_PRESET_COUNT: usize = 13;

/// Public factory names from line 3 of the thirteen protected `.fac` files
/// (Default, General, Music, Voice, Volume Boost, Gaming, Classic Processing,
/// Light Processing, Bass Boost, Streaming Video, Movies, TV, Transcription).
/// The effect/EQ values below are a controlled test fixture, not factory data.
pub const FACTORY_NAMES: [&str; FACTORY_PRESET_COUNT] = [
    "Default",
    "General",
    "Music",
    "Voice",
    "Volume Boost",
    "Gaming",
    "Classic Processing",
    "Light Processing",
    "Bass Boost",
    "Streaming Video",
    "Movies",
    "TV",
    "Transcription",
];

/// Synthetic values over the real public factory names. Used only to prove
/// selection/reset behaviour identically in tests and preview captures.
pub fn factory_fixtures() -> Vec<PresetFixture> {
    let mut presets = vec![PresetFixture::flat(FACTORY_NAMES[0], true)];
    for number in 1..=12u32 {
        let effects: [f32; EFFECT_COUNT] =
            std::array::from_fn(|index| ((number * (index as u32 + 2)) % 11) as f32);
        let eq: [f32; BAND_COUNT] = std::array::from_fn(|band| {
            let distance = band as f32 - 15.0;
            let curve = (number as f32 * 0.55).sin() * 6.0 * (-(distance * distance) / 90.0).exp();
            curve.round().clamp(-12.0, 12.0)
        });
        presets.push(PresetFixture {
            label: FACTORY_NAMES[number as usize].to_string(),
            factory: true,
            effects,
            eq,
            path: String::new(),
        });
    }
    presets
}

/// One local menu row (preset or output).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuRow {
    /// Row text.
    pub label: String,
    /// Secondary text (`""` when unused).
    pub detail: String,
    /// Group heading row.
    pub is_group: bool,
    /// Disabled row, not selectable.
    pub disabled: bool,
    /// Device name an output row routes to ("" elsewhere); passed to the
    /// engine verbatim, like the fxpw_set_output contract.
    pub output_name: String,
    /// Port an output row routes to ("" elsewhere).
    pub output_port: String,
}

impl MenuRow {
    fn entry(label: &str, detail: &str) -> Self {
        Self {
            label: label.to_string(),
            detail: detail.to_string(),
            is_group: false,
            disabled: false,
            output_name: String::new(),
            output_port: String::new(),
        }
    }

    /// Group heading row.
    pub fn group(label: &str) -> Self {
        Self {
            label: label.to_string(),
            detail: String::new(),
            is_group: true,
            disabled: false,
            output_name: String::new(),
            output_port: String::new(),
        }
    }

    /// A selectable output row that routes to a concrete device/port.
    pub fn output(label: &str, detail: &str, device: &str, port: &str, available: bool) -> Self {
        Self {
            label: label.to_string(),
            detail: detail.to_string(),
            is_group: false,
            disabled: !available,
            output_name: device.to_string(),
            output_port: port.to_string(),
        }
    }
}

/// Rows plus the values behind each value row, aligned by index.
#[derive(Clone, Debug)]
pub struct SceneCatalog {
    /// Preset rows: FACTORY group, CUSTOM group, then the save action.
    pub presets: Vec<MenuRow>,
    /// Values for each row (`None` for groups and the save action).
    pub preset_values: Vec<Option<PresetFixture>>,
    /// Output rows, including disabled entries.
    pub outputs: Vec<MenuRow>,
}

impl SceneCatalog {
    fn build(factory: &[PresetFixture], saved: &[PresetFixture]) -> Self {
        let mut presets = vec![MenuRow::group("FACTORY")];
        let mut preset_values: Vec<Option<PresetFixture>> = vec![None];
        for fixture in factory {
            presets.push(MenuRow::entry(&fixture.label, ""));
            preset_values.push(Some(fixture.clone()));
        }
        if !saved.is_empty() {
            presets.push(MenuRow::group("CUSTOM"));
            preset_values.push(None);
            for fixture in saved {
                presets.push(MenuRow::entry(&fixture.label, "Custom"));
                preset_values.push(Some(fixture.clone()));
            }
        }
        presets.push(MenuRow::entry("Save current\u{2026}", ""));
        preset_values.push(None);

        Self {
            presets,
            preset_values,
            outputs: vec![
                MenuRow::group("WITH SAUCEFX EFFECTS"),
                MenuRow::output(
                    "Automatic",
                    "Chooses a connected headset or speakers",
                    "",
                    "",
                    true,
                ),
                MenuRow::output(
                    "Test speakers",
                    "Built-in audio",
                    "alsa_output.test-speakers",
                    "",
                    true,
                ),
                MenuRow::output(
                    "Test headphones",
                    "Built-in headphone jack",
                    "alsa_output.test-headphones",
                    "",
                    true,
                ),
                MenuRow::group("WITHOUT EFFECTS"),
                MenuRow::output(
                    "Direct playback",
                    "Use system output without SauceFX",
                    "direct",
                    "",
                    true,
                ),
            ],
        }
    }
}

impl Default for SceneCatalog {
    fn default() -> Self {
        Self::build(&factory_fixtures(), &[])
    }
}

/// Ten bands of ten history slots, updated with the exact FxVisualizer loop:
/// for each band, read `row[step + 1]` and copy it to both `row[step]` and
/// `row[9 - step]` before writing the new frame into the middle slot. The
/// read-before-write order is what makes one impulse travel outward as
/// `5 -> 4/5 -> 3/6 -> 2/7 -> 1/8 -> 0/9 -> gone`.
#[derive(Clone, Debug, PartialEq)]
pub struct SpectrumHistory {
    rows: [[f32; SPECTRUM_HISTORY]; SPECTRUM_BANDS],
    last_active_ms: Option<u64>,
}

impl Default for SpectrumHistory {
    fn default() -> Self {
        Self {
            rows: [[0.0; SPECTRUM_HISTORY]; SPECTRUM_BANDS],
            last_active_ms: None,
        }
    }
}

impl SpectrumHistory {
    /// Shift both halves outward, then place the new band values in the centre.
    pub fn push(&mut self, values: &[f32; SPECTRUM_BANDS], now_ms: u64) {
        for (band, row) in self.rows.iter_mut().enumerate() {
            for step in 0..SPECTRUM_HISTORY / 2 {
                let next = row[step + 1];
                row[step] = next;
                row[SPECTRUM_HISTORY - 1 - step] = next;
            }
            row[SPECTRUM_HISTORY / 2] = values[band];
        }
        self.last_active_ms = Some(now_ms);
    }

    /// Drop every slot, as power-off does.
    pub fn clear(&mut self) {
        self.rows = [[0.0; SPECTRUM_HISTORY]; SPECTRUM_BANDS];
        self.last_active_ms = None;
    }

    /// Flat band-major bar heights: `band * 10 + step`.
    pub fn bars(&self) -> [f32; SPECTRUM_SLOTS] {
        std::array::from_fn(|index| self.rows[index / SPECTRUM_HISTORY][index % SPECTRUM_HISTORY])
    }

    /// `1.0` while a frame arrived within the last 1200 ms, else `0.78`.
    pub fn alpha(&self, now_ms: u64) -> f32 {
        match self.last_active_ms {
            Some(at) if now_ms.saturating_sub(at) < SPECTRUM_ACTIVE_MS => 1.0,
            _ => SPECTRUM_IDLE_ALPHA,
        }
    }

    /// Timestamp of the last pushed frame, if any.
    pub fn last_active_ms(&self) -> Option<u64> {
        self.last_active_ms
    }
}

/// Forwarding sink for the real engine behind the binding. The preview and
/// the tests run without one; the host binary installs an implementation
/// that pushes every applied change to the DfxDsp/PipeWire engine. All calls
/// stay on the Slint event-loop thread, like the fxpw_* contract.
pub trait Engine: 'static {
    /// Power (bypass) state.
    fn apply_power(&mut self, on: bool);
    /// One effect value on the UI's 0..10 scale.
    fn apply_effect(&mut self, index: usize, value: f32);
    /// One EQ gain in dB (the engine's band frequency is resolved by the
    /// implementation's own band table).
    fn apply_eq_gain(&mut self, index: usize, gain: f32);
    /// Remember the physical device/port without enabling SauceFX routing.
    fn set_output_preference(&mut self, name: &str, port: &str);
    /// Route to a physical output; an empty name selects automatic.
    fn apply_output(&mut self, name: &str, port: &str);
    /// Load a .fac file into the engine. Returns the resulting live effect
    /// and EQ values so the view state mirrors the engine exactly.
    fn load_preset(&mut self, path: &str) -> Option<([f32; EFFECT_COUNT], [f32; BAND_COUNT])>;
}

/// The whole preview state: applied audio settings plus view-only selection,
/// dialog and synthetic spectrum data.
#[derive(Clone)]
pub struct UiBinding {
    state: UiState,
    catalog: SceneCatalog,
    saved_presets: Vec<PresetFixture>,
    selected_preset: usize,
    selected_output: usize,
    theme: ThemeMode,
    accent: u32,
    custom_accent: bool,
    selected_accent: usize,
    autostart: bool,
    update_status: UpdateStatus,
    history: SpectrumHistory,
    preset_open: bool,
    output_open: bool,
    settings_open: bool,
    save_open: bool,
    save_name: String,
    active_eq_band: Option<usize>,
    toast: Option<String>,
    /// Real engine sink, when the host installed one.
    engine: Option<Rc<RefCell<dyn Engine>>>,
    /// Last (rows, selection) pushed into each menu model, so sync() does
    /// not replace the model instances on every spectrum tick. RefCell
    /// because sync takes `&self`, matching the window's own interior
    /// mutability.
    menu_cache_preset: RefCell<(Vec<MenuRow>, usize)>,
    menu_cache_output: RefCell<(Vec<MenuRow>, usize)>,
}

impl Default for UiBinding {
    fn default() -> Self {
        Self::new()
    }
}

impl UiBinding {
    /// A fresh preview: power on, flat EQ, `Default` preset, `Test speakers`.
    pub fn new() -> Self {
        register_ui_fonts();
        Self {
            state: UiState::new(),
            catalog: SceneCatalog::default(),
            saved_presets: Vec::new(),
            selected_preset: 1,
            selected_output: 1,
            theme: ThemeMode::Dark,
            accent: ACCENT_ROSE,
            custom_accent: false,
            selected_accent: 0,
            autostart: true,
            update_status: UpdateStatus::Ready,
            history: SpectrumHistory::default(),
            preset_open: false,
            output_open: false,
            settings_open: false,
            save_open: false,
            save_name: String::new(),
            active_eq_band: None,
            toast: None,
            engine: None,
            menu_cache_preset: RefCell::new((Vec::new(), 0)),
            menu_cache_output: RefCell::new((Vec::new(), 0)),
        }
    }

    /// Install the real engine sink. After this, every applied power/effect/
    /// EQ/output/preset change is forwarded to the engine as well.
    pub fn set_engine(&mut self, engine: Rc<RefCell<dyn Engine>>) {
        self.engine = Some(engine);
    }

    /// Replace the synthetic catalog with real preset/output rows. `factory`
    /// carries the .fac library fixtures (real paths, zeroed values — the
    /// engine loads them on selection); `outputs` are engine-provided rows.
    pub fn install_catalog(
        &mut self,
        factory: Vec<PresetFixture>,
        saved: Vec<PresetFixture>,
        outputs: Vec<MenuRow>,
        selected_output: usize,
    ) {
        self.catalog = SceneCatalog::build(&factory, &saved);
        self.catalog.outputs = outputs;
        self.selected_preset = 1;
        self.selected_output = selected_output;
    }

    /// Restore the saved output identity, even when the device is disconnected.
    pub fn restore_output(&mut self, name: &str, port: &str, routed: bool) {
        self.state.set_output_preference(name, port);
        self.state.set_active_output("", "", routed);
        self.refresh_outputs(self.catalog.outputs.clone());
        if let Some(engine) = &self.engine {
            let mut engine = engine.borrow_mut();
            if !routed {
                engine.set_output_preference(name, port);
            }
            engine.apply_output(
                if routed { name } else { "direct" },
                if routed { port } else { "" },
            );
        }
    }

    /// Refresh device rows without applying routing or changing audio edits.
    pub fn refresh_outputs(&mut self, mut outputs: Vec<MenuRow>) -> bool {
        let preference = self.state.output();
        let name = if preference.routed {
            preference.preferred_name.as_str()
        } else {
            "direct"
        };
        let matches = |row: &MenuRow| {
            !row.is_group
                && row.output_name == name
                && (name.is_empty()
                    || name == "direct"
                    || preference.preferred_port.is_empty()
                    || row.output_port == preference.preferred_port)
        };
        let selected = outputs.iter().position(matches).unwrap_or_else(|| {
            if preference.routed && !name.is_empty() {
                let mut missing = self
                    .catalog
                    .outputs
                    .iter()
                    .find(|row| matches(row))
                    .cloned()
                    .unwrap_or_else(|| {
                        MenuRow::output(name, "", name, &preference.preferred_port, false)
                    });
                missing.disabled = true;
                missing.detail = "Disconnected".to_string();
                let index = outputs
                    .iter()
                    .position(|row| row.is_group && row.label == "WITHOUT EFFECTS")
                    .unwrap_or(outputs.len());
                outputs.insert(index, missing);
                index
            } else {
                outputs
                    .iter()
                    .position(|row| !row.is_group && !row.disabled)
                    .unwrap_or(0)
            }
        });
        if self.catalog.outputs == outputs && self.selected_output == selected {
            return false;
        }
        self.catalog.outputs = outputs;
        self.selected_output = selected;
        true
    }

    /// Path of the selected preset fixture ("" for synthetic rows).
    pub fn selected_preset_path(&self) -> &str {
        self.catalog
            .preset_values
            .get(self.selected_preset)
            .and_then(|value| value.as_ref())
            .map_or("", |fixture| fixture.path.as_str())
    }

    /// Index of the currently selected output row.
    pub fn selected_output(&self) -> usize {
        self.selected_output
    }

    /// Catalog index of the preset fixture with this exact .fac path.
    pub fn find_preset_by_path(&self, path: &str) -> Option<usize> {
        if path.is_empty() {
            return None;
        }
        self.catalog
            .preset_values
            .iter()
            .position(|value| value.as_ref().is_some_and(|fixture| fixture.path == path))
    }

    /// Catalog index of the preset whose .fac file name matches `basename`,
    /// for the session's factory relocation rule.
    pub fn find_preset_by_basename(&self, basename: &str) -> Option<usize> {
        if basename.is_empty() {
            return None;
        }
        self.catalog.preset_values.iter().position(|value| {
            value.as_ref().is_some_and(|fixture| {
                Path::new(&fixture.path)
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case(basename))
            })
        })
    }

    fn forward_power(&self) {
        if let Some(engine) = &self.engine {
            engine.borrow_mut().apply_power(self.state.power());
        }
    }

    fn forward_effect(&self, index: usize) {
        if let Some(engine) = &self.engine
            && let Some(value) = self.state.effect(index)
        {
            engine.borrow_mut().apply_effect(index, value);
        }
    }

    fn forward_eq_gain(&self, index: usize) {
        if let Some(engine) = &self.engine
            && let Some(gain) = self.state.eq_gain(index)
        {
            engine.borrow_mut().apply_eq_gain(index, gain);
        }
    }

    /// The bounded state behind the view.
    pub fn state(&self) -> &UiState {
        &self.state
    }

    /// Processing enabled?
    pub fn power(&self) -> bool {
        self.state.power()
    }

    /// All five effect values on the view's 0..10 scale.
    pub fn effects(&self) -> [f32; EFFECT_COUNT] {
        std::array::from_fn(|index| self.state.effect(index).unwrap_or(0.0))
    }

    /// All 31 EQ gains in dB.
    pub fn eq_gains(&self) -> [f32; BAND_COUNT] {
        std::array::from_fn(|index| self.state.eq_gain(index).unwrap_or(0.0))
    }

    pub fn active_eq_band(&self) -> Option<usize> {
        self.active_eq_band
    }

    /// Hover/focus target for the EQ caption. `None` restores the default
    /// subtitle, exactly like `Equalizer.tsx` clearing `activeBand`.
    pub fn set_active_eq_band(&mut self, band: Option<usize>) {
        self.active_eq_band = band.filter(|index| *index < BAND_COUNT);
    }

    pub fn eq_readout(&self) -> String {
        let Some(band) = self.active_eq_band else {
            return String::new();
        };
        format!(
            "{} · {} dB",
            EQ_FREQUENCY_CAPTIONS[band],
            format_gain(self.eq_gains()[band])
        )
    }

    /// Load preset/session EQ gains on the same whole-dB scale as manual
    /// edits. Round before capturing the baseline and forwarding to the DSP,
    /// keeping the readout, resets and saved values consistent.
    pub fn apply_captured_values(&mut self, effects: &[f32], eq: &[f32]) -> bool {
        for (band, gain) in eq.iter().take(BAND_COUNT).enumerate() {
            if !gain.is_finite()
                || !(-12.0..=12.0).contains(gain)
                || self.state.set_eq_gain(band, gain.round()).is_err()
            {
                return false;
            }
        }
        for (effect, value) in effects.iter().take(EFFECT_COUNT).enumerate() {
            if self.state.set_effect(effect, *value).is_err() {
                return false;
            }
        }
        self.state.mark_applied();
        if let Some(engine) = &self.engine {
            let mut sink = engine.borrow_mut();
            for index in 0..EFFECT_COUNT {
                if let Some(value) = self.state.effect(index) {
                    sink.apply_effect(index, value);
                }
            }
            for index in 0..BAND_COUNT {
                if let Some(gain) = self.state.eq_gain(index) {
                    sink.apply_eq_gain(index, gain);
                }
            }
        }
        true
    }

    /// Save dialog open?
    pub fn save_open(&self) -> bool {
        self.save_open
    }

    /// Preset menu open?
    pub fn preset_menu_open(&self) -> bool {
        self.preset_open
    }

    /// Output menu open?
    pub fn output_menu_open(&self) -> bool {
        self.output_open
    }

    /// Toggle processing.
    pub fn toggle_power(&mut self) {
        self.state.set_power(!self.state.power());
        if !self.state.power() {
            self.preset_open = false;
            self.output_open = false;
            self.history.clear();
        }
        self.forward_power();
    }

    /// Change one effect on the UI's 0..10 scale.
    pub fn set_effect(&mut self, index: usize, value: f32) -> EditOutcome {
        if !self.state.power() {
            return EditOutcome::PoweredOff;
        }
        if !value.is_finite() {
            return EditOutcome::NotFinite;
        }
        // The reference slider clamps at its ends instead of refusing the drag.
        match self.state.set_effect(index, value.round().clamp(0.0, 10.0)) {
            Ok(()) => {
                self.forward_effect(index);
                EditOutcome::Applied
            }
            Err(error) => EditOutcome::from_error(error),
        }
    }

    /// Change one EQ gain in dB.
    pub fn set_eq_gain(&mut self, index: usize, gain: f32) -> EditOutcome {
        if !self.state.power() {
            return EditOutcome::PoweredOff;
        }
        // The reference slider uses step=1, so the view only ever sends whole
        // dB, and like set_effect it clamps at its ends (Home/End jumps of
        // ±24 dB must land on the limits, not be refused).
        match self
            .state
            .set_eq_gain(index, gain.round().clamp(-12.0, 12.0))
        {
            Ok(()) => {
                self.forward_eq_gain(index);
                EditOutcome::Applied
            }
            Err(error) => EditOutcome::from_error(error),
        }
    }

    /// Restore every effect to the applied baseline.
    pub fn reset_effects(&mut self) {
        if !self.state.power() {
            return;
        }
        for index in 0..EFFECT_COUNT {
            if let Some(value) = self.state.applied_effect(index)
                && self.state.set_effect(index, value).is_ok()
            {
                self.forward_effect(index);
            }
        }
    }

    /// Restore every EQ band to the applied baseline.
    pub fn reset_eq(&mut self) {
        if !self.state.power() {
            return;
        }
        for index in 0..BAND_COUNT {
            if let Some(gain) = self.state.applied_eq_gain(index)
                && self.state.set_eq_gain(index, gain).is_ok()
            {
                self.forward_eq_gain(index);
            }
        }
    }

    /// Treat the current values as the new applied baseline.
    pub fn apply_current(&mut self) {
        self.state.mark_applied();
    }

    /// `true` when effects or EQ differ from the applied baseline.
    pub fn edited(&self) -> bool {
        self.state.edited()
    }

    /// Currently selected preset caption.
    pub fn preset_name(&self) -> &str {
        self.catalog
            .presets
            .get(self.selected_preset)
            .map_or("Default", |row| row.label.as_str())
    }

    /// Per-effect reset to the applied baseline (double-click in the source).
    pub fn reset_effect(&mut self, index: usize) -> EditOutcome {
        if !self.state.power() {
            return EditOutcome::PoweredOff;
        }
        let Some(value) = self.state.applied_effect(index) else {
            return EditOutcome::OutOfRange;
        };
        match self.state.set_effect(index, value) {
            Ok(()) => {
                self.forward_effect(index);
                EditOutcome::Applied
            }
            Err(error) => EditOutcome::from_error(error),
        }
    }

    /// Per-band reset to the applied baseline (right-click in the source).
    pub fn reset_eq_band(&mut self, index: usize) -> EditOutcome {
        if !self.state.power() {
            return EditOutcome::PoweredOff;
        }
        let Some(gain) = self.state.applied_eq_gain(index) else {
            return EditOutcome::OutOfRange;
        };
        match self.state.set_eq_gain(index, gain.round()) {
            Ok(()) => {
                self.forward_eq_gain(index);
                EditOutcome::Applied
            }
            Err(error) => EditOutcome::from_error(error),
        }
    }

    /// Reset All: every effect and band returns to the applied baseline.
    pub fn reset_all(&mut self) {
        if !self.state.power() {
            return;
        }
        self.reset_effects();
        self.reset_eq();
    }

    /// Currently selected output caption.
    pub fn output_name(&self) -> &str {
        self.catalog
            .outputs
            .get(self.selected_output)
            .map_or("Test speakers", |row| row.label.as_str())
    }

    /// Open a dropdown, closing the other one.
    pub fn toggle_preset_menu(&mut self) {
        if !self.state.power() {
            return;
        }
        self.output_open = false;
        self.preset_open = !self.preset_open;
    }

    /// Open the output dropdown, closing the other one.
    pub fn toggle_output_menu(&mut self) {
        self.preset_open = false;
        self.output_open = !self.output_open;
    }

    /// Dismiss any open dropdown.
    pub fn dismiss_menu(&mut self) {
        self.preset_open = false;
        self.output_open = false;
    }

    /// First-launch fallback: load the currently selected preset into the
    /// engine. `install_catalog` only selects the row; the reference applies
    /// the Default preset for new users, so a host without a saved session
    /// calls this once at startup instead of running preset-less.
    pub fn load_selected_preset(&mut self) -> bool {
        let index = self.selected_preset;
        self.choose_preset(index)
    }

    /// Pick a preset row. Group and disabled rows keep the current selection.
    /// Real library rows (with a .fac path) load through the engine and
    /// mirror the resulting live values; synthetic rows apply their fixture.
    pub fn choose_preset(&mut self, index: usize) -> bool {
        if !self.state.power() {
            return false;
        }
        let Some(row) = self.catalog.presets.get(index) else {
            return false;
        };
        if row.is_group || row.disabled {
            return false;
        }
        if row.label.starts_with("Save current") {
            self.save_open = true;
            self.preset_open = false;
            return true;
        }
        let Some(Some(fixture)) = self.catalog.preset_values.get(index) else {
            return false;
        };
        if !fixture.path.is_empty()
            && let Some(engine) = &self.engine
        {
            let Some((effects, eq)) = engine.borrow_mut().load_preset(&fixture.path) else {
                return false;
            };
            if !self.apply_captured_values(&effects, &eq) {
                return false;
            }
            self.selected_preset = index;
            self.preset_open = false;
            return true;
        }
        let effects = fixture.effects;
        let eq = fixture.eq;
        if !self.apply_captured_values(&effects, &eq) {
            return false;
        }
        self.selected_preset = index;
        self.preset_open = false;
        true
    }

    /// Pick an output row. Group and disabled rows keep the current selection.
    pub fn choose_output(&mut self, index: usize) -> bool {
        let Some(row) = self.catalog.outputs.get(index) else {
            return false;
        };
        if row.is_group || row.disabled {
            return false;
        }
        self.selected_output = index;
        self.output_open = false;
        // Main.cpp webSetOutput: "direct" unroutes and keeps the saved
        // device; Automatic clears the preference; a device routes into the
        // sink with that device selected.
        let direct = row.output_name == "direct";
        if !direct {
            self.state
                .set_output_preference(&row.output_name, &row.output_port);
        }
        self.state.set_active_output(
            if direct { "" } else { &row.output_name },
            if direct { "" } else { &row.output_port },
            !direct,
        );
        if let Some(engine) = &self.engine {
            engine
                .borrow_mut()
                .apply_output(&row.output_name, &row.output_port);
        }
        true
    }

    /// Open the settings dialog.
    pub fn open_settings(&mut self) {
        self.settings_open = true;
        self.preset_open = false;
        self.output_open = false;
    }

    /// Close the settings dialog.
    pub fn close_settings(&mut self) {
        self.settings_open = false;
    }

    /// Settings dialog open?
    pub fn settings_open(&self) -> bool {
        self.settings_open
    }

    /// Saved theme.
    pub fn theme(&self) -> ThemeMode {
        self.theme
    }

    /// Switch theme, as the settings dialog does.
    pub fn set_light(&mut self, light: bool) {
        self.theme = if light {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
    }

    /// Stored accent as opaque ARGB (may differ from what is drawn).
    pub fn accent(&self) -> u32 {
        self.accent
    }

    /// Pick an accent colour (ARGB, alpha forced opaque).
    pub fn set_accent(&mut self, accent: u32) {
        self.accent = 0xff00_0000 | (accent & 0x00ff_ffff);
    }

    /// Whether a custom accent hex is enabled (`App.tsx` customAccent).
    pub fn custom_accent(&self) -> bool {
        self.custom_accent
    }

    /// Enable or disable custom accent hexes.
    pub fn set_custom_accent(&mut self, custom: bool) {
        self.custom_accent = custom;
    }

    /// The accent actually drawn: `App.tsx` uses the Rose fallback unless the
    /// custom flag is on *and* the stored hex is valid.
    pub fn display_accent(&self) -> u32 {
        if self.custom_accent && self.accent != 0 {
            self.accent
        } else {
            ACCENTS[self.selected_accent.min(ACCENTS.len() - 1)].0
        }
    }

    /// Caption for the drawn accent: a shipped name, or `Custom`.
    pub fn accent_name(&self) -> &'static str {
        let drawn = self.display_accent();
        ACCENTS
            .iter()
            .find(|(argb, _)| *argb == drawn)
            .map(|(_, name)| *name)
            .unwrap_or("Custom")
    }

    /// Autostart switch, preview only.
    pub fn autostart(&self) -> bool {
        self.autostart
    }

    /// Flip the preview autostart switch; nothing is registered.
    pub fn set_autostart(&mut self, autostart: bool) {
        self.autostart = autostart;
    }

    /// Set the updater status supplied by the host.
    pub fn set_update_status(&mut self, status: UpdateStatus) {
        self.update_status = status;
    }

    /// Current updater status.
    pub fn update_status(&self) -> &UpdateStatus {
        &self.update_status
    }

    /// Open the save dialog pre-filled with the reference suggestion
    /// (`<preset> Custom`, numbered when taken).
    pub fn open_save(&mut self) {
        self.save_open = true;
        self.save_name = self.save_suggestion();
    }

    /// The name `SavePresetDialog` suggests when it opens.
    pub fn save_suggestion(&self) -> String {
        let base = format!("{} Custom", self.preset_name());
        let names = self.existing_preset_names();
        if !names.iter().any(|name| name == &base) {
            return base;
        }
        let mut counter = 2;
        while names
            .iter()
            .any(|name| name == &format!("{base} {counter}"))
        {
            counter += 1;
        }
        format!("{base} {counter}")
    }

    /// Every preset name the save dialog treats as taken.
    pub fn existing_preset_names(&self) -> Vec<String> {
        self.catalog
            .presets
            .iter()
            .filter(|row| !row.is_group && !row.disabled)
            .map(|row| row.label.clone())
            .filter(|label| !label.starts_with("Save current"))
            .collect()
    }

    /// The exact validation text the reference shows under the name field.
    pub fn save_error(&self) -> Option<&'static str> {
        let normalized = self.save_name.trim();
        if normalized.is_empty() {
            return Some("Enter a name for this preset.");
        }
        let duplicate = self
            .existing_preset_names()
            .iter()
            .any(|existing| existing.trim().to_lowercase() == normalized.to_lowercase());
        if duplicate {
            return Some("A preset with this name already exists.");
        }
        None
    }

    /// Close the save dialog without saving.
    pub fn close_save(&mut self) {
        self.save_open = false;
    }

    /// Current save-dialog name.
    pub fn save_name(&self) -> &str {
        &self.save_name
    }

    /// Update the save-dialog name.
    pub fn set_save_name(&mut self, name: &str) {
        self.save_name = name.to_string();
    }

    /// Save is allowed only when a name is present and not already taken.
    pub fn save_valid(&self) -> bool {
        self.save_error().is_none()
    }

    /// Confirm the save: closes the dialog and shows a local toast. No file is
    /// written in the preview.
    pub fn confirm_save(&mut self) -> bool {
        if !self.save_valid() {
            return false;
        }
        let name = self.save_name.trim().to_string();
        let effects = self.effects();
        let eq = self.eq_gains();
        // The saved values become the applied baseline, so the view is clean.
        self.state.mark_applied();
        self.saved_presets.push(PresetFixture {
            label: name.clone(),
            factory: false,
            effects,
            eq,
            path: String::new(),
        });
        self.catalog = SceneCatalog::build(&factory_fixtures(), &self.saved_presets);
        // The new custom preset becomes the current selection, as in the source.
        if let Some(index) = self
            .catalog
            .presets
            .iter()
            .position(|row| !row.is_group && row.label == name)
        {
            self.selected_preset = index;
        }
        self.save_open = false;
        // Opened from settings, a successful save closes both layers.
        self.settings_open = false;
        self.toast = Some(format!(
            "Preset \u{201c}{name}\u{201d} saved (preview only)"
        ));
        true
    }

    /// Install a refreshed preset list (the host's post-import rescan),
    /// keeping the outputs untouched and the current selection when its
    /// path still exists.
    pub fn install_presets(&mut self, fixtures: Vec<PresetFixture>) {
        let selected = self.selected_preset_path().to_string();
        self.catalog = SceneCatalog::build(&fixtures, &self.saved_presets);
        if let Some(index) = self.find_preset_by_path(&selected) {
            self.selected_preset = index;
        }
    }

    /// Select a preset row by its file path without loading it through the
    /// engine (the import flow selects the last imported file; loading is
    /// the caller's choice, and the power gate does not apply).
    pub fn select_preset_by_path(&mut self, path: &str) -> bool {
        if let Some(index) = self.find_preset_by_path(path) {
            self.selected_preset = index;
            return true;
        }
        false
    }

    /// Display label of the selected preset row ("" for synthetic rows).
    pub fn selected_preset_label(&self) -> String {
        self.catalog
            .presets
            .get(self.selected_preset)
            .map_or_else(String::new, |row| row.label.clone())
    }

    /// Display labels of the selectable preset rows, for the host's
    /// unique-name check (the reference compares against the preset box).
    pub fn preset_labels(&self) -> Vec<String> {
        self.catalog
            .presets
            .iter()
            .filter(|row| !row.is_group)
            .map(|row| row.label.clone())
            .collect()
    }

    /// Apply a REAL save: the host has already written `path` through the
    /// engine and rescanned the library, so `factory` is the fresh fixture
    /// list (factory + user files). Mirrors the tail of the preview's
    /// confirm_save — values become the applied baseline, the saved preset
    /// is selected, both layers close and the reference toast shows —
    /// without re-loading the file (the engine state IS the saved state).
    pub fn apply_saved_preset(&mut self, name: &str, path: &str, factory: Vec<PresetFixture>) {
        self.state.mark_applied();
        self.catalog = SceneCatalog::build(&factory, &self.saved_presets);
        if let Some(index) = self.find_preset_by_path(path) {
            self.selected_preset = index;
        }
        self.save_open = false;
        self.settings_open = false;
        self.toast = Some(format!("Saved preset \"{name}\"."));
    }

    /// Push a toast from the host (save errors, status messages).
    pub fn show_toast(&mut self, message: &str) {
        self.toast = Some(message.to_string());
    }

    /// The current toast message, when one is showing.
    pub fn toast_message(&self) -> Option<&str> {
        self.toast.as_deref()
    }

    /// Import is not wired in the preview: it explains itself in a toast.
    pub fn import_presets(&mut self) {
        self.toast = Some("Preview only: preset import is not wired.".into());
    }

    /// Export is not wired in the preview: it explains itself in a toast.
    pub fn export_presets(&mut self) {
        self.toast = Some("Preview only: preset export is not wired.".into());
    }

    /// The reference quits the app. The preview shows an explanatory toast and
    /// keeps running; the real host binary overrides the callback and calls
    /// `slint::quit_event_loop()` instead.
    pub fn quit_requested(&mut self) {
        self.settings_open = false;
        self.toast = Some("Preview only: the real app quits from here.".into());
    }

    /// Index of the drawn accent in `ACCENTS`, or -1 for a custom colour.
    pub fn accent_index(&self) -> i32 {
        if !self.custom_accent {
            return self.selected_accent as i32;
        }
        let drawn = self.display_accent();
        ACCENTS
            .iter()
            .position(|(argb, _)| *argb == drawn)
            .map(|index| index as i32)
            .unwrap_or(-1)
    }

    /// Caption shown next to "Appearance".
    pub fn accent_label(&self) -> &'static str {
        self.accent_name()
    }

    /// Pick a shipped accent and turn custom mode off, as the swatches do.
    pub fn choose_accent(&mut self, index: usize) {
        if let Some((argb, _)) = ACCENTS.get(index) {
            self.accent = *argb;
            self.custom_accent = false;
            self.selected_accent = index;
        }
    }

    /// The custom colour button: enables custom mode with the stored hex.
    pub fn choose_custom_accent(&mut self) {
        self.custom_accent = true;
    }

    /// Apply a hex colour typed into the custom accent field, mirroring the
    /// reference `<input type="color">` (`#rrggbb` or `rrggbb`). Invalid or
    /// unfinished text is rejected without touching the stored colour.
    pub fn set_custom_hex(&mut self, hex: &str) -> bool {
        let digits = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
        if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
        let Ok(rgb) = u32::from_str_radix(digits, 16) else {
            return false;
        };
        self.accent = 0xff00_0000 | rgb;
        self.custom_accent = true;
        true
    }

    /// The current accent as the canonical `#rrggbb` field text.
    pub fn custom_hex(&self) -> String {
        let [_, red, green, blue] = self.display_accent().to_be_bytes();
        format!("#{red:02x}{green:02x}{blue:02x}")
    }

    /// The reference version line in the settings footer. The Rust workspace
    /// tracks the shipped application version for parity.
    pub const APP_VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Dismiss the toast.
    pub fn clear_toast(&mut self) {
        self.toast = None;
    }

    /// Current toast text, if any.
    pub fn toast(&self) -> Option<&str> {
        self.toast.as_deref()
    }

    /// Push one declared synthetic frame; the history shifts as in the source.
    pub fn push_spectrum(&mut self, values: &[f32; SPECTRUM_BANDS], now_ms: u64) {
        self.history.push(values, now_ms);
    }

    /// Flat band-major bar heights for the drawn frame.
    pub fn spectrum_bars(&self) -> [f32; SPECTRUM_SLOTS] {
        self.history.bars()
    }

    /// Bar alpha for the supplied clock.
    pub fn spectrum_alpha(&self, now_ms: u64) -> f32 {
        self.history.alpha(now_ms)
    }

    /// Gradient edge and centre colours for the drawn accent.
    pub fn spectrum_colours(&self) -> (u32, u32) {
        let [_, red, green, blue] = self.display_accent().to_be_bytes();
        let edge = (0xff << 24)
            | ((red as u32 * 82 / 100) << 16)
            | ((green as u32 * 82 / 100) << 8)
            | (blue as u32 * 82 / 100);
        let lighten = |channel: u8| -> u32 {
            (channel as f32 + (255.0 - channel as f32) * 0.16).round() as u32
        };
        let centre = (0xff << 24) | (lighten(red) << 16) | (lighten(green) << 8) | lighten(blue);
        (edge, centre)
    }

    /// Menu rows for the preset selector.
    pub fn preset_rows(&self) -> &[MenuRow] {
        &self.catalog.presets
    }

    /// Menu rows for the output selector.
    pub fn output_rows(&self) -> &[MenuRow] {
        &self.catalog.outputs
    }

    /// Text for the effect value labels (one decimal, no trailing `.0`).
    pub fn effect_labels(&self) -> Vec<String> {
        (0..EFFECT_COUNT)
            .map(|index| {
                let value = self.state.effect(index).unwrap_or(0.0);
                if (value - value.round()).abs() < 0.05 {
                    format!("{}", value.round() as i32)
                } else {
                    format!("{value:.1}")
                }
            })
            .collect()
    }

    fn menu_entries(&self, rows: &[MenuRow], selected: usize) -> Vec<MenuEntry> {
        rows.iter()
            .enumerate()
            .map(|(index, row)| MenuEntry {
                label: row.label.clone().into(),
                detail: row.detail.clone().into(),
                is_group: row.is_group,
                disabled: row.disabled,
                selected: index == selected && !row.is_group,
            })
            .collect()
    }

    /// Content y of every menu row in px, for the popup's keyboard scroll.
    /// selectors.slint draws group rows 22px, entry rows 32px, with 1px
    /// spacing between rows; Slint pure functions cannot scan, so the host
    /// supplies the running offset.
    fn menu_offsets(rows: &[MenuRow]) -> Vec<f32> {
        let mut offsets = Vec::with_capacity(rows.len());
        let mut y = 0.0f32;
        for row in rows {
            offsets.push(y);
            y += if row.is_group { 23.0 } else { 33.0 };
        }
        offsets
    }

    /// First and last selectable row index, or -1 when nothing is selectable.
    /// Feeds the popup's Home/End handling, which cannot loop in Slint.
    fn menu_enabled_bounds(rows: &[MenuRow]) -> (i32, i32) {
        let first = rows
            .iter()
            .position(|row| !row.is_group && !row.disabled)
            .map_or(-1, |index| index as i32);
        let last = rows
            .iter()
            .rposition(|row| !row.is_group && !row.disabled)
            .map_or(-1, |index| index as i32);
        (first, last)
    }

    /// Hand the effect values to the window without replacing its model.
    ///
    /// `ModelRc::from(&[f32])` allocates a fresh `VecModel` on every call, and
    /// Slint's `Repeater` rebuilds every instantiated row whenever the model
    /// instance it observes changes (`ModelRc::eq` compares the model pointers).
    /// That rebuild destroyed the row that was holding the pointer grab, the
    /// double-click pair and the keyboard focus, so a drag stopped after its
    /// first step. Updating only the rows that actually changed notifies
    /// `row_changed` and leaves the instances alive.
    ///
    /// The model lives on the window, never on `UiBinding`, so cloned bindings
    /// and separate windows keep independent models instead of sharing one
    /// mutable view model.
    fn push_effect_values(window: &AppWindow, values: &[f32]) {
        let current = window.get_effect_values();
        if let Some(model) = current.as_any().downcast_ref::<slint::VecModel<f32>>()
            && model.row_count() == values.len()
        {
            for (index, value) in values.iter().enumerate() {
                if model.row_data(index) != Some(*value) {
                    model.set_row_data(index, *value);
                }
            }
            return;
        }
        window.set_effect_values(values.into());
    }

    /// Push the whole bound state into the real Slint window.
    pub fn sync(&self, window: &AppWindow) {
        window.set_power(self.state.power());
        Self::push_effect_values(
            window,
            &(0..EFFECT_COUNT)
                .map(|index| self.state.effect(index).unwrap_or(0.0))
                .collect::<Vec<f32>>(),
        );
        window.set_effect_names(
            EFFECT_NAMES
                .iter()
                .map(|name| (*name).into())
                .collect::<Vec<slint::SharedString>>()
                .as_slice()
                .into(),
        );
        window.set_effect_labels(
            self.effect_labels()
                .into_iter()
                .map(Into::into)
                .collect::<Vec<slint::SharedString>>()
                .as_slice()
                .into(),
        );
        window.set_edited(self.state.edited());
        window.set_power(self.state.power());
        window.set_eq_gains(
            (0..BAND_COUNT)
                .map(|index| self.state.eq_gain(index).unwrap_or(0.0))
                .collect::<Vec<f32>>()
                .as_slice()
                .into(),
        );
        let gains: [f32; BAND_COUNT] =
            std::array::from_fn(|index| self.state.eq_gain(index).unwrap_or(0.0));
        window.set_eq_curve(
            eq_curve_commands(&gains, geometry::EQ_VIEW_WIDTH, geometry::EQ_VIEW_HEIGHT).into(),
        );
        window.set_eq_curve_fill(
            eq_curve_fill_commands(&gains, geometry::EQ_VIEW_WIDTH, geometry::EQ_VIEW_HEIGHT)
                .into(),
        );
        window.set_eq_labels(
            EQ_LABELS
                .iter()
                .map(|label| (*label).into())
                .collect::<Vec<slint::SharedString>>()
                .as_slice()
                .into(),
        );
        window.set_eq_label_positions(EQ_LABEL_POSITIONS.to_vec().as_slice().into());
        window.set_eq_readout(self.eq_readout().into());
        window.set_spectrum_bars(self.spectrum_bars().to_vec().as_slice().into());
        let (edge, centre) = self.spectrum_colours();
        window.set_spectrum_edge(argb_to_color(edge));
        window.set_spectrum_centre(argb_to_color(centre));
        window.set_spectrum_alpha(self.history.alpha(now_ms()));
        window.set_preset_name(self.preset_name().into());
        window.set_preset_detail(
            if self
                .catalog
                .preset_values
                .get(self.selected_preset)
                .and_then(|value| value.as_ref())
                .is_some_and(|preset| preset.factory)
            {
                "Factory preset".into()
            } else {
                "Custom preset".into()
            },
        );
        window.set_output_name(self.output_name().into());
        // Menu models are pushed only when their content or selection
        // changed: the sync runs on every spectrum tick, and replacing the
        // model instance resets every open dropdown's repeater rows.
        let mut preset_cache = self.menu_cache_preset.borrow_mut();
        if preset_cache.0 != self.catalog.presets || preset_cache.1 != self.selected_preset {
            window.set_preset_entries(
                self.menu_entries(&self.catalog.presets, self.selected_preset)
                    .as_slice()
                    .into(),
            );
            window.set_preset_entry_offsets(
                Self::menu_offsets(&self.catalog.presets).as_slice().into(),
            );
            let (first, last) = Self::menu_enabled_bounds(&self.catalog.presets);
            window.set_preset_first_enabled(first);
            window.set_preset_last_enabled(last);
            window.set_preset_initial_highlight(self.selected_preset as i32);
            *preset_cache = (self.catalog.presets.clone(), self.selected_preset);
        }
        drop(preset_cache);
        let mut output_cache = self.menu_cache_output.borrow_mut();
        if output_cache.0 != self.catalog.outputs || output_cache.1 != self.selected_output {
            window.set_output_entries(
                self.menu_entries(&self.catalog.outputs, self.selected_output)
                    .as_slice()
                    .into(),
            );
            window.set_output_entry_offsets(
                Self::menu_offsets(&self.catalog.outputs).as_slice().into(),
            );
            let (first, last) = Self::menu_enabled_bounds(&self.catalog.outputs);
            window.set_output_first_enabled(first);
            window.set_output_last_enabled(last);
            window.set_output_initial_highlight(self.selected_output as i32);
            *output_cache = (self.catalog.outputs.clone(), self.selected_output);
        }
        drop(output_cache);
        window.set_preset_open(self.preset_open);
        window.set_output_open(self.output_open);
        window.set_settings_open(self.settings_open);
        window.set_save_open(self.save_open);
        window.set_save_name(self.save_name.clone().into());
        window.set_save_valid(self.save_valid());
        window.set_custom_accent(self.custom_accent);
        window.set_custom_hex_text(self.custom_hex().into());
        window.set_accent_index(self.accent_index());
        window.set_accent_label(self.accent_label().into());
        window.set_save_error(self.save_error().unwrap_or_default().into());
        window.set_save_help(if self.save_error().is_some() {
            "".into()
        } else {
            "The current effect and EQ values will be saved with this name.".into()
        });
        window.set_app_version(Self::APP_VERSION.into());
        window.set_light(self.theme == ThemeMode::Light);
        window.set_autostart(self.autostart);
        window.set_update_status(self.update_status.message().into());
        window.set_update_action_label(self.update_status.action_label().into());
        window.set_update_action_enabled(
            self.update_status.action_enabled() && !self.update_status.busy(),
        );
        window.set_update_busy(self.update_status.busy());
        window.set_toast_visible(self.toast.is_some());
        window.set_toast_message(self.toast.clone().unwrap_or_default().into());

        let palette = window.global::<Palette>();
        palette.set_light(self.theme == ThemeMode::Light);
        palette.set_accent(argb_to_color(self.display_accent()));
    }

    /// Install every real callback of the window, then sync once.
    pub fn install(window: &AppWindow) -> Rc<RefCell<Self>> {
        let binding = Rc::new(RefCell::new(Self::new()));
        macro_rules! wire {
            ($setter:ident, $body:expr) => {{
                let cell = Rc::clone(&binding);
                let weak = window.as_weak();
                let run: fn(&mut Self) = $body;
                window.$setter(move || {
                    if let Some(window) = weak.upgrade() {
                        let mut state = cell.borrow_mut();
                        run(&mut state);
                        state.sync(&window);
                    }
                });
            }};
        }

        wire!(on_power_toggled, |s: &mut Self| s.toggle_power());
        wire!(on_settings_clicked, |s: &mut Self| s.open_settings());
        wire!(on_settings_closed, |s: &mut Self| s.close_settings());
        wire!(on_reset_all, |s: &mut Self| s.reset_all());
        wire!(on_eq_reset, |s: &mut Self| s.reset_eq());
        wire!(on_preset_toggle, |s: &mut Self| s.toggle_preset_menu());
        wire!(on_output_toggle, |s: &mut Self| s.toggle_output_menu());
        wire!(on_menu_dismissed, |s: &mut Self| s.dismiss_menu());
        wire!(on_save_opened, |s: &mut Self| s.open_save());
        wire!(on_save_closed, |s: &mut Self| s.close_save());
        wire!(on_save_confirmed, |s: &mut Self| {
            s.confirm_save();
        });
        wire!(on_import_clicked, |s: &mut Self| s.import_presets());
        wire!(on_export_clicked, |s: &mut Self| s.export_presets());
        wire!(on_quit_clicked, |s: &mut Self| s.quit_requested());
        wire!(on_custom_accent_clicked, |s: &mut Self| {
            s.choose_custom_accent()
        });

        // Custom accent text: apply valid `#rrggbb` input, otherwise leave the
        // stored colour alone (the sync re-pushes the canonical field text).
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_custom_accent_text_changed(move |text| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.set_custom_hex(&text);
                    state.sync(&window);
                }
            });
        }

        // Window controls. The reference X hides to a running background
        // (Main.cpp close path), the minus minimizes and the title bar starts
        // a native drag; all three act on the real window, not on view state.
        // The preview binary runs `run_event_loop_until_quit`, so a hidden
        // window keeps the process alive exactly like the reference app.
        {
            let weak = window.as_weak();
            window.on_minimize_clicked(move || {
                if let Some(window) = weak.upgrade() {
                    window.window().with_winit_window(|native| {
                        native.set_minimized(true);
                    });
                }
            });
        }
        {
            let weak = window.as_weak();
            window.on_drag_requested(move || {
                if let Some(window) = weak.upgrade() {
                    window.window().with_winit_window(|native| {
                        let _ = native.drag_window();
                    });
                }
            });
        }
        {
            let weak = window.as_weak();
            window.on_close_clicked(move || {
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
            });
        }

        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_effect_reset(move |index| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.reset_effect(index.max(0) as usize);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_eq_band_reset(move |index| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.reset_eq_band(index.max(0) as usize);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_effect_changed(move |index, value| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.set_effect(index.max(0) as usize, value);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_eq_band_stepped(move |index, delta| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    let index = index.max(0) as usize;
                    if let Some(current) = state.state().eq_gain(index) {
                        state.set_eq_gain(index, current + delta as f32);
                    }
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_effect_stepped(move |index, delta| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    let index = index.max(0) as usize;
                    if let Some(current) = state.state().effect(index) {
                        state.set_effect(index, current + delta as f32);
                    }
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_eq_band_hovered(move |index| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.set_active_eq_band(Some(index.max(0) as usize));
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_eq_band_blurred(move |_index| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.set_active_eq_band(None);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_eq_band_changed(move |index, gain| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.set_eq_gain(index.max(0) as usize, gain);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_preset_chosen(move |index| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.choose_preset(index.max(0) as usize);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_output_chosen(move |index| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.choose_output(index.max(0) as usize);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_theme_changed(move |light| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.set_light(light);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_accent_chosen(move |index| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.choose_accent(index.max(0) as usize);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_autostart_changed(move |autostart| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.set_autostart(autostart);
                    state.sync(&window);
                }
            });
        }
        {
            let cell = Rc::clone(&binding);
            let weak = window.as_weak();
            window.on_save_name_changed(move |name| {
                if let Some(window) = weak.upgrade() {
                    let mut state = cell.borrow_mut();
                    state.set_save_name(name.as_str());
                    state.sync(&window);
                }
            });
        }

        binding.borrow().sync(window);
        binding
    }
}

/// Monotonic milliseconds for the preview clock.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Equal-index positions and dB->y mapping from `Equalizer.tsx`:
/// x = (index + 0.5) / 31 * 1000, y = TOP + (12 - gain) / 24 * (BOTTOM - TOP).
fn eq_points(gains: &[f32; BAND_COUNT], width: f32) -> Vec<(f64, f64)> {
    let top = f64::from(geometry::EQ_PLOT_TOP);
    let bottom = f64::from(geometry::EQ_PLOT_BOTTOM);
    (0..BAND_COUNT)
        .map(|index| {
            let x = (index as f64 + 0.5) / BAND_COUNT as f64 * f64::from(width);
            let gain = f64::from(gains[index].clamp(-12.0, 12.0));
            let y = top + (12.0 - gain) / 24.0 * (bottom - top);
            (x, y)
        })
        .collect()
}

/// The reference Equalizer.tsx `smoothPath`, ported exactly: Catmull-Rom
/// tangents `current +/- (neighbour gap) / 6` through points at band-cell
/// centres, with the start/end clamps (`previous` clamps to the first point,
/// `after` to the last). C1 by construction — the curvature jumps at knots
/// are the bendy character the reference shows; a natural cubic spline
/// minimises curvature and renders visibly flatter at small preset gains.
///
/// This is the gain-control trace, not the combined DSP frequency response.
/// Catmull-Rom can overshoot between bands. Do not clamp the interpolated
/// curve: doing so would create flat spots and corners.
fn eq_path(gains: &[f32; BAND_COUNT], width: f32) -> String {
    let points = eq_points(gains, width);

    // Six decimals keep serialized control points continuous well below a
    // pixel. Both stroke and fill reuse this path.
    let mut path = format!("M {:.6} {:.6}", points[0].0, points[0].1);
    for index in 0..points.len() - 1 {
        let current = points[index];
        let next = points[index + 1];
        let previous = points[index.saturating_sub(1)];
        let after = points.get(index + 2).unwrap_or(&next);
        path.push_str(&format!(
            " C {:.6} {:.6}, {:.6} {:.6}, {:.6} {:.6}",
            current.0 + (next.0 - previous.0) / 6.0,
            current.1 + (next.1 - previous.1) / 6.0,
            next.0 - (after.0 - current.0) / 6.0,
            next.1 - (after.1 - current.1) / 6.0,
            next.0,
            next.1
        ));
    }
    path
}

/// Build the EQ trace as the source's cubic path in its 1000x200 virtual space.
pub fn eq_curve_commands(gains: &[f32; BAND_COUNT], width: f32, _height: f32) -> String {
    eq_path(gains, width)
}

/// The same trace closed down to the plot bottom at both plot edges.
pub fn eq_curve_fill_commands(gains: &[f32; BAND_COUNT], width: f32, _height: f32) -> String {
    let bottom = geometry::EQ_PLOT_BOTTOM;
    format!(
        "{} L {:.2} {:.2} L 0.00 {:.2} Z",
        eq_path(gains, width),
        width,
        bottom,
        bottom
    )
}

/// ARGB word -> Slint colour.
pub fn argb_to_color(argb: u32) -> slint::Color {
    slint::Color::from_argb_encoded(argb)
}

/// Slint colour -> ARGB word with opaque alpha.
pub fn color_to_argb(color: slint::Color) -> u32 {
    0xff00_0000 | color.as_argb_encoded() & 0x00ff_ffff
}
