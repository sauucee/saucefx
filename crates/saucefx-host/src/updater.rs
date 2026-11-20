//! Explicit Flatpak self-updates. D-Bus work stays on a worker thread; the
//! UI thread polls events and keeps ownership of the audio engine.

use std::collections::HashMap;
use std::future::Future;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use futures_lite::io::AsyncReadExt;
use futures_lite::{StreamExt, future};
use saucefx_core::updates::UpdateStatus;
use zbus::zvariant::{Fd, OwnedObjectPath, OwnedValue, Value};

const APP_ID: &str = "io.github.sauucee.SauceFX";
const SERVICE: &str = "org.freedesktop.portal.Flatpak";
const PATH: &str = "/org/freedesktop/portal/Flatpak";
const MONITOR: &str = "org.freedesktop.portal.Flatpak.UpdateMonitor";
const METHOD_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const DEPLOYMENT_TIMEOUT: Duration = Duration::from_secs(15);
const METADATA_LIMIT: usize = 64 * 1024;
const METADATA_ERROR: &str = "Could not verify the installed SauceFX deployment. Try again.";
const LATEST_VERSION: u32 = 2;

type Info = HashMap<String, OwnedValue>;

#[derive(Debug)]
pub enum Event {
    Status(UpdateStatus),
    RestartLaunched,
}

#[derive(Default)]
pub struct Updater {
    receiver: Option<Receiver<Event>>,
    worker: Option<JoinHandle<()>>,
    pending_terminal: Option<Event>,
}

impl Updater {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self) -> bool {
        self.launch(false)
    }

    pub fn restart(&mut self) -> bool {
        self.launch(true)
    }

    fn launch(&mut self, restart: bool) -> bool {
        if self.pending_terminal.is_some()
            || self
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
        {
            return false;
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.pending_terminal = None;
        self.worker = Some(thread::spawn(move || {
            if !supported() {
                let _ = sender.send(Event::Status(UpdateStatus::Unsupported));
                return;
            }
            let _ = sender.send(Event::Status(UpdateStatus::Checking));
            let result = future::block_on(async {
                let connection = zbus::connection::Builder::session()
                    .map_err(service_error)?
                    .method_timeout(METHOD_TIMEOUT)
                    .build()
                    .await
                    .map_err(service_error)?;
                if restart {
                    restart_latest(&connection).await?;
                    let _ = sender.send(Event::RestartLaunched);
                    Ok(())
                } else {
                    let running_deployment = read_running_deployment()?;
                    update(&connection, &sender, &running_deployment).await
                }
            });
            if let Err(message) = result {
                let _ = sender.send(Event::Status(UpdateStatus::Failed(message)));
            }
        }));
        true
    }

    pub fn poll(&mut self) -> Option<Event> {
        if let Some(event) = self.pending_terminal.take() {
            if self
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
            {
                self.pending_terminal = Some(event);
                return None;
            }
            if let Some(worker) = self.worker.take()
                && worker.join().is_err()
            {
                return Some(Event::Status(UpdateStatus::Failed(
                    "The update service stopped unexpectedly. Try again.".into(),
                )));
            }
            return Some(event);
        }

        let event = match self.receiver.as_ref()?.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                if self
                    .worker
                    .as_ref()
                    .is_some_and(|worker| !worker.is_finished())
                {
                    return None;
                }
                self.receiver = None;
                if let Some(worker) = self.worker.take()
                    && worker.join().is_err()
                {
                    return Some(Event::Status(UpdateStatus::Failed(
                        "The update service stopped unexpectedly. Try again.".into(),
                    )));
                }
                return None;
            }
        };
        if matches!(event, Event::Status(ref status) if !status.busy()) {
            self.pending_terminal = Some(event);
            return self.poll();
        }
        Some(event)
    }
}

fn supported() -> bool {
    std::env::var("FLATPAK_ID").as_deref() == Ok(APP_ID)
        && std::fs::read_to_string("/.flatpak-info")
            .ok()
            .is_some_and(|text| application_name(&text) == Some(APP_ID))
}

fn application_name(text: &str) -> Option<&str> {
    let mut application = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            application = line == "[Application]";
        } else if application && let Some(name) = line.strip_prefix("name=") {
            return Some(name);
        }
    }
    None
}

fn service_error(error: impl std::fmt::Display) -> String {
    let detail = error.to_string();
    if detail.contains("AccessDenied")
        || detail.contains("NotAllowed")
        || detail.contains("Cancelled")
    {
        "Update permission was not granted. You can try again from Settings.".into()
    } else if detail.contains("NotSupported") {
        "Flatpak cannot apply this update automatically. Check your Software application.".into()
    } else if detail.contains("RemoteNotFound")
        || detail.contains("remote not found")
        || detail.contains("No such remote")
    {
        "This installation has no update source. Install the current SauceFX .flatpakref once."
            .into()
    } else {
        format!(
            "Could not update SauceFX: {}",
            detail.chars().take(240).collect::<String>()
        )
    }
}

async fn update(
    connection: &zbus::Connection,
    sender: &Sender<Event>,
    running: &Deployment,
) -> Result<(), String> {
    let portal = zbus::Proxy::new(connection, SERVICE, PATH, SERVICE)
        .await
        .map_err(service_error)?;
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let handle: OwnedObjectPath = portal
        .call("CreateUpdateMonitor", &(options,))
        .await
        .map_err(service_error)?;
    let monitor = zbus::Proxy::new(connection, SERVICE, handle.as_str(), MONITOR)
        .await
        .map_err(service_error)?;
    let result = async {
        // Subscribe before Update: even an empty transaction can emit its
        // terminal progress before the method reply arrives.
        let mut progress = monitor
            .receive_signal("Progress")
            .await
            .map_err(service_error)?;
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        let (): () = monitor
            .call("Update", &("", options))
            .await
            .map_err(service_error)?;
        let transaction = async {
            while let Some(message) = progress.next().await {
                let (info,): (Info,) = message.body().deserialize().map_err(service_error)?;
                let status =
                    progress_status(&info, running, async { latest_deployment(&portal).await })
                        .await?;
                if !status.busy() {
                    return Ok(status);
                }
                if sender.send(Event::Status(status)).is_err() {
                    return Err("The update window closed.".to_string());
                }
            }
            Err("The update service disconnected. Try again.".into())
        };
        future::or(transaction, async {
            async_io::Timer::after(UPDATE_TIMEOUT).await;
            Err("The update took too long. Check your connection and try again.".into())
        })
        .await
    }
    .await;
    let close: Result<(), _> = monitor.call("Close", &()).await;
    let status = result?;
    let _ = close;
    sender
        .send(Event::Status(status))
        .map_err(|_| "The update window closed.".to_string())
}

#[derive(Debug, PartialEq, Eq)]
struct Deployment {
    app_id: String,
    arch: String,
    branch: String,
    installation: PathBuf,
    commit: String,
}

fn read_running_deployment() -> Result<Deployment, String> {
    let bytes = std::fs::read("/.flatpak-info").map_err(|_| METADATA_ERROR.to_string())?;
    parse_deployment(&bytes).map_err(|_| METADATA_ERROR.to_string())
}

fn deployment_field<'a>(values: &HashMap<&str, &'a str>, key: &str) -> Result<&'a str, String> {
    values
        .get(key)
        .copied()
        .ok_or_else(|| METADATA_ERROR.to_string())
}

fn parse_deployment(bytes: &[u8]) -> Result<Deployment, String> {
    if bytes.len() > METADATA_LIMIT {
        return Err(METADATA_ERROR.into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| METADATA_ERROR)?;
    let mut section = "";
    let mut application = HashMap::new();
    let mut instance = HashMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let target = match section {
            "Application" => &mut application,
            "Instance" => &mut instance,
            _ => continue,
        };
        if target.contains_key(key) {
            return Err(METADATA_ERROR.into());
        }
        target.insert(key, raw_value);
    }

    let app_id = deployment_field(&application, "name")?.to_string();
    let arch = deployment_field(&instance, "arch")?.to_string();
    let branch = deployment_field(&instance, "branch")?.to_string();
    let commit = deployment_field(&instance, "app-commit")?.to_string();
    let deployment_path = instance
        .get("original-app-path")
        .or_else(|| instance.get("app-path"))
        .ok_or_else(|| METADATA_ERROR.to_string())?;
    let deployment_path = decode_keyfile_path(deployment_path)?;
    if app_id != APP_ID || arch.is_empty() || branch.is_empty() {
        return Err(METADATA_ERROR.into());
    }
    let installation = deployment_installation(&deployment_path, &app_id, &arch, &branch, &commit)?;

    Ok(Deployment {
        app_id,
        arch,
        branch,
        installation,
        commit,
    })
}

fn deployment_installation(
    app_path: &Path,
    app_id: &str,
    arch: &str,
    branch: &str,
    app_commit: &str,
) -> Result<PathBuf, String> {
    if !app_path.is_absolute()
        || app_commit.len() != 64
        || !app_commit
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(METADATA_ERROR.into());
    }
    let components: Vec<_> = app_path.components().collect();
    if components.len() < 7 {
        return Err(METADATA_ERROR.into());
    }
    let suffix = &components[components.len() - 6..];
    if !matches!(suffix[0], Component::Normal(name) if name == "app")
        || !matches!(suffix[1], Component::Normal(name) if name.to_str() == Some(app_id))
        || !matches!(suffix[2], Component::Normal(name) if name.to_str() == Some(arch))
        || !matches!(suffix[3], Component::Normal(name) if name.to_str() == Some(branch))
        || !matches!(suffix[5], Component::Normal(name) if name == "files")
        || components
            .iter()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(METADATA_ERROR.into());
    }
    let checkout = match suffix[4] {
        Component::Normal(name) => name.to_str().ok_or(METADATA_ERROR)?,
        _ => return Err(METADATA_ERROR.into()),
    };
    if checkout != app_commit
        && !checkout.strip_prefix(app_commit).is_some_and(|subpath| {
            subpath.starts_with('-') && subpath.len() > 1 && subpath.len() <= 256
        })
    {
        return Err(METADATA_ERROR.into());
    }
    app_path
        .ancestors()
        .nth(6)
        .map(Path::to_path_buf)
        .ok_or_else(|| METADATA_ERROR.into())
}

fn decode_keyfile_path(value: &str) -> Result<PathBuf, String> {
    let mut path = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('s') => path.push(' '),
                Some('t') => path.push('\t'),
                Some('n') => path.push('\n'),
                Some('r') => path.push('\r'),
                Some('\\') => path.push('\\'),
                _ => return Err(METADATA_ERROR.into()),
            }
        } else {
            path.push(ch);
        }
    }
    if !path.starts_with('/') {
        return Err(METADATA_ERROR.into());
    }
    Ok(PathBuf::from(path))
}

async fn latest_deployment(portal: &zbus::Proxy<'_>) -> Result<Deployment, String> {
    let mut exited = portal
        .receive_signal("SpawnExited")
        .await
        .map_err(service_error)?;
    let (reader, writer) = UnixStream::pair().map_err(|_| METADATA_ERROR.to_string())?;
    reader
        .set_nonblocking(true)
        .map_err(|_| METADATA_ERROR.to_string())?;
    let cwd = b"/app\0".to_vec();
    let argv = vec![b"/usr/bin/cat\0".to_vec(), b"/.flatpak-info\0".to_vec()];
    let envs = HashMap::<String, String>::new();
    let fds = HashMap::from([(1u32, Fd::from(writer.as_fd()))]);
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let pid = portal
        .call("Spawn", &(cwd, argv, fds, envs, LATEST_VERSION, options))
        .await
        .map_err(service_error)?;
    if pid == 0 {
        return Err(METADATA_ERROR.into());
    }
    drop(writer);

    let capture = async {
        let mut reader = async_io::Async::new(reader).map_err(|_| METADATA_ERROR.to_string())?;
        let mut output = Vec::with_capacity(1024);
        let mut buffer = [0u8; 2048];
        loop {
            let count = reader
                .read(&mut buffer)
                .await
                .map_err(|_| METADATA_ERROR.to_string())?;
            if count == 0 {
                return Ok(output);
            }
            if output.len() + count > METADATA_LIMIT {
                return Err(METADATA_ERROR.to_string());
            }
            output.extend_from_slice(&buffer[..count]);
        }
    };
    let process_exit = async {
        while let Some(message) = exited.next().await {
            let (candidate, status): (u32, u32) =
                message.body().deserialize().map_err(service_error)?;
            if message
                .header()
                .path()
                .is_some_and(|path| path == portal.path())
                && candidate == pid
            {
                let wait_status = status as i32;
                if !libc::WIFEXITED(wait_status) || libc::WEXITSTATUS(wait_status) != 0 {
                    return Err(METADATA_ERROR.to_string());
                }
                return Ok(());
            }
        }
        Err(METADATA_ERROR.to_string())
    };
    let (bytes, ()) = future::or(future::try_zip(capture, process_exit), async {
        async_io::Timer::after(DEPLOYMENT_TIMEOUT).await;
        Err(METADATA_ERROR.to_string())
    })
    .await?;
    parse_deployment(&bytes).map_err(|_| METADATA_ERROR.to_string())
}

fn number(info: &Info, key: &str) -> Result<Option<u32>, String> {
    info.get(key)
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| format!("The update service sent an invalid {key} value."))
        })
        .transpose()
}

fn classify_terminal(
    code: u32,
    running: &Deployment,
    deployed: &Deployment,
) -> Result<UpdateStatus, String> {
    if !matches!(code, 1 | 2) {
        return Err(METADATA_ERROR.into());
    }
    if running.app_id != deployed.app_id
        || running.arch != deployed.arch
        || running.branch != deployed.branch
        || running.installation != deployed.installation
    {
        return Err(METADATA_ERROR.into());
    }
    Ok(if running.commit == deployed.commit {
        UpdateStatus::UpToDate
    } else {
        UpdateStatus::RestartRequired
    })
}

async fn progress_status<F>(
    info: &Info,
    running: &Deployment,
    deployment: F,
) -> Result<UpdateStatus, String>
where
    F: Future<Output = Result<Deployment, String>>,
{
    let status = number(info, "status")?.ok_or("The update service omitted its status.")?;
    match status {
        0 => {
            let progress = number(info, "progress")?.unwrap_or(0);
            let operations = number(info, "n_ops")?.unwrap_or(1).max(1);
            let operation = number(info, "op")?.unwrap_or(0).min(operations - 1);
            let percent =
                (u64::from(operation) * 100 + u64::from(progress.min(100))) / u64::from(operations);
            Ok(UpdateStatus::Installing((percent as u32).min(99)))
        }
        1 | 2 => classify_terminal(status, running, &deployment.await?),
        3 => {
            let detail = info
                .get("error_message")
                .and_then(|value| <&str>::try_from(value).ok())
                .unwrap_or("The update could not be installed. Try again.");
            Ok(UpdateStatus::Failed(service_error(detail)))
        }
        _ => Err("The update service returned an unknown status.".into()),
    }
}

async fn restart_latest(connection: &zbus::Connection) -> Result<(), String> {
    let portal = zbus::Proxy::new(connection, SERVICE, PATH, SERVICE)
        .await
        .map_err(service_error)?;
    let cwd = b"/app\0".to_vec();
    let argv = vec![b"/app/bin/SauceFX\0".to_vec(), b"--restart-wait\0".to_vec()];
    // Inherit the normal stdout/stderr so a failed replacement startup is
    // diagnosable, just like launching the application normally.
    use std::os::fd::AsFd;
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let fds: HashMap<u32, Fd<'_>> =
        HashMap::from([(1, Fd::from(stdout.as_fd())), (2, Fd::from(stderr.as_fd()))]);
    // Preserve only the app's data paths. Older portal versions do not
    // retain XDG_STATE_HOME when creating the replacement sandbox.
    let envs: HashMap<String, String> = [
        "XDG_STATE_HOME",
        "XDG_DATA_HOME",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
    ]
    .into_iter()
    .filter_map(|name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.is_empty())
            .map(|value| (name.to_string(), value))
    })
    .collect();
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let mut started = portal
        .receive_signal("SpawnStarted")
        .await
        .map_err(service_error)?;
    let mut exited = portal
        .receive_signal("SpawnExited")
        .await
        .map_err(service_error)?;
    // LATEST_VERSION=2 uses the new deployment; NOTIFY_START=64 confirms it
    // started before the running application saves, stops audio and exits.
    let pid: u32 = portal
        .call("Spawn", &(cwd, argv, fds, envs, 66u32, options))
        .await
        .map_err(service_error)?;
    if pid == 0 {
        return Err("Flatpak could not start the updated version. Try again.".into());
    }
    let ready = async {
        while let Some(message) = started.next().await {
            let (candidate, _relative): (u32, u32) =
                message.body().deserialize().map_err(service_error)?;
            if candidate == pid {
                return Ok(());
            }
        }
        Err("Flatpak disconnected before starting the update. Try again.".into())
    };
    let failed = async {
        while let Some(message) = exited.next().await {
            let (candidate, _status): (u32, u32) =
                message.body().deserialize().map_err(service_error)?;
            if candidate == pid {
                return Err("The updated application could not start. Try again.".into());
            }
        }
        Err("Flatpak disconnected before starting the update. Try again.".into())
    };
    future::or(future::or(ready, failed), async {
        async_io::Timer::after(Duration::from_secs(10)).await;
        Err("Flatpak did not confirm the restart. Try again.".into())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNING_COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const UPDATED_COMMIT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn metadata(root: &str, name: &str, arch: &str, branch: &str, commit: &str) -> String {
        format!(
            "[Application]\nname={name}\n\
             [Instance]\narch={arch}\nbranch={branch}\napp-commit={commit}\n\
             app-path={root}/app/{name}/{arch}/{branch}/{commit}/files\n"
        )
    }

    fn deployment(commit: &str) -> Deployment {
        parse_deployment(
            metadata("/var/lib/flatpak", APP_ID, "x86_64", "master", commit).as_bytes(),
        )
        .unwrap()
    }

    fn terminal_status(
        code: u32,
        deployed: Result<Deployment, String>,
    ) -> Result<UpdateStatus, String> {
        future::block_on(progress_status(
            &HashMap::from([("status".into(), code.into())]),
            &deployment(RUNNING_COMMIT),
            future::ready(deployed),
        ))
    }

    #[test]
    fn validates_running_application_identity() {
        assert_eq!(
            application_name(&format!(
                "[Instance]\nname=wrong\n[Application]\nname={APP_ID}\n"
            )),
            Some(APP_ID)
        );
        assert_eq!(
            application_name("[Instance]\nname=io.github.sauucee.SauceFX\n"),
            None
        );
        assert_eq!(
            parse_deployment(
                metadata(
                    "/var/lib/flatpak",
                    APP_ID,
                    "x86_64",
                    "master",
                    RUNNING_COMMIT
                )
                .as_bytes()
            )
            .unwrap()
            .commit,
            RUNNING_COMMIT
        );
    }

    #[test]
    fn terminal_status_is_not_delivered_until_the_worker_can_accept_restart() {
        let timeout = Duration::from_secs(5);
        let (sender, receiver) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            sender
                .send(Event::Status(UpdateStatus::RestartRequired))
                .unwrap();
            ready_tx.send(()).unwrap();
            let _ = release_rx.recv();
        });
        ready_rx
            .recv_timeout(timeout)
            .expect("worker sent terminal event before poll");
        let mut updater = Updater {
            receiver: Some(receiver),
            worker: Some(worker),
            ..Updater::default()
        };
        // A synchronous join regression must fail the test, not hang it.
        let (polled_tx, polled_rx) = mpsc::channel();
        let poller = thread::spawn(move || {
            let _ = polled_tx.send(updater.poll());
            updater
        });
        let first_poll = polled_rx
            .recv_timeout(timeout)
            .expect("poll must return while the worker is blocked");
        assert!(first_poll.is_none());
        let mut updater = poller.join().unwrap();
        assert!(updater.pending_terminal.is_some());
        release_tx.send(()).unwrap();
        let deadline = std::time::Instant::now() + timeout;
        while !updater.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not finish after release"
            );
            thread::sleep(Duration::from_millis(1));
        }
        assert!(matches!(
            updater.poll(),
            Some(Event::Status(UpdateStatus::RestartRequired))
        ));
    }

    #[test]
    fn receiver_only_updater_returns_stashed_terminal_event() {
        let (sender, receiver) = mpsc::channel();
        let mut updater = Updater {
            receiver: Some(receiver),
            ..Updater::default()
        };
        sender
            .send(Event::Status(UpdateStatus::RestartRequired))
            .unwrap();
        assert!(matches!(
            updater.poll(),
            Some(Event::Status(UpdateStatus::RestartRequired))
        ));
    }

    #[test]
    fn empty_recheck_keeps_an_installed_update_pending_restart() {
        let (sender, receiver) = mpsc::channel();
        let mut updater = Updater {
            receiver: Some(receiver),
            ..Updater::default()
        };
        for code in [2, 1] {
            sender
                .send(Event::Status(
                    terminal_status(code, Ok(deployment(UPDATED_COMMIT))).unwrap(),
                ))
                .unwrap();
            assert!(matches!(
                updater.poll(),
                Some(Event::Status(UpdateStatus::RestartRequired))
            ));
        }
    }

    #[test]
    fn a_fresh_current_deployment_can_clear_a_previous_restart_result() {
        let (sender, receiver) = mpsc::channel();
        let mut updater = Updater {
            receiver: Some(receiver),
            ..Updater::default()
        };
        for (commit, expected) in [
            (UPDATED_COMMIT, UpdateStatus::RestartRequired),
            (RUNNING_COMMIT, UpdateStatus::UpToDate),
        ] {
            sender
                .send(Event::Status(
                    terminal_status(2, Ok(deployment(commit))).unwrap(),
                ))
                .unwrap();
            assert!(matches!(updater.poll(), Some(Event::Status(status)) if status == expected));
        }
    }

    #[test]
    fn newest_deployment_path_is_authoritative_over_the_previous_mount() {
        let metadata = format!(
            "[Application]\nname={APP_ID}\n[Instance]\narch=x86_64\nbranch=master\napp-commit={UPDATED_COMMIT}\napp-path=/var/lib/flatpak/app/{APP_ID}/x86_64/master/{RUNNING_COMMIT}/files\noriginal-app-path=/var/lib/flatpak/app/{APP_ID}/x86_64/master/{UPDATED_COMMIT}/files\n"
        );
        let updated = parse_deployment(metadata.as_bytes()).unwrap();
        assert_eq!(updated.commit, UPDATED_COMMIT);
        assert_eq!(
            classify_terminal(2, &deployment(RUNNING_COMMIT), &updated).unwrap(),
            UpdateStatus::RestartRequired
        );

        let missing_app_mount = format!(
            "[Application]\nname={APP_ID}\n[Instance]\narch=x86_64\nbranch=master\napp-commit={UPDATED_COMMIT}\napp-path=\noriginal-app-path=/var/lib/flatpak/app/{APP_ID}/x86_64/master/{UPDATED_COMMIT}/files\n"
        );
        assert_eq!(
            parse_deployment(missing_app_mount.as_bytes())
                .unwrap()
                .commit,
            UPDATED_COMMIT
        );
    }

    #[test]
    fn current_app_never_requires_restart_after_empty_or_completed_transaction() {
        for code in [1, 2] {
            assert_eq!(
                terminal_status(code, Ok(deployment(RUNNING_COMMIT))).unwrap(),
                UpdateStatus::UpToDate
            );
        }
    }

    #[test]
    fn runtime_only_change_does_not_report_an_app_update() {
        let running = metadata(
            "/var/lib/flatpak",
            APP_ID,
            "x86_64",
            "master",
            RUNNING_COMMIT,
        );
        let deployed = format!("{running}runtime-commit={UPDATED_COMMIT}\n");
        assert_eq!(
            terminal_status(2, parse_deployment(deployed.as_bytes())).unwrap(),
            UpdateStatus::UpToDate
        );
    }

    #[test]
    fn changed_app_commit_requires_restart_even_with_the_same_version() {
        assert_eq!(
            terminal_status(2, Ok(deployment(UPDATED_COMMIT))).unwrap(),
            UpdateStatus::RestartRequired
        );
    }

    #[test]
    fn empty_transaction_detects_an_update_already_installed_outside_the_process() {
        assert_eq!(
            terminal_status(1, Ok(deployment(UPDATED_COMMIT))).unwrap(),
            UpdateStatus::RestartRequired
        );
    }

    #[test]
    fn deployment_query_failure_never_reports_success() {
        for code in [1, 2] {
            assert_eq!(
                terminal_status(code, Err("Deployment query failed.".into())),
                Err("Deployment query failed.".into())
            );
        }
    }

    #[test]
    fn deployment_identity_must_match_before_commits_are_compared() {
        for text in [
            metadata(
                "/home/test/.local/share/flatpak",
                APP_ID,
                "x86_64",
                "master",
                UPDATED_COMMIT,
            ),
            metadata(
                "/var/lib/flatpak",
                APP_ID,
                "aarch64",
                "master",
                UPDATED_COMMIT,
            ),
            metadata("/var/lib/flatpak", APP_ID, "x86_64", "beta", UPDATED_COMMIT),
            metadata(
                "/var/lib/flatpak",
                "io.github.example.Other",
                "x86_64",
                "master",
                UPDATED_COMMIT,
            ),
            format!(
                "[Application]\nname={APP_ID}\n[Instance]\narch=x86_64\nbranch=master\napp-commit={UPDATED_COMMIT}\napp-path=/var/lib/flatpak/app/{APP_ID}/x86_64/master/{RUNNING_COMMIT}/files\noriginal-app-path=/home/test/.local/share/flatpak/app/{APP_ID}/x86_64/master/{UPDATED_COMMIT}/files\n"
            ),
        ] {
            for code in [1, 2] {
                assert!(terminal_status(code, parse_deployment(text.as_bytes())).is_err());
            }
        }
    }

    #[test]
    fn deployment_metadata_requires_unambiguous_instance_fields_and_a_valid_layout() {
        let valid = metadata(
            "/var/lib/flatpak",
            APP_ID,
            "x86_64",
            "master",
            RUNNING_COMMIT,
        );
        for invalid in [
            valid.replace("[Instance]", "[Other]"),
            valid.replace("arch=x86_64\n", ""),
            valid.replace("branch=master\n", ""),
            valid.replace(&format!("app-commit={RUNNING_COMMIT}\n"), ""),
            valid.replace(RUNNING_COMMIT, "not-a-commit"),
            valid.replace("app-commit=", "app-commit=ABC"),
            valid.replace("app-path=/var/lib", "app-path=var/lib"),
            valid.replace(
                "/app/io.github.sauucee.SauceFX/",
                "/app/io.github.example.Other/",
            ),
            valid.replace("/x86_64/master/", "/aarch64/master/"),
            valid.replace("/master/", "/beta/"),
            valid.replace("/files\n", "/../files\n"),
            format!("{valid}app-commit={UPDATED_COMMIT}\n"),
            format!("{valid}[Application]\nname=io.github.example.Other\n"),
            format!("{valid}\n[Instance]\noriginal-app-path=relative\n"),
        ] {
            assert!(
                parse_deployment(invalid.as_bytes()).is_err(),
                "accepted {invalid}"
            );
        }
        assert!(parse_deployment(b"\xff").is_err());
        assert!(parse_deployment(&vec![b' '; METADATA_LIMIT + 1]).is_err());
    }

    #[test]
    fn escaped_installation_paths_compare_as_keyfile_strings() {
        let escaped = metadata(
            "/home/test/Flatpak\\sApps",
            APP_ID,
            "x86_64",
            "master",
            RUNNING_COMMIT,
        );
        let plain = metadata(
            "/home/test/Flatpak Apps",
            APP_ID,
            "x86_64",
            "master",
            RUNNING_COMMIT,
        );
        assert_eq!(
            parse_deployment(escaped.as_bytes()).unwrap(),
            parse_deployment(plain.as_bytes()).unwrap()
        );
    }

    #[test]
    fn failed_transactions_do_not_query_or_report_a_deployment() {
        for info in [
            HashMap::from([("status".into(), 3u32.into())]),
            HashMap::from([
                ("status".into(), 3u32.into()),
                (
                    "error_message".into(),
                    OwnedValue::from(zbus::zvariant::Str::from("signature rejected")),
                ),
            ]),
        ] {
            let result =
                future::block_on(progress_status(&info, &deployment(RUNNING_COMMIT), async {
                    panic!("failed transactions must not query a deployment")
                }))
                .unwrap();
            assert!(matches!(result, UpdateStatus::Failed(_)));
        }
    }

    #[test]
    fn malformed_progress_never_reports_success() {
        for info in [
            HashMap::new(),
            HashMap::from([("status".into(), 9u32.into())]),
            HashMap::from([("status".into(), true.into())]),
        ] {
            assert!(
                future::block_on(progress_status(&info, &deployment(RUNNING_COMMIT), async {
                    panic!("malformed progress must not query a deployment")
                },))
                .is_err()
            );
        }
    }

    #[test]
    fn progress_accounts_for_multiple_operations_and_waits_for_done() {
        for (info, expected) in [
            (
                HashMap::from([
                    ("status".into(), 0u32.into()),
                    ("n_ops".into(), 2u32.into()),
                    ("op".into(), 1u32.into()),
                    ("progress".into(), 50u32.into()),
                ]),
                UpdateStatus::Installing(75),
            ),
            (
                HashMap::from([
                    ("status".into(), 0u32.into()),
                    ("progress".into(), 100u32.into()),
                ]),
                UpdateStatus::Installing(99),
            ),
        ] {
            assert_eq!(
                future::block_on(progress_status(&info, &deployment(RUNNING_COMMIT), async {
                    panic!("progress must not query a deployment before completion")
                },))
                .unwrap(),
                expected
            );
        }
    }
}
