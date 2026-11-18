//! Single-instance coordination, mirroring Main.cpp: the first instance
//! holds an flock on `<state>/app.lock` for its lifetime; a second launch
//! drops a `show.request` marker (so the hidden first instance brings its
//! window back) and exits without starting a second engine — silently when
//! launched via --autostart.

use std::path::{Path, PathBuf};

/// The process lock (`app.lock` in the state directory).
pub fn lock_file(state_dir: &Path) -> PathBuf {
    state_dir.join("app.lock")
}

/// The second-launch marker (`show.request`).
pub fn show_request_file(state_dir: &Path) -> PathBuf {
    state_dir.join("show.request")
}

/// A second launch asks the hidden first instance to come back.
pub fn write_show_request(state_dir: &Path) -> bool {
    std::fs::write(show_request_file(state_dir), "show\n").is_ok()
}

/// The first instance consumes a pending marker: true when one existed
/// (and was deleted), false otherwise.
pub fn consume_show_request(state_dir: &Path) -> bool {
    let file = show_request_file(state_dir);
    if !file.exists() {
        return false;
    }
    std::fs::remove_file(file).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_file_lives_in_the_state_dir() {
        assert_eq!(lock_file(Path::new("/s")), PathBuf::from("/s/app.lock"));
    }

    #[test]
    fn show_request_file_lives_in_the_state_dir() {
        assert_eq!(
            show_request_file(Path::new("/s")),
            PathBuf::from("/s/show.request")
        );
    }

    #[test]
    fn write_show_request_drops_the_marker() {
        let dir = std::env::temp_dir().join("saucefx-si-write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(write_show_request(&dir), "the marker is written");
        let text = std::fs::read_to_string(show_request_file(&dir)).unwrap();
        assert_eq!(text, "show\n", "the marker carries the show request");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn consume_show_request_deletes_and_reports() {
        let dir = std::env::temp_dir().join("saucefx-si-consume");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            !consume_show_request(&dir),
            "an absent marker consumes to false"
        );
        assert!(write_show_request(&dir));
        assert!(
            consume_show_request(&dir),
            "a present marker consumes to true"
        );
        assert!(
            !show_request_file(&dir).exists(),
            "the marker is deleted after consuming"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
