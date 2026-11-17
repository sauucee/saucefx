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
