//! Native file dialogs. The DSP writer always appends `.fac`; never point it
//! at a user's directory before confirming which final file may be replaced.
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::Command;

fn zenity() -> Command {
    let mut cmd = Command::new("zenity");
    if let Some(host) = std::env::var_os("SAUCEFX_HOST_LD_LIBRARY_PATH") {
        if host.is_empty() {
            cmd.env_remove("LD_LIBRARY_PATH");
        } else {
            cmd.env("LD_LIBRARY_PATH", host);
        }
    }
    cmd
}

fn run(cmd: &mut Command) -> Result<Option<Vec<u8>>, String> {
    let output = cmd
        .output()
        .map_err(|e| format!("Could not open the system dialog (Zenity): {e}"))?;
    if output.status.success() {
        let mut bytes = output.stdout;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        Ok(Some(bytes))
    } else if output.status.code() == Some(1)
        && !String::from_utf8_lossy(&output.stderr).contains("cannot open display")
    {
        Ok(None)
    } else {
        eprintln!(
            "saucefx: zenity {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        Err(format!(
            "The system dialog could not open (Zenity: {}).",
            output.status
        ))
    }
}

pub fn message(text: &str) -> Result<Option<Vec<u8>>, String> {
    run(zenity().args(["--error", "--no-markup", "--title=SauceFX", "--text", text]))
}

// GTK returns absolute local paths without repeated slashes. Two adjacent
// slashes therefore distinguish files even when names contain pipes/newlines.
const SEPARATOR: &str = "\n//\n";
fn selected_paths(bytes: &[u8]) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    let mut rest = bytes;
    loop {
        let end = rest
            .windows(SEPARATOR.len())
            .position(|s| s == SEPARATOR.as_bytes());
        let path = PathBuf::from(OsStr::from_bytes(&rest[..end.unwrap_or(rest.len())]));
        if !path.is_absolute() {
            return Err("The file picker returned an invalid path.".into());
        }
        paths.push(path);
        let Some(end) = end else {
            break;
        };
        rest = &rest[end + SEPARATOR.len()..];
    }
    Ok(paths)
}

pub fn select_files(start: &Path) -> Result<Vec<PathBuf>, String> {
    let mut dir = start.as_os_str().to_owned();
    dir.push("/");
    match run(zenity()
        .args([
            "--file-selection",
            "--multiple",
            "--separator",
            SEPARATOR,
            "--title=Import presets",
            "--file-filter=Presets | *.fac *.FAC",
            "--filename",
        ])
        .arg(dir))?
    {
        Some(bytes) => selected_paths(&bytes),
        None => Ok(Vec::new()),
    }
}

pub fn export(
    start: &Path,
    save: impl FnOnce(&str, &Path) -> bool,
) -> Result<Option<PathBuf>, String> {
    let Some(bytes) = run(zenity()
        .args([
            "--file-selection",
            "--save",
            "--title=Export current preset",
            "--filename",
        ])
        .arg(start))?
    else {
        return Ok(None);
    };
    export_to(
        PathBuf::from(OsString::from_vec(bytes)),
        |target| {
            let text = format!("Replace this preset?\n{}", target.display());
            run(zenity().args([
                "--question",
                "--no-markup",
                "--default-cancel",
                "--title=Replace preset?",
                "--text",
                &text,
            ]))
            .map(|choice| choice.is_some())
        },
        save,
    )
}

fn export_to(
    mut target: PathBuf,
    confirm: impl FnOnce(&Path) -> Result<bool, String>,
    save: impl FnOnce(&str, &Path) -> bool,
) -> Result<Option<PathBuf>, String> {
    if !target.is_absolute() || target.file_name().is_none() {
        return Err("Choose a preset filename.".into());
    }
    if !target
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("fac"))
    {
        target.set_extension("fac");
    }
    let name = target
        .file_stem()
        .and_then(OsStr::to_str)
        .filter(|s| !s.is_empty())
        .ok_or("Unsupported preset filename.")?;
    let dir = target.parent().ok_or("Choose a preset folder.")?;
    let fail = |e: std::io::Error| format!("Could not export {}: {e}", target.display());
    let replace = match std::fs::symlink_metadata(&target) {
        Ok(meta) if meta.is_file() => {
            if !confirm(&target)? {
                return Ok(None);
            }
            true
        }
        Ok(_) => return Err("Choose a regular file, not a link or folder.".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(fail(e)),
    };
    let stage = tempfile::tempdir_in(dir).map_err(fail)?;
    if !save(name, stage.path()) {
        return Err("Could not write the preset.".into());
    }
    let staged = stage.path().join(format!("{name}.fac"));
    if !std::fs::metadata(&staged).is_ok_and(|m| m.is_file() && m.len() > 0) {
        return Err("The preset writer produced no data.".into());
    }
    if replace {
        std::fs::rename(&staged, &target).map_err(fail)?;
    } else {
        // persist_noclobber also protects a file created while the dialog was closing.
        let staged = tempfile::TempPath::try_from_path(staged).map_err(fail)?;
        staged
            .persist_noclobber(&target)
            .map_err(|e| fail(e.error))?;
    }
    Ok(Some(target))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multi_select_preserves_whitespace_pipes_and_newlines() {
        assert_eq!(
            selected_paths(format!("/d/a|b.fac{SEPARATOR}/d/ c\nd.fac").as_bytes()).unwrap(),
            [PathBuf::from("/d/a|b.fac"), PathBuf::from("/d/ c\nd.fac")]
        );
    }
    #[test]
    fn export_only_replaces_the_exact_confirmed_name() {
        let d = tempfile::tempdir().unwrap();
        for (typed, actual) in [
            ("evening", "evening.fac"),
            ("evening.fac", "evening.fac"),
            ("evening.FAC", "evening.FAC"),
        ] {
            let target = d.path().join(actual);
            std::fs::write(&target, b"original").unwrap();
            assert_eq!(
                export_to(
                    d.path().join(typed),
                    |p| {
                        assert_eq!(p, target);
                        Ok(false)
                    },
                    |_, _| panic!("cancelled")
                )
                .unwrap(),
                None
            );
            assert_eq!(std::fs::read(&target).unwrap(), b"original");
            export_to(
                d.path().join(typed),
                |p| {
                    assert_eq!(p, target);
                    Ok(true)
                },
                |n, dir| std::fs::write(dir.join(format!("{n}.fac")), b"changed").is_ok(),
            )
            .unwrap();
            assert_eq!(std::fs::read(&target).unwrap(), b"changed");
            std::fs::remove_file(&target).unwrap();
            assert_eq!(
                export_to(
                    d.path().join(typed),
                    |_| panic!("absent"),
                    |n, dir| std::fs::write(dir.join(format!("{n}.fac")), b"new").is_ok()
                )
                .unwrap(),
                Some(target.clone())
            );
            std::fs::remove_file(target).unwrap();
        }
    }
}
