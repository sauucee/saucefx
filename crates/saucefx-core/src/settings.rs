//! `settings.txt` codec.
//!
//! Mirrors `loadGuiSettings` / `saveGuiSettings` in `src/gui/Main.cpp:537-605`:
//! six keys, LF-terminated, split at the first `=`. Keys are matched
//! case-sensitively as prefixes (`"theme="`), values are trimmed for
//! theme/accent/autostart/output but kept verbatim for the device and port
//! names so a sink identity is never replaced by its description.

/// Dark/light theme, stored as `dark` / `light`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    /// Dark theme (`theme=dark`, the source's mode 0 and default).
    #[default]
    Dark,
    /// Light theme (`theme=light`, source mode 1).
    Light,
}

/// The six persisted GUI settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuiSettings {
    /// Saved theme.
    pub theme: ThemeMode,
    /// Whether `accent` is a user colour rather than the default.
    pub custom_accent: bool,
    /// Accent colour as ARGB with alpha forced to `0xff`, exactly like the
    /// source's `0xff000000u | rgb`.
    pub accent: u32,
    /// Login autostart preference (`autostart=on|off`).
    pub autostart: bool,
    /// Whether the system default output is routed through SauceFX
    /// (`output=saucefx|system`).
    pub output_routed: bool,
    /// Remembered output device name, verbatim (`output-device=`).
    pub output_device: String,
    /// Remembered output port name, verbatim (`output-port=`).
    pub output_port: String,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            custom_accent: false,
            accent: 0xffe6_3462,
            autostart: false,
            output_routed: false,
            output_device: String::new(),
            output_port: String::new(),
        }
    }
}

/// Parse `settings.txt`.
///
/// One leading U+FEFF encoding marker is consumed before the first key, the way
/// JUCE's `loadFileAsString` discards a UTF-8 BOM (`Main.cpp:544`,
/// `juce_String.cpp` `createStringFromData`). It is only the first character of
/// the file: markers inside values are left alone.
pub fn parse_settings(text: &str) -> GuiSettings {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut settings = GuiSettings::default();
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(value) = line.strip_prefix("theme=") {
            settings.theme = if value.trim().eq_ignore_ascii_case("light") {
                ThemeMode::Light
            } else {
                ThemeMode::Dark
            };
        } else if let Some(value) = line.strip_prefix("accent=") {
            apply_accent(&mut settings, value.trim());
        } else if let Some(value) = line.strip_prefix("autostart=") {
            settings.autostart = !value.trim().eq_ignore_ascii_case("off");
        } else if let Some(value) = line.strip_prefix("output=") {
            settings.output_routed = value.trim().eq_ignore_ascii_case("saucefx");
        } else if let Some(value) = line.strip_prefix("output-device=") {
            settings.output_device = value.to_string();
        } else if let Some(value) = line.strip_prefix("output-port=") {
            settings.output_port = value.to_string();
        }
    }
    settings
}

/// `accent=` handling: `default`/empty clears the custom flag (the previously
/// parsed colour is kept, as in the source), otherwise a `#`-prefixed
/// six-character hex colour is parsed with the alpha forced opaque.
fn apply_accent(settings: &mut GuiSettings, value: &str) {
    if value.is_empty() || value.eq_ignore_ascii_case("default") {
        settings.custom_accent = false;
        return;
    }
    let digits = value.strip_prefix('#').unwrap_or(value);
    if digits.chars().count() != 6 {
        return;
    }
    settings.accent = 0xff00_0000 | (hex_digits(digits) & 0x00ff_ffff);
    settings.custom_accent = true;
}

/// Every hex digit in `value`, in order, zero when there are none.
///
/// This is the behaviour of JUCE's `CharacterFunctions::HexParser`, which
/// ignores non-hex characters rather than terminating the parse; the source
/// calls it through `String::getHexValue32()` (`Main.cpp:551-563`).
fn hex_digits(value: &str) -> u32 {
    let mut parsed = 0u32;
    for digit in value.chars() {
        if let Some(nibble) = digit.to_digit(16) {
            parsed = (parsed << 4) | nibble;
        }
    }
    parsed
}

/// Why a settings text cannot be written. Names are stored verbatim, so a name
/// containing a line break or NUL would forge a key line or break the C string
/// the native side reads; such a name is refused instead of escaped, truncated
/// or rewritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// `output_device` contains CR, LF or NUL.
    UnrepresentableDeviceName,
    /// `output_port` contains CR, LF or NUL.
    UnrepresentablePortName,
}

/// Whether `name` can be written into a single `key=value` line.
fn is_single_line(name: &str) -> bool {
    !name.contains(['\r', '\n', '\0'])
}

/// Render `settings.txt`, LF-terminated, ending with a final newline.
///
/// Returns an error when a stored identity string cannot be represented as one
/// line; every other setting is still written exactly as before.
pub fn encode_settings(settings: &GuiSettings) -> Result<String, SettingsError> {
    if !is_single_line(&settings.output_device) {
        return Err(SettingsError::UnrepresentableDeviceName);
    }
    if !is_single_line(&settings.output_port) {
        return Err(SettingsError::UnrepresentablePortName);
    }
    let theme = match settings.theme {
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
    };
    let accent = if settings.custom_accent {
        format!("#{:06x}", settings.accent & 0x00ff_ffff)
    } else {
        "default".to_string()
    };
    Ok(format!(
        "theme={theme}\n\
         accent={accent}\n\
         autostart={}\n\
         output={}\n\
         output-device={}\n\
         output-port={}\n",
        if settings.autostart { "on" } else { "off" },
        if settings.output_routed {
            "saucefx"
        } else {
            "system"
        },
        settings.output_device,
        settings.output_port
    ))
}
