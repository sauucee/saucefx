//! Offscreen UI capture: renders the real Slint interface with the software
//! renderer into a pixel buffer and writes a PNG, so visual comparisons work
//! on machines where window capture is unavailable.
//!
//! Usage: capture <scene> <output.png>
//! Scenes mirror the preview's: default, menu, settings, save, off, light,
//! eq, empty.

use std::rc::Rc;

use slint::PhysicalSize;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, PlatformError, WindowAdapter};

use saucefx_ui::{AppWindow, binding::UiBinding};

const WIDTH: usize = 1040;
const HEIGHT: usize = 568;

thread_local! {
    static WINDOW: Rc<MinimalSoftwareWindow> =
        MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
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
        "settings" => binding.open_settings(),
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

fn main() {
    let mut args = std::env::args().skip(1);
    let scene = args.next().unwrap_or_else(|| "default".into());
    let output = args.next().unwrap_or_else(|| format!("{scene}.png"));

    slint::platform::set_platform(Box::new(CapturePlatform)).ok();
    let window = window();
    window.set_size(PhysicalSize::new(WIDTH as u32, HEIGHT as u32));

    let app = AppWindow::new().expect("create the SauceFX window");
    let binding = UiBinding::install(&app);
    apply_scene(&mut binding.borrow_mut(), &scene);
    binding.borrow().sync(&app);

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
