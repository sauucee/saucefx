//! `preset-session.txt` (schema version 1) codec.
//!
//! Mirrors `loadPresetSession` and `savePresetSessionNow` in
//! `src/gui/Main.cpp:607-700` and `src/gui/Main.cpp:3224-3272`. A session is
//! only usable when it is complete: version 1, all five effect fields, a band
//! count of 1..=32 and at least that many accepted freq and gain lines. An
//! incomplete or unknown-version file is reported as invalid instead of being
//! loaded as an all-zero session, so a caller can fall back to Default.

/// Effects in a session (`DfxDsp::NumEffects`).
pub const SESSION_EFFECT_COUNT: usize = 5;
/// The source keeps `std::array<float, 32>` for the band fields.
pub const SESSION_MAX_BANDS: usize = 32;
/// The only schema version the source accepts.
pub const SESSION_VERSION: i32 = 1;

/// Why a session file was rejected. These are the source's own conditions,
/// reported one at a time instead of collapsing into `valid = false`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    /// `version=` was missing or not 1.
    UnsupportedVersion(i32),
    /// Fewer than five distinct `effect0..4` fields.
    MissingEffects,
    /// `bands=` outside 1..=SESSION_MAX_BANDS. Raised when parsing a file and
    /// also when encoding a session whose band count does not fit the arrays.
    BandCountOutOfRange(usize),
    /// Fewer accepted `eq<i>freq=` lines than `bands=`.
    MissingFrequencies,
    /// Fewer accepted `eq<i>gain=` lines than `bands=`.
    MissingGains,
    /// Encoding a session whose effects or used band slots are NaN or infinite.
    /// Such text could never be read back (`std::isfinite` guard in the source
    /// parser), so it must not be saved as a success. Slots past `eq_bands` are
    /// unused and not checked.
    NonFiniteValue,
    /// Encoding a session whose preset path or factory name contains CR, LF or
    /// NUL. Names are written verbatim, so such a name would forge another
    /// field; it is refused instead of escaped, truncated or rewritten.
    UnrepresentableName,
}

/// A captured live session.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetSession {
    /// Selected preset path; empty when the selection is Default.
    pub preset_path: String,
    /// `factory-preset=` field, kept as written.
    pub factory_preset: Option<String>,
    /// Processing switch at capture time.
    pub power: bool,
    /// The five effect values on the 0..10 scale.
    pub effects: [f32; SESSION_EFFECT_COUNT],
    /// Number of EQ bands the file describes.
    pub eq_bands: usize,
    /// Band centres in Hz; only the first `eq_bands` entries are meaningful.
    pub eq_freqs: [f32; SESSION_MAX_BANDS],
    /// Band gains in dB; only the first `eq_bands` entries are meaningful.
    pub eq_gains: [f32; SESSION_MAX_BANDS],
}

impl PresetSession {
    /// The stable factory basename the source is willing to relocate by name:
    /// non-empty, no path separator and a `.fac` suffix. Whether that file
    /// exists is a filesystem question and deliberately left to the caller.
    pub fn factory_preset_basename(&self) -> Option<&str> {
        let candidate = self.factory_preset.as_deref()?;
        if candidate.is_empty()
            || candidate.contains(['/', '\\'])
            || !candidate.to_ascii_lowercase().ends_with(".fac")
        {
            return None;
        }
        Some(candidate)
    }
}

/// Parse `preset-session.txt`.
///
/// One leading U+FEFF encoding marker is consumed before the first key, the way
/// JUCE's `loadFileAsString` discards a UTF-8 BOM (`Main.cpp:631`,
/// `juce_String.cpp` `createStringFromData`). Otherwise a BOM-prefixed legacy
/// file would report `UnsupportedVersion(0)` and lose the whole session.
pub fn parse_session(text: &str) -> Result<PresetSession, SessionError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut version = 0i32;
    let mut preset_path = String::new();
    let mut factory_preset: Option<String> = None;
    let mut power = true;
    let mut effects = [0.0f32; SESSION_EFFECT_COUNT];
    let mut effect_seen = [false; SESSION_EFFECT_COUNT];
    let mut effects_seen = 0usize;
    let mut bands_raw = 0i32;
    let mut eq_freqs = [0.0f32; SESSION_MAX_BANDS];
    let mut eq_gains = [0.0f32; SESSION_MAX_BANDS];
    let mut freq_seen = 0usize;
    let mut gain_seen = 0usize;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(value) = line.strip_prefix("version=") {
            version = leading_int(value);
        } else if let Some(value) = line.strip_prefix("preset=") {
            preset_path = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("factory-preset=") {
            factory_preset = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("power=") {
            let v = value.trim();
            power = !v.eq_ignore_ascii_case("off") && v != "0";
        } else if let Some(value) = line.strip_prefix("bands=") {
            bands_raw = leading_int(value);
        } else if let Some(rest) = line.strip_prefix("effect") {
            let (key_tail, value) = rest.split_once('=').unwrap_or((rest, ""));
            let index = leading_int(key_tail);
            if let Some(v) = parse_bounded_float(value, 0, SESSION_EFFECT_COUNT, index) {
                let slot = index as usize;
                effects[slot] = v;
                if !effect_seen[slot] {
                    effect_seen[slot] = true;
                    effects_seen += 1;
                }
            }
        } else if let Some(rest) = line.strip_prefix("eq") {
            let (key_tail, value) = rest.split_once('=').unwrap_or((rest, ""));
            let index = leading_int(key_tail);
            if let Some(v) = parse_bounded_float(value, 0, SESSION_MAX_BANDS, index) {
                let slot = index as usize;
                if key_tail.contains("freq") {
                    eq_freqs[slot] = v;
                    freq_seen += 1;
                } else if key_tail.contains("gain") {
                    eq_gains[slot] = v;
                    gain_seen += 1;
                }
            }
        }
    }

    if version != SESSION_VERSION {
        return Err(SessionError::UnsupportedVersion(version));
    }
    if effects_seen != SESSION_EFFECT_COUNT {
        return Err(SessionError::MissingEffects);
    }
    let Ok(eq_bands) = usize::try_from(bands_raw) else {
        return Err(SessionError::BandCountOutOfRange(0));
    };
    if !(1..=SESSION_MAX_BANDS).contains(&eq_bands) {
        return Err(SessionError::BandCountOutOfRange(eq_bands));
    }
    if freq_seen < eq_bands {
        return Err(SessionError::MissingFrequencies);
    }
    if gain_seen < eq_bands {
        return Err(SessionError::MissingGains);
    }

    Ok(PresetSession {
        preset_path,
        factory_preset,
        power,
        effects,
        eq_bands,
        eq_freqs,
        eq_gains,
    })
}

/// Parse a float field, accepting it only when the index is inside
/// `0..limit` and the value is finite and parseable.
///
/// Note: the source calls JUCE `String::getFloatValue()`, which yields 0.0 for
/// text that is not a number (`src/gui/Main.cpp:658-662`), so a legacy
/// `effect0=abc` would have loaded as 0.0 there. This parser refuses such a
/// field instead, which makes the session invalid and lets the caller fall
/// back to Default. Deliberate, documented divergence — see the packet report.
fn parse_bounded_float(value: &str, min: i32, limit: usize, index: i32) -> Option<f32> {
    if index < min || index as usize >= limit {
        return None;
    }
    let parsed = value.trim().parse::<f32>().ok()?;
    parsed.is_finite().then_some(parsed)
}

/// Leading signed decimal digits, zero when there are none (`getIntValue`).
fn leading_int(value: &str) -> i32 {
    let trimmed = value.trim_start();
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let mut parsed = 0i32;
    let mut any = false;
    for digit in digits.chars() {
        let Some(nibble) = digit.to_digit(10) else {
            break;
        };
        any = true;
        parsed = parsed.saturating_mul(10).saturating_add(nibble as i32);
    }
    if !any {
        return 0;
    }
    if negative {
        parsed.saturating_neg()
    } else {
        parsed
    }
}

/// Whether `name` can be written into a single `key=value` line.
fn is_single_line(name: &str) -> bool {
    !name.contains(['\r', '\n', '\0'])
}

/// Render `preset-session.txt`, LF-terminated, with six-decimal numbers.
///
/// Returns [`SessionError::BandCountOutOfRange`] when `eq_bands` is 0 or larger
/// than the arrays can hold, instead of indexing past them, and
/// [`SessionError::NonFiniteValue`] when a value that will be written (every
/// effect, plus the band slots below `eq_bands`) is not finite. Nothing is
/// truncated or invented: an unencodable session simply is not encodable.
pub fn encode_session(session: &PresetSession) -> Result<String, SessionError> {
    if !(1..=SESSION_MAX_BANDS).contains(&session.eq_bands) {
        return Err(SessionError::BandCountOutOfRange(session.eq_bands));
    }
    if !session.effects.iter().all(|value| value.is_finite())
        || !session.eq_freqs[..session.eq_bands]
            .iter()
            .all(|value| value.is_finite())
        || !session.eq_gains[..session.eq_bands]
            .iter()
            .all(|value| value.is_finite())
    {
        return Err(SessionError::NonFiniteValue);
    }
    let names_representable = session.factory_preset.as_deref().is_none_or(is_single_line);
    if !is_single_line(&session.preset_path) || !names_representable {
        return Err(SessionError::UnrepresentableName);
    }
    let mut text = String::from("version=1\n");
    if !session.preset_path.is_empty() {
        text.push_str(&format!("preset={}\n", session.preset_path));
    }
    if let Some(factory_preset) = session.factory_preset.as_deref() {
        text.push_str(&format!("factory-preset={factory_preset}\n"));
    }
    text.push_str(&format!("power={}\n", u8::from(session.power)));
    for (index, value) in session.effects.iter().enumerate() {
        text.push_str(&format!("effect{index}={:.6}\n", f64::from(*value)));
    }
    text.push_str(&format!("bands={}\n", session.eq_bands));
    for band in 0..session.eq_bands {
        let freq = f64::from(session.eq_freqs[band]);
        let gain = f64::from(session.eq_gains[band]);
        text.push_str(&format!("eq{band}freq={freq:.6}\n"));
        text.push_str(&format!("eq{band}gain={gain:.6}\n"));
    }
    Ok(text)
}
