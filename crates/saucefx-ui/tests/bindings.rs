//! Behavioural tests for the preview binding against the protected reference
//! semantics (`frontend/src/App.tsx`, `Spectrum.tsx`, `Equalizer.tsx`).

use std::cell::RefCell;
use std::rc::Rc;

use saucefx_core::state::{BAND_COUNT, EFFECT_COUNT};
use saucefx_ui::binding::{
    ACCENT_ROSE, EditOutcome, FACTORY_PRESET_COUNT, PresetFixture, SPECTRUM_BANDS,
    SPECTRUM_HISTORY, SPECTRUM_SLOTS, SpectrumHistory, UiBinding, argb_to_color, color_to_argb,
    eq_curve_commands, factory_fixtures, format_gain,
};

const BLUE: u32 = 0xff3b82f6;
const ROSE: u32 = ACCENT_ROSE;

#[test]
fn default_binding_starts_powered_with_a_clean_session() {
    let binding = UiBinding::default();
    assert!(binding.power());
    assert_eq!(binding.effects(), [0.0; EFFECT_COUNT]);
    assert_eq!(binding.eq_gains(), [0.0; BAND_COUNT]);
    assert!(!binding.edited());
    assert_eq!(binding.preset_name(), "Default");
    // The reference starts unrouted on Automatic until the user chooses.
    assert_eq!(binding.output_name(), "Automatic");
    assert!(!binding.settings_open());
    assert!(!binding.save_open());
    assert!(
        binding.autostart(),
        "start at login is on by default (owner decision 2026-11-19)"
    );
}

#[test]
fn effect_and_eq_edits_stay_pending_until_applied() {
    let mut binding = UiBinding::default();
    assert_eq!(binding.set_effect(0, 7.0), EditOutcome::Applied);
    assert_eq!(binding.effects()[0], 7.0);
    assert!(binding.edited());
    assert_eq!(binding.state().applied_effect(0), Some(0.0));

    binding.apply_current();
    assert!(!binding.edited());
    assert_eq!(binding.state().applied_effect(0), Some(7.0));
}

#[test]
fn effect_edits_are_clamped_to_the_reference_zero_to_ten_scale() {
    let mut binding = UiBinding::default();
    assert_eq!(binding.set_effect(2, 42.0), EditOutcome::Applied);
    assert_eq!(binding.effects()[2], 10.0);
    binding.set_effect(2, -3.0);
    assert_eq!(binding.effects()[2], 0.0);
    assert_eq!(binding.set_effect(5, 1.0), EditOutcome::OutOfRange);
    assert_eq!(binding.set_effect(0, f32::NAN), EditOutcome::NotFinite);
}

#[test]
fn eq_sliders_move_in_whole_decibels_like_the_reference() {
    let mut binding = UiBinding::default();
    assert_eq!(binding.set_eq_gain(4, 3.4), EditOutcome::Applied);
    assert_eq!(binding.eq_gains()[4], 3.0);
    assert_eq!(binding.set_eq_gain(4, 3.6), EditOutcome::Applied);
    assert_eq!(binding.eq_gains()[4], 4.0);
    assert_eq!(binding.set_eq_gain(4, -12.4), EditOutcome::Applied);
    assert_eq!(binding.eq_gains()[4], -12.0);
}

#[test]
fn power_off_blocks_preset_effect_eq_and_reset_but_not_output_or_settings() {
    let mut binding = UiBinding::default();
    binding.set_effect(1, 5.0);
    binding.set_eq_gain(2, 6.0);
    binding.apply_current();

    binding.toggle_power();
    assert!(!binding.power());

    assert_eq!(
        binding.set_effect(1, 2.0),
        EditOutcome::PoweredOff,
        "effect edits are disabled while processing is off"
    );
    assert_eq!(binding.set_eq_gain(2, 1.0), EditOutcome::PoweredOff);

    binding.reset_effects();
    binding.reset_eq();
    assert_eq!(
        binding.effects()[1],
        5.0,
        "Reset All cannot rewrite a paused rack"
    );
    assert_eq!(binding.eq_gains()[2], 6.0);

    assert!(
        !binding.choose_preset(1),
        "preset switching is disabled while processing is off"
    );
    assert_eq!(binding.preset_name(), "Default");

    assert!(
        binding.choose_output(5),
        "output routing stays available while paused"
    );
    assert_eq!(binding.output_name(), "Direct playback");
    binding.open_settings();
    assert!(
        binding.settings_open(),
        "settings stay available while paused"
    );
    binding.close_settings();

    binding.toggle_power();
    assert_eq!(
        binding.set_effect(1, 2.0),
        EditOutcome::Applied,
        "editing resumes when processing returns"
    );
}

#[test]
fn accent_display_forces_rose_unless_a_valid_custom_hex_is_enabled() {
    let mut binding = UiBinding::default();

    binding.set_custom_accent(false);
    binding.set_accent(BLUE);
    assert_eq!(
        binding.display_accent(),
        ROSE,
        "App.tsx uses ACCENTS[0] when customAccent is false, even for a known hex"
    );
    assert_eq!(binding.accent_name(), "Red");

    binding.set_custom_accent(true);
    assert_eq!(binding.display_accent(), BLUE);
    assert_eq!(
        binding.accent_name(),
        "Blue",
        "a known valid hex keeps its real name once custom accents are on"
    );

    binding.set_accent(0xff010203);
    assert_eq!(binding.display_accent(), 0xff010203);
    assert_eq!(binding.accent_name(), "Custom");

    binding.set_custom_accent(false);
    assert_eq!(
        binding.display_accent(),
        ROSE,
        "invalid hex falls back to Rose"
    );
}

#[test]
fn stale_accent_hex_is_not_trusted_when_the_custom_flag_is_off() {
    let mut binding = UiBinding::default();
    binding.set_accent(0xff12a8a0);
    binding.set_custom_accent(true);
    assert_eq!(binding.display_accent(), 0xff12a8a0);
    binding.set_custom_accent(false);
    assert_eq!(binding.display_accent(), ROSE);
    assert_eq!(binding.accent_name(), "Red");
}

#[test]
fn spectrum_history_keeps_ten_slots_per_band_and_one_impulse_travels_centre_out() {
    assert_eq!(SPECTRUM_BANDS, 10);
    assert_eq!(SPECTRUM_HISTORY, 10);
    assert_eq!(SPECTRUM_SLOTS, 100);

    let mut history = SpectrumHistory::default();
    assert!(
        history.bars().iter().all(|bar| *bar == 0.0),
        "the native host starts with an empty history"
    );

    let mut values = [0.0; SPECTRUM_BANDS];
    values[3] = 1.0;
    history.push(&values, 0);
    assert_eq!(history.bars()[3 * SPECTRUM_HISTORY + 5], 1.0);
    assert_eq!(
        history.bars().iter().filter(|bar| **bar > 0.0).count(),
        1,
        "a single push fills only the middle slot"
    );

    // 5 -> 4 (and 5) -> (3,6) -> (2,7) -> (1,8) -> (0,9) -> gone
    let mut visible = vec![];
    for step in 0..6 {
        if step > 0 {
            history.push(&[0.0; SPECTRUM_BANDS], step as u64);
        }
        let row: Vec<usize> = (0..SPECTRUM_HISTORY)
            .filter(|slot| history.bars()[3 * SPECTRUM_HISTORY + slot] > 0.0)
            .collect();
        visible.push(row);
    }
    assert_eq!(
        visible,
        vec![
            vec![5],
            vec![4],
            vec![3, 6],
            vec![2, 7],
            vec![1, 8],
            vec![0, 9],
        ]
    );

    history.push(&[0.0; SPECTRUM_BANDS], 7);
    assert!(
        history.bars().iter().all(|bar| *bar == 0.0),
        "the impulse leaves the row after five further pushes"
    );
}

#[test]
fn spectrum_history_commits_band_zero_before_later_bands() {
    let mut history = SpectrumHistory::default();
    let mut values = [0.0; SPECTRUM_BANDS];
    for (band, slot) in values.iter_mut().enumerate() {
        *slot = (band + 1) as f32 / 10.0;
    }
    history.push(&values, 0);
    for (band, value) in values.iter().enumerate() {
        assert_eq!(
            history.bars()[band * SPECTRUM_HISTORY + SPECTRUM_HISTORY / 2],
            *value
        );
    }
}

#[test]
fn spectrum_bars_go_idle_after_the_reference_timeout_and_clear_when_off() {
    let mut binding = UiBinding::default();
    let mut values = [0.0; SPECTRUM_BANDS];
    values[0] = 0.8;
    binding.push_spectrum(&values, 1_000);
    assert_eq!(binding.spectrum_alpha(1_000), 1.0);
    assert_eq!(binding.spectrum_alpha(2_199), 1.0);
    assert_eq!(
        binding.spectrum_alpha(2_200),
        0.78,
        "bars fade to 0.78 once 1200 ms pass without a frame"
    );

    binding.toggle_power();
    assert!(
        binding.spectrum_bars().iter().all(|bar| *bar == 0.0),
        "power off clears every bar"
    );
    binding.toggle_power();
    assert!(
        binding.spectrum_bars().iter().all(|bar| *bar == 0.0),
        "returning does not resurrect stale history"
    );
}

#[test]
fn spectrum_colours_follow_the_source_gradient_maths() {
    let binding = UiBinding::default();
    let (edge, centre) = binding.spectrum_colours();
    // Pulse red #ff2e57: edge = channel * 0.82, centre = channel + (255-channel) * 0.16.
    assert_eq!(edge, 0xffd12547);
    assert_eq!(centre, 0xffff4f72);

    let mut binding = binding;
    binding.set_custom_accent(true);
    binding.set_accent(0xff000000);
    let (edge, centre) = binding.spectrum_colours();
    assert_eq!(edge, 0xff000000);
    assert_eq!(
        centre, 0xff292929,
        "black lightens by 16% of the way to 255"
    );
}

#[test]
fn eq_curve_is_a_closed_form_cubic_through_equal_index_positions() {
    let gains = [0.0; BAND_COUNT];
    let commands = eq_curve_commands(&gains, 1000.0, 200.0);
    assert!(commands.starts_with("M 16.13 100.00"), "got {commands}");
    assert!(
        commands.contains("C "),
        "the reference smooths with cubic segments, not a polyline"
    );
    assert_eq!(
        commands.matches("C ").count(),
        BAND_COUNT - 1,
        "one cubic segment joins each pair of bands"
    );
    let last = commands.rsplit_once(' ').map(|(_, y)| y).unwrap_or("");
    assert_eq!(last, "100.00", "the curve ends at the 31st band centre");

    let top = eq_curve_commands(&[12.0; BAND_COUNT], 1000.0, 200.0);
    assert!(
        top.contains("M 16.13 12.00"),
        "+12 dB sits on the reference plot top (y=12)"
    );
    let bottom = eq_curve_commands(&[-12.0; BAND_COUNT], 1000.0, 200.0);
    assert!(bottom.contains("M 16.13 188.00"), "-12 dB sits at y=188");
}

#[test]
fn palette_colour_round_trips_between_argb_and_slint() {
    for argb in [0xfff2486f, 0xff000000, 0xffffffff] {
        assert_eq!(color_to_argb(argb_to_color(argb)), argb);
    }
    assert_eq!(
        color_to_argb(argb_to_color(0x8012_3456)),
        0xff12_3456,
        "the preview palette always draws opaque colours"
    );
}

#[test]
fn save_dialog_validates_names_and_suggests_the_reference_title() {
    let mut binding = UiBinding::default();
    assert_eq!(binding.save_suggestion(), "Default Custom");
    binding.open_save();
    assert_eq!(binding.save_name(), "Default Custom");
    assert!(binding.save_valid());
    assert_eq!(binding.save_error(), None);

    binding.set_save_name("   ");
    assert_eq!(binding.save_error(), Some("Enter a name for this preset."));
    assert!(!binding.save_valid());
    assert!(!binding.confirm_save(), "an invalid name cannot save");

    binding.set_save_name("music");
    assert_eq!(
        binding.save_error(),
        Some("A preset with this name already exists."),
        "duplicate detection is case-insensitive like the reference"
    );
    assert!(!binding.save_valid());

    binding.set_save_name("Evening listen");
    assert!(binding.save_valid());
    assert!(binding.confirm_save());
    assert!(!binding.save_open());
}

#[test]
fn apply_saved_preset_installs_selects_and_closes_the_layers() {
    let mut binding = UiBinding::default();
    binding.open_save();
    binding.set_save_name("Evening listen");

    // The host rescans the library after the engine writes the file; the
    // scanned fixtures carry the real path and the CUSTOM group.
    let scanned = vec![PresetFixture {
        label: "Evening listen".into(),
        factory: false,
        effects: [0.0; EFFECT_COUNT],
        eq: [0.0; BAND_COUNT],
        path: "/home/u/.local/share/saucefx/presets/Evening listen.fac".into(),
    }];
    let saved_path = scanned[0].path.clone();
    binding.apply_saved_preset("Evening listen", &saved_path, scanned);

    assert!(!binding.save_open(), "the save layer closes");
    assert!(
        !binding.settings_open(),
        "an in-settings save closes both layers"
    );
    assert_eq!(
        binding.selected_preset_path(),
        "/home/u/.local/share/saucefx/presets/Evening listen.fac",
        "the saved preset becomes the selection"
    );
    let labels = binding.preset_labels();
    assert!(
        labels.iter().any(|label| label == "Evening listen"),
        "the saved preset joins the catalog: {labels:?}"
    );
    match binding.toast_message() {
        Some(message) => assert!(message.contains("Evening listen"), "{message}"),
        None => panic!("the save announces itself in a toast"),
    }
}

#[test]
fn preset_selection_by_path_selects_without_touching_the_engine() {
    let mut binding = UiBinding::default();
    let fixture = PresetFixture {
        label: "Imported".into(),
        factory: false,
        effects: [1.0; EFFECT_COUNT],
        eq: [2.0; BAND_COUNT],
        path: "/x/Imported.fac".into(),
    };
    let refreshed_fixture = fixture.clone();
    let saved_path = fixture.path.clone();
    binding.apply_saved_preset("Imported", &saved_path, vec![fixture]);

    // The host refreshes the preset rows after an import; the outputs and
    // the current selection row stay intact across the reinstall.
    let refreshed = vec![
        PresetFixture {
            label: "Factory".into(),
            factory: true,
            effects: [0.0; EFFECT_COUNT],
            eq: [0.0; BAND_COUNT],
            path: "/fac/Factory.fac".into(),
        },
        refreshed_fixture,
    ];
    binding.install_presets(refreshed);
    assert!(binding.preset_labels().contains(&"Factory".to_string()));
    assert_eq!(binding.selected_preset_path(), "/x/Imported.fac");
    assert!(binding.select_preset_by_path("/x/Imported.fac"));
    assert_eq!(binding.selected_preset_path(), "/x/Imported.fac");
    assert_eq!(binding.selected_preset_label(), "Imported");
    assert!(
        !binding.select_preset_by_path("/x/absent.fac"),
        "an unknown path keeps the current selection"
    );
    assert_eq!(binding.selected_preset_path(), "/x/Imported.fac");
}

#[test]
fn settings_actions_explain_themselves_without_touching_files() {
    let mut binding = UiBinding::default();
    binding.open_settings();
    binding.import_presets();
    assert_eq!(
        binding.toast(),
        Some("Preview only: preset import is not wired.")
    );
    binding.export_presets();
    assert_eq!(
        binding.toast(),
        Some("Preview only: preset export is not wired.")
    );
    binding.quit_requested();
    assert!(!binding.settings_open());
    assert_eq!(
        binding.toast(),
        Some("Preview only: the real app quits from here.")
    );
}

#[test]
fn accent_swatches_select_shipped_colours_and_custom_mode_is_separate() {
    let mut binding = UiBinding::default();
    assert_eq!(binding.accent_index(), 0, "Rose is selected by default");
    assert_eq!(binding.accent_label(), "Red");
    binding.choose_accent(3);
    assert_eq!(binding.accent_label(), "Teal");
    assert_eq!(binding.accent_index(), 3);
    assert_eq!(binding.display_accent(), 0xff12a8a0);

    binding.choose_custom_accent();
    assert!(binding.custom_accent());
    assert_eq!(
        binding.accent_index(),
        3,
        "a custom mode with a shipped hex still names that swatch"
    );
    binding.set_accent(0xff0a0b0c);
    assert_eq!(binding.accent_index(), -1);
    assert_eq!(binding.accent_label(), "Custom");
}

#[test]
fn choosing_a_factory_preset_applies_its_values_and_moves_the_baseline() {
    let mut binding = UiBinding::default();
    let factory = factory_fixtures();
    assert_eq!(factory.len(), FACTORY_PRESET_COUNT);

    // Row 1 is Default; each later row is the next public factory name.
    assert!(binding.choose_preset(3), "Music is selectable");
    let chosen = &factory[2];
    assert_eq!(binding.preset_name(), "Music");
    assert_eq!(binding.effects(), chosen.effects);
    assert_eq!(binding.eq_gains(), chosen.eq);
    assert!(
        !binding.edited(),
        "a chosen preset is the new clean baseline"
    );
    assert_eq!(
        binding.state().applied_effect(0),
        Some(chosen.effects[0]),
        "the baseline was captured"
    );

    binding.set_effect(0, 9.5);
    assert!(binding.edited());
    binding.reset_all();
    assert_eq!(
        binding.effects(),
        chosen.effects,
        "Reset All restores that preset"
    );
    assert!(!binding.edited(), "and it is not called Flat");
}

#[test]
fn saved_presets_are_added_and_become_the_selected_baseline() {
    let mut binding = UiBinding::default();
    binding.set_effect(2, 7.5); // the view's 1.0 step rounds to 8
    binding.set_eq_gain(9, -4.0);
    binding.open_settings();
    binding.open_save();
    assert!(
        binding.settings_open(),
        "settings stays behind the save dialog"
    );
    assert_eq!(binding.save_suggestion(), "Default Custom");
    binding.set_save_name("Evening listen");
    assert!(binding.confirm_save());

    assert!(
        binding
            .existing_preset_names()
            .iter()
            .any(|name| name == "Evening listen"),
        "a successful save really adds the preset"
    );
    assert_eq!(binding.preset_name(), "Evening listen");
    assert_eq!(binding.effects()[2], 8.0);
    assert_eq!(binding.eq_gains()[9], -4.0);
    assert!(!binding.edited());

    // Duplicate rejection now covers the new preset too.
    binding.open_save();
    binding.set_save_name("evening LISTEN");
    assert_eq!(
        binding.save_error(),
        Some("A preset with this name already exists.")
    );
    assert!(!binding.confirm_save());
}

#[test]
fn power_off_blocks_the_preset_menu_and_flat_reset_visibly() {
    let mut binding = UiBinding::default();
    binding.toggle_power();
    binding.toggle_preset_menu();
    assert!(
        !binding.preset_menu_open(),
        "the preset menu cannot open while off"
    );
    binding.toggle_output_menu();
    assert!(binding.output_menu_open(), "the output menu still opens");

    let mut binding = UiBinding::default();
    binding.choose_preset(4);
    let baseline = binding.effects();
    binding.set_effect(1, 8.0);
    binding.toggle_power();
    binding.reset_all();
    assert_eq!(
        binding.effects()[1],
        8.0,
        "Reset All is disabled while processing is off, so the edit stays"
    );
    binding.toggle_power();
    binding.reset_all();
    assert_eq!(binding.effects(), baseline);
}

#[test]
fn per_effect_and_per_band_resets_restore_only_that_control() {
    let mut binding = UiBinding::default();
    binding.choose_preset(5);
    let effects = binding.effects();
    let eq = binding.eq_gains();

    binding.set_effect(0, 10.0);
    binding.set_eq_gain(2, 11.0);
    assert_eq!(binding.reset_effect(0), EditOutcome::Applied);
    assert_eq!(binding.effects()[0], effects[0]);
    assert_eq!(binding.eq_gains()[2], 11.0, "the band edit is untouched");
    assert_eq!(binding.reset_eq_band(2), EditOutcome::Applied);
    assert_eq!(binding.eq_gains()[2], eq[2]);
    assert_eq!(binding.reset_effect(EFFECT_COUNT), EditOutcome::OutOfRange);
    assert_eq!(binding.reset_eq_band(BAND_COUNT), EditOutcome::OutOfRange);
}

#[test]
fn format_gain_matches_the_javascript_half_rounding_rule() {
    // JS Math.round(x) is floor(x + 0.5), so negative halves round towards +inf:
    // -1.25 -> -1.2 (not -1.3) and -0.25 -> -0.2 (not -0.3).
    assert_eq!(format_gain(-1.25), "-1.2");
    assert_eq!(format_gain(-0.25), "-0.2");
    assert_eq!(format_gain(0.25), "+0.3");
    assert_eq!(format_gain(1.25), "+1.3");
    assert_eq!(format_gain(-0.0), "0");
    assert_eq!(format_gain(0.0), "0");

    // More exact halves. Rust's f32::round would move -0.75/-2.25/-3.75 away
    // from zero; the reference moves them towards +infinity.
    assert_eq!(format_gain(-0.75), "-0.7");
    assert_eq!(format_gain(-2.25), "-2.2");
    assert_eq!(format_gain(-3.75), "-3.7");
    assert_eq!(format_gain(0.75), "+0.8");
    assert_eq!(format_gain(2.25), "+2.3");
    assert_eq!(format_gain(3.75), "+3.8");

    // Zero/sign handling, one-decimal precision and the no-suffix contract.
    assert_eq!(format_gain(0.04), "0");
    assert_eq!(format_gain(-0.04), "0");
    assert_eq!(format_gain(0.06), "+0.1");
    assert_eq!(format_gain(-0.06), "-0.1");
    assert_eq!(format_gain(1.2), "+1.2");
    assert_eq!(format_gain(-1.2), "-1.2");
    assert_eq!(format_gain(12.0), "+12");
    assert_eq!(format_gain(-12.0), "-12");
}

#[test]
fn format_gain_does_not_double_round_through_f32() {
    // 1.15f32 is 1.149999976158142. Multiplying in f32 rounds that to 11.5,
    // which would format as +1.2; the reference (and f64 arithmetic on the
    // same value) rounds 11.4999997... down to 1.1.
    assert_eq!(format_gain(1.15), "+1.1");
    assert_eq!(format_gain(-1.15), "-1.1");
}

#[test]
fn format_gain_never_mutates_preset_or_session_state() {
    let mut binding = UiBinding::default();
    let mut captured = [0.0f32; BAND_COUNT];
    captured[17] = -1.25;
    captured[4] = 1.15;
    assert!(binding.apply_captured_values(&[0.0; EFFECT_COUNT], &captured));
    let before = binding.eq_gains();
    assert_eq!(before[17], -1.25, "captured floats keep their precision");
    assert_eq!(before[4], 1.15);

    binding.set_active_eq_band(Some(17));
    assert_eq!(binding.eq_readout(), "1 kHz \u{b7} -1.2 dB");
    binding.set_active_eq_band(Some(4));
    assert_eq!(binding.eq_readout(), "50 Hz \u{b7} +1.1 dB");
    binding.set_active_eq_band(None);
    assert_eq!(binding.eq_readout(), "");

    // Formatting is read-only: neither the live gains nor the applied
    // baseline moved.
    assert_eq!(binding.eq_gains(), before);
    assert_eq!(binding.state().applied_eq_gain(17), Some(-1.25));
    assert_eq!(binding.state().applied_eq_gain(4), Some(1.15));
}

#[test]
fn eq_readout_uses_the_31_frequency_band_not_the_compact_caption_list() {
    // frontend/src/data.ts:41-65 — 31 frequencies, full unit captions, and
    // formatGain's one-decimal precision with '+' only for positives.
    let mut binding = UiBinding::default();
    assert_eq!(
        binding.active_eq_band(),
        None,
        "no band is active at startup"
    );
    assert_eq!(binding.eq_readout(), "");

    binding.set_active_eq_band(Some(1));
    assert_eq!(binding.eq_readout(), "25 Hz \u{b7} 0 dB");
    binding.set_active_eq_band(Some(17));
    assert_eq!(binding.eq_readout(), "1 kHz \u{b7} 0 dB");
    binding.set_active_eq_band(Some(30));
    assert_eq!(binding.eq_readout(), "20 kHz \u{b7} 0 dB");

    // A captured preset keeps fractional EQ values; the readout must not round.
    let mut music = UiBinding::default();
    let mut captured = [0.0f32; BAND_COUNT];
    captured[17] = 1.2;
    assert!(music.apply_captured_values(&[0.0; EFFECT_COUNT], &captured));
    assert_eq!(music.eq_gains()[17], 1.2);
    music.set_active_eq_band(Some(17));
    assert_eq!(music.eq_readout(), "1 kHz \u{b7} +1.2 dB");

    music.set_active_eq_band(None);
    assert_eq!(music.eq_readout(), "");
}

#[test]
fn custom_hex_applies_valid_input_and_keeps_the_accent_on_invalid_text() {
    let mut binding = UiBinding::default();
    assert!(binding.set_custom_hex("#3B82F6"));
    assert!(binding.custom_accent(), "a valid hex enables custom mode");
    assert_eq!(binding.display_accent(), 0xff3b_82f6);
    assert_eq!(binding.custom_hex(), "#3b82f6", "field text is canonical");

    // Invalid or unfinished text is refused without touching the colour.
    assert!(!binding.set_custom_hex("#12zz45"));
    assert!(!binding.set_custom_hex("12345"));
    assert!(!binding.set_custom_hex("#1234567"));
    assert_eq!(binding.display_accent(), 0xff3b_82f6);
    assert_eq!(binding.custom_hex(), "#3b82f6");

    assert!(binding.set_custom_hex("0a0b0c"), "the # prefix is optional");
    assert_eq!(binding.display_accent(), 0xff0a_0b0c);
}

/// Records every forwarded change, standing in for the real engine.
#[derive(Default)]
struct MockEngine {
    power: Vec<bool>,
    effects: Vec<(usize, f32)>,
    eq: Vec<(usize, f32)>,
    outputs: Vec<(String, String)>,
    loaded: Vec<String>,
}

impl saucefx_ui::binding::Engine for MockEngine {
    fn apply_power(&mut self, on: bool) {
        self.power.push(on);
    }
    fn apply_effect(&mut self, index: usize, value: f32) {
        self.effects.push((index, value));
    }
    fn apply_eq_gain(&mut self, index: usize, gain: f32) {
        self.eq.push((index, gain));
    }
    fn apply_output(&mut self, name: &str, port: &str) {
        self.outputs.push((name.to_string(), port.to_string()));
    }
    fn load_preset(&mut self, path: &str) -> Option<([f32; EFFECT_COUNT], [f32; BAND_COUNT])> {
        self.loaded.push(path.to_string());
        let mut effects = [0.0; EFFECT_COUNT];
        effects[0] = 7.5;
        let mut eq = [0.0; BAND_COUNT];
        eq[17] = -4.25;
        Some((effects, eq))
    }
}

#[test]
fn engine_sink_forwards_applied_changes_and_real_preset_paths() {
    let engine = Rc::new(RefCell::new(MockEngine::default()));
    let mut binding = UiBinding::default();
    binding.set_engine(Rc::clone(&engine) as Rc<RefCell<dyn saucefx_ui::binding::Engine>>);

    // Effects, EQ, power and output changes all reach the engine.
    assert_eq!(binding.set_effect(1, 4.5), EditOutcome::Applied);
    assert_eq!(binding.set_eq_gain(17, -3.0), EditOutcome::Applied);
    binding.toggle_power();
    binding.choose_output(2);
    // The forwarded value is the applied one: the view rounds to its 1.0 step.
    assert_eq!(engine.borrow().effects, vec![(1, 5.0)]);
    assert_eq!(engine.borrow().eq, vec![(17, -3.0)]);
    assert_eq!(engine.borrow().power, vec![false]);
    assert_eq!(
        engine.borrow().outputs,
        vec![("alsa_output.test-speakers".to_string(), "".to_string())]
    );

    // Power off blocks edits without forwarding anything.
    let before = engine.borrow().effects.len();
    assert_eq!(binding.set_effect(2, 1.0), EditOutcome::PoweredOff);
    assert_eq!(engine.borrow().effects.len(), before);

    // A catalog row with a real .fac path loads through the engine instead
    // of applying synthetic fixture values, and mirrors the live result.
    binding.toggle_power();
    let mut real = PresetFixture::flat("Factsoft 1", true);
    real.path = "/library/Factsoft 1.fac".to_string();
    binding.install_catalog(vec![real], Vec::new(), Vec::new(), 1);
    assert!(binding.choose_preset(1), "the real row loads");
    assert_eq!(
        engine.borrow().loaded,
        vec!["/library/Factsoft 1.fac".to_string()]
    );
    assert_eq!(binding.preset_name(), "Factsoft 1");
    assert_eq!(binding.effects()[0], 7.5, "engine values mirror the view");
    assert_eq!(binding.eq_gains()[17], -4.25);
    assert!(!binding.edited(), "the loaded preset is the new baseline");

    // Synthetic rows (no path) keep applying their fixture values.
    let synthetic = PresetFixture::flat("Music", true);
    binding.install_catalog(vec![synthetic], Vec::new(), Vec::new(), 1);
    assert!(binding.choose_preset(1));
    assert_eq!(
        binding.effects()[0],
        0.0,
        "fixture values, not engine reads"
    );
    assert_eq!(engine.borrow().loaded.len(), 1, "no second engine load");
}

#[test]
fn output_refresh_tracks_devices_without_reapplying_audio() {
    use saucefx_ui::binding::MenuRow;

    let engine = Rc::new(RefCell::new(MockEngine::default()));
    let mut binding = UiBinding::default();
    binding.set_engine(Rc::clone(&engine) as Rc<RefCell<dyn saucefx_ui::binding::Engine>>);
    assert!(binding.choose_output(2));
    binding.set_effect(1, 4.0);
    binding.set_eq_gain(3, -2.0);
    binding.toggle_output_menu();
    let state = binding.state().clone();
    let preset = binding.preset_name().to_string();
    let presets = binding.preset_rows().to_vec();
    let calls = engine.borrow().outputs.clone();

    let mut outputs = binding.output_rows().to_vec();
    outputs.insert(
        2,
        MenuRow::output("USB DAC", "USB audio", "usb.dac", "", true),
    );
    assert!(binding.refresh_outputs(outputs.clone()));
    assert_eq!(binding.output_rows(), outputs);
    assert_eq!(
        binding.selected_output(),
        3,
        "selection follows device identity"
    );
    assert_eq!(binding.output_name(), "Test speakers");
    assert_eq!(binding.state(), &state);
    assert_eq!(binding.preset_name(), preset);
    assert_eq!(binding.preset_rows(), presets);
    assert!(binding.output_menu_open());
    assert_eq!(
        engine.borrow().outputs,
        calls,
        "refresh does not reroute audio"
    );
    assert!(
        !binding.refresh_outputs(outputs),
        "unchanged rows need no update"
    );

    let mut disconnected = binding.output_rows().to_vec();
    disconnected.retain(|row| row.output_name != "alsa_output.test-speakers");
    assert!(binding.refresh_outputs(disconnected));
    assert_eq!(binding.output_name(), "Test speakers");
    assert!(binding.output_rows()[binding.selected_output()].disabled);
    assert_eq!(
        binding.state(),
        &state,
        "unplugging preserves the preferred output"
    );

    let mut reconnected = binding.output_rows().to_vec();
    reconnected.retain(|row| row.output_name != "alsa_output.test-speakers");
    reconnected.insert(
        2,
        MenuRow::output(
            "Test speakers",
            "Built-in audio",
            "alsa_output.test-speakers",
            "",
            true,
        ),
    );
    assert!(binding.refresh_outputs(reconnected));
    assert_eq!(binding.output_name(), "Test speakers");
    assert!(!binding.output_rows()[binding.selected_output()].disabled);
    assert_eq!(binding.state(), &state);
    assert_eq!(engine.borrow().outputs, calls);
}

#[test]
fn saved_output_restores_the_exact_port() {
    use saucefx_ui::binding::MenuRow;

    let engine = Rc::new(RefCell::new(MockEngine::default()));
    let mut binding = UiBinding::default();
    binding.set_engine(Rc::clone(&engine) as Rc<RefCell<dyn saucefx_ui::binding::Engine>>);
    let mut outputs = binding.output_rows().to_vec();
    outputs.insert(
        2,
        MenuRow::output(
            "Speakers",
            "Built-in audio",
            "alsa_output.shared",
            "analog-output-speaker",
            true,
        ),
    );
    outputs.insert(
        3,
        MenuRow::output(
            "Headphones",
            "Built-in audio",
            "alsa_output.shared",
            "analog-output-headphones",
            true,
        ),
    );
    binding.install_catalog(factory_fixtures(), Vec::new(), outputs.clone(), 1);

    binding.restore_output("alsa_output.shared", "analog-output-headphones", true);
    assert_eq!(binding.selected_output(), 3);
    assert_eq!(binding.output_name(), "Headphones");
    assert_eq!(
        binding.state().output().preferred_name,
        "alsa_output.shared"
    );
    assert_eq!(
        binding.state().output().preferred_port,
        "analog-output-headphones"
    );
    let expected = vec![(
        "alsa_output.shared".to_string(),
        "analog-output-headphones".to_string(),
    )];
    assert_eq!(engine.borrow().outputs, expected);

    outputs.swap(2, 3);
    assert!(binding.refresh_outputs(outputs));
    assert_eq!(binding.selected_output(), 2);
    assert_eq!(binding.output_name(), "Headphones");
    assert_eq!(engine.borrow().outputs, expected);
}

#[test]
fn disconnected_saved_output_preserves_its_device_and_port() {
    use saucefx_ui::binding::MenuRow;

    let engine = Rc::new(RefCell::new(MockEngine::default()));
    let mut binding = UiBinding::default();
    binding.set_engine(Rc::clone(&engine) as Rc<RefCell<dyn saucefx_ui::binding::Engine>>);
    binding.restore_output("usb.saved", "headphones", true);
    let saved = binding.state().output().clone();
    assert!(saved.routed);
    assert_eq!(saved.preferred_name, "usb.saved");
    assert_eq!(saved.preferred_port, "headphones");
    let row = &binding.output_rows()[binding.selected_output()];
    assert!(row.disabled);
    assert_eq!(row.output_name, "usb.saved");
    assert_eq!(row.output_port, "headphones");
    assert_eq!(
        engine.borrow().outputs,
        vec![("usb.saved".to_string(), "headphones".to_string())]
    );

    let mut connected = binding.output_rows().to_vec();
    connected.retain(|row| row.output_name != "usb.saved");
    connected.insert(
        2,
        MenuRow::output(
            "USB headphones",
            "USB audio",
            "usb.saved",
            "headphones",
            true,
        ),
    );
    assert!(binding.refresh_outputs(connected));
    assert_eq!(binding.output_name(), "USB headphones");
    assert!(!binding.output_rows()[binding.selected_output()].disabled);
    assert_eq!(binding.state().output(), &saved);
    assert_eq!(engine.borrow().outputs.len(), 1);
}

#[test]
fn direct_playback_restore_keeps_the_saved_device_preference() {
    let engine = Rc::new(RefCell::new(MockEngine::default()));
    let mut binding = UiBinding::default();
    binding.set_engine(Rc::clone(&engine) as Rc<RefCell<dyn saucefx_ui::binding::Engine>>);
    binding.restore_output("usb.saved", "headphones", false);
    assert_eq!(binding.output_name(), "Direct playback");
    assert!(!binding.state().output().routed);
    assert_eq!(binding.state().output().preferred_name, "usb.saved");
    assert_eq!(binding.state().output().preferred_port, "headphones");
    assert_eq!(
        engine.borrow().outputs,
        vec![("direct".to_string(), String::new())]
    );
    let rows = binding.output_rows().to_vec();
    assert!(!binding.refresh_outputs(rows));
    assert_eq!(binding.output_name(), "Direct playback");
    assert_eq!(engine.borrow().outputs.len(), 1);
}

#[test]
fn captured_eq_gains_round_to_whole_decibels_before_becoming_baseline() {
    let engine = Rc::new(RefCell::new(MockEngine::default()));
    let mut binding = UiBinding::default();
    binding.set_engine(Rc::clone(&engine) as Rc<RefCell<dyn saucefx_ui::binding::Engine>>);
    let gains = [
        -12.0, -11.6, -11.5, -11.4, -10.2, -9.9, -8.7, -7.5, -6.4, -5.5, -4.49, -3.51, -2.5, -1.2,
        -0.6, -0.5, -0.49, 0.0, 0.49, 0.5, 0.6, 1.2, 1.5, 2.5, 3.49, 4.51, 5.5, 7.2, 8.8, 11.6,
        12.0,
    ];
    let expected = [
        -12.0, -12.0, -12.0, -11.0, -10.0, -10.0, -9.0, -8.0, -6.0, -6.0, -4.0, -4.0, -3.0, -1.0,
        -1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 3.0, 3.0, 5.0, 6.0, 7.0, 9.0, 12.0, 12.0,
    ];
    let effects = [0.25, 1.2, 2.5, 3.75, 10.0];
    assert!(binding.apply_captured_values(&effects, &gains));
    assert_eq!(
        binding.eq_gains(),
        expected,
        "all preset/session gains use whole dB"
    );
    assert_eq!(
        binding.effects(),
        effects,
        "EQ rounding does not alter effects"
    );
    assert!(!binding.edited());
    assert_eq!(
        engine.borrow().eq,
        expected.into_iter().enumerate().collect::<Vec<_>>()
    );
    for (band, gain) in expected.into_iter().enumerate() {
        assert_eq!(binding.state().applied_eq_gain(band), Some(gain));
        assert_eq!(
            binding.set_eq_gain(band, if gain > 0.0 { -12.0 } else { 12.0 }),
            EditOutcome::Applied
        );
        assert_eq!(binding.reset_eq_band(band), EditOutcome::Applied);
        assert_eq!(binding.eq_gains()[band], gain);
        assert!(
            !binding.edited(),
            "reset restores the rounded preset baseline"
        );
    }
}

#[test]
fn install_catalog_resets_selection_and_reports_paths() {
    let mut binding = UiBinding::default();
    let mut real = PresetFixture::flat("Library 1", true);
    real.path = "/library/1.fac".to_string();
    binding.install_catalog(vec![real], Vec::new(), Vec::new(), 1);
    assert_eq!(binding.preset_name(), "Library 1");
    assert_eq!(binding.selected_preset_path(), "/library/1.fac");
    assert_eq!(binding.selected_output(), 1);
    assert!(binding.find_preset_by_path("/library/1.fac").is_some());
    assert!(binding.find_preset_by_path("").is_none());
    assert_eq!(binding.find_preset_by_basename("1.FAC"), Some(1));
    assert!(binding.find_preset_by_basename("missing.fac").is_none());
}

#[test]
fn first_launch_loads_the_selected_preset_into_the_engine() {
    let engine = Rc::new(RefCell::new(MockEngine::default()));
    let mut binding = UiBinding::default();
    binding.set_engine(Rc::clone(&engine) as Rc<RefCell<dyn saucefx_ui::binding::Engine>>);
    let mut default = PresetFixture::flat("Default", true);
    default.path = "/library/Factsoft/Default.fac".to_string();
    binding.install_catalog(vec![default], Vec::new(), Vec::new(), 1);

    // Selecting the row alone does not load it (install_catalog semantics).
    assert!(engine.borrow().loaded.is_empty());

    // The host's first-launch fallback loads the selected preset and mirrors
    // the live engine values, exactly like a user selection — the reference
    // applies Default for new users instead of running preset-less.
    assert!(binding.load_selected_preset());
    assert_eq!(
        engine.borrow().loaded,
        vec!["/library/Factsoft/Default.fac".to_string()]
    );
    assert_eq!(binding.effects()[0], 7.5, "engine values mirror the view");
    assert_eq!(binding.eq_gains()[17], -4.25);
    assert!(!binding.edited(), "the loaded preset is the new baseline");

    // Power off blocks the fallback without forwarding anything.
    binding.toggle_power();
    assert!(!binding.load_selected_preset());
    assert_eq!(engine.borrow().loaded.len(), 1, "no second engine load");
}
