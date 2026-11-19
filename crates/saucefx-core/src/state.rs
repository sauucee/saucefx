//! Pure UI/engine value state: power, the five effect levels, the 31 EQ bands.
//!
//! Ranges and defaults come from the existing app and engine, not from a new
//! design: effect values are the UI's 0..10 scale pushed to the bridge
//! (`src/pwbridge/fxpw.cpp:1168-1185`), EQ gain is the engine's +-12 dB
//! (`dsp/ptutil/include/dfxpDefs.h:265`) and the 31 band centres are the
//! engine's own list (`dsp/ptutil/DspUtil/GraphicEq/GraphicEqSet.cpp:484-486`)
//! which the reference UI also pins (`frontend/src/data.ts:46-51`).

/// Number of DSP effects (`DfxDsp::NumEffects`).
pub const EFFECT_COUNT: usize = 5;
/// Number of graphic-EQ bands (`DFXP_GRAPHIC_EQ_NUM_BANDS`).
pub const BAND_COUNT: usize = 31;

/// Effect levels are the UI's 0..10 scale.
pub const EFFECT_MIN: f32 = 0.0;
/// `fxpw_set_effect_value` clamps at 10.0 and the engine stores value/10.
pub const EFFECT_MAX: f32 = 10.0;

/// EQ boost/cut range in dB (`DFXP_GRAPHIC_EQ_MAX_BOOST_OR_CUT_DB`).
pub const EQ_GAIN_MIN: f32 = -12.0;
/// Upper EQ boost/cut bound in dB.
pub const EQ_GAIN_MAX: f32 = 12.0;

/// Engine band-frequency limits (`GraphicEqSet.cpp:179-188`).
pub const EQ_FREQ_MIN: f32 = 20.0;
/// Upper band-frequency bound in Hz.
pub const EQ_FREQ_MAX: f32 = 20000.0;

/// The 31 band centres, in Hz, exactly as the reference UI and the engine use
/// them.
pub const EQ_FREQUENCIES: [f32; BAND_COUNT] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
    500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0,
    8000.0, 10000.0, 12500.0, 16000.0, 20000.0,
];

/// A refused edit. The previous value is always kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    /// Effect index outside 0..EFFECT_COUNT.
    EffectIndex(usize),
    /// EQ band index outside 0..BAND_COUNT.
    BandIndex(usize),
    /// NaN or infinite effect value.
    NonFiniteEffect,
    /// Effect value outside EFFECT_MIN..=EFFECT_MAX.
    EffectOutOfRange,
    /// NaN or infinite EQ gain.
    NonFiniteGain,
    /// EQ gain outside EQ_GAIN_MIN..=EQ_GAIN_MAX.
    GainOutOfRange,
    /// NaN or infinite band frequency.
    NonFiniteFrequency,
    /// Band frequency outside EQ_FREQ_MIN..=EQ_FREQ_MAX.
    FrequencyOutOfRange,
}

/// Output preference/activity, mirroring `FXPWOutputState` (`fxpw.h:79-85`).
/// Names, not descriptions, identify a device; they are kept verbatim.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutputState {
    /// Remembered device name; empty means automatic.
    pub preferred_name: String,
    /// Remembered port name on that device.
    pub preferred_port: String,
    /// Device currently being rendered to.
    pub active_name: String,
    /// Port currently active on that device.
    pub active_port: String,
    /// Whether the system default sink is routed through SauceFX.
    pub routed: bool,
}

/// Power, the five effect values, the 31 EQ bands and the applied baseline.
///
/// Power is independent of the effect values and of output routing, as in the
/// bridge: `fxpw_set_power` only calls `dsp->powerOn` and output calls only
/// touch routing state.
#[derive(Clone, Debug, PartialEq)]
pub struct UiState {
    power: bool,
    effects: [f32; EFFECT_COUNT],
    eq_freqs: [f32; BAND_COUNT],
    eq_gains: [f32; BAND_COUNT],
    applied_effects: [f32; EFFECT_COUNT],
    applied_eq_gains: [f32; BAND_COUNT],
    output: OutputState,
}

impl Default for UiState {
    /// `Default` and [`UiState::new`] are the same constructor, so no fallback
    /// path can produce the invalid all-zero/power-off state.
    fn default() -> Self {
        Self::new()
    }
}

impl UiState {
    /// Startup state: power on, effects 0, flat EQ at the pinned band centres.
    /// Also what `Default` builds.
    pub fn new() -> Self {
        Self {
            power: true,
            effects: [0.0; EFFECT_COUNT],
            eq_freqs: EQ_FREQUENCIES,
            eq_gains: [0.0; BAND_COUNT],
            applied_effects: [0.0; EFFECT_COUNT],
            applied_eq_gains: [0.0; BAND_COUNT],
            output: OutputState::default(),
        }
    }

    /// Current power switch.
    pub fn power(&self) -> bool {
        self.power
    }

    /// Power is independent of every other value.
    pub fn set_power(&mut self, on: bool) {
        self.power = on;
    }

    /// One effect value on the 0..10 scale.
    pub fn effect(&self, index: usize) -> Option<f32> {
        self.effects.get(index).copied()
    }

    /// Set one effect value on the 0..10 scale.
    pub fn set_effect(&mut self, index: usize, value: f32) -> Result<(), EditError> {
        if index >= EFFECT_COUNT {
            return Err(EditError::EffectIndex(index));
        }
        if !value.is_finite() {
            return Err(EditError::NonFiniteEffect);
        }
        if value < EFFECT_MIN || value > EFFECT_MAX {
            return Err(EditError::EffectOutOfRange);
        }
        self.effects[index] = value;
        Ok(())
    }

    /// One EQ band gain in dB.
    pub fn eq_gain(&self, index: usize) -> Option<f32> {
        self.eq_gains.get(index).copied()
    }

    /// Set one EQ band gain in dB.
    pub fn set_eq_gain(&mut self, index: usize, gain: f32) -> Result<(), EditError> {
        if index >= BAND_COUNT {
            return Err(EditError::BandIndex(index));
        }
        if !gain.is_finite() {
            return Err(EditError::NonFiniteGain);
        }
        if gain < EQ_GAIN_MIN || gain > EQ_GAIN_MAX {
            return Err(EditError::GainOutOfRange);
        }
        self.eq_gains[index] = gain;
        Ok(())
    }

    /// One EQ band centre in Hz.
    pub fn eq_frequency(&self, index: usize) -> Option<f32> {
        self.eq_freqs.get(index).copied()
    }

    /// All 31 band centres in Hz.
    pub fn eq_frequencies(&self) -> [f32; BAND_COUNT] {
        self.eq_freqs
    }

    /// Set one EQ band centre in Hz.
    pub fn set_eq_frequency(&mut self, index: usize, hz: f32) -> Result<(), EditError> {
        if index >= BAND_COUNT {
            return Err(EditError::BandIndex(index));
        }
        if !hz.is_finite() {
            return Err(EditError::NonFiniteFrequency);
        }
        if hz < EQ_FREQ_MIN || hz > EQ_FREQ_MAX {
            return Err(EditError::FrequencyOutOfRange);
        }
        self.eq_freqs[index] = hz;
        Ok(())
    }

    /// Current output preference and activity.
    pub fn output(&self) -> &OutputState {
        &self.output
    }

    /// One effect value from the applied baseline.
    pub fn applied_effect(&self, index: usize) -> Option<f32> {
        self.applied_effects.get(index).copied()
    }

    /// One EQ gain from the applied baseline.
    pub fn applied_eq_gain(&self, index: usize) -> Option<f32> {
        self.applied_eq_gains.get(index).copied()
    }

    /// Whether the current effects/EQ differ from the applied baseline.
    pub fn edited(&self) -> bool {
        self.effects != self.applied_effects || self.eq_gains != self.applied_eq_gains
    }

    /// Treat the current effects/EQ as the applied baseline.
    pub fn mark_applied(&mut self) {
        self.applied_effects = self.effects;
        self.applied_eq_gains = self.eq_gains;
    }

    /// Remember which output device/port the user prefers. Never touches
    /// power, the effect values, the EQ values or the applied baseline.
    pub fn set_output_preference(&mut self, name: &str, port: &str) {
        self.output.preferred_name = name.to_string();
        self.output.preferred_port = port.to_string();
    }

    /// Record the output that is actually being rendered to. Never touches the
    /// baseline either, so a routing change is not an edited setting.
    pub fn set_active_output(&mut self, name: &str, port: &str, routed: bool) {
        self.output.active_name = name.to_string();
        self.output.active_port = port.to_string();
        self.output.routed = routed;
    }
}
