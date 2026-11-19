//! User paths shared with the previous host. Flatpak redirects XDG state
//! and data into its writable app directory; login entries stay on the host.

use std::path::{Path, PathBuf};

/// App-specific XDG directory, with the normal home-relative fallback.
pub fn app_dir(home: &Path, xdg: Option<&str>, fallback: &str) -> PathBuf {
    xdg.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(fallback))
        .join("saucefx")
}

/// Autostart must use the host's .config directory in Flatpak. The manifest
/// grants access specifically to ~/.config/autostart.
pub fn config_dir(home: &Path, xdg: Option<&str>, flatpak: bool) -> PathBuf {
    if flatpak {
        return home.join(".config");
    }
    xdg.filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpak_state_uses_the_writable_xdg_location() {
        let state = app_dir(
            Path::new("/home/u"),
            Some("/home/u/.var/app/io.github.sauucee.SauceFX/.local/state"),
            ".local/state",
        );
        assert_eq!(
            state.join("settings.txt"),
            PathBuf::from(
                "/home/u/.var/app/io.github.sauucee.SauceFX/.local/state/saucefx/settings.txt"
            )
        );
    }

    #[test]
    fn custom_presets_use_the_same_xdg_data_path_as_the_previous_host() {
        let data = app_dir(
            Path::new("/home/u"),
            Some("/home/u/.var/app/io.github.sauucee.SauceFX/data"),
            ".local/share",
        );
        assert_eq!(
            data.join("presets/My preset.fac"),
            PathBuf::from(
                "/home/u/.var/app/io.github.sauucee.SauceFX/data/saucefx/presets/My preset.fac"
            )
        );
    }

    #[test]
    fn unset_or_empty_xdg_paths_keep_the_home_relative_fallback() {
        for xdg in [None, Some("")] {
            assert_eq!(
                app_dir(Path::new("/home/u"), xdg, ".local/state"),
                PathBuf::from("/home/u/.local/state/saucefx")
            );
            assert_eq!(
                app_dir(Path::new("/home/u"), xdg, ".local/share"),
                PathBuf::from("/home/u/.local/share/saucefx")
            );
        }
    }

    #[test]
    fn flatpak_autostart_uses_the_host_config_path() {
        assert_eq!(
            config_dir(Path::new("/home/u"), Some("/private/config"), true),
            PathBuf::from("/home/u/.config")
        );
    }

    #[test]
    fn native_autostart_honours_xdg_config_or_home_fallback() {
        assert_eq!(
            config_dir(Path::new("/home/u"), Some("/custom/config"), false),
            PathBuf::from("/custom/config")
        );
        for xdg in [None, Some("")] {
            assert_eq!(
                config_dir(Path::new("/home/u"), xdg, false),
                PathBuf::from("/home/u/.config")
            );
        }
    }
}
