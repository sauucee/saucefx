use saucefx_core::state::{BAND_COUNT, EFFECT_COUNT};
use saucefx_ui::binding::UiBinding;

// Same leading levels returned by the pre-Rust engine on selecting Music.
// These are preset gains, not pointer edits; only a manual edit snaps to 1 dB.
#[test]
fn pre_rust_music_keeps_the_intermediate_points_and_reset_target() {
    let mut ui = UiBinding::default();
    let mut gains = [0.0; BAND_COUNT];
    gains[..5].copy_from_slice(&[0.0, 0.6, 1.2, 1.8, 2.0]);
    assert!(ui.apply_captured_values(&[0.0; EFFECT_COUNT], &gains));
    assert_eq!(&ui.eq_gains()[..5], &gains[..5]);
    ui.set_eq_gain(1, 3.7);
    assert_eq!(ui.eq_gains()[1], 4.0);
    assert_eq!(ui.eq_gains()[2], 1.2);
    ui.reset_eq_band(1);
    assert_eq!(ui.eq_gains(), gains);
    assert!(!ui.edited());
}
