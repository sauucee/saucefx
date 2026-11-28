//! XDG autostart registration, mirroring `applyAutostartSetting`,
//! `autostartFile` and the launch resolution in Main.cpp: the entry lives
//! at `<config>/autostart/saucefx.desktop` and starts SauceFX with
//! `--autostart` so login doesn't pop the window over the desktop.

use std::path::{Path, PathBuf};

/// Apply the initial window visibility for this launch.
pub fn show_on_launch(
    window: &slint::Window,
    autostart_launch: bool,
) -> Result<(), slint::PlatformError> {
    if !autostart_launch {
        window.show()?;
    }
    Ok(())
}

/// `<config>/autostart/saucefx.desktop` (`configDir().getChildFile(...)`).
pub fn autostart_file(config_dir: &Path) -> PathBuf {
    config_dir.join("autostart").join("saucefx.desktop")
}

/// The desktop entry text for a resolved launch command and icon. Desktop
/// entries must use LF endings, not the CRLF a naive writer might produce.
pub fn autostart_entry(exec: &str, icon: &str) -> String {
    let mut text = String::new();
    text.push_str("[Desktop Entry]\n");
    text.push_str("Type=Application\n");
    text.push_str("Version=1.0\n");
    text.push_str("Name=SauceFX\n");
    text.push_str("Comment=SauceFX sound EQ and audio enhancement for Linux\n");
    let icon = icon
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    text.push_str(&format!("Icon={icon}\n"));
    text.push_str(&format!("Exec={exec}\n"));
    text.push_str("Terminal=false\n");
    text.push_str("X-GNOME-Autostart-enabled=true\n");
    text.push_str("X-GNOME-Autostart-Delay=2\n");
    text
}

/// Resolve the Exec line and icon the way Main.cpp does: a Flatpak sandbox
/// asks Flatpak to launch the installed app; an AppImage launch points at
/// the original portable file (a mounted path disappears at exit) with the
/// icon copied beside the data; otherwise the current executable.
/// `current_exe` is the resolved executable path.
pub fn resolve_launch(
    current_exe: &Path,
    flatpak_id: Option<&str>,
    appimage: Option<&str>,
    data_dir: &Path,
) -> (String, String) {
    if let Some(flatpak_id) = flatpak_id.filter(|id| !id.is_empty()) {
        return (
            format!("flatpak run {flatpak_id} --autostart"),
            flatpak_id.to_string(),
        );
    }
    let exe = current_exe
        .to_string_lossy()
        .replace('"', "\\\"")
        .replace('%', "%%");
    let mut exec = format!("\"{exe}\" --autostart");
    let mut icon = current_exe
        .parent()
        .map(|dir| dir.join("images").join("saucefx-logo-512.png"))
        .unwrap_or_else(|| PathBuf::from("saucefx-logo-512.png"));
    if let Some(appimage) = appimage.filter(|path| !path.is_empty()) {
        let portable = PathBuf::from(appimage);
        if portable.is_file() {
            let path = portable
                .to_string_lossy()
                .replace('"', "\\\"")
                .replace('%', "%%");
            exec = format!("\"{path}\" --autostart");
            let saved_icon = data_dir.join("app-icon.png");
            if std::fs::create_dir_all(saved_icon.parent().unwrap()).is_ok()
                && std::fs::copy(icon.clone(), &saved_icon).is_ok()
            {
                icon = saved_icon;
            }
        }
    }
    let icon = icon.to_string_lossy().into_owned();
    (exec, icon)
}

/// Create/remove the XDG autostart entry. Returns false when the file
/// operation fails (the caller reverts the toggle and warns, like the
/// reference "Could not update the login item.").
pub fn apply_autostart(enabled: bool, file: &Path, entry: &str) -> bool {
    if !enabled {
        if !file.exists() {
            return true;
        }
        let ok = std::fs::remove_file(file).is_ok();
        if !ok {
            eprintln!("saucefx: could not remove {}", file.display());
        }
        return ok;
    }
    let Some(parent) = file.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    std::fs::write(file, entry).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_launch_stays_hidden_until_opened_normally() {
        use slint::ComponentHandle;
        use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
        use slint::platform::{Platform, PlatformError, WindowAdapter};
        use std::rc::Rc;

        struct OffscreenPlatform;
        impl Platform for OffscreenPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
                Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
            }
        }
        slint::platform::set_platform(Box::new(OffscreenPlatform)).unwrap();
        let app = saucefx_ui::AppWindow::new().unwrap();
        show_on_launch(app.window(), true).unwrap();
        assert!(!app.window().is_visible(), "login must not pop up a window");
        show_on_launch(app.window(), false).unwrap();
        assert!(
            app.window().is_visible(),
            "a normal launch shows the window"
        );
        app.hide().unwrap();
    }

    #[test]
    fn autostart_file_is_the_xdg_entry_under_the_config_dir() {
        assert_eq!(
            autostart_file(Path::new("/home/u/.config")),
            PathBuf::from("/home/u/.config/autostart/saucefx.desktop")
        );
    }

    #[test]
    fn entry_text_carries_the_reference_fields_with_lf_endings() {
        let text = autostart_entry("\"/opt/bin/SauceFX\" --autostart", "/img/logo.png");
        for needle in [
            "[Desktop Entry]\n",
            "Type=Application\n",
            "Version=1.0\n",
            "Name=SauceFX\n",
            "Comment=SauceFX sound EQ and audio enhancement for Linux\n",
            "Icon=/img/logo.png\n",
            "Exec=\"/opt/bin/SauceFX\" --autostart\n",
            "Terminal=false\n",
            "X-GNOME-Autostart-enabled=true\n",
            "X-GNOME-Autostart-Delay=2\n",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in {text:?}");
        }
        assert!(!text.contains("\r"), "desktop entries use LF only");
    }

    #[test]
    fn flatpak_launch_uses_the_installed_app() {
        let (exec, icon) = resolve_launch(
            Path::new("/app/bin/SauceFX"),
            Some("org.saucefx.SauceFX"),
            None,
            Path::new("/d"),
        );
        assert_eq!(exec, "flatpak run org.saucefx.SauceFX --autostart");
        assert_eq!(icon, "org.saucefx.SauceFX");
    }

    #[test]
    fn plain_launch_escapes_the_executable_and_quotes_it() {
        let (exec, icon) = resolve_launch(
            Path::new("/opt/my dir/SauceFX"),
            None,
            None,
            Path::new("/d"),
        );
        assert_eq!(exec, "\"/opt/my dir/SauceFX\" --autostart");
        assert_eq!(icon, "/opt/my dir/images/saucefx-logo-512.png");
    }

    #[test]
    fn appimage_launch_points_at_the_portable_file() {
        let dir = std::env::temp_dir().join("saucefx-autostart-appimage");
        let _ = std::fs::remove_dir_all(&dir);
        // The icon source lives next to the (mounted) executable, exactly
        // where resolve_launch looks for it.
        let mount = dir.join(".mount_SauceFX");
        std::fs::create_dir_all(mount.join("images")).unwrap();
        std::fs::write(mount.join("images").join("saucefx-logo-512.png"), "png").unwrap();
        std::fs::write(dir.join("SauceFX.AppImage"), "app").unwrap();
        std::fs::write(mount.join("SauceFX"), "bin").unwrap();
        let appimage = dir.join("SauceFX.AppImage");
        let (exec, icon) = resolve_launch(
            &mount.join("SauceFX"),
            None,
            Some(&appimage.to_string_lossy()),
            &dir,
        );
        assert_eq!(exec, format!("\"{}\" --autostart", appimage.display()));
        assert_eq!(icon, dir.join("app-icon.png").to_string_lossy());
        assert!(
            dir.join("app-icon.png").is_file(),
            "the icon is saved to the data dir: {}",
            dir.join("app-icon.png").display()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_writes_and_removes_the_entry() {
        let dir = std::env::temp_dir().join("saucefx-autostart-apply");
        let _ = std::fs::remove_dir_all(&dir);
        let file = autostart_file(&dir);
        assert!(
            apply_autostart(false, &file, ""),
            "removing an absent entry is fine"
        );
        assert!(apply_autostart(true, &file, "[Desktop Entry]\n"));
        assert!(file.is_file(), "the entry is written");
        assert!(apply_autostart(false, &file, ""));
        assert!(!file.exists(), "the entry is removed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
