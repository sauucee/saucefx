//! .fac library scanning, mirroring the reference `findPresets()`:
//! the factory library directory first, then the user's imported presets,
//! each sorted case-insensitively by file name.

use std::path::{Path, PathBuf};

use saucefx_core::state::{BAND_COUNT, EFFECT_COUNT};
use saucefx_ui::binding::PresetFixture;

/// The factory .fac library: launch override, source checkout, installed data.
pub fn library_dir(root: Option<&Path>, exe: Option<&Path>) -> PathBuf {
    if let Some(root) = root {
        return root.join("presets").join("Factsoft");
    }
    if let Some(exe) = exe {
        for current in exe.ancestors().skip(1) {
            let candidate = current.join("presets").join("Factsoft");
            if candidate.is_dir() {
                return candidate;
            }
        }
        // Installed layouts, including Flatpak's /app/bin and /app/share.
        for current in exe.ancestors().skip(1) {
            let candidate = current.join("share/saucefx/presets/Factsoft");
            if candidate.is_dir() {
                return candidate;
            }
        }
    }
    PathBuf::from("/usr/share/saucefx")
        .join("presets")
        .join("Factsoft")
}

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

/// One file's display name as the engine reports it: `Some(trimmed name)`
/// when the engine read succeeded (empty string = fall back to the stem),
/// `None` when the file could not be read at all ("not readable").
pub type ResolvedPresetEntry = (PathBuf, Option<String>);

pub struct ImportOutcome {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub last: Option<PathBuf>,
}

/// Mirror of `importPresets(files)` in Main.cpp: copy the selected .fac
/// files into the user preset directory, skipping case-insensitive display
/// duplicates and unreadable files.
pub fn import_preset_files(
    entries: &[ResolvedPresetEntry],
    user_dir: &Path,
    existing_labels: &[String],
) -> ImportOutcome {
    let mut existing: Vec<String> = existing_labels
        .iter()
        .map(|label| label.trim().to_lowercase())
        .collect();
    let mut outcome = ImportOutcome {
        imported: 0,
        skipped: 0,
        failed: 0,
        last: None,
    };
    for (source, display) in entries {
        let Some(raw) = display else {
            outcome.failed += 1;
            continue;
        };
        let mut name = raw.trim().to_string();
        if name.is_empty() {
            name = source
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        if existing.contains(&name.to_lowercase()) {
            outcome.skipped += 1;
            continue;
        }
        let mut safe_name = sanitize_filename(&name);
        if safe_name.is_empty() {
            safe_name = "Preset".to_string();
        }
        let mut target = user_dir.join(format!("{safe_name}.fac"));
        let mut n = 2;
        while target.exists() {
            target = user_dir.join(format!("{safe_name} {n}.fac"));
            n += 1;
        }
        if std::fs::copy(source, &target).is_err() {
            outcome.failed += 1;
            continue;
        }
        existing.push(name.to_lowercase());
        outcome.last = Some(target);
        outcome.imported += 1;
    }
    outcome
}

/// The filename-safe transformation shared with the Save dialog
/// (`replaceCharacters("/\\:*?\"<>|", "_________")` in Main.cpp).
fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => out.push('_'),
            other => out.push(other),
        }
    }
    out
}

/// The composite import toast, mirroring `importPresets`' message assembly
/// in Main.cpp ("Imported N presets", ", skipped N duplicates",
/// ", N not readable", or "No presets imported.").
pub fn import_outcome_message(outcome: &ImportOutcome) -> String {
    let mut message = String::new();
    if outcome.imported > 0 {
        message.push_str(&format!(
            "Imported {} {}",
            outcome.imported,
            if outcome.imported == 1 {
                "preset"
            } else {
                "presets"
            }
        ));
    }
    if outcome.skipped > 0 {
        if !message.is_empty() {
            message.push_str(", ");
        }
        message.push_str(&format!(
            "skipped {} {}",
            outcome.skipped,
            if outcome.skipped == 1 {
                "duplicate"
            } else {
                "duplicates"
            }
        ));
    }
    if outcome.failed > 0 {
        if !message.is_empty() {
            message.push_str(", ");
        }
        message.push_str(&format!("{} not readable", outcome.failed));
    }
    if message.is_empty() {
        message.push_str("No presets imported.");
    }
    message
}

/// The suggested save target for the export chooser
/// (`chooseExportPreset` in Main.cpp): the selected preset's display name
/// in the home directory, "SauceFX" when the label is empty.
pub fn export_suggested_file(label: &str, home: &Path) -> PathBuf {
    let name = label.trim();
    let name = if name.is_empty() { "SauceFX" } else { name };
    home.join(format!("{name}.fac"))
}

/// The export toast (`exportCurrentPreset` in Main.cpp):
/// "Exported <file name>" on success, "Export failed: <full path>" otherwise.
pub fn export_message(target: &Path, ok: bool) -> String {
    if ok {
        let name = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("Exported {name}")
    } else {
        format!("Export failed: {}", target.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn installed_flatpak_discovers_factory_default() {
        let dir =
            std::env::temp_dir().join(format!("saucefx-flatpak-library-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("app/bin/SauceFX");
        let factory = dir.join("app/share/saucefx/presets/Factsoft");
        std::fs::create_dir_all(&factory).unwrap();
        let default = factory.join("Default.fac");
        std::fs::write(&default, "CLASS1 : Effect Type\n9: Version\nDefault\n").unwrap();

        let resolved = library_dir(None, Some(&exe));
        let scanned = scan(&resolved, &dir.join("user"));
        let found = scanned
            .iter()
            .any(|preset| preset.path == default && preset.factory);
        std::fs::remove_dir_all(&dir).unwrap();

        assert!(
            found,
            "factory Default must be discovered beside the installed prefix; resolved {}",
            resolved.display()
        );
        assert_eq!(resolved, factory);
        assert_eq!(scanned.len(), 1);
    }

    #[test]
    fn importing_a_readable_preset_copies_it_and_reports_one_import() {
        let dir = std::env::temp_dir().join("saucefx-import-happy");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("staged").join("Evening.fac");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "CLASS1 : Effect Type\n").unwrap();
        let user = dir.join("user");
        std::fs::create_dir_all(&user).unwrap();

        let entries = vec![(source.clone(), Some("Evening listen".to_string()))];
        let outcome = import_preset_files(&entries, &user, &[]);

        assert_eq!(outcome.imported, 1, "the readable file imports");
        assert_eq!(outcome.skipped, 0);
        assert_eq!(outcome.failed, 0);
        let target = user.join("Evening listen.fac");
        assert!(
            target.is_file() && std::fs::read_to_string(&target).unwrap().contains("CLASS1"),
            "the .fac content is copied to the user directory"
        );
        assert_eq!(outcome.last, Some(target));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn importing_skips_case_insensitive_duplicates_and_in_batch_names() {
        let dir = std::env::temp_dir().join("saucefx-import-dup");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let user = dir.join("user");
        std::fs::create_dir_all(&user).unwrap();
        let make = |name: &str| {
            let source = dir.join(format!("{name}.fac"));
            std::fs::write(&source, "x").unwrap();
            (source, Some(name.to_string()))
        };

        let entries = vec![
            make("Evening"),
            make("evening"), // case-insensitive duplicate of the first
            make("Late"),    // collides only after "Late" imports earlier in the batch
            make("Late"),
        ];
        // "Late" vs the existing label "evening listen" is a different name;
        // the batch-internal collision is what the last entry tests.
        let outcome = import_preset_files(&entries, &user, &["Evening Listen".into()]);

        assert_eq!(outcome.imported, 2, "Evening and the first Late import");
        assert_eq!(outcome.skipped, 2, "both duplicates are skipped");
        assert_eq!(outcome.failed, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Regression locks for the C++-ported semantics that were implemented in
    // the first GREEN cycle (these pass immediately by design; they pin the
    // behavior so it cannot drift).
    #[test]
    fn unreadable_files_count_as_failed_and_copy_nothing() {
        let dir = std::env::temp_dir().join("saucefx-import-failed");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let user = dir.join("user");
        std::fs::create_dir_all(&user).unwrap();
        let entries = vec![(dir.join("gone.fac"), None)];
        let outcome = import_preset_files(&entries, &user, &[]);
        assert_eq!(
            (outcome.imported, outcome.skipped, outcome.failed),
            (0, 0, 1)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn target_collisions_get_numeric_suffixes() {
        let dir = std::env::temp_dir().join("saucefx-import-suffix");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let user = dir.join("user");
        std::fs::create_dir_all(&user).unwrap();
        // A bare file on disk (no catalog label): the target exists, so the
        // copy gets the numeric suffix instead of overwriting.
        std::fs::write(user.join("Preset.fac"), "old").unwrap();
        let source = dir.join("new.fac");
        std::fs::write(&source, "new").unwrap();
        let entries = vec![(source, Some("Preset".to_string()))];
        let outcome = import_preset_files(&entries, &user, &[]);
        assert_eq!(outcome.imported, 1);
        let second = user.join("Preset 2.fac");
        assert!(second.is_file(), "the suffix chain avoids the collision");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "new");
        assert_eq!(
            std::fs::read_to_string(user.join("Preset.fac")).unwrap(),
            "old"
        );
        assert_eq!(outcome.last, Some(second));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_messages_follow_the_reference_wording() {
        let message = |imported: usize, skipped: usize, failed: usize| {
            import_outcome_message(&ImportOutcome {
                imported,
                skipped,
                failed,
                last: None,
            })
        };
        assert_eq!(message(1, 0, 0), "Imported 1 preset");
        assert_eq!(message(3, 0, 0), "Imported 3 presets");
        assert_eq!(message(2, 1, 0), "Imported 2 presets, skipped 1 duplicate");
        assert_eq!(message(2, 2, 0), "Imported 2 presets, skipped 2 duplicates");
        assert_eq!(message(1, 0, 2), "Imported 1 preset, 2 not readable");
        assert_eq!(message(0, 0, 0), "No presets imported.");
        assert_eq!(message(0, 1, 1), "skipped 1 duplicate, 1 not readable");
    }

    #[test]
    fn export_suggestion_falls_back_to_saucefx_in_the_home_directory() {
        let home = std::path::Path::new("/home/u");
        assert_eq!(
            export_suggested_file("Evening listen", home),
            home.join("Evening listen.fac"),
            "the selected preset's display name suggests the file"
        );
        assert_eq!(
            export_suggested_file("", home),
            home.join("SauceFX.fac"),
            "an empty label falls back to SauceFX"
        );
    }

    #[test]
    fn export_messages_follow_the_reference_wording() {
        assert_eq!(
            export_message(&std::path::Path::new("/x/Evening.fac"), true),
            "Exported Evening.fac"
        );
        assert_eq!(
            export_message(&std::path::Path::new("/x/Evening.fac"), false),
            "Export failed: /x/Evening.fac"
        );
    }

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
