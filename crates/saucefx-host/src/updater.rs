//! Explicit Flatpak self-updates. D-Bus work stays on a worker thread; the
//! UI thread polls events and keeps ownership of the audio engine.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use futures_lite::{StreamExt, future};
use saucefx_core::updates::UpdateStatus;
use zbus::zvariant::{Fd, OwnedObjectPath, OwnedValue, Value};

const APP_ID: &str = "io.github.sauucee.SauceFX";
const SERVICE: &str = "org.freedesktop.portal.Flatpak";
const PATH: &str = "/org/freedesktop/portal/Flatpak";
const MONITOR: &str = "org.freedesktop.portal.Flatpak.UpdateMonitor";
const METHOD_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

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
        if self
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
                    update(&connection, &sender).await
                }
            });
            if let Err(message) = result {
                let _ = sender.send(Event::Status(UpdateStatus::Failed(message)));
            }
        }));
        true
    }

    pub fn poll(&mut self) -> Option<Event> {
        let receiver = self.receiver.as_ref()?;
        match receiver.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.receiver = None;
                if let Some(worker) = self.worker.take() {
                    if worker.join().is_err() {
                        return Some(Event::Status(UpdateStatus::Failed(
                            "The update service stopped unexpectedly. Try again.".into(),
                        )));
                    }
                }
                None
            }
        }
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

async fn update(connection: &zbus::Connection, sender: &Sender<Event>) -> Result<(), String> {
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
                let status = progress_status(&info)?;
                let terminal = !status.busy();
                if sender.send(Event::Status(status)).is_err() {
                    return Err("The update window closed.".to_string());
                }
                if terminal {
                    return Ok(());
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
    let _: Result<(), _> = monitor.call("Close", &()).await;
    result
}

fn number(info: &Info, key: &str) -> Result<Option<u32>, String> {
    info.get(key)
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| format!("The update service sent an invalid {key} value."))
        })
        .transpose()
}

fn progress_status(info: &Info) -> Result<UpdateStatus, String> {
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
        1 => Ok(UpdateStatus::UpToDate),
        2 => Ok(UpdateStatus::RestartRequired),
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
    let fds: HashMap<u32, Fd<'_>> = HashMap::new();
    let envs: HashMap<&str, &str> = HashMap::new();
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

    #[test]
    fn validates_application_identity_in_the_application_section() {
        assert_eq!(
            application_name(
                "[Instance]\nname=wrong\n[Application]\nname=io.github.sauucee.SauceFX\n"
            ),
            Some(APP_ID)
        );
        assert_eq!(
            application_name("[Instance]\nname=io.github.sauucee.SauceFX\n"),
            None
        );
    }

    #[test]
    fn update_terminal_states_are_distinct() {
        for (code, expected) in [
            (1u32, UpdateStatus::UpToDate),
            (2u32, UpdateStatus::RestartRequired),
        ] {
            assert_eq!(
                progress_status(&HashMap::from([("status".into(), code.into())])).unwrap(),
                expected
            );
        }
        let result = progress_status(&HashMap::from([("status".into(), 3u32.into())])).unwrap();
        assert!(matches!(result, UpdateStatus::Failed(_)));
    }

    #[test]
    fn malformed_progress_never_reports_success() {
        assert!(progress_status(&HashMap::new()).is_err());
        assert!(progress_status(&HashMap::from([("status".into(), 9u32.into())])).is_err());
        assert!(progress_status(&HashMap::from([("status".into(), true.into())])).is_err());
    }

    #[test]
    fn progress_accounts_for_multiple_operations_and_waits_for_done() {
        let info = HashMap::from([
            ("status".into(), 0u32.into()),
            ("n_ops".into(), 2u32.into()),
            ("op".into(), 1u32.into()),
            ("progress".into(), 50u32.into()),
        ]);
        assert_eq!(
            progress_status(&info).unwrap(),
            UpdateStatus::Installing(75)
        );
        let info = HashMap::from([
            ("status".into(), 0u32.into()),
            ("progress".into(), 100u32.into()),
        ]);
        assert_eq!(
            progress_status(&info).unwrap(),
            UpdateStatus::Installing(99)
        );
    }
}
