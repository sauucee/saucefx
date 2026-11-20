//! CI executable for the same update backend used by Settings. No audio.
#[path = "../src/single_instance.rs"]
mod single_instance;
#[path = "../src/updater.rs"]
mod updater;

use saucefx_core::updates::UpdateStatus;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let state = PathBuf::from(std::env::var("XDG_STATE_HOME").expect("Flatpak state directory"))
        .join("saucefx");
    std::fs::create_dir_all(&state).unwrap();
    if args.iter().any(|arg| arg == "--state-path") {
        println!("{}", state.display());
        return;
    }
    let lock = std::fs::File::create(single_instance::lock_file(&state)).unwrap();
    if args.iter().any(|arg| arg == "--restart-wait") {
        assert!(single_instance::acquire(&lock, Duration::from_secs(15)).unwrap());
        assert_eq!(
            std::fs::read_to_string("/app/version-marker").unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read_to_string(state.join("saved-before-restart")).unwrap(),
            "saved"
        );
        std::fs::write(state.join("restarted-version"), "new").unwrap();
        return;
    }
    assert!(single_instance::acquire(&lock, Duration::ZERO).unwrap());
    let expect_current = args.iter().any(|arg| arg == "--expect-up-to-date");
    let expect_error = args.iter().any(|arg| arg == "--expect-error");
    let restart_after_update = args.iter().any(|arg| arg == "--test-restart");
    let mut updater = updater::Updater::new();
    assert!(updater.start());
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut restarting = false;
    loop {
        while let Some(event) = updater.poll() {
            match event {
                updater::Event::Status(status) => {
                    println!("{status:?}");
                    if status.busy() {
                        continue;
                    }
                    if expect_error {
                        assert!(
                            matches!(status, UpdateStatus::Failed(_)),
                            "a failed update must not report success: {status:?}"
                        );
                        return;
                    }
                    let wanted = if expect_current {
                        UpdateStatus::UpToDate
                    } else {
                        UpdateStatus::RestartRequired
                    };
                    assert_eq!(
                        status, wanted,
                        "real Flatpak update returned the wrong terminal state"
                    );
                    if !restart_after_update {
                        return;
                    }
                    // A terminal event can precede the worker thread's return.
                    while !updater.restart() {
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    restarting = true;
                }
                updater::Event::RestartLaunched => {
                    assert!(restarting, "unexpected restart during update probe");
                    // Match main: persist first, then release the instance lock
                    // so the latest deployment can finish starting.
                    std::fs::write(state.join("saved-before-restart"), "saved").unwrap();
                    drop(lock);
                    return;
                }
            }
        }
        assert!(Instant::now() < deadline, "update probe timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}
