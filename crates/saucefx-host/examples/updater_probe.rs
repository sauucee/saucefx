//! CI executable for the same update backend used by Settings. No audio.
#[path = "../src/updater.rs"]
mod updater;

use saucefx_core::updates::UpdateStatus;
use std::time::{Duration, Instant};

fn main() {
    let expect_current = std::env::args().any(|arg| arg == "--expect-up-to-date");
    let mut updater = updater::Updater::new();
    assert!(updater.start());
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        while let Some(event) = updater.poll() {
            match event {
                updater::Event::Status(status) => {
                    println!("{status:?}");
                    if status.busy() {
                        continue;
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
                    return;
                }
                updater::Event::RestartLaunched => panic!("unexpected restart during update probe"),
            }
        }
        assert!(Instant::now() < deadline, "update probe timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}
