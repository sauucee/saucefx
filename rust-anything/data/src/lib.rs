//! JS bindings for `src/data.ts`: generated from the signatures in `port.rs` by rust-anything.
//! Regenerate with `rust-anything refresh` after changing them.

use napi_derive::napi;

pub mod js;
pub mod port;
pub mod wire;

#[allow(unused_imports)]
use serde_json::Value;

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

#[allow(dead_code)]
fn run<T>(f: impl FnOnce() -> Result<T, String>) -> napi::Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(result) => result.map_err(napi::Error::from_reason),
        Err(payload) => Err(napi::Error::from_reason(format!(
            "Rust panic: {}",
            panic_message(&*payload)
        ))),
    }
}

/// `formatFrequency`.
#[napi(js_name = "formatFrequency")]
pub fn format_frequency(frequency: f64, compact: Option<bool>) -> napi::Result<String> {
    run(move || {
        let out = port::format_frequency(frequency, compact)?;
        Ok(out)
    })
}

/// `formatGain`.
#[napi(js_name = "formatGain")]
pub fn format_gain(value: f64) -> napi::Result<String> {
    run(move || {
        let out = port::format_gain(value)?;
        Ok(out)
    })
}

/// `getAccent`.
#[napi(js_name = "getAccent")]
pub fn get_accent(id: String) -> napi::Result<String> {
    run(move || {
        let out = port::get_accent(id)?;
        crate::js::to_json(&out)
    })
}

/// `accentIdFor`.
#[napi(js_name = "accentIdFor")]
pub fn accent_id_for(value: String) -> napi::Result<String> {
    run(move || {
        let out = port::accent_id_for(value)?;
        Ok(out)
    })
}

// ---- the binary wire ------------------------------------------------------------------------

thread_local! {
    static WIRE: std::cell::RefCell<Option<(napi::bindgen_prelude::Uint8Array, napi::bindgen_prelude::Uint8Array)>> =
        const { std::cell::RefCell::new(None) };
    static PENDING: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Registers the argument and result buffers `index.cjs` writes to and reads from.
#[napi(js_name = "rustAnythingWireInit")]
pub fn rust_anything_wire_init(
    input: napi::bindgen_prelude::Uint8Array,
    output: napi::bindgen_prelude::Uint8Array,
) {
    WIRE.with(|w| *w.borrow_mut() = Some((input, output)));
}

/// Copies a result that did not fit into the (now larger) result buffer.
#[napi(js_name = "rustAnythingWireFlush")]
pub fn rust_anything_wire_flush() -> f64 {
    let bytes = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    WIRE.with(|w| {
        let mut w = w.borrow_mut();
        let Some((_, output)) = w.as_mut() else {
            return -1.0;
        };
        // SAFETY: the call is synchronous; JS does not touch `output` until it returns.
        let out = unsafe { output.as_mut() };
        let n = bytes.len().min(out.len());
        out[..n].copy_from_slice(&bytes[..n]);
        n as f64
    })
}

/// Reads `len` argument bytes, runs `f`, and writes its result; a negative return is the size
/// of a result that did not fit, kept for `rustAnythingWireFlush`.
fn wire_io(
    len: u32,
    f: impl FnOnce(&mut wire::Reader, &mut wire::Writer) -> Result<(), String>,
) -> napi::Result<f64> {
    WIRE.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some((input, output)) = guard.as_mut() else {
            return Err(napi::Error::from_reason(
                "rust-anything: the wire buffers are not registered",
            ));
        };
        let len = (len as usize).min(input.len());
        let mut w = wire::Writer::default();
        match run(|| f(&mut wire::Reader::new(&input[..len]), &mut w)) {
            // A result of another shape than the TS type: JS runs the call through JSON.
            Err(e) if e.reason.starts_with(wire::MISMATCH) => return Ok(f64::NAN),
            other => other?,
        }
        // SAFETY: the call is synchronous; JS does not touch `output` until it returns.
        let out = unsafe { output.as_mut() };
        let n = w.0.len();
        if n > out.len() {
            PENDING.with(|p| *p.borrow_mut() = w.0);
            return Ok(-(n as f64));
        }
        out[..n].copy_from_slice(&w.0);
        Ok(n as f64)
    })
}

static S_GET_ACCENT_OUT: wire::Schema = wire::Schema::OptJs(&wire::Schema::Obj(&[
    ("id", wire::Schema::Dyn),
    ("label", wire::Schema::Str16),
    ("value", wire::Schema::Str16),
]));

/// `getAccent` over the binary wire.
#[napi(js_name = "getAccentWire")]
pub fn get_accent_wire(len: u32) -> napi::Result<f64> {
    wire_io(len, |r, w| {
        let id: String = r.string16()?;
        let out = &port::get_accent(id)?;
        match out {
            Some(x0) => {
                w.tag(1);
                wire::write_value(w, x0, S_GET_ACCENT_OUT.inner())?;
            }
            None => w.tag(0),
        }
        Ok(())
    })
}
