//! Offscreen UI capture: renders the real Slint interface with the software
//! renderer into a pixel buffer and writes a PNG, so visual comparisons work
//! on machines where window capture is unavailable.
//!
//! Usage: capture <scene> <output.png>
//! Scenes mirror the preview's: default, menu, settings, save, off, light,
//! eq, empty.

use std::rc::Rc;

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize};

use saucefx_core::updates::UpdateStatus;
use saucefx_ui::{AppWindow, binding::UiBinding};

const WIDTH: usize = 1040;
const HEIGHT: usize = 568;

thread_local! {
    static WINDOW: Rc<MinimalSoftwareWindow> =
        MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
}

fn window() -> Rc<MinimalSoftwareWindow> {
    WINDOW.with(|window| window.clone())
}

struct CapturePlatform;

impl Platform for CapturePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(window())
    }
}

fn apply_scene(binding: &mut UiBinding, scene: &str) {
    // Same synthetic fixtures as the preview binary.
    if scene != "menu" && !scene.starts_with("empty") {
        let base = [0.3, 0.56, 0.78, 0.68, 0.48, 0.58, 0.44, 0.31, 0.22, 0.15];
        for step in 0..5usize {
            let frame: [f32; 10] = std::array::from_fn(|band| {
                let shape = 0.55 + 0.32 * ((band + step) as f32 * 0.72).sin();
                (base[band] * shape).clamp(0.02, 0.9)
            });
            binding.push_spectrum(&frame, step as u64 * 60);
        }
    }
    match scene {
        "menu" => binding.toggle_preset_menu(),
        "outputs" => binding.toggle_output_menu(),
        "settings" => binding.open_settings(),
        "updates-busy" | "updates-error" | "updates-restart" | "updates-light" => {
            binding.open_settings();
            binding.set_update_status(match scene {
                "updates-busy" => UpdateStatus::Installing(62),
                "updates-error" => UpdateStatus::Failed(
                    "Could not reach the update source. Check your connection and try again."
                        .into(),
                ),
                "updates-restart" => UpdateStatus::RestartRequired,
                _ => UpdateStatus::Ready,
            });
            if scene == "updates-light" {
                binding.set_light(true);
            }
        }
        "save" => {
            binding.open_settings();
            binding.open_save();
            binding.set_save_name("Evening listen");
        }
        "off" => binding.toggle_power(),
        "light" => {
            binding.set_light(true);
            binding.set_custom_accent(true);
            binding.set_accent(0xff12_a8a0);
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
        _ => {}
    }
}

/// Pointer-event helpers for the interaction probes below. Offscreen pointer
/// edits go through the same `Window::dispatch_event` path the save-field
/// focus click uses; programmatic focus is refused on this window.
fn pointer(app: &AppWindow, event: slint::platform::WindowEvent) {
    app.window().dispatch_event(event);
}

fn move_pointer(app: &AppWindow, x: f32, y: f32) {
    pointer(
        app,
        slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(x, y),
        },
    );
}

fn press(app: &AppWindow, x: f32, y: f32) {
    pointer(
        app,
        slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(x, y),
            button: slint::platform::PointerEventButton::Left,
        },
    );
}

fn release(app: &AppWindow, x: f32, y: f32) {
    pointer(
        app,
        slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(x, y),
            button: slint::platform::PointerEventButton::Left,
        },
    );
}

/// Render one more full frame into the reused buffer after pointer work.
fn render_frame(window: &Rc<MinimalSoftwareWindow>, buffer: &mut [PremultipliedRgbaColor]) {
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(buffer, WIDTH);
    });
}

fn main() {
    let mut args = std::env::args().skip(1);
    let scene = args.next().unwrap_or_else(|| "default".into());
    let output = args.next().unwrap_or_else(|| format!("{scene}.png"));

    slint::platform::set_platform(Box::new(CapturePlatform)).ok();
    let window = window();
    window.set_size(PhysicalSize::new(WIDTH as u32, HEIGHT as u32));

    let app = AppWindow::new().expect("create the SauceFX window");
    // Show the window before the scene so dialog-open focus (the Save dialog
    // autofocuses its name field) lands like it does in the shown real app;
    // the reference captures render the focused state. MinimalSoftwareWindow
    // never reports activation by itself, so mirror the winit "window gained
    // focus" event or Slint refuses programmatic focus on the inactive window.
    app.show().expect("show the capture window");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
    let binding = UiBinding::install(&app);
    apply_scene(&mut binding.borrow_mut(), &scene);
    binding.borrow().sync(&app);
    eprintln!("capture: focus={}", app.get_focus_report());

    window.request_redraw();
    let mut buffer = vec![PremultipliedRgbaColor::default(); WIDTH * HEIGHT];
    let mut drawn = false;
    for _ in 0..5 {
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut buffer, WIDTH);
            drawn = true;
        });
        if drawn {
            break;
        }
    }
    // Drag probe for the effect sliders: press on the Clarity track, drag
    // right, and render mid-drag plus at rest, so a drag that stops
    // following the pointer is visible in the PNG and in the value dump.
    if scene == "eff-drag" {
        press(&app, 160.0, 353.0);
        move_pointer(&app, 200.0, 353.0);
        slint::platform::update_timers_and_animations();
        render_frame(&window, &mut buffer);
        move_pointer(&app, 230.0, 353.0);
        slint::platform::update_timers_and_animations();
        render_frame(&window, &mut buffer);
        eprintln!("capture: eff-drag values={:?}", app.get_effect_values());
        release(&app, 230.0, 353.0);
    }

    // The Save scene must show the focused name field (the reference fixture
    // fills the input, which focuses it). Programmatic focus is refused on
    // this window (focus stays "save:none"), so click into the field the way
    // a user does; the dialog auto-focus path covers the interactive app.
    if scene == "save" {
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerPressed {
                position: slint::LogicalPosition::new(523.0, 330.0),
                button: slint::platform::PointerEventButton::Left,
            });
        app.window()
            .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                position: slint::LogicalPosition::new(523.0, 330.0),
                button: slint::platform::PointerEventButton::Left,
            });
        slint::platform::update_timers_and_animations();
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut buffer, WIDTH);
        });
        eprintln!("capture: focus after click={}", app.get_focus_report());
    }
    if !drawn {
        eprintln!("capture: the window never rendered");
        std::process::exit(1);
    }

    // The software renderer writes premultiplied pixels; PNG wants straight
    // alpha, so divide the channels back out.
    let mut rgba = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for pixel in &buffer {
        let alpha = pixel.alpha as u16;
        // Un-premultiply; the alpha==0 case is fully transparent, where the
        // channels carry no colour information.
        #[allow(unknown_lints, clippy::manual_checked_ops)]
        let unpremultiply = |channel: u8| -> u8 {
            if alpha == 0 {
                0
            } else {
                ((channel as u16 * 255 + alpha / 2) / alpha).min(255) as u8
            }
        };
        rgba.push(unpremultiply(pixel.red));
        rgba.push(unpremultiply(pixel.green));
        rgba.push(unpremultiply(pixel.blue));
        rgba.push(pixel.alpha);
    }
    image::save_buffer(
        &output,
        &rgba,
        WIDTH as u32,
        HEIGHT as u32,
        image::ColorType::Rgba8,
    )
    .expect("write the capture PNG");
    println!("wrote {output} ({scene})");
}
