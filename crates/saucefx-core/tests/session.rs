use saucefx_core::session::{PresetSession, parse_session};

/// Synthetic session text in the writer's own field order.
fn synthetic_session(power: &str, bands: usize) -> String {
    let mut text = String::from("version=1\npreset=/synthetic/presets/Night Drive.fxp\n");
    text.push_str(&format!("power={power}\n"));
    for effect in 0..5 {
        text.push_str(&format!("effect{effect}={}.500000\n", effect + 1));
    }
    text.push_str(&format!("bands={bands}\n"));
    for band in 0..bands {
        let freq = 20.0 * 1.26_f64.powi(band as i32);
        let gain = if band % 2 == 0 { 3.5 } else { -3.5 };
        text.push_str(&format!("eq{band}freq={freq:.6}\n"));
        text.push_str(&format!("eq{band}gain={gain:.6}\n"));
    }
    text
}

#[test]
fn parses_a_complete_31_band_session() {
    let text = synthetic_session("1", 31);
    let session: PresetSession = parse_session(&text).expect("a complete session is valid");

    assert_eq!(session.preset_path, "/synthetic/presets/Night Drive.fxp");
    assert_eq!(session.factory_preset, None);
    assert!(session.power);
    for (i, value) in session.effects.iter().enumerate() {
        assert_eq!(*value, (i + 1) as f32 + 0.5, "effect{i} kept its value");
    }
    assert_eq!(session.eq_bands, 31);
    assert!((session.eq_freqs[0] - 20.0).abs() < 1e-4);
    assert!((session.eq_gains[0] - 3.5).abs() < 1e-6);
    assert!((session.eq_gains[1] + 3.5).abs() < 1e-6);
    assert_eq!(
        session.eq_freqs[31], 0.0,
        "field 31/32 exists in the source array but stays untouched"
    );
}

#[test]
fn factory_preset_basename_follows_the_relocation_rule() {
    let with_factory = synthetic_session("1", 2);
    let text = format!("{with_factory}factory-preset=Club Night.fac\n");
    let session = parse_session(&text).expect("complete session");
    assert_eq!(session.factory_preset.as_deref(), Some("Club Night.fac"));
    assert_eq!(session.factory_preset_basename(), Some("Club Night.fac"));

    let nested = format!("{with_factory}factory-preset=factory/Club Night.fac\n");
    let session = parse_session(&nested).expect("complete session");
    assert_eq!(
        session.factory_preset_basename(),
        None,
        "a path with a separator is not a relocatable basename"
    );

    let windows = format!("{with_factory}factory-preset=factory\\Club Night.fac\n");
    let session = parse_session(&windows).expect("complete session");
    assert_eq!(session.factory_preset_basename(), None);

    let wrong_suffix = format!("{with_factory}factory-preset=Club Night.fxp\n");
    let session = parse_session(&wrong_suffix).expect("complete session");
    assert_eq!(session.factory_preset_basename(), None);

    let upper = format!("{with_factory}factory-preset=CLUB.FAC\n");
    let session = parse_session(&upper).expect("complete session");
    assert_eq!(
        session.factory_preset_basename(),
        Some("CLUB.FAC"),
        "the suffix check ignores case"
    );

    let empty = format!("{with_factory}factory-preset=\n");
    let session = parse_session(&empty).expect("complete session");
    assert_eq!(session.factory_preset_basename(), None);

    let session = parse_session(&with_factory).expect("complete session");
    assert_eq!(session.factory_preset, None);
}

#[test]
fn incomplete_sessions_are_rejected_with_the_source_reason() {
    use saucefx_core::session::SessionError;

    let base = synthetic_session("1", 31);

    let version_two = base.replace("version=1", "version=2");
    assert_eq!(
        parse_session(&version_two),
        Err(SessionError::UnsupportedVersion(2))
    );

    let no_version = base.replace("version=1\n", "");
    assert_eq!(
        parse_session(&no_version),
        Err(SessionError::UnsupportedVersion(0))
    );

    let missing_effect: String = base
        .lines()
        .filter(|line| !line.starts_with("effect3="))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(
        parse_session(&missing_effect),
        Err(SessionError::MissingEffects)
    );

    let zero_bands = base.replace("bands=31", "bands=0");
    assert_eq!(
        parse_session(&zero_bands),
        Err(SessionError::BandCountOutOfRange(0))
    );

    let too_many = synthetic_session("1", 31).replace("bands=31", "bands=33");
    assert_eq!(
        parse_session(&too_many),
        Err(SessionError::BandCountOutOfRange(33))
    );

    let missing_freq: String = base
        .lines()
        .filter(|line| !line.starts_with("eq17freq="))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(
        parse_session(&missing_freq),
        Err(SessionError::MissingFrequencies)
    );

    let missing_gain: String = base
        .lines()
        .filter(|line| !line.starts_with("eq4gain="))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(
        parse_session(&missing_gain),
        Err(SessionError::MissingGains)
    );

    /* Documented divergence: an empty or non-numeric field is refused here,
     * while JUCE's getFloatValue() would have produced 0.0 for it
     * (src/gui/Main.cpp:658-662). See the packet report. */
    let empty_value: String = base
        .lines()
        .map(|line| {
            if line.starts_with("eq5freq=") {
                "eq5freq=\n".to_string()
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    assert_eq!(
        parse_session(&empty_value),
        Err(SessionError::MissingFrequencies)
    );
}

#[test]
fn band_lines_are_counted_not_covered_and_power_spellings_match() {
    let mut duplicated = String::from("version=1\n");
    for effect in 0..5 {
        duplicated.push_str(&format!("effect{effect}=0.000000\n"));
    }
    duplicated.push_str("bands=31\n");
    for _ in 0..31 {
        duplicated.push_str("eq0freq=1000.000000\n");
        duplicated.push_str("eq0gain=-2.000000\n");
    }

    let session = parse_session(&duplicated)
        .expect("the source counts accepted eq lines, it does not require one line per band");
    assert_eq!(session.eq_bands, 31);
    assert_eq!(session.eq_freqs[0], 1000.0);
    assert_eq!(
        session.eq_freqs[30], 0.0,
        "the uncovered bands stay at zero, exactly like the source arrays"
    );

    for (spelling, expected) in [("off", false), ("0", false), ("false", true), ("", true)] {
        let text = synthetic_session(spelling, 1);
        let session = parse_session(&text).expect("complete session");
        assert_eq!(session.power, expected, "power={spelling} means {expected}");
    }

    let crlf = synthetic_session("1", 31).replace('\n', "\r\n");
    let session = parse_session(&crlf).expect("CRLF files load too");
    assert_eq!(session.eq_bands, 31);
    assert_eq!(session.preset_path, "/synthetic/presets/Night Drive.fxp");

    let out_of_range = synthetic_session("1", 1).replace("effect4=5.500000", "effect9=5.500000");
    assert_eq!(
        parse_session(&out_of_range),
        Err(saucefx_core::session::SessionError::MissingEffects)
    );
}

#[test]
fn encode_reproduces_the_legacy_fixture_byte_for_byte() {
    use saucefx_core::session::encode_session;

    const LEGACY: &str = include_str!("../../../tests/fixtures/legacy-session-v1.txt");

    let session = parse_session(LEGACY).expect("the fixture is a complete session");
    let text = encode_session(&session).expect("a complete session is encodable");
    assert_eq!(text, LEGACY);
    assert_eq!(parse_session(&text).expect("re-parsed"), session);
}

#[test]
fn encode_keeps_paths_identity_power_and_signed_values() {
    use saucefx_core::session::{PresetSession, encode_session};

    let mut eq_freqs = [0.0f32; saucefx_core::session::SESSION_MAX_BANDS];
    eq_freqs[0] = 20.0;
    eq_freqs[1] = 20000.0;
    let mut eq_gains = [0.0f32; saucefx_core::session::SESSION_MAX_BANDS];
    eq_gains[0] = -12.0;
    eq_gains[1] = 12.0;

    let mut session = PresetSession {
        preset_path: "/synthetic/presets/Ünïcode  Space.fxp".to_string(),
        factory_preset: None,
        power: false,
        effects: [0.0, 1.0, 10.0, 0.5, 0.1],
        eq_bands: 2,
        eq_freqs,
        eq_gains,
    };

    let text = encode_session(&session).expect("a complete session is encodable");
    assert!(
        !text.contains("factory-preset="),
        "no factory basename means no factory line"
    );
    assert!(
        text.starts_with("version=1\npreset=/synthetic/presets/Ünïcode  Space.fxp\npower=0\n"),
        "path and name are written verbatim and power-off is retained: {text}"
    );
    assert!(text.contains("\neffect0=0.000000\neffect1=1.000000\neffect2=10.000000\n"));
    assert!(text.contains("\neffect4=0.100000\n"));
    assert!(text.contains("\nbands=2\neq0freq=20.000000\neq0gain=-12.000000\n"));
    assert!(text.contains("eq1freq=20000.000000\neq1gain=12.000000\n"));

    let reparsed = parse_session(&text).expect("encoded text parses again");
    assert_eq!(reparsed, session);
    assert_eq!(encode_session(&reparsed).expect("still encodable"), text);

    session.preset_path = String::new();
    let text = encode_session(&session).expect("a complete session is encodable");
    assert!(
        !text.contains("preset="),
        "an empty preset path omits the line instead of writing preset="
    );
    assert_eq!(parse_session(&text).expect("still valid").preset_path, "");
}

/// A session whose band count is arbitrary, used to probe the encoder bounds.
fn session_with_bands(eq_bands: usize) -> PresetSession {
    PresetSession {
        preset_path: "/synthetic/presets/Edge.fxp".to_string(),
        factory_preset: None,
        power: true,
        effects: [1.0; saucefx_core::session::SESSION_EFFECT_COUNT],
        eq_bands,
        eq_freqs: [1000.0; saucefx_core::session::SESSION_MAX_BANDS],
        eq_gains: [0.0; saucefx_core::session::SESSION_MAX_BANDS],
    }
}

#[test]
fn encode_does_not_panic_on_out_of_range_band_counts() {
    use saucefx_core::session::{SESSION_MAX_BANDS, encode_session};

    let session = session_with_bands(SESSION_MAX_BANDS + 1);
    let outcome = std::panic::catch_unwind(|| encode_session(&session));
    assert!(
        outcome.is_ok(),
        "encode_session panicked instead of rejecting the band count"
    );
}

#[test]
fn encode_rejects_every_out_of_range_band_count_explicitly() {
    use saucefx_core::session::{SESSION_MAX_BANDS, SessionError, encode_session};

    for bands in [0usize, SESSION_MAX_BANDS + 1, usize::MAX] {
        let session = session_with_bands(bands);
        let encoded = std::panic::catch_unwind(|| encode_session(&session))
            .unwrap_or_else(|_| panic!("encode_session panicked for bands={bands}"));
        assert_eq!(
            encoded,
            Err(SessionError::BandCountOutOfRange(bands)),
            "bands={bands} must be refused before any array is indexed"
        );
    }

    let at_limit = session_with_bands(SESSION_MAX_BANDS);
    let text = encode_session(&at_limit).expect("32 bands is the boundary and is encodable");
    assert!(text.contains("bands=32\n"));
    assert!(text.contains("eq31freq=1000.000000\neq31gain=0.000000\n"));
    assert_eq!(
        parse_session(&text).expect("boundary text parses").eq_bands,
        SESSION_MAX_BANDS
    );
}

#[test]
fn a_single_leading_bom_does_not_hide_the_session_version() {
    use saucefx_core::session::parse_session;

    const WITH_BOM: &str = "\u{feff}version=1\npower=0\neffect0=0\neffect1=0\neffect2=0\neffect3=0\neffect4=0\nbands=1\neq0freq=20\neq0gain=0\n";
    const PLAIN: &str = "version=1\npower=0\neffect0=0\neffect1=0\neffect2=0\neffect3=0\neffect4=0\nbands=1\neq0freq=20\neq0gain=0\n";

    let with_bom = parse_session(WITH_BOM).expect("the BOM is an encoding marker, not the version");
    assert!(!with_bom.power);
    assert_eq!(with_bom.eq_bands, 1);
    assert_eq!(with_bom, parse_session(PLAIN).expect("plain text parses"));
}

#[test]
fn encode_refuses_non_finite_effect_and_used_band_values() {
    use saucefx_core::session::{SESSION_EFFECT_COUNT, SessionError, encode_session};

    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for slot in 0..SESSION_EFFECT_COUNT {
            let mut session = session_with_bands(1);
            session.effects[slot] = bad;
            assert_eq!(
                encode_session(&session),
                Err(SessionError::NonFiniteValue),
                "effect{slot}={bad} must not produce a successful save"
            );
        }

        let mut session = session_with_bands(1);
        session.eq_freqs[0] = bad;
        assert_eq!(
            encode_session(&session),
            Err(SessionError::NonFiniteValue),
            "used eq0freq={bad} must not produce a successful save"
        );

        let mut session = session_with_bands(2);
        session.eq_gains[1] = bad;
        assert_eq!(
            encode_session(&session),
            Err(SessionError::NonFiniteValue),
            "used eq1gain={bad} must not produce a successful save"
        );
    }
}

#[test]
fn encode_ignores_non_finite_values_in_unused_band_slots() {
    use saucefx_core::session::{SESSION_MAX_BANDS, encode_session};

    let mut session = session_with_bands(2);
    session.eq_freqs[2] = f32::NAN;
    session.eq_gains[SESSION_MAX_BANDS - 1] = f32::INFINITY;

    let text = encode_session(&session).expect("unused slots are never written");
    assert!(text.contains("bands=2\n"));
    assert!(!text.contains("eq2freq"));
    assert!(!text.contains("NaN") && !text.contains("inf"));
}

#[test]
fn encode_rejects_session_names_that_would_forge_a_line() {
    use saucefx_core::session::{SessionError, encode_session};

    for bad in ["x\nversion=2", "x\rpower=1", "x\0hidden"] {
        let mut session = session_with_bands(1);
        session.preset_path = bad.to_string();
        assert_eq!(
            encode_session(&session),
            Err(SessionError::UnrepresentableName),
            "preset path {bad:?} must be refused, not escaped or truncated"
        );

        let mut session = session_with_bands(1);
        session.factory_preset = Some(bad.to_string());
        assert_eq!(
            encode_session(&session),
            Err(SessionError::UnrepresentableName),
            "factory preset name {bad:?} must be refused"
        );
    }
}

#[test]
fn session_names_with_unicode_spaces_and_equals_are_preserved() {
    use saucefx_core::session::{encode_session, parse_session};

    let mut session = session_with_bands(1);
    session.preset_path = "/synthetic/presets/A = B (\u{fc}n\u{ef}).fxp".to_string();
    session.factory_preset = Some("Club = Night (v2).fac".to_string());

    let text = encode_session(&session).expect("these names are representable");
    assert!(text.contains("preset=/synthetic/presets/A = B (\u{fc}n\u{ef}).fxp\n"));
    assert!(text.contains("factory-preset=Club = Night (v2).fac\n"));

    let reparsed = parse_session(&text).expect("encoded text parses again");
    assert_eq!(reparsed.preset_path, session.preset_path);
    assert_eq!(reparsed.factory_preset, session.factory_preset);
}
