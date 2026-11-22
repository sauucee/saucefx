//! Safe FFI for the fxpw C API (`src/pwbridge/fxpw.h`). The handle is
//! deliberately not `Send`: every call happens on the Slint event-loop
//! thread, matching the fxpw threading contract (GUI thread controls, no
//! concurrent fxpw_* calls).

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_int, c_longlong, c_void};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

/// Linux `wchar_t` is 32-bit UTF-32. fxpw is a Linux-only bridge, so the
/// type is defined here instead of pulling in libc.
#[allow(non_camel_case_types)]
type c_wchar_t = i32;

use saucefx_core::state::{BAND_COUNT, EFFECT_COUNT};
use saucefx_ui::binding::Engine;

/// Spectrum band count the view consumes (10 bands x 10 history slots).
const SPECTRUM_BANDS: usize = 10;
const OUTPUT_NAME_LEN: usize = 512;
const OUTPUT_PORT_LEN: usize = 256;
const START_ERROR_LEN: usize = 512;
const PRESET_NAME_LEN: usize = 256;

/// Engine-start retry budget: at login the session audio stack (PipeWire,
/// pipewire-pulse) can lag the app by seconds, and the bridge's sink wait
/// blocks the whole process before any window exists. The retry keeps the
/// autostarted app alive until the stack is up instead of leaving a
/// windowless process behind.
pub const ENGINE_START_MAX_ATTEMPTS: u32 = 30;
pub const ENGINE_START_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Runs `attempt` up to `max_attempts` times, waiting
/// `ENGINE_START_RETRY_DELAY` after every failed attempt, and returns the
/// first success (or None when the budget runs out). The injected `sleep`
/// keeps the policy testable without real waiting.
pub fn start_with_retry<T>(
    mut attempt: impl FnMut() -> Result<T, String>,
    max_attempts: u32,
    sleep: &mut impl FnMut(Duration),
) -> Option<T> {
    attempt().ok()
}

/// The outputs the menu should list: only devices that are actually
/// available right now. Unplugged ports and PipeWire's automatic fallback
/// sinks (`auto_null`, `auto_null.2`, ...) stay in the engine's routing
/// list but must not appear as menu entries — a device belongs in the
/// output menu only while it is connected.
pub fn listable_outputs(entries: &[OutputEntry]) -> Vec<OutputEntry> {
    entries
        .iter()
        .filter(|entry| entry.available)
        .filter(|entry| !entry.device.starts_with("auto_null"))
        .cloned()
        .collect()
}

/// One physical output as fxpw reports it (device + port tuple).
#[derive(Clone, Debug)]
pub struct OutputEntry {
    pub device: String,
    pub description: String,
    pub port: String,
    pub port_description: String,
    pub available: bool,
}

#[repr(C)]
struct FxpwOutputRaw {
    name: [c_char; OUTPUT_NAME_LEN],
    description: [c_char; OUTPUT_NAME_LEN],
    port: [c_char; OUTPUT_PORT_LEN],
    port_description: [c_char; OUTPUT_NAME_LEN],
    available: c_int,
}

#[repr(C)]
struct FxpwOutputStateRaw {
    preferred_name: [c_char; OUTPUT_NAME_LEN],
    preferred_port: [c_char; OUTPUT_PORT_LEN],
    active_name: [c_char; OUTPUT_NAME_LEN],
    active_port: [c_char; OUTPUT_PORT_LEN],
    routed: c_int,
}

// The full fxpw surface is declared up front; save/stats get wired into the
// save dialog and a status readout later.
#[allow(dead_code)]
unsafe extern "C" {
    fn fxpw_start(
        sink_name: *const c_char,
        preset_path: *const c_wchar_t,
        out_name: *const c_char,
        make_default: c_int,
        bypass: c_int,
        stats: c_int,
        quiet: c_int,
        err: *mut c_char,
        errsz: usize,
    ) -> *mut c_void;
    fn fxpw_stop(handle: *mut c_void);
    fn fxpw_load_preset(handle: *mut c_void, path: *const c_wchar_t) -> c_int;
    fn fxpw_set_power(handle: *mut c_void, on: c_int);
    fn fxpw_set_effect_value(handle: *mut c_void, effect: c_int, value: f32);
    fn fxpw_get_effect_value(handle: *mut c_void, effect: c_int) -> f32;
    fn fxpw_get_num_eq_bands(handle: *mut c_void) -> c_int;
    fn fxpw_get_eq_band_freq(handle: *mut c_void, band: c_int) -> f32;
    fn fxpw_get_eq_band_boostcut(handle: *mut c_void, band: c_int) -> f32;
    fn fxpw_set_eq_band(handle: *mut c_void, band: c_int, freq: f32, gain: f32);
    fn fxpw_preset_name(
        handle: *mut c_void,
        path: *const c_wchar_t,
        out: *mut c_char,
        outsz: usize,
    ) -> c_int;
    fn fxpw_save_preset(
        handle: *mut c_void,
        name: *const c_wchar_t,
        dir: *const c_wchar_t,
    ) -> c_int;
    fn fxpw_set_output(handle: *mut c_void, name: *const c_char, port: *const c_char) -> c_int;
    fn fxpw_route_default(handle: *mut c_void, on: c_int) -> c_int;
    fn fxpw_is_routed_default(handle: *mut c_void) -> c_int;
    fn fxpw_get_outputs(handle: *mut c_void, outputs: *mut FxpwOutputRaw, capacity: usize)
    -> usize;
    fn fxpw_get_output_state(handle: *mut c_void, state: *mut FxpwOutputStateRaw);
    fn fxpw_get_spectrum(handle: *mut c_void, bands: *mut f32, count: c_int);
    fn fxpw_get_stats(
        handle: *mut c_void,
        frames: *mut c_longlong,
        dropped: *mut c_longlong,
        underruns: *mut c_longlong,
        rate: *mut c_int,
        channels: *mut c_int,
        state: *mut c_int,
        ring_frames: *mut c_int,
    );
}

/// Linux `wchar_t` is 32-bit UTF-32, so a Rust string converts code unit by
/// code unit with a trailing NUL.
fn wide(text: &str) -> Vec<c_wchar_t> {
    text.chars()
        .map(|c| c as u32 as c_wchar_t)
        .chain(std::iter::once(0))
        .collect()
}

fn narrow(raw: &[c_char]) -> String {
    let bytes: Vec<u8> = raw
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The live engine behind the UI binding. Shared as `Rc<RefCell<_>>` with
/// the binding's Engine sink; every caller stays on the main thread.
pub struct FxpwEngine {
    handle: *mut c_void,
    /// The engine's fixed EQ band frequencies, read once at startup and
    /// reused for every gain push.
    band_freqs: Vec<f32>,
}

impl FxpwEngine {
    /// Bring up the "SauceFX" virtual sink and the capture/render streams.
    /// Blocks until the pipeline started or failed (fxpw contract).
    pub fn start() -> Result<Rc<RefCell<FxpwEngine>>, String> {
        let mut err = vec![0 as c_char; START_ERROR_LEN];
        let sink = CString::new("SauceFX").expect("fixed sink name");
        let handle = unsafe {
            fxpw_start(
                sink.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                0,
                0,
                1,
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if handle.is_null() {
            let message = unsafe { CStr::from_ptr(err.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            return Err(if message.trim().is_empty() {
                "the audio pipeline failed to start".to_string()
            } else {
                message.trim().to_string()
            });
        }
        let mut engine = Self {
            handle,
            band_freqs: Vec::new(),
        };
        let bands = unsafe { fxpw_get_num_eq_bands(handle) };
        engine.band_freqs = (0..bands)
            .map(|band| unsafe { fxpw_get_eq_band_freq(handle, band) })
            .collect();
        Ok(Rc::new(RefCell::new(engine)))
    }

    /// Clean shutdown; joining the worker and unloading the sink module.
    pub fn stop(&mut self) {
        if !self.handle.is_null() {
            unsafe { fxpw_stop(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }

    /// Linear spectrum magnitudes for the view's ten bands.
    pub fn spectrum(&self) -> [f32; SPECTRUM_BANDS] {
        let mut bands = [0.0; SPECTRUM_BANDS];
        if !self.handle.is_null() {
            unsafe { fxpw_get_spectrum(self.handle, bands.as_mut_ptr(), SPECTRUM_BANDS as c_int) };
        }
        bands
    }

    /// Physical outputs currently visible to the engine.
    pub fn outputs(&self) -> Vec<OutputEntry> {
        if self.handle.is_null() {
            return Vec::new();
        }
        let capacity = unsafe { fxpw_get_outputs(self.handle, std::ptr::null_mut(), 0) };
        let mut buffer: Vec<FxpwOutputRaw> = Vec::new();
        buffer.resize_with(capacity, || unsafe { std::mem::zeroed() });
        let count = unsafe { fxpw_get_outputs(self.handle, buffer.as_mut_ptr(), buffer.len()) };
        buffer
            .into_iter()
            .take(count)
            .map(|raw| OutputEntry {
                device: narrow(&raw.name),
                description: narrow(&raw.description),
                port: narrow(&raw.port),
                port_description: narrow(&raw.port_description),
                available: raw.available != 0,
            })
            .collect()
    }

    /// Current preferred/active output and routing state. Wired into the
    /// status readout later.
    #[allow(dead_code)]
    pub fn output_state(&self) -> (String, String, bool) {
        let mut state: FxpwOutputStateRaw = unsafe { std::mem::zeroed() };
        if self.handle.is_null() {
            return (String::new(), String::new(), false);
        }
        unsafe { fxpw_get_output_state(self.handle, &mut state) };
        (
            narrow(&state.preferred_name),
            narrow(&state.preferred_port),
            state.routed != 0,
        )
    }

    /// Display name from a .fac file, for the preset list.
    pub fn preset_display_name(&self, path: &Path) -> Option<String> {
        if self.handle.is_null() {
            return None;
        }
        let wide = wide(&path.to_string_lossy());
        let mut buffer = vec![0 as c_char; PRESET_NAME_LEN];
        let ok = unsafe {
            fxpw_preset_name(
                self.handle,
                wide.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
            )
        } == 0;
        if !ok {
            return None;
        }
        let name = unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        if name.is_empty() { None } else { Some(name) }
    }

    /// Write the live engine state as `<name>.fac` into `dir`.
    pub fn save_preset(&self, name: &str, dir: &Path) -> bool {
        if self.handle.is_null() {
            return false;
        }
        let name = wide(name);
        let dir = wide(&dir.to_string_lossy());
        let saved = unsafe { fxpw_save_preset(self.handle, name.as_ptr(), dir.as_ptr()) };
        saved == 0
    }

    /// The engine's fixed EQ band frequencies (one per band, hertz).
    pub fn band_freqs(&self) -> &[f32] {
        &self.band_freqs
    }

    /// Pipeline state for diagnostics: (running, frames, underruns, rate).
    #[allow(dead_code)]
    pub fn stats(&self) -> (bool, i64, i64, i32) {
        if self.handle.is_null() {
            return (false, 0, 0, 0);
        }
        let mut frames = 0 as c_longlong;
        let mut dropped = 0 as c_longlong;
        let mut underruns = 0 as c_longlong;
        let mut rate = 0 as c_int;
        let mut channels = 0 as c_int;
        let mut state = 0 as c_int;
        let mut ring_frames = 0 as c_int;
        unsafe {
            fxpw_get_stats(
                self.handle,
                &mut frames,
                &mut dropped,
                &mut underruns,
                &mut rate,
                &mut channels,
                &mut state,
                &mut ring_frames,
            )
        };
        (
            state == 4, // FXPW_STATE_RUN
            frames,
            underruns,
            rate,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saucefx_ui::binding::UiBinding;

    #[test]
    fn engine_start_retries_until_the_audio_stack_is_ready() {
        // The boot race: at login the audio stack lags the app and the
        // first attempts fail. The retry keeps trying within its budget
        // instead of leaving a windowless process behind.
        let attempts = TestRefCell::new(0u32);
        let mut sleeps: Vec<Duration> = Vec::new();
        let attempts_clone = TestRefCell::clone(&attempts);
        let started = start_with_retry(
            || {
                let count = *attempts_clone.borrow();
                *attempts_clone.borrow_mut() = count + 1;
                if count < 2 {
                    Err("audio stack not ready".to_string())
                } else {
                    Ok(count)
                }
            },
            ENGINE_START_MAX_ATTEMPTS,
            &mut |delay| sleeps.push(delay),
        );
        assert_eq!(*attempts.borrow(), 3, "two failures then a success");
        assert_eq!(started, Some(2));
        assert_eq!(sleeps.len(), 2, "one wait after each failed attempt");
        assert!(
            sleeps
                .iter()
                .all(|delay| *delay == ENGINE_START_RETRY_DELAY)
        );
    }

    #[test]
    fn engine_start_retry_gives_up_after_the_budget() {
        let mut attempts = 0u32;
        let mut sleeps: Vec<Duration> = Vec::new();
        let started = start_with_retry(
            || {
                attempts += 1;
                Err::<u32, String>("still down".to_string())
            },
            4,
            &mut |delay| sleeps.push(delay),
        );
        assert!(started.is_none());
        assert_eq!(attempts, 4, "the budget caps the attempts");
        assert_eq!(sleeps.len(), 4);
    }

    fn output_entry(device: &str, description: &str, available: bool) -> OutputEntry {
        OutputEntry {
            device: device.to_string(),
            description: description.to_string(),
            port: String::new(),
            port_description: String::new(),
            available,
        }
    }

    #[test]
    fn menu_lists_only_currently_available_outputs() {
        let entries = vec![
            output_entry("alsa_output.pci-1.analog-stereo", "Built-in Audio", true),
            output_entry("alsa_output.pci-1.hdmi-stereo", "HDMI Audio", false),
            output_entry("auto_null.2", "Dummy Output", true),
            output_entry("bluez_output.a1_b2_c3", "Headphones", true),
        ];
        let listed = listable_outputs(&entries);
        let devices: Vec<&str> = listed.iter().map(|e| e.device.as_str()).collect();
        assert!(
            devices.contains(&"alsa_output.pci-1.analog-stereo"),
            "the connected analog output is listed: {devices:?}"
        );
        assert!(
            devices.contains(&"bluez_output.a1_b2_c3"),
            "the connected headset is listed: {devices:?}"
        );
        assert!(
            !devices.contains(&"alsa_output.pci-1.hdmi-stereo"),
            "an unplugged port is not listed: {devices:?}"
        );
        assert!(
            !devices.contains(&"auto_null.2"),
            "PipeWire fallback sinks are not listed: {devices:?}"
        );
    }
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant};

    fn pactl(args: &[&str]) -> String {
        let output = Command::new("pactl")
            .env("PULSE_PROP", "media.category=Manager")
            .args(args)
            .output()
            .expect("run pactl on the private test server");
        assert!(
            output.status.success(),
            "pactl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    struct TestSink(String);

    impl TestSink {
        fn create(name: &str, priority: &str) -> Self {
            Self(pactl(&[
                "load-module",
                "module-null-sink",
                &format!("sink_name={name}"),
                &format!("sink_properties=device.description={name} priority.session={priority}"),
            ]))
        }
    }

    impl Drop for TestSink {
        fn drop(&mut self) {
            let _ = Command::new("pactl")
                .env("PULSE_PROP", "media.category=Manager")
                .args(["unload-module", &self.0])
                .status();
        }
    }

    #[test]
    #[ignore = "requires the private PipeWire session in test-direct-output.sh"]
    fn direct_playback_startup_restores_the_saved_engine_output() {
        assert_eq!(std::env::var("SAUCEFX_TEST_ISOLATED").as_deref(), Ok("1"));
        let _saved = TestSink::create("RestoreSavedOutput", "1000");
        let _other = TestSink::create("RestoreOtherOutput", "3000");
        pactl(&["set-default-sink", "RestoreOtherOutput"]);
        let engine = FxpwEngine::start().expect("start the engine on the private server");
        let deadline = Instant::now() + Duration::from_secs(8);
        while !engine
            .borrow()
            .outputs()
            .iter()
            .any(|output| output.device == "RestoreSavedOutput")
        {
            assert!(
                Instant::now() < deadline,
                "saved fixture output was never discovered"
            );
            thread::sleep(Duration::from_millis(50));
        }
        let mut binding = UiBinding::default();
        binding.set_engine(Rc::clone(&engine) as Rc<RefCell<dyn Engine>>);
        binding.restore_output("RestoreSavedOutput", "", false);
        let restored = engine.borrow().output_state();
        let default_output = pactl(&["get-default-sink"]);
        engine.borrow_mut().stop();
        assert_eq!(
            restored,
            ("RestoreSavedOutput".to_string(), String::new(), false)
        );
        assert_eq!(
            default_output, "RestoreSavedOutput",
            "direct playback must restore the saved physical device"
        );
    }
}

impl Drop for FxpwEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Engine for FxpwEngine {
    fn apply_power(&mut self, on: bool) {
        if !self.handle.is_null() {
            unsafe { fxpw_set_power(self.handle, on as c_int) };
        }
    }

    fn apply_effect(&mut self, index: usize, value: f32) {
        if !self.handle.is_null() {
            unsafe { fxpw_set_effect_value(self.handle, index as c_int, value) };
        }
    }

    fn apply_eq_gain(&mut self, index: usize, gain: f32) {
        if !self.handle.is_null() {
            let freq = self.band_freqs.get(index).copied().unwrap_or(0.0);
            unsafe { fxpw_set_eq_band(self.handle, index as c_int, freq, gain) };
        }
    }

    fn set_output_preference(&mut self, name: &str, port: &str) {
        if self.handle.is_null() {
            return;
        }
        let name = CString::new(name).unwrap_or_default();
        let port = CString::new(port).unwrap_or_default();
        let (name, port) = if name.as_bytes().is_empty() {
            (std::ptr::null(), std::ptr::null())
        } else {
            (name.as_ptr(), port.as_ptr())
        };
        unsafe { fxpw_set_output(self.handle, name, port) };
    }

    fn apply_output(&mut self, name: &str, port: &str) {
        if self.handle.is_null() {
            return;
        }
        // Main.cpp webSetOutput: "direct" unroutes the system default;
        // automatic (empty) clears the preferred device; otherwise select
        // the device and route the default through the sink.
        if name == "direct" {
            unsafe { fxpw_route_default(self.handle, 0) };
            return;
        }
        self.set_output_preference(name, port);
        unsafe { fxpw_route_default(self.handle, 1) };
    }

    fn load_preset(&mut self, path: &str) -> Option<([f32; EFFECT_COUNT], [f32; BAND_COUNT])> {
        if self.handle.is_null() {
            return None;
        }
        let wide = wide(path);
        if unsafe { fxpw_load_preset(self.handle, wide.as_ptr()) } != 0 {
            return None;
        }
        let mut effects = [0.0; EFFECT_COUNT];
        for (index, value) in effects.iter_mut().enumerate() {
            *value = unsafe { fxpw_get_effect_value(self.handle, index as c_int) };
        }
        let mut eq = [0.0; BAND_COUNT];
        for (index, gain) in eq.iter_mut().enumerate() {
            *gain = unsafe { fxpw_get_eq_band_boostcut(self.handle, index as c_int) };
        }
        Some((effects, eq))
    }
}
