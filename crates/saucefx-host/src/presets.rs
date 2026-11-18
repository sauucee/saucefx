//! .fac library scanning, mirroring the reference `findPresets()`:
//! the factory library directory first, then the user's imported presets,
//! each sorted case-insensitively by file name.

use std::path::{Path, PathBuf};

use saucefx_core::state::{BAND_COUNT, EFFECT_COUNT};
use saucefx_ui::binding::PresetFixture;

/// One scanned .fac file with the directory kind it came from.
pub struct ScannedPreset {
    pub path: PathBuf,
    pub factory: bool,
}

/// Scan one directory for .fac files sorted by file name (case-insensitive),
/// like `findPresetsIn` in Main.cpp.
fn scan_dir(dir: &Path, factory: bool) -> Vec<ScannedPreset> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("fac"))
        {
            out.push(ScannedPreset { path, factory });
        }
    }
    out.sort_by_key(|preset| {
        preset
            .path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    out
}

/// Factory library first, then the user's imported presets.
pub fn scan(factory_dir: &Path, user_dir: &Path) -> Vec<ScannedPreset> {
    let mut out = scan_dir(factory_dir, true);
    out.extend(scan_dir(user_dir, false));
    out
}

/// Build menu fixtures from the scanned files. Display names come from the
/// engine (`fxpw_preset_name`, falling back to the file stem); values stay
/// zeroed because a selection loads the .fac through the engine.
pub fn fixtures(engine: &super::fxpw::FxpwEngine, scanned: &[ScannedPreset]) -> Vec<PresetFixture> {
    scanned
        .iter()
        .map(|preset| {
            let stem = preset
                .path
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let label = engine.preset_display_name(&preset.path).unwrap_or(stem);
            PresetFixture {
                label,
                factory: preset.factory,
                effects: [0.0; EFFECT_COUNT],
                eq: [0.0; BAND_COUNT],
                path: preset.path.to_string_lossy().into_owned(),
            }
        })
        .collect()
}

/// Sanitize a raw Save-dialog name into a safe .fac display name, mirroring
/// `sanitizedPresetName` in Main.cpp: trim, replace filename-unsafe
/// punctuation with `_`, control characters with spaces, cap at 80 chars,
/// and reject `.` / `..`. Empty means "invalid".
pub fn sanitized_preset_name(raw: &str) -> String {
    let mut name = raw.trim().to_string();
    if name.is_empty() {
        return String::new();
    }
    for ch in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
        name = name.replace(ch, "_");
    }
    name = name.replace(['\t', '\r', '\n'], " ");
    name = name
        .chars()
        .map(|c| if (c as u32) < 32 { ' ' } else { c })
        .collect();
    name = name.chars().take(80).collect::<String>().trim().to_string();
    if name == "." || name == ".." {
        return String::new();
    }
    name
}

/// The display-name-uniqueness path from `uniqueCustomPresetName` in
/// Main.cpp: neither an existing user file nor any catalog display name
/// (case-insensitive) may be shadowed, so colliding names get
/// `" (2)"`…" (999)"` suffixes and a hex timestamp as the last resort.
pub fn unique_custom_preset_name(base: &str, user_dir: &Path, display_labels: &[String]) -> String {
    let is_taken = |name: &str| -> bool {
        user_dir.join(format!("{name}.fac")).is_file()
            || display_labels
                .iter()
                .any(|label| label.eq_ignore_ascii_case(name))
    };
    if !is_taken(base) {
        return base.to_string();
    }
    for n in 2..1000 {
        let candidate = format!("{base} ({n})");
        if !is_taken(&candidate) {
            return candidate;
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    format!("{base} {stamp:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitization_trims_and_replaces_unsafe_characters() {
        assert_eq!(sanitized_preset_name("  Evening  "), "Evening");
        assert_eq!(sanitized_preset_name("a/b:c*d"), "a_b_c_d");
        assert_eq!(sanitized_preset_name("tab\tname"), "tab name");
        assert_eq!(sanitized_preset_name("q\"x<y>|z"), "q_x_y__z");
    }

    #[test]
    fn sanitization_rejects_invalid_names() {
        assert_eq!(sanitized_preset_name("   "), "");
        assert_eq!(sanitized_preset_name("."), "");
        assert_eq!(sanitized_preset_name(".."), "");
    }

    #[test]
    fn sanitization_caps_at_eighty_characters() {
        let long = "x".repeat(100);
        assert_eq!(sanitized_preset_name(&long).chars().count(), 80);
    }

    #[test]
    fn unique_names_pass_through_when_free() {
        let dir = std::env::temp_dir().join("saucefx-unique-free");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_custom_preset_name("Evening", &dir, &[]), "Evening");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unique_names_suffix_existing_files_then_labels() {
        let dir = std::env::temp_dir().join("saucefx-unique-taken");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Evening.fac"), "x").unwrap();
        // A file collision gets the (2) suffix.
        assert_eq!(
            unique_custom_preset_name("Evening", &dir, &[]),
            "Evening (2)"
        );
        // A display-name collision (case-insensitive) suffixes too.
        assert_eq!(
            unique_custom_preset_name("evening", &dir, &["Evening".into()]),
            "evening (2)"
        );
        std::fs::write(dir.join("Evening (2).fac"), "x").unwrap();
        assert_eq!(
            unique_custom_preset_name("Evening", &dir, &[]),
            "Evening (3)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
