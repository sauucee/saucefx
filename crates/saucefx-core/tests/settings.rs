use saucefx_core::settings::GuiSettings;

#[test]
fn blank_input_yields_the_startup_defaults() {
    let expected = GuiSettings {
        theme: saucefx_core::settings::ThemeMode::Dark,
        custom_accent: false,
        accent: 0xffe6_3462,
        // Start at login is ON by default (owner decision 2026-11-19);
        // the JUCE source defaulted it off.
        autostart: true,
        output_routed: false,
        output_device: String::new(),
        output_port: String::new(),
    };

    assert_eq!(saucefx_core::settings::parse_settings(""), expected);
    assert_eq!(
        saucefx_core::settings::parse_settings("\n\n   \n"),
        expected
    );
    assert_eq!(expected, GuiSettings::default());
}

#[test]
fn parses_light_theme_custom_accent_and_routing() {
    let text = "theme=light\n\
                accent=#1A2B3C\n\
                autostart=on\n\
                output=saucefx\n\
                output-device=alsa_output.pci-0000_00_1f.3.analog-stereo\n\
                output-port=analog-output-lineout\n";

    let settings = saucefx_core::settings::parse_settings(text);

    assert_eq!(settings.theme, saucefx_core::settings::ThemeMode::Light);
    assert!(settings.custom_accent);
    assert_eq!(settings.accent, 0xff1a_2b3c);
    assert!(settings.autostart);
    assert!(settings.output_routed);
    assert_eq!(
        settings.output_device,
        "alsa_output.pci-0000_00_1f.3.analog-stereo"
    );
    assert_eq!(settings.output_port, "analog-output-lineout");
}

#[test]
fn duplicate_and_unknown_keys_follow_the_source_rules() {
    let text = "theme=light\n\
                theme=dark\n\
                accent=#112233\n\
                accent=default\n\
                output=saucefx\n\
                output=system\n\
                brightness=0.8\n\
                Theme=light\n\
                theme = light\n\
                garbage\n\
                output-device=alsa_output.usb-Device_1\n";

    let settings = saucefx_core::settings::parse_settings(text);

    assert_eq!(
        settings.theme,
        saucefx_core::settings::ThemeMode::Dark,
        "the last theme= wins and a differently-spelled key is ignored"
    );
    assert!(
        !settings.custom_accent,
        "a later accent=default clears the custom flag"
    );
    assert_eq!(
        settings.accent, 0xff11_2233,
        "the colour parsed before accent=default is kept, as in the source"
    );
    assert!(!settings.output_routed, "the last output= wins");
    assert_eq!(settings.output_device, "alsa_output.usb-Device_1");
    assert_eq!(settings.output_port, "");
}

#[test]
fn invalid_values_fall_back_and_names_keep_their_identity() {
    let invalid = saucefx_core::settings::parse_settings(
        "theme=bright\nautostart=maybe\noutput=hdmi\naccent=#12345\n",
    );
    assert_eq!(invalid.theme, saucefx_core::settings::ThemeMode::Dark);
    assert!(invalid.autostart, "only \"off\" disables autostart");
    assert!(!invalid.output_routed, "only \"saucefx\" routes output");
    assert!(
        !invalid.custom_accent,
        "a five-digit accent is ignored rather than reinterpreted"
    );
    assert_eq!(invalid.accent, 0xffe6_3462);

    let case_insensitive = saucefx_core::settings::parse_settings(
        "theme=LIGHT\nautostart=OFF\noutput=SAUCEFX\naccent=#Zz0011\n",
    );
    assert_eq!(
        case_insensitive.theme,
        saucefx_core::settings::ThemeMode::Light
    );
    assert!(!case_insensitive.autostart);
    assert!(case_insensitive.output_routed);
    assert!(case_insensitive.custom_accent);
    assert_eq!(
        case_insensitive.accent, 0xff00_0011,
        "six characters are accepted and non-hex characters are skipped, as JUCE's hex parser does"
    );

    let names = saucefx_core::settings::parse_settings(
        "output-device= Line Out (USB)\noutput-port=analog-output-headphones  \n",
    );
    assert_eq!(
        names.output_device, " Line Out (USB)",
        "the value after = is kept verbatim, including leading spaces"
    );
    assert_eq!(
        names.output_port, "analog-output-headphones",
        "trailing spaces go with the line trim"
    );
}

#[test]
fn encode_writes_the_exact_six_key_text() {
    use saucefx_core::settings::{GuiSettings, ThemeMode, encode_settings};

    let routed = GuiSettings {
        theme: ThemeMode::Light,
        custom_accent: true,
        accent: 0xff1a_2b3c,
        autostart: true,
        output_routed: true,
        output_device: "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
        output_port: "analog-output-lineout".to_string(),
    };
    assert_eq!(
        encode_settings(&routed).expect("representable names"),
        "theme=light\n\
         accent=#1a2b3c\n\
         autostart=on\n\
         output=saucefx\n\
         output-device=alsa_output.pci-0000_00_1f.3.analog-stereo\n\
         output-port=analog-output-lineout\n"
    );

    assert_eq!(
        encode_settings(&GuiSettings::default()).expect("representable names"),
        "theme=dark\n\
         accent=default\n\
         autostart=on\n\
         output=system\n\
         output-device=\n\
         output-port=\n"
    );

    let low_accent = GuiSettings {
        custom_accent: true,
        accent: 0xff00_0009,
        ..GuiSettings::default()
    };
    assert!(
        encode_settings(&low_accent)
            .expect("representable names")
            .contains("accent=#000009\n"),
        "the colour is zero-padded to six digits"
    );
}

#[test]
fn legacy_fixture_round_trips_and_stays_independent_of_power_and_effects() {
    use saucefx_core::settings::{GuiSettings, ThemeMode, encode_settings, parse_settings};
    use saucefx_core::state::UiState;

    const LEGACY: &str = include_str!("../../../tests/fixtures/legacy-settings.txt");

    let parsed = parse_settings(LEGACY);
    assert_eq!(parsed.theme, ThemeMode::Light);
    assert!(parsed.custom_accent);
    assert_eq!(parsed.accent, 0xff4c_8dff);
    assert!(parsed.autostart);
    assert!(parsed.output_routed);
    assert_eq!(
        parsed.output_device,
        "alsa_output.usb-SauceFX_Test_DAC-00.analog-stereo"
    );
    assert_eq!(parsed.output_port, "analog-output-speaker");

    let encoded = encode_settings(&parsed).expect("the fixture names are representable");
    assert_eq!(
        encoded, LEGACY,
        "encoding the parsed fixture reproduces it byte for byte"
    );
    assert_eq!(parse_settings(&encoded), parsed);

    let mut state = UiState::new();
    assert!(state.set_effect(0, 7.5).is_ok());
    state.set_power(false);
    assert_eq!(
        encoded, LEGACY,
        "output routing is stored independently of power and effect values"
    );

    let defaults = encode_settings(&GuiSettings::default()).expect("representable names");
    let keys: Vec<&str> = defaults
        .lines()
        .map(|line| line.split('=').next().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "theme",
            "accent",
            "autostart",
            "output",
            "output-device",
            "output-port"
        ]
    );
}

#[test]
fn a_single_leading_bom_does_not_hide_the_first_setting() {
    use saucefx_core::settings::{ThemeMode, parse_settings};

    let with_bom = parse_settings("\u{feff}theme=light\naccent=#112233\n");
    assert_eq!(
        with_bom.theme,
        ThemeMode::Light,
        "a UTF-8 BOM is an encoding marker, not part of the first key"
    );
    assert_eq!(with_bom.accent, 0xff11_2233);
    assert!(with_bom.custom_accent);

    let without_bom = parse_settings("theme=light\naccent=#112233\n");
    assert_eq!(with_bom, without_bom);
}

#[test]
fn encode_rejects_identity_strings_that_would_forge_a_line() {
    use saucefx_core::settings::{GuiSettings, SettingsError, encode_settings};

    for bad in ["dev\noutput=saucefx", "dev\rtheme=light", "dev\0hidden"] {
        let device = GuiSettings {
            output_device: bad.to_string(),
            ..GuiSettings::default()
        };
        assert_eq!(
            encode_settings(&device),
            Err(SettingsError::UnrepresentableDeviceName),
            "output-device {bad:?} must be refused, not escaped or truncated"
        );

        let port = GuiSettings {
            output_port: bad.to_string(),
            ..GuiSettings::default()
        };
        assert_eq!(
            encode_settings(&port),
            Err(SettingsError::UnrepresentablePortName),
            "output-port {bad:?} must be refused"
        );
    }
}

#[test]
fn controlled_identities_with_unicode_spaces_and_equals_are_preserved() {
    use saucefx_core::settings::{GuiSettings, encode_settings, parse_settings};

    let settings = GuiSettings {
        output_device: " Line Out (USB) = 2 \u{fc}ml\u{e4}ut".to_string(),
        output_port: "analog-output=speaker".to_string(),
        ..GuiSettings::default()
    };

    let text = encode_settings(&settings).expect("these names are representable");
    assert!(text.contains("output-device= Line Out (USB) = 2 \u{fc}ml\u{e4}ut\n"));
    assert!(text.contains("output-port=analog-output=speaker\n"));
    assert_eq!(
        parse_settings(&text),
        settings,
        "identity is preserved, only the first = is a separator"
    );
}

#[test]
fn partial_hex_accent_follows_the_juce_hex_parser() {
    use saucefx_core::settings::parse_settings;

    let mixed = parse_settings("accent=#12zz34\n");
    assert!(mixed.custom_accent);
    assert_eq!(
        mixed.accent, 0xff00_1234,
        "JUCE's hex parser skips non-hex characters instead of stopping at them"
    );

    let prefixed = parse_settings("accent=#Zz0011\n");
    assert_eq!(prefixed.accent, 0xff00_0011);

    let valid = parse_settings("accent=#12ab34\n");
    assert_eq!(
        valid.accent, 0xff12_ab34,
        "valid six-digit accents are unchanged"
    );
}
