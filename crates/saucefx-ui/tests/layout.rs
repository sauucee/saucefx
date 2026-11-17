//! Layout contract against the real .slint sources. These read the UI files, so a
//! silent redesign or a drifted constant fails the test.

use std::fs;
use std::path::Path;

fn ui(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui").join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path:?}: {error}"))
}

#[test]
fn shell_uses_the_reference_dimensions_and_spacing() {
    let palette = ui("palette.slint");
    for needed in [
        "window-width: 1040px",
        "window-height: 568px",
        "shell-radius: 14px",
        "header-height: 52px",
        "select-height: 42px",
        "spectrum-height: 128px",
        "effects-width: 276px",
        "workspace-gap: 13px",
        "workspace-pad-h: 18px",
        "workspace-pad-top: 14px",
        "workspace-pad-bottom: 16px",
        "panel-radius: 12px",
        "eq-bands: 31",
        "spectrum-bands: 100",
        "effects: 5",
    ] {
        assert!(
            palette.contains(needed),
            "palette.slint must declare `{needed}`"
        );
    }

    let window = ui("app-window.slint");
    assert!(window.contains("no-frame: true"));
    assert!(window.contains("width: Layout.window-width"));
    assert!(window.contains("height: Layout.window-height"));
    assert!(
        window.contains("SauceFX Rust UI Preview"),
        "the preview must name itself so it cannot be mistaken for the app"
    );
    assert!(window.contains("no audio connection"));

    let header = ui("header.slint");
    assert!(header.contains("height: Layout.header-height"));
    assert!(
        header.contains("frontend/public/saucefx-mark.svg"),
        "the shipped brand mark is reused"
    );
    for label in ["SauceFX", "AUDIO ENHANCER"] {
        assert!(header.contains(label), "header keeps `{label}`");
    }
}

#[test]
fn spectrum_draws_one_hundred_band_major_bars_that_clear_when_off() {
    let spectrum = ui("spectrum.slint");

    assert!(
        spectrum.contains("in property <[float]> bars"),
        "the spectrum is fed a flat 100-bar model, not ten band values"
    );
    assert!(
        spectrum.contains("for bar[i] in root.bars"),
        "every one of the 100 bars is drawn (10 bands x 10 history slots)"
    );
    for constant in ["960px", "120px", "27px", "9.1px", "4px", "100px"] {
        assert!(
            spectrum.contains(constant),
            "the original FxVisualizer geometry needs `{constant}`"
        );
    }
    assert!(
        spectrum.contains("max(1px"),
        "a zero level still draws a one-pixel centred bar while processing is on"
    );
    assert!(
        spectrum.contains("@linear-gradient"),
        "the accent gradient replaces a flat fill"
    );
    assert!(
        spectrum.contains("Audio processing is off"),
        "power-off shows the exact reference message"
    );
    assert!(
        !spectrum.contains("Processing paused"),
        "the invented paused caption must be gone"
    );
    assert!(
        spectrum.contains("if !root.power"),
        "power-off has its own branch that clears the bars"
    );
}

#[test]
fn equalizer_uses_the_source_virtual_geometry_and_cubic_curve() {
    let equalizer = ui("equalizer.slint");

    for constant in ["1000", "200"] {
        assert!(
            equalizer.contains(constant),
            "the curve is authored in the source's virtual space (`{constant}`)"
        );
    }
    assert!(
        equalizer.contains("viewbox-width: 1000") && equalizer.contains("viewbox-height: 200"),
        "the path is scaled from the 1000x200 virtual space, not redrawn in a new projection"
    );
    assert!(
        equalizer.contains("fill:"),
        "the curve keeps its filled area"
    );

    let binding = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("binding.rs"),
    )
    .expect("read binding.rs");
    assert!(
        binding.contains("EQ_VIEW_WIDTH: f32 = 1000.0")
            && binding.contains("EQ_VIEW_HEIGHT: f32 = 200.0")
            && binding.contains("EQ_PLOT_TOP: f32 = 12.0")
            && binding.contains("EQ_PLOT_BOTTOM: f32 = 188.0"),
        "binding geometry mirrors Equalizer.tsx TOP/BOTTOM and virtual width"
    );
}

#[test]
fn every_effect_label_and_eq_caption_is_the_reference_string() {
    let binding = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("binding.rs"),
    )
    .expect("read binding.rs");
    for name in [
        "Clarity",
        "Ambience",
        "Surround",
        "Dynamic Boost",
        "Bass Boost",
    ] {
        assert!(binding.contains(name), "effect label `{name}` is preserved");
    }
    for caption in [
        "20", "50", "100", "200", "400", "800", "1.3k", "2.5k", "5k", "10k", "20k",
    ] {
        assert!(
            binding.contains(&format!("\"{caption}\"")),
            "EQ caption `{caption}` from data.ts is preserved"
        );
    }
    assert!(
        binding
            .contains("EQ_LABEL_INDICES: [usize; 11] = [0, 4, 7, 10, 13, 16, 18, 21, 24, 27, 30]"),
        "caption positions follow the real data.ts indices"
    );
}

#[test]
fn dialogs_carry_the_full_reference_controls() {
    let settings = ui("settings.slint");
    for needle in [
        "Settings",
        "Appearance, presets, and startup behavior.",
        "APPEARANCE",
        "PRESETS",
        "Theme",
        "Soft light or low-contrast dark surfaces.",
        "Accent",
        "Used for controls and the live spectrum.",
        "Dark",
        "Light",
        "Import",
        "Export",
        "Save preset",
        "Start at login",
        "Keep the engine ready in the background.",
        "Quit SauceFX",
        "AGPL-3.0",
    ] {
        assert!(settings.contains(needle), "settings keeps `{needle}`");
    }
    assert!(settings.contains("width: 604px"));

    let save = ui("save-preset.slint");
    for needle in [
        "Save preset",
        "Creates a new custom preset. Factory presets are never overwritten.",
        "Preset name",
        "Cancel",
        "root.error",
    ] {
        assert!(save.contains(needle), "save dialog keeps `{needle}`");
    }
    assert!(save.contains("width: 424px"));
}
