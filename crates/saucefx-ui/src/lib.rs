//! Native SauceFX UI preview (Slint + Winit + FemtoVG).
//!
//! This crate renders the existing design natively and keeps its view state in
//! memory only. It deliberately has **no** audio handle, engine binding or
//! routing code: the preview must never open an audio server.

pub mod binding;

slint::include_modules!();
