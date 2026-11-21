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
/// Reads one `<int>: <label>` line at `lines[index]`, advancing the index.
fn labeled_int(lines: &[&str], index: &mut usize, label: &str) -> Result<usize, &'static str> {
    let line = *lines.get(*index).ok_or("preset ends early")?;
    let Some((value, seen)) = line.rsplit_once(':') else {
        return Err("expected a '<value>: <label>' line");
    };
    if seen.trim() != label {
        return Err("unexpected line in the preset structure");
    }
    let parsed = value
        .trim()
        .parse::<usize>()
        .map_err(|_| "expected an integer value")?;
    *index += 1;
    Ok(parsed)
}

/// Reads one `<int>: <kind> <n>` line, requiring the index to be `n`.
fn indexed_int(
    lines: &[&str],
    index: &mut usize,
    kind: &str,
    n: usize,
) -> Result<(), &'static str> {
    let expected = format!("{kind} {n}");
    let line = *lines.get(*index).ok_or("preset ends early")?;
    let Some((value, seen)) = line.rsplit_once(':') else {
        return Err("expected a '<value>: <label>' line");
    };
    if seen.trim() != expected {
        return Err("preset value lines are shifted or out of order");
    }
    value
        .trim()
        .parse::<usize>()
        .map_err(|_| "expected an integer value")?;
    *index += 1;
    Ok(())
}

/// Structural validation for a `.fac` preset (the CLASS1 text format the
/// engine writes and reads). The legacy engine parser accepts structurally
/// malformed files — a band block with extra or missing lines shifts every
/// later line and produced a wild +21 dB gain in diagnosis — so the Rust
/// side rejects them before handing the file to the engine.
pub fn validate_fac_structure(text: &str) -> Result<(), &'static str> {
    let lines: Vec<&str> = text.lines().map(str::trim_start).collect();
    let mut index = 0;

    if *lines.first().ok_or("preset is empty")? != "CLASS1 : Effect Type" {
        return Err("the CLASS1 header line is missing");
    }
    index += 1;
    labeled_int(&lines, &mut index, "Version")?;
    // The display name is free text: any single line.
    if lines.get(index).is_none() {
        return Err("preset name line is missing");
    }
    index += 1;
    labeled_int(&lines, &mut index, "Double Params Flag")?;

    let elements = labeled_int(&lines, &mut index, "Total number of elements")?;
    for element in 0..elements {
        let number = labeled_int(&lines, &mut index, "Element Number")?;
        if number != element {
            return Err("element numbering is out of order");
        }
        for param in 0..7 {
            indexed_int(&lines, &mut index, "Param", param)?;
        }
    }

    let integers = labeled_int(
        &lines,
        &mut index,
        "Number of Application Dependent Integers",
    )?;
    for integer in 0..integers {
        indexed_int(&lines, &mut index, "Integer", integer)?;
    }
    // No nonzero Reals/Strings section exists in the corpus, so their value
    // lines are consumed as opaque: only the count is enforced.
    let reals = labeled_int(&lines, &mut index, "Number of Application Dependent Reals")?;
    index += reals;
    let strings = labeled_int(
        &lines,
        &mut index,
        "Number of Application Dependent Strings",
    )?;
    index += strings;

    let bands = labeled_int(&lines, &mut index, "Number of EQ Bands")?;
    if lines
        .get(index)
        .is_some_and(|line| line.ends_with(": On/Off Flag"))
    {
        index += 1;
    }
    for band in 1..=bands {
        let header = *lines.get(index).ok_or("band section ends early")?;
        if header != format!("Band {band}") {
            return Err("band blocks do not match the declared count");
        }
        let cf = lines.get(index + 1).ok_or("band block ends early")?;
        let Some((cf_value, cf_label)) = cf.rsplit_once(':') else {
            return Err("band block is missing a CF line");
        };
        if cf_label.trim() != "CF" || cf_value.trim().parse::<f64>().is_err() {
            return Err("band block is missing a numeric CF line");
        }
        let gain = lines.get(index + 2).ok_or("band block ends early")?;
        let Some((gain_value, gain_label)) = gain.rsplit_once(':') else {
            return Err("band block is missing a Boost/Cut line");
        };
        if gain_label.trim() != "Boost/Cut" || gain_value.trim().parse::<i64>().is_err() {
            return Err("band block is missing a Boost/Cut value");
        }
        index += 3;
    }
    // Nothing but blank lines may follow the last band block.
    if lines[index..].iter().any(|line| !line.trim().is_empty()) {
        return Err("unexpected lines at the end of the preset");
    }
    Ok(())
}

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

    /// A minimal well-formed preset in the exact line grammar the engine's
    /// writer produces (mirrors `presets/Factsoft/Default.fac`).
    fn valid_fac() -> String {
        let mut lines: Vec<String> = [
            "CLASS1 : Effect Type",
            "9: Version",
            "Default",
            "0: Double Params Flag",
            "1: Total number of elements",
            "0: Main 0",
            "0: Main 1",
            "0: Main 2",
            "0: Main 3",
            "0: Main 4",
            "0: Main 5",
            "0: Element Number",
            "   0: Param 0",
            "   0: Param 1",
            "   0: Param 2",
            "   0: Param 3",
            "   0: Param 4",
            "   0: Param 5",
            "   0: Param 6",
            "7: Number of Application Dependent Integers",
            "0: Number of Application Dependent Reals",
            "0: Number of Application Dependent Strings",
            "1: Integer[0]",
            "1: Integer[1]",
            "1: Integer[2]",
            "1: Integer[3]",
            "1: Integer[4]",
            "0: Integer[5]",
            "2: Integer[6]",
            "10: Number of EQ Bands",
            "1: On/Off Flag",
        ]
        .iter()
        .map(|line| line.to_string())
        .collect();
        for band in 1..=10 {
            lines.push(format!("Band {band}"));
            lines.push(format!("   {}: CF", band * 55));
            lines.push("   0: Boost/Cut".to_string());
        }
        let mut text = lines.join("\n");
        text.push('\n');
        text
    }

    #[test]
    fn a_well_formed_preset_passes_structure_validation() {
        assert_eq!(validate_fac_structure(&valid_fac()), Ok(()));
    }

    #[test]
    fn a_shifted_band_block_is_rejected_as_malformed() {
        // The corruption demonstrated on 2026-11-21: duplicated CF/Boost-Cut
        // lines inside one band block shift every later line; the legacy
        // parser accepted the file and applied a wild +21 dB gain.
        let malformed =
            valid_fac().replacen("Band 5\n", "   416.5: CF\n   0: Boost/Cut\nBand 5\n", 1);
        assert_eq!(malformed.lines().count(), valid_fac().lines().count() + 2);
        assert!(validate_fac_structure(&malformed).is_err());
    }

    #[test]
    fn a_missing_class1_header_is_rejected() {
        let malformed = valid_fac().replacen("CLASS1 : Effect Type\n", "", 1);
        assert!(validate_fac_structure(&malformed).is_err());
    }

    #[test]
    fn a_missing_version_line_is_rejected() {
        let malformed = valid_fac().replacen("9: Version\n", "", 1);
        assert!(validate_fac_structure(&malformed).is_err());
    }

    #[test]
    fn a_shifted_element_section_is_rejected() {
        // One extra Param line after the element header shifts every later
        // Param line: the same corruption class as the shifted band block.
        let malformed = valid_fac().replacen(
            "0: Element Number\n",
            "0: Element Number\n   0: Param 0\n",
            1,
        );
        assert!(validate_fac_structure(&malformed).is_err());
    }

    #[test]
    fn a_missing_integer_value_is_rejected() {
        // The declared integer count no longer matches the value lines that
        // follow, which is how lost or duplicated lines announce themselves.
        let malformed = valid_fac().replacen("0: Integer[5]\n", "", 1);
        assert!(validate_fac_structure(&malformed).is_err());
    }

    #[test]
    fn every_factory_preset_passes_structure_validation() {
        // Regression lock over the real shipped library: the validator must
        // accept every factory file, so strictness cannot break the catalog.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets/Factsoft");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("factory preset library exists") {
            let path = entry.expect("readable factory entry").path();
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("fac"))
            {
                let text = std::fs::read_to_string(&path).expect("factory preset is text");
                assert_eq!(
                    validate_fac_structure(&text),
                    Ok(()),
                    "factory preset {} must stay structurally valid",
                    path.display()
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 13,
            "expected the 13 factory presets, found {checked}"
        );
    }

    #[test]
    fn scan_skips_malformed_presets() {
        let dir = std::env::temp_dir().join("saucefx-scan-guard");
        let _ = std::fs::remove_dir_all(&dir);
        let factory = dir.join("factory");
        std::fs::create_dir_all(&factory).unwrap();
        std::fs::write(factory.join("Good.fac"), valid_fac()).unwrap();
        let shifted =
            valid_fac().replacen("Band 5\n", "   416.5: CF\n   0: Boost/Cut\nBand 5\n", 1);
        std::fs::write(factory.join("Broken.fac"), shifted).unwrap();

        let scanned = scan(&factory, &dir.join("user"));

        assert_eq!(scanned.len(), 1, "only the well-formed preset is offered");
        assert!(scanned[0].path.ends_with("Good.fac"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn importing_a_malformed_preset_counts_as_failed() {
        let dir = std::env::temp_dir().join("saucefx-import-guard");
        let _ = std::fs::remove_dir_all(&dir);
        let staged = dir.join("staged");
        std::fs::create_dir_all(&staged).unwrap();
        let shifted =
            valid_fac().replacen("Band 5\n", "   416.5: CF\n   0: Boost/Cut\nBand 5\n", 1);
        let source = staged.join("Broken.fac");
        std::fs::write(&source, shifted).unwrap();
        let user = dir.join("user");
        std::fs::create_dir_all(&user).unwrap();

        let entries = vec![(source, Some("Broken preset".to_string()))];
        let outcome = import_preset_files(&entries, &user, &[]);

        assert_eq!(
            outcome.imported, 0,
            "a malformed file must not reach the library"
        );
        assert_eq!(
            outcome.failed, 1,
            "the malformed file counts as not readable"
        );
        assert_eq!(outcome.last, None);
        assert!(
            std::fs::read_dir(&user).unwrap().next().is_none(),
            "nothing is copied"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

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
