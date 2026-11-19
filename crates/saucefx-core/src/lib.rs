//! Standard-library-only SauceFX app state, settings and preset-session codecs.
//!
//! The contracts reproduced here are the ones the existing JUCE front-end uses:
//! `settings.txt` (six keys) and `preset-session.txt` (schema version 1). The
//! file formats stay compatible so both front-ends can read and write them.
//!
//! No file I/O, audio, device routing or native handles live in this crate.

pub mod session;
pub mod settings;
pub mod state;
