//! Pace the visualizer from drawable frames, not an always-running audio poll.
use std::time::{Duration, Instant};

use saucefx_ui::AppWindow;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent};
use slint::{ComponentHandle, Timer, TimerMode};

const INTERVAL: Duration = Duration::from_millis(50);

/// A hidden login launch may already have consumed the first redraw event.
/// Request a frame on the newly mapped native surface, not on a hidden timer.
pub fn request_first_frame(window: &AppWindow) {
    let weak = window.as_weak();
    slint::spawn_local(async move {
        if let Some(window) = weak.upgrade()
            && let Ok(native) = window.window().winit_window().await
            && window.window().is_visible()
        {
            native.request_redraw();
        }
    })
    .expect("request initial visualizer frame on the UI thread");
}

pub fn install(window: &AppWindow, mut sample: impl FnMut(&AppWindow, bool) + 'static) {
    let weak = window.as_weak();
    let wake = Timer::default();
    let mut occluded = false;
    let mut last_frame: Option<Instant> = None;
    window.window().on_winit_window_event(move |native, event| {
        match event {
            WindowEvent::Occluded(covered) => {
                occluded = *covered;
                if occluded {
                    wake.stop();
                    last_frame = None;
                } else if native.is_visible() {
                    native.request_redraw();
                }
            }
            WindowEvent::Destroyed => {
                // Slint may create a new native window when reopened.
                occluded = false;
                last_frame = None;
                wake.stop();
            }
            WindowEvent::RedrawRequested if !wake.running() && !occluded => {
                if !native.is_visible()
                    || native.with_winit_window(|w| w.is_minimized()).flatten() == Some(true)
                {
                    last_frame = None;
                } else if let Some(window) = weak.upgrade() {
                    let now = Instant::now();
                    let fresh = last_frame
                        .replace(now)
                        .is_none_or(|previous| now.duration_since(previous) > INTERVAL * 2);
                    sample(&window, fresh);
                    let next = weak.clone();
                    // Only a delivered redraw can sample or re-arm this timer.
                    // Wayland doesn't expose is_minimized, but withholds frames
                    // for surfaces the compositor isn't drawing.
                    wake.start(TimerMode::SingleShot, INTERVAL, move || {
                        if let Some(window) = next.upgrade()
                            && window.window().is_visible()
                        {
                            window.window().request_redraw();
                        }
                    });
                }
            }
            _ => {}
        }
        EventResult::Propagate
    });
}
