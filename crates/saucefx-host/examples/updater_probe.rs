//! CI executable for the same update backend used by Settings. No audio.
#[path = "../src/single_instance.rs"]
mod single_instance;
#[path = "../src/updater.rs"]
mod updater;

use saucefx_core::updates::UpdateStatus;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
enum Expected {
    UpToDate,
    RestartRequired,
    Failed,
}

fn check(updater: &mut updater::Updater, expected: Expected, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(90);
    // A terminal event can precede the worker thread's return.
    while !updater.start() {
        assert!(
            Instant::now() < deadline,
            "{label}: update worker did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    loop {
        while let Some(event) = updater.poll() {
            match event {
                updater::Event::Status(status) => {
                    println!("{label}: {status:?}");
                    if status.busy() {
                        continue;
                    }
                    match expected {
                        Expected::UpToDate => assert_eq!(status, UpdateStatus::UpToDate, "{label}"),
                        Expected::RestartRequired => {
                            assert_eq!(status, UpdateStatus::RestartRequired, "{label}");
                        }
                        Expected::Failed => assert!(
                            matches!(status, UpdateStatus::Failed(_)),
                            "{label}: a failed update must not report success: {status:?}"
                        ),
                    }
                    return;
                }
                updater::Event::RestartLaunched => {
                    panic!("{label}: unexpected restart during check")
                }
            }
        }
        assert!(Instant::now() < deadline, "{label}: update probe timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn running_commit() -> String {
    std::fs::read_to_string("/.flatpak-info")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("app-commit="))
        .expect("running Flatpak app commit")
        .to_owned()
}

fn wait_for_staged_update(state: &Path) {
    let original_commit = running_commit();
    let running_marker = std::fs::read_to_string("/app/version-marker").unwrap();
    assert_eq!(
        running_marker, "new",
        "the staged probe must start from app B"
    );
    std::fs::write(state.join("stage-running-commit"), &original_commit).unwrap();
    std::fs::write(state.join("stage-ready"), &running_marker).unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    while !state.join("stage-release").is_file() {
        assert!(
            Instant::now() < deadline,
            "host did not release the staged-update gate"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        running_commit(),
        original_commit,
        "the staged probe must keep running its original deployment"
    );
    assert_eq!(
        std::fs::read_to_string("/app/version-marker").unwrap(),
        running_marker
    );
}

fn with_metadata_value(info: &str, section: &str, key: &str, value: &str) -> String {
    let entry = format!("{key}={value}");
    let prefix = format!("{key}=");
    let mut lines: Vec<String> = info.lines().map(str::to_owned).collect();
    let start = lines
        .iter()
        .position(|line| line.as_str() == section)
        .expect("Flatpak fixture metadata section")
        + 1;
    let end = (start..lines.len())
        .find(|&index| lines[index].starts_with('['))
        .unwrap_or(lines.len());
    if let Some(index) = (start..end).find(|&index| lines[index].starts_with(prefix.as_str())) {
        lines[index] = entry;
    } else {
        lines.insert(start, entry);
    }
    format!("{}\n", lines.join("\n"))
}

fn selected_app_path(info: &str) -> &str {
    let mut instance = info
        .lines()
        .skip_while(|line| *line != "[Instance]")
        .skip(1)
        .take_while(|line| !line.starts_with('['));
    instance
        .clone()
        .find_map(|line| line.strip_prefix("original-app-path="))
        .or_else(|| instance.find_map(|line| line.strip_prefix("app-path=")))
        .expect("Flatpak fixture deployment path")
}

// Supply both keys even on runners that omit original-app-path so the path
// fault cases catch a parser that reads or falls back to the wrong field.
fn with_deployment_paths(info: &str, fallback: &str, original: &str) -> String {
    let info = with_metadata_value(info, "[Instance]", "app-path", fallback);
    with_metadata_value(&info, "[Instance]", "original-app-path", original)
}

// Fault runtimes install this probe as /usr/bin/cat. Handle that argv0 before
// state initialization or the app lock: this process is the updater's read-only
// deployment query, not a second application instance. The normal runtime uses
// the runner's real cat and never enters this branch.
fn fixture_query(args: &[String]) {
    assert_eq!(
        args.len(),
        2,
        "the deployment query must use one fixed argument"
    );
    assert_eq!(args[1], "/.flatpak-info");
    let mode = std::fs::read_to_string("/usr/deployment-query-mode").unwrap();
    let mode = mode.trim();
    let state = std::fs::read_to_string("/usr/deployment-query-state").unwrap();
    let state = Path::new(state.trim());
    let info = std::fs::read_to_string("/.flatpak-info").unwrap();
    let output = match mode {
        // Valid stdout plus a failed exit proves the caller checks both.
        "exit" => info,
        "malformed" => {
            let path = selected_app_path(&info);
            with_deployment_paths(&info, path, "relative-path")
        }
        "identity" => {
            let path = selected_app_path(&info);
            let (root, suffix) = path
                .rsplit_once("/app/io.github.sauucee.SauceFX/")
                .expect("SauceFX deployment path");
            let other_path = format!("{root}/app/io.github.sauucee.OtherApp/{suffix}");
            let info =
                with_metadata_value(&info, "[Application]", "name", "io.github.sauucee.OtherApp");
            with_deployment_paths(&info, &other_path, &other_path)
        }
        "installation" => {
            let path = selected_app_path(&info);
            let app_path = Path::new(path);
            let installation = app_path.ancestors().nth(6).unwrap();
            let other_path = Path::new("/saucefx-fixture-other-installation")
                .join(app_path.strip_prefix(installation).unwrap());
            assert_ne!(other_path, app_path);
            with_deployment_paths(&info, path, other_path.to_str().unwrap())
        }
        _ => panic!("unknown deployment query fault: {mode}"),
    };
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(output.as_bytes()).unwrap();
    stdout.flush().unwrap();
    std::fs::write(state.join(format!("query-{mode}-observed")), "queried").unwrap();
    if mode == "exit" {
        std::process::exit(42);
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if Path::new(&args[0]).file_name() == Some(std::ffi::OsStr::new("cat")) {
        fixture_query(&args);
        return;
    }
    eprintln!(
        "updater probe: args={args:?}, state={:?}, version={:?}",
        std::env::var("XDG_STATE_HOME"),
        std::fs::read_to_string("/app/version-marker")
    );
    // App A, app B and the externally staged app C deliberately advertise the
    // same version. Only their payload/deployment commits differ.
    assert_eq!(
        std::fs::read_to_string("/app/release-version").unwrap(),
        "0.3.2"
    );
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
        // The host's completion marker must not race a final lock release.
        drop(lock);
        std::fs::write(state.join("restarted-version"), "new").unwrap();
        return;
    }
    assert!(single_instance::acquire(&lock, Duration::ZERO).unwrap());
    let expect_current = args.iter().any(|arg| arg == "--expect-up-to-date");
    let expect_error = args.iter().any(|arg| arg == "--expect-error");
    assert!(!(expect_current && expect_error));
    let expected = if expect_error {
        Expected::Failed
    } else if expect_current {
        Expected::UpToDate
    } else {
        Expected::RestartRequired
    };
    if args.iter().any(|arg| arg == "--wait-for-staged-update") {
        wait_for_staged_update(&state);
    }
    let mut updater = updater::Updater::new();
    check(&mut updater, expected, "initial check");
    if args.iter().any(|arg| arg == "--repeat-check") {
        check(&mut updater, expected, "repeat check");
        // A fresh updater in the old process must detect a pending restart
        // without relying on a latch left by the first successful transaction.
        check(
            &mut updater::Updater::new(),
            expected,
            "fresh updater check",
        );
    }
    if !args.iter().any(|arg| arg == "--test-restart") {
        return;
    }
    assert!(matches!(expected, Expected::RestartRequired));
    let deadline = Instant::now() + Duration::from_secs(30);
    while !updater.restart() {
        assert!(
            Instant::now() < deadline,
            "update worker did not finish before restart"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    loop {
        while let Some(event) = updater.poll() {
            match event {
                updater::Event::Status(status) => {
                    println!("restart: {status:?}");
                    assert!(
                        status.busy(),
                        "the latest deployment did not restart: {status:?}"
                    );
                }
                updater::Event::RestartLaunched => {
                    // Match main: persist first, then release the instance lock
                    // so the latest deployment can finish starting.
                    std::fs::write(state.join("saved-before-restart"), "saved").unwrap();
                    drop(lock);
                    return;
                }
            }
        }
        assert!(Instant::now() < deadline, "restart probe timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}
