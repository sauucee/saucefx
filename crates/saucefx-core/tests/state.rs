use saucefx_core::state::{BAND_COUNT, EFFECT_COUNT, EQ_FREQUENCIES, EditError, UiState};

#[test]
fn defaults_pin_effects_frequencies_flat_eq_and_power_on() {
    let state = UiState::new();

    assert!(state.power(), "a fresh state starts with processing on");

    let effects: [f32; EFFECT_COUNT] = std::array::from_fn(|i| state.effect(i).unwrap());
    assert_eq!(effects, [0.0; EFFECT_COUNT], "effects start at zero");

    let gains: [f32; BAND_COUNT] = std::array::from_fn(|i| state.eq_gain(i).unwrap());
    assert_eq!(gains, [0.0; BAND_COUNT], "EQ starts flat");

    assert_eq!(
        state.eq_frequencies(),
        EQ_FREQUENCIES,
        "band centres come from the engine/reference-UI list"
    );
    assert_eq!(state.effect(EFFECT_COUNT), None);
    assert_eq!(state.eq_gain(BAND_COUNT), None);
}

#[test]
fn out_of_range_edits_are_refused_and_keep_prior_state() {
    let mut state = UiState::new();
    assert!(state.set_effect(0, 4.0).is_ok());
    assert!(state.set_eq_gain(3, 2.0).is_ok());

    assert_eq!(
        state.set_effect(0, 10.5),
        Err(EditError::EffectOutOfRange),
        "effects are capped at the UI's 10.0"
    );
    assert_eq!(state.effect(0), Some(4.0), "a refused edit keeps 4.0");
    assert_eq!(state.set_effect(1, -0.5), Err(EditError::EffectOutOfRange));
    assert_eq!(state.effect(1), Some(0.0));

    assert!(state.set_effect(2, 0.0).is_ok());
    assert!(state.set_effect(2, 10.0).is_ok());
    assert_eq!(state.effect(2), Some(10.0), "10.0 itself is allowed");

    assert_eq!(state.set_eq_gain(3, 12.5), Err(EditError::GainOutOfRange));
    assert_eq!(state.eq_gain(3), Some(2.0), "a refused gain keeps 2.0 dB");
    assert!(state.set_eq_gain(3, -12.0).is_ok());
    assert!(state.set_eq_gain(3, 12.0).is_ok());
    assert_eq!(state.eq_gain(3), Some(12.0));

    assert_eq!(
        state.set_effect(EFFECT_COUNT, 1.0),
        Err(EditError::EffectIndex(EFFECT_COUNT))
    );
    assert_eq!(
        state.set_eq_gain(BAND_COUNT, 1.0),
        Err(EditError::BandIndex(BAND_COUNT))
    );
    assert_eq!(
        state.set_eq_frequency(BAND_COUNT, 1000.0),
        Err(EditError::BandIndex(BAND_COUNT))
    );
    assert_eq!(
        state.eq_frequencies(),
        EQ_FREQUENCIES,
        "no refused edit moved a band centre"
    );
}

#[test]
fn non_finite_edits_are_refused_and_keep_prior_state() {
    let mut state = UiState::new();
    assert!(state.set_effect(0, 3.0).is_ok());
    assert!(state.set_eq_gain(0, -1.5).is_ok());

    assert_eq!(
        state.set_effect(0, f32::NAN),
        Err(EditError::NonFiniteEffect),
        "NaN must not slip past the range comparison"
    );
    assert_eq!(state.effect(0), Some(3.0));
    assert_eq!(
        state.set_effect(0, f32::INFINITY),
        Err(EditError::NonFiniteEffect)
    );
    assert_eq!(
        state.set_effect(0, f32::NEG_INFINITY),
        Err(EditError::NonFiniteEffect)
    );

    assert_eq!(
        state.set_eq_gain(0, f32::NAN),
        Err(EditError::NonFiniteGain)
    );
    assert_eq!(
        state.set_eq_gain(0, f32::INFINITY),
        Err(EditError::NonFiniteGain)
    );
    assert_eq!(state.eq_gain(0), Some(-1.5));

    assert_eq!(
        state.set_eq_frequency(0, f32::NAN),
        Err(EditError::NonFiniteFrequency)
    );
    assert_eq!(
        state.set_eq_frequency(0, f32::INFINITY),
        Err(EditError::NonFiniteFrequency)
    );
    assert_eq!(
        state.eq_frequencies(),
        EQ_FREQUENCIES,
        "no refused edit moved a band centre"
    );
}

#[test]
fn edited_tracks_the_applied_baseline_and_output_updates_change_nothing_else() {
    let mut state = UiState::new();
    assert!(!state.edited(), "a fresh state matches its baseline");

    assert!(state.set_effect(2, 4.0).is_ok());
    assert!(state.edited(), "changing an effect marks the state edited");
    assert!(state.effect(2) == Some(4.0));

    assert!(state.set_effect(2, 0.0).is_ok());
    assert!(
        !state.edited(),
        "restoring the baseline clears the edited flag"
    );

    assert!(state.set_eq_gain(5, 3.0).is_ok());
    assert!(state.edited(), "changing an EQ gain marks the state edited");
    assert!(state.set_eq_gain(5, 0.0).is_ok());
    assert!(!state.edited());

    let mut applied = UiState::new();
    assert!(applied.set_effect(1, 6.0).is_ok());
    assert!(applied.set_eq_gain(9, -4.0).is_ok());
    assert!(applied.edited());
    applied.mark_applied();
    assert!(
        !applied.edited(),
        "the current values become the applied baseline"
    );
    assert!(applied.set_effect(1, 0.0).is_ok());
    assert!(
        applied.edited(),
        "moving away from the new baseline is an edit again"
    );

    let mut state = UiState::new();
    assert!(state.set_effect(0, 2.5).is_ok());
    let effects = [state.effect(0), state.effect(1)];
    let gains = [state.eq_gain(0), state.eq_gain(30)];
    let power = state.power();
    let edited = state.edited();

    state.set_output_preference("alsa_output.usb-SauceFX_Test_DAC", "analog-output-speaker");
    state.set_active_output(
        "alsa_output.usb-SauceFX_Test_DAC",
        "analog-output-speaker",
        true,
    );

    assert_eq!(state.power(), power);
    assert_eq!([state.effect(0), state.effect(1)], effects);
    assert_eq!([state.eq_gain(0), state.eq_gain(30)], gains);
    assert_eq!(state.edited(), edited, "output updates are not edits");
    assert_eq!(
        state.output().preferred_name,
        "alsa_output.usb-SauceFX_Test_DAC"
    );
    assert_eq!(state.output().active_port, "analog-output-speaker");
    assert!(state.output().routed);

    let edited_before_power = state.edited();
    state.set_power(false);
    assert!(!state.power());
    assert_eq!(
        state.edited(),
        edited_before_power,
        "power is not part of the applied baseline"
    );
}

#[test]
fn default_is_the_same_valid_startup_as_new() {
    let fallback = UiState::default();
    let fresh = UiState::new();

    assert!(
        fallback.power(),
        "UiState::default() must not start with processing off"
    );
    assert_eq!(
        fallback.eq_frequencies(),
        EQ_FREQUENCIES,
        "UiState::default() must carry the real band centres, not zeros"
    );
    assert_eq!(
        fallback, fresh,
        "Default and new must establish the same startup invariants"
    );
}

#[test]
fn applied_baseline_accessors_report_the_baseline_not_the_edit() {
    let mut state = UiState::new();
    assert_eq!(state.applied_effect(0), Some(0.0));
    assert_eq!(state.applied_eq_gain(30), Some(0.0));
    assert_eq!(state.applied_effect(EFFECT_COUNT), None);
    assert_eq!(state.applied_eq_gain(BAND_COUNT), None);

    assert!(state.set_effect(0, 7.0).is_ok());
    assert!(state.set_eq_gain(3, -5.0).is_ok());
    assert_eq!(
        state.applied_effect(0),
        Some(0.0),
        "the baseline still holds the applied value"
    );
    assert_eq!(state.applied_eq_gain(3), Some(0.0));

    state.mark_applied();
    assert_eq!(state.applied_effect(0), Some(7.0));
    assert_eq!(state.applied_eq_gain(3), Some(-5.0));
}
