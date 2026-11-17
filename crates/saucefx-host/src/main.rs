//! The real SauceFX host: the Slint interface bound to the live
//! DfxDsp/PipeWire engine through fxpw, with the same on-disk contracts as
//! the JUCE app (settings.txt, preset-session.txt, .fac library dirs).

mod fxpw;
mod presets;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use saucefx_core::session::{PresetSession, SESSION_MAX_BANDS, encode_session, parse_session};
use saucefx_core::settings::{GuiSettings, ThemeMode, encode_settings, parse_settings};
use saucefx_core::state::BAND_COUNT;
use saucefx_ui::AppWindow;
use saucefx_ui::binding::{MenuRow, UiBinding};
use slint::{ComponentHandle, Timer, TimerMode};

const SPECTRUM_INTERVAL_MS: u64 = 50;
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
    home_dir().join(".local").join("state").join("saucefx")
}

fn data_dir() -> PathBuf {
    home_dir().join(".local").join("share").join("saucefx")
}

fn user_preset_dir() -> PathBuf {
    data_dir().join("presets")
}

/// The factory .fac library: `SAUCEFX_ROOT` override, else the repo checkout
/// found from the executable path, else the installed data dir (Main.cpp's
/// presetLibraryDir resolution order).
fn preset_library_dir() -> PathBuf {
    if let Ok(root) = std::env::var("SAUCEFX_ROOT") {
        return PathBuf::from(root).join("presets").join("Factsoft");
    }
    if let Ok(exe) = std::fs::read_link("/proc/self/exe") {
        let mut dir = exe.parent().map(Path::to_path_buf);
        while let Some(current) = dir {
            let candidate = current.join("presets").join("Factsoft");
            if candidate.is_dir() {
                return candidate;
            }
            dir = current.parent().map(Path::to_path_buf);
        }
    }
    PathBuf::from("/usr/share/saucefx")
        .join("presets")
        .join("Factsoft")
}

fn read_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Output menu rows from the engine, with the saved/preferred selection.
/// Row 1 is Automatic; an empty device name routes automatically.
fn output_rows(engine: &fxpw::FxpwEngine, settings: &GuiSettings) -> (Vec<MenuRow>, usize) {
    let mut rows = vec![MenuRow::output(
        "Automatic",
        "Follow the system output",
        "",
        "",
        true,
    )];
    for entry in engine.outputs() {
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
    let (engine_preferred, engine_port, _) = engine.output_state();
    let preferred_device = if settings.output_device.is_empty() {
        engine_preferred
    } else {
        settings.output_device.clone()
    };
    let preferred_port = if settings.output_device.is_empty() {
        engine_port
    } else {
        settings.output_port.clone()
    };
    let selected = rows
        .iter()
        .position(|row| {
            !row.is_group
                && row.output_name == preferred_device
                && row.output_port == preferred_port
        })
        .unwrap_or(1);
    (rows, selected)
}

/// Restore the live session (selected preset + unsaved tweaks + power).
fn restore_session(binding: &Rc<RefCell<UiBinding>>, state_dir: &Path) {
    let Some(text) = read_file(&state_dir.join("preset-session.txt")) else {
        return;
    };
    let Ok(session) = parse_session(&text) else {
        return;
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
    let settings = read_file(&state.join("settings.txt"))
        .map(|text| parse_settings(&text))
        .unwrap_or_default();

    // The pipeline must exist before the catalog can read preset names.
    let engine = fxpw::FxpwEngine::start().unwrap_or_else(|error| {
        eprintln!("saucefx: {error}");
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
        let (outputs, selected_output) = output_rows(&engine.borrow(), &settings);
        binding.install_catalog(factory, Vec::new(), outputs, selected_output);
    }
    restore_session(&binding, &state);
    // The saved (or preferred) output routes through the engine.
    let selected_output = binding.borrow().selected_output();
    binding.borrow_mut().choose_output(selected_output);
    binding.borrow().sync(&window);

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

    window.show().expect("show the SauceFX window");

    // Live spectrum: the engine's analyzer feeds the view's history.
    let spectrum_timer = Timer::default();
    {
        let binding = Rc::clone(&binding);
        let engine = Rc::clone(&engine);
        let window = window.as_weak();
        spectrum_timer.start(
            TimerMode::Repeated,
            Duration::from_millis(SPECTRUM_INTERVAL_MS),
            move || {
                let bands = engine.borrow().spectrum();
                let now = now_ms();
                binding.borrow_mut().push_spectrum(&bands, now);
                if let Some(window) = window.upgrade() {
                    binding.borrow().sync(&window);
                }
            },
        );
    }

    // Persist settings and the live session; both timers run on the event
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

    slint::run_event_loop_until_quit().expect("run the SauceFX event loop");
}
