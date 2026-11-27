//! The real SauceFX host: the Slint interface bound to the live
//! DfxDsp/PipeWire engine through fxpw, with the same on-disk contracts as
//! the JUCE app (settings.txt, preset-session.txt, .fac library dirs).

mod autostart;
mod fxpw;
mod paths;
mod presets;
mod single_instance;
mod updater;
mod visualizer;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use saucefx_core::session::{PresetSession, SESSION_MAX_BANDS, encode_session, parse_session};
use saucefx_core::settings::{GuiSettings, ThemeMode, encode_settings, parse_settings};
use saucefx_core::state::BAND_COUNT;
use saucefx_core::updates::UpdateStatus;
use saucefx_ui::AppWindow;
use saucefx_ui::binding::{MenuRow, UiBinding};
use slint::{ComponentHandle, Timer, TimerMode};

const OUTPUT_INTERVAL_MS: u64 = 1000;
const SAVE_INTERVAL_MS: u64 = 2000;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn state_dir() -> PathBuf {
    paths::app_dir(
        &home_dir(),
        std::env::var("XDG_STATE_HOME").ok().as_deref(),
        ".local/state",
    )
}

fn data_dir() -> PathBuf {
    paths::app_dir(
        &home_dir(),
        std::env::var("XDG_DATA_HOME").ok().as_deref(),
        ".local/share",
    )
}

fn user_preset_dir() -> PathBuf {
    data_dir().join("presets")
}

/// Match the previous host's autostart path, including Flatpak's host-side
/// ~/.config/autostart exception.
fn config_dir() -> PathBuf {
    paths::config_dir(
        &home_dir(),
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var_os("FLATPAK_ID").is_some(),
    )
}

/// Resolve the factory library using the launch override and executable path.
fn preset_library_dir() -> PathBuf {
    let root = std::env::var("SAUCEFX_ROOT").ok().map(PathBuf::from);
    let exe = std::fs::read_link("/proc/self/exe").ok();
    presets::library_dir(root.as_deref(), exe.as_deref())
}

fn read_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Multiple-file chooser via zenity (the JUCE host used its own chooser;
/// zenity ships with GNOME and needs no extra build dependency). Cancel
/// yields an empty list.
fn zenity_select_files(title: &str, start_dir: &Path) -> Vec<PathBuf> {
    let output = std::process::Command::new("zenity")
        .args([
            "--file-selection",
            "--multiple",
            &format!("--title={title}"),
            "--file-filter=Presets | *.fac *.FAC",
            &format!("--filename={}/", start_dir.display()),
        ])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Save-mode chooser via zenity; cancel yields None.
fn zenity_save_file(title: &str, start: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new("zenity")
        .args([
            "--file-selection",
            "--save",
            "--confirm-overwrite",
            &format!("--title={title}"),
            &format!("--filename={}", start.display()),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(PathBuf::from(text))
    }
}

/// Output menu rows from the engine. Row 1 is Automatic; an empty device
/// name routes automatically. Device and port together identify an output.
fn output_rows(entries: Vec<fxpw::OutputEntry>) -> Vec<MenuRow> {
    let entries = fxpw::listable_outputs(&entries);
    // The reference output menu: WITH SAUCEFX EFFECTS (automatic + devices),
    // WITHOUT EFFECTS (direct playback), matching webSetOutput's ids.
    let mut rows = vec![MenuRow::group("WITH SAUCEFX EFFECTS")];
    rows.push(MenuRow::output(
        "Automatic",
        "Chooses a connected headset or speakers",
        "",
        "",
        true,
    ));
    for entry in entries {
        let label = if entry.port_description.is_empty() {
            entry.description.clone()
        } else {
            entry.port_description.clone()
        };
        let label = if label.is_empty() {
            entry.device.clone()
        } else {
            label
        };
        rows.push(MenuRow::output(
            &label,
            &entry.description,
            &entry.device,
            &entry.port,
            entry.available,
        ));
    }
    rows.push(MenuRow::group("WITHOUT EFFECTS"));
    rows.push(MenuRow::output(
        "Direct playback",
        "Use system output without SauceFX",
        "direct",
        "",
        true,
    ));
    rows
}

/// Restore the live session (selected preset + unsaved tweaks + power).
/// Returns false when no usable session exists (first launch).
fn restore_session(binding: &Rc<RefCell<UiBinding>>, state_dir: &Path) -> bool {
    let Some(text) = read_file(&state_dir.join("preset-session.txt")) else {
        return false;
    };
    let Ok(session) = parse_session(&text) else {
        return false;
    };
    if session.power != binding.borrow().power() {
        binding.borrow_mut().toggle_power();
    }
    let index = binding
        .borrow()
        .find_preset_by_path(&session.preset_path)
        .or_else(|| {
            session
                .factory_preset_basename()
                .and_then(|basename| binding.borrow().find_preset_by_basename(basename))
        });
    if let Some(index) = index {
        binding.borrow_mut().choose_preset(index);
    }
    binding
        .borrow_mut()
        .apply_captured_values(&session.effects, &session.eq_gains);
    true
}

/// Write settings.txt and preset-session.txt when their content changed.
fn save_state(
    binding: &Rc<RefCell<UiBinding>>,
    engine: &fxpw::FxpwEngine,
    state_dir: &Path,
    factory_dir: &Path,
    last_written: &RefCell<(String, String)>,
) {
    let binding = binding.borrow();
    let output = binding.state().output();
    let gui = GuiSettings {
        theme: binding.theme(),
        custom_accent: binding.custom_accent(),
        accent: binding.display_accent(),
        autostart: binding.autostart(),
        output_routed: output.routed,
        output_device: output.preferred_name.clone(),
        output_port: output.preferred_port.clone(),
    };
    let Ok(settings_text) = encode_settings(&gui) else {
        return;
    };

    let path = binding.selected_preset_path().to_string();
    let factory_preset = if !path.is_empty() && Path::new(&path).parent() == Some(factory_dir) {
        Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    } else {
        None
    };
    let mut eq_freqs = [0.0; SESSION_MAX_BANDS];
    let mut eq_gains = [0.0; SESSION_MAX_BANDS];
    let bands = engine.band_freqs().len().min(SESSION_MAX_BANDS);
    eq_freqs[..bands].copy_from_slice(&engine.band_freqs()[..bands]);
    let gains = binding.eq_gains();
    eq_gains[..BAND_COUNT].copy_from_slice(&gains);
    let session = PresetSession {
        preset_path: path,
        factory_preset,
        power: binding.power(),
        effects: binding.effects(),
        eq_bands: bands,
        eq_freqs,
        eq_gains,
    };
    let Ok(session_text) = encode_session(&session) else {
        return;
    };

    let mut last = last_written.borrow_mut();
    if last.0 != settings_text
        && std::fs::write(state_dir.join("settings.txt"), &settings_text).is_ok()
    {
        last.0 = settings_text;
    }
    if last.1 != session_text
        && std::fs::write(state_dir.join("preset-session.txt"), &session_text).is_ok()
    {
        last.1 = session_text;
    }
}

fn main() {
    let state = state_dir();
    let factory_dir = preset_library_dir();

    // Single instance: the first launch holds an flock on app.lock for the
    // process lifetime. A second launch drops a show.request marker and
    // leaves without starting a second engine/sink — silently when the
    // launch came from login autostart (Main.cpp's instance lock).
    std::fs::create_dir_all(&state).ok();
    let auto_start_launch = std::env::args().any(|arg| arg == "--autostart");
    let restart_launch = std::env::args().any(|arg| arg == "--restart-wait");
    let lock = std::fs::File::create(single_instance::lock_file(&state))
        .expect("create the app.lock file");
    {
        let wait = if restart_launch {
            Duration::from_secs(15)
        } else {
            Duration::ZERO
        };
        if !single_instance::acquire(&lock, wait).expect("acquire the SauceFX process lock") {
            if !auto_start_launch {
                if !single_instance::write_show_request(&state) {
                    eprintln!(
                        "saucefx: could not write {}",
                        single_instance::show_request_file(&state).display()
                    );
                }
                eprintln!(
                    "saucefx: another SauceFX instance is running - asked it to show its window"
                );
            } else {
                eprintln!("saucefx: autostart launch ignored; another instance is running");
            }
            std::process::exit(0);
        }
    }
    // A stale show marker belongs to a previous session.
    let _ = std::fs::remove_file(single_instance::show_request_file(&state));
    // The lock file handle stays open for the process lifetime.
    let settings = read_file(&state.join("settings.txt"))
        .map(|text| parse_settings(&text))
        .unwrap_or_default();

    // The pipeline must exist before the catalog can read preset names.
    // At login the session audio stack can lag the app by seconds, so the
    // start is retried within a budget instead of leaving a windowless
    // process behind (the boot failure reported while testing 0.3.3).
    let engine = fxpw::start_with_retry(
        || {
            fxpw::FxpwEngine::start().map_err(|error| {
                eprintln!("saucefx: {error}; retrying the audio engine");
                error.to_string()
            })
        },
        fxpw::ENGINE_START_MAX_ATTEMPTS,
        &mut |delay| std::thread::sleep(delay),
    )
    .unwrap_or_else(|| {
        eprintln!("saucefx: audio engine did not come up after retries");
        std::process::exit(1);
    });

    let window = AppWindow::new().expect("create the SauceFX window");
    let binding = UiBinding::install(&window);

    {
        let mut binding = binding.borrow_mut();
        binding.set_engine(Rc::clone(&engine) as Rc<RefCell<dyn saucefx_ui::binding::Engine>>);
        binding.set_light(settings.theme == ThemeMode::Light);
        if settings.custom_accent {
            binding.set_accent(settings.accent);
        }
        binding.set_custom_accent(settings.custom_accent);
        binding.set_autostart(settings.autostart);
        let scanned = presets::scan(&factory_dir, &user_preset_dir());
        let factory = presets::fixtures(&engine.borrow(), &scanned);
        let outputs = output_rows(engine.borrow().outputs());
        binding.install_catalog(factory, Vec::new(), outputs, 1);
    }
    // First launch (no usable session): new users start on the Default
    // preset like the reference, located by name — the scanned library's
    // row order is not fixed, so the catalog's default row index is not
    // reliably Default. Loading it here also keeps the engine from running
    // preset-less until the user picks one.
    if !restore_session(&binding, &state) {
        let default = binding.borrow().find_preset_by_basename("Default.fac");
        match default {
            Some(index) => {
                binding.borrow_mut().choose_preset(index);
            }
            None => {
                binding.borrow_mut().load_selected_preset();
            }
        }
    }
    // Restore the saved identity directly, including a disconnected device's
    // port. Choosing a fallback menu row here would erase that preference.
    binding.borrow_mut().restore_output(
        &settings.output_device,
        &settings.output_port,
        settings.output_routed,
    );
    let updates = Rc::new(RefCell::new(updater::Updater::new()));
    if std::env::var("FLATPAK_ID").as_deref() != Ok("io.github.sauucee.SauceFX") {
        binding
            .borrow_mut()
            .set_update_status(UpdateStatus::Unsupported);
    }
    binding.borrow().sync(&window);

    // The update service runs off the UI thread; only this explicit action
    // starts a check/install or asks Flatpak to spawn the updated version.
    {
        let updates = Rc::clone(&updates);
        let binding = Rc::clone(&binding);
        let window_weak = window.as_weak();
        window.on_update_clicked(move || {
            let status = binding.borrow().update_status().clone();
            if !status.action_enabled() {
                return;
            }
            let started = if status == UpdateStatus::RestartRequired {
                updates.borrow_mut().restart()
            } else {
                updates.borrow_mut().start()
            };
            if started {
                binding
                    .borrow_mut()
                    .set_update_status(UpdateStatus::Checking);
                if let Some(window) = window_weak.upgrade() {
                    binding.borrow().sync(&window);
                }
            }
        });
    }
    let update_timer = Timer::default();
    {
        let updates = Rc::clone(&updates);
        let binding = Rc::clone(&binding);
        let engine = Rc::clone(&engine);
        let state = state.clone();
        let factory_dir = factory_dir.clone();
        let window = window.as_weak();
        update_timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            loop {
                let event = updates.borrow_mut().poll();
                match event {
                    Some(updater::Event::Status(status)) => {
                        binding.borrow_mut().set_update_status(status);
                        if let Some(window) = window.upgrade() {
                            binding.borrow().sync(&window);
                        }
                    }
                    Some(updater::Event::RestartLaunched) => {
                        save_state(
                            &binding,
                            &engine.borrow(),
                            &state,
                            &factory_dir,
                            &RefCell::new((String::new(), String::new())),
                        );
                        engine.borrow_mut().stop();
                        let _ = slint::quit_event_loop();
                        break;
                    }
                    None => break,
                }
            }
        });
    }

    // Settings -> Quit SauceFX: save, stop the engine, end the process.
    {
        let binding = Rc::clone(&binding);
        let engine = Rc::clone(&engine);
        let state = state.clone();
        let factory_dir = factory_dir.clone();
        let last_written = Rc::new(RefCell::new((String::new(), String::new())));
        let quit_last = Rc::clone(&last_written);
        window.on_quit_clicked(move || {
            save_state(&binding, &engine.borrow(), &state, &factory_dir, &quit_last);
            engine.borrow_mut().stop();
            let _ = slint::quit_event_loop();
        });
    }

    // Save dialog -> confirm: the engine writes the live state as
    // <name>.fac into the user preset directory (sanitized, never
    // overwriting an existing file or display name), the library is
    // rescanned, the saved preset is selected and both layers close —
    // the reference webSavePreset/writeCustomPreset semantics.
    {
        let binding = Rc::clone(&binding);
        let engine = Rc::clone(&engine);
        let factory_dir = factory_dir.clone();
        let state = state.clone();
        let last_written = Rc::new(RefCell::new((String::new(), String::new())));
        window.on_save_confirmed(move || {
            let raw = binding.borrow().save_name().to_string();
            let name = presets::sanitized_preset_name(&raw);
            if name.is_empty() {
                binding
                    .borrow_mut()
                    .show_toast("Enter a valid preset name.");
                return;
            }
            let labels = binding.borrow().preset_labels();
            let unique = presets::unique_custom_preset_name(&name, &user_preset_dir(), &labels);
            if std::fs::create_dir_all(user_preset_dir()).is_err() {
                binding
                    .borrow_mut()
                    .show_toast("Could not create the preset folder.");
                return;
            }
            if !engine.borrow().save_preset(&unique, &user_preset_dir()) {
                binding
                    .borrow_mut()
                    .show_toast(&format!("Could not save preset \"{unique}\"."));
                return;
            }
            let target = user_preset_dir().join(format!("{unique}.fac"));
            let written =
                target.is_file() && std::fs::metadata(&target).is_ok_and(|meta| meta.len() > 0);
            if !written {
                binding
                    .borrow_mut()
                    .show_toast(&format!("Could not save preset \"{unique}\"."));
                return;
            }
            let scanned = presets::scan(&factory_dir, &user_preset_dir());
            let factory = presets::fixtures(&engine.borrow(), &scanned);
            binding
                .borrow_mut()
                .apply_saved_preset(&unique, &target.to_string_lossy(), factory);
            save_state(
                &binding,
                &engine.borrow(),
                &state,
                &factory_dir,
                &last_written,
            );
        });
    }

    // Settings -> Import presets: choose .fac files, copy them into the
    // user preset directory (never shadowing a display name), rescan and
    // select the last import — the reference chooseImportPresets flow.
    {
        let binding = Rc::clone(&binding);
        let engine = Rc::clone(&engine);
        let factory_dir = factory_dir.clone();
        let state = state.clone();
        let last_written = Rc::new(RefCell::new((String::new(), String::new())));
        window.on_import_clicked(move || {
            let start = if user_preset_dir().is_dir() {
                user_preset_dir()
            } else {
                home_dir()
            };
            let files = zenity_select_files("Import presets", &start);
            if files.is_empty() {
                return;
            }
            let entries: Vec<(PathBuf, Option<String>)> = files
                .iter()
                .map(|file| (file.clone(), engine.borrow().preset_display_name(file)))
                .collect();
            let labels = binding.borrow().preset_labels();
            let outcome = presets::import_preset_files(&entries, &user_preset_dir(), &labels);
            if outcome.imported > 0 {
                let scanned = presets::scan(&factory_dir, &user_preset_dir());
                let factory = presets::fixtures(&engine.borrow(), &scanned);
                binding.borrow_mut().install_presets(factory);
                if let Some(last) = outcome.last.as_deref() {
                    binding
                        .borrow_mut()
                        .select_preset_by_path(&last.to_string_lossy());
                }
                save_state(
                    &binding,
                    &engine.borrow(),
                    &state,
                    &factory_dir,
                    &last_written,
                );
            }
            binding
                .borrow_mut()
                .show_toast(&presets::import_outcome_message(&outcome));
        });
    }

    // Settings -> Export preset: save the live engine state under the
    // chosen name in the chosen directory — chooseExportPreset +
    // exportCurrentPreset in the reference.
    {
        let binding = Rc::clone(&binding);
        let engine = Rc::clone(&engine);
        window.on_export_clicked(move || {
            let label = binding.borrow().selected_preset_label();
            let suggested = presets::export_suggested_file(&label, &home_dir());
            let Some(target) = zenity_save_file("Export current preset", &suggested) else {
                return;
            };
            let target = match target.extension() {
                Some(extension) if extension.eq_ignore_ascii_case("fac") => target,
                _ => target.with_extension("fac"),
            };
            let name = target
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dir = target.parent().map(Path::to_path_buf).unwrap_or_default();
            let mut ok = engine.borrow().save_preset(&name, &dir);
            if ok {
                ok = target.is_file();
            }
            binding
                .borrow_mut()
                .show_toast(&presets::export_message(&target, ok));
        });
    }

    // The autostart toggle registers/unregisters the XDG entry immediately,
    // reverting the switch and warning when the registration fails — the
    // reference applyAutostartSetting flow ("Could not update the login
    // item.").
    {
        let binding = Rc::clone(&binding);
        let window_weak = window.as_weak();
        let launch = autostart::resolve_launch(
            &std::env::current_exe().unwrap_or_else(|_| PathBuf::from("saucefx")),
            std::env::var("FLATPAK_ID").ok().as_deref(),
            std::env::var("APPIMAGE").ok().as_deref(),
            &data_dir(),
        );
        let entry = autostart::autostart_entry(&launch.0, &launch.1);
        let file = autostart::autostart_file(&config_dir());
        window.on_autostart_changed(move |enabled| {
            if autostart::apply_autostart(enabled, &file, &entry) {
                binding.borrow_mut().set_autostart(enabled);
            } else {
                binding
                    .borrow_mut()
                    .show_toast("Could not update the login item.");
            }
            if let Some(window) = window_weak.upgrade() {
                binding.borrow().sync(&window);
            }
        });
    }

    // Re-apply the login item at startup (the saved state, or on by default
    // for new users) — registering it must not depend on the user toggling
    // the switch, matching the reference's startup behaviour.
    {
        let enabled = binding.borrow().autostart();
        let launch = autostart::resolve_launch(
            &std::env::current_exe().unwrap_or_else(|_| PathBuf::from("saucefx")),
            std::env::var("FLATPAK_ID").ok().as_deref(),
            std::env::var("APPIMAGE").ok().as_deref(),
            &data_dir(),
        );
        let entry = autostart::autostart_entry(&launch.0, &launch.1);
        let file = autostart::autostart_file(&config_dir());
        if !autostart::apply_autostart(enabled, &file, &entry) {
            binding.borrow_mut().set_autostart(false);
            binding
                .borrow_mut()
                .show_toast("Could not update the login item.");
        }
    }

    {
        let binding = Rc::clone(&binding);
        let engine = Rc::clone(&engine);
        let trace = std::env::var_os("SAUCEFX_TRACE_SPECTRUM").is_some();
        if trace {
            eprintln!("saucefx: spectrum trace ready");
        }
        visualizer::install(&window, move |window, fresh| {
            let bands = engine.borrow().spectrum();
            if trace {
                eprintln!("saucefx: spectrum sample");
            }
            let mut binding = binding.borrow_mut();
            if fresh {
                binding.clear_spectrum();
            }
            binding.push_spectrum(&bands, now_ms());
            binding.sync(window);
        });
    }
    autostart::show_on_launch(window.window(), auto_start_launch)
        .expect("set initial SauceFX window visibility");

    // Keep the device menu current without applying routing again.
    let output_timer = Timer::default();
    {
        let binding = Rc::clone(&binding);
        let engine = Rc::clone(&engine);
        let window = window.as_weak();
        output_timer.start(
            TimerMode::Repeated,
            Duration::from_millis(OUTPUT_INTERVAL_MS),
            move || {
                let rows = output_rows(engine.borrow().outputs());
                let changed = binding.borrow_mut().refresh_outputs(rows);
                if changed && let Some(window) = window.upgrade() {
                    binding.borrow().sync(&window);
                }
            },
        );
    }

    // Persist settings and the live session; timers run on the event
    // loop thread, matching the fxpw and RefCell threading rules.
    let save_timer = Timer::default();
    {
        let binding = Rc::clone(&binding);
        let engine = Rc::clone(&engine);
        let state = state.clone();
        let factory_dir = factory_dir.clone();
        let last_written = Rc::new(RefCell::new((String::new(), String::new())));
        save_timer.start(
            TimerMode::Repeated,
            Duration::from_millis(SAVE_INTERVAL_MS),
            move || {
                save_state(
                    &binding,
                    &engine.borrow(),
                    &state,
                    &factory_dir,
                    &last_written,
                );
            },
        );
    }

    // A second launch dropped a show.request marker: bring the (possibly
    // hidden) window back and refresh its state.
    let restore_timer = Timer::default();
    {
        let binding = Rc::clone(&binding);
        let window_weak = window.as_weak();
        let state = state.clone();
        restore_timer.start(TimerMode::Repeated, Duration::from_millis(200), move || {
            if single_instance::consume_show_request(&state) {
                if let Some(window) = window_weak.upgrade() {
                    window.show().expect("show the SauceFX window");
                    binding.borrow().sync(&window);
                }
                eprintln!("saucefx: window restored from background");
            }
        });
    }

    slint::run_event_loop_until_quit().expect("run the SauceFX event loop");
}
