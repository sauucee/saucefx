//! Locally launched preview window for the Packet 005 screenshots.
//!
//! This binary renders the real Slint window with the real Winit/FemtoVG
//! backend. It never creates an audio handle, opens an audio server, reads a
//! user file or writes to disk: every scene is synthetic and lives in memory.

use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;

use saucefx_ui::AppWindow;
use saucefx_ui::binding::UiBinding;

const BAND_BASE: [f32; 10] = [0.3, 0.56, 0.78, 0.68, 0.48, 0.58, 0.44, 0.31, 0.22, 0.15];

fn fixture_frame(step: usize) -> [f32; 10] {
    std::array::from_fn(|band| {
        let shape = 0.55 + 0.32 * ((band + step) as f32 * 0.72).sin();
        (BAND_BASE[band] * shape).clamp(0.02, 0.9)
    })
}

fn apply_scene(binding: &mut UiBinding, scene: &str) {
    // A declared synthetic fixture: five centre-out frames, no audio involved.
    // The `empty-*` scenes stay at the host's all-zero starting history so they
    // can be compared against the React build's untouched initial state.
    if !scene.starts_with("empty") {
        for step in 0..5 {
            binding.push_spectrum(&fixture_frame(step), step as u64 * 60);
        }
    }

    match scene {
        "empty-settings" => binding.open_settings(),
        "off" => binding.toggle_power(),
        "light" => {
            binding.set_light(true);
            binding.set_custom_accent(true);
            binding.set_accent(0xff12_a8a0);
        }
        "accent" => {
            binding.set_custom_accent(true);
            binding.set_accent(0xff3b_82f6);
        }
        "eq" => {
            let curve = [
                -6.0, -4.5, -3.0, -1.5, 0.0, 1.5, 3.0, 4.5, 6.0, 7.0, 8.0, 8.5, 9.0, 8.5, 8.0, 7.0,
                6.0, 4.5, 3.0, 1.5, 0.0, -1.5, -3.0, -4.5, -6.0, -6.5, -5.0, -3.0, -1.0, 0.5, 1.0,
            ];
            for (band, gain) in curve.iter().enumerate() {
                binding.set_eq_gain(band, *gain);
            }
            binding.set_effect(0, 6.0);
            binding.set_effect(1, 4.0);
        }
        "settings" => binding.open_settings(),
        "save" => {
            binding.open_settings();
            binding.open_save();
            binding.set_save_name("Evening listen");
        }
        _ => {}
    }
}

fn main() {
    let scene = std::env::var("SAUCEFX_PREVIEW_SCENE").unwrap_or_else(|_| "default".into());
    let exit_ms = std::env::var("SAUCEFX_PREVIEW_EXIT_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(4_000);

    let window = AppWindow::new().expect("create the Slint window");
    let binding = UiBinding::install(&window);
    apply_scene(&mut binding.borrow_mut(), &scene);
    binding.borrow().sync(&window);

    // The preview keeps the binding's placeholder toast for everything except
    // Quit: here the real app contract applies and the process exits, like
    // Settings -> Quit SauceFX in the reference host.
    window.on_quit_clicked(|| {
        let _ = slint::quit_event_loop();
    });

    window.show().expect("show the window");

    if std::env::var("SAUCEFX_PREVIEW_GEOMETRY").is_ok() {
        let position = window.window().position();
        println!(
            "GEOMETRY x={} y={} scale={}",
            position.x,
            position.y,
            window.window().scale_factor()
        );
    }

    // Kept alive for the whole event loop; a dropped Timer stops ticking.
    let tracer = slint::Timer::default();
    if std::env::var("SAUCEFX_PREVIEW_TRACE").is_ok() {
        let traced = Rc::clone(&binding);
        let traced_window = window.as_weak();
        let mut last = String::new();
        tracer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(200),
            move || {
                let state = traced.borrow();
                let Some(view) = traced_window.upgrade() else {
                    return;
                };
                let line = format!(
                    "TRACE preset={} edited={} power={} effects={:?} eq0={:?} eq17={:?} band={:?} save_name={:?} menu={} save={} settings={} theme={:?} autostart={} accent={} focus={} readout={}",
                    state.preset_name(),
                    state.edited(),
                    state.power(),
                    state.effects(),
                    state.eq_gains()[0],
                    state.eq_gains()[17],
                    state.active_eq_band(),
                    state.save_name(),
                    state.preset_menu_open(),
                    state.save_open(),
                    state.settings_open(),
                    state.theme(),
                    state.autostart(),
                    state.accent_name(),
                    view.get_focus_report(),
                    view.get_eq_readout(),
                );
                if line != last {
                    println!("{line}");
                    last = line.clone();
                }
                let _ = std::io::Write::flush(&mut std::io::stdout());
            },
        );
    }

    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(exit_ms),
        || {
            let _ = slint::quit_event_loop();
        },
    );

    // `run_event_loop_until_quit` instead of `run_event_loop`: closing the
    // window hides it to a running background (the reference close-to-tray
    // behaviour) instead of ending the process. Only Quit ends the loop.
    slint::run_event_loop_until_quit().expect("run the Slint event loop");
}
