//! The `curve()` factory-preset generator from `src/data.ts:79`, ported and
//! locked to the arrays the TypeScript produces (generated once with
//! `node --experimental-strip-types` straight from `data.ts`). The harness
//! keeps this in a test file so `rust-anything refresh` does not wire it:
//! the exported API of `data.ts` is the four functions in `port.rs`, while
//! the presets are UI-fixture data the live app loads from the engine's
//! `.fac` files instead.

#![forbid(unsafe_code)]

/// One curve anchor: band index and dB value.
type Anchor = (usize, i32);

/// The factory preset definitions (`src/data.ts:107`): id, effect values,
/// curve anchors.
type PresetDef = (&'static str, [i32; 5], &'static [Anchor]);

/// `curve(anchors)` (`src/data.ts:79`): linear interpolation between anchor
/// points with `Math.round` per sample (half towards +infinity), then a
/// +-12 dB clamp across the output.
fn curve(anchors: &[Anchor]) -> Vec<i32> {
    const BANDS: usize = 31;
    const EQ_MAX: i32 = 12;
    let mut output = vec![0i32; BANDS];
    for pair in anchors.windows(2) {
        let (start_index, start_value) = pair[0];
        let (end_index, end_value) = pair[1];
        let span = (end_index - start_index).max(1);
        // Direct port of the source loop; the index maths mirrors data.ts.
        #[allow(clippy::needless_range_loop)]
        for index in start_index..=end_index {
            let t = (index - start_index) as f64 / span as f64;
            let value = start_value as f64 + ((end_value - start_value) as f64 * t);
            // Math.round: floor(x + 0.5); the ±0.5 special cases collapse to
            // the same integer after the clamp.
            output[index] = (value + 0.5).floor() as i32;
        }
    }
    output
        .into_iter()
        .map(|value| value.clamp(-EQ_MAX, EQ_MAX))
        .collect()
}

const FACTORY_PRESETS: [PresetDef; 13] = [
    ("default", [0, 0, 0, 0, 0], &[(0, 0), (30, 0)]),
    (
        "general",
        [3, 4, 4, 3, 4],
        &[
            (0, 1),
            (4, 2),
            (8, 1),
            (13, 0),
            (18, 1),
            (23, 2),
            (27, 2),
            (30, 1),
        ],
    ),
    (
        "music",
        [5, 6, 4, 3, 6],
        &[
            (0, 4),
            (5, 3),
            (9, 1),
            (14, 0),
            (18, 1),
            (22, 2),
            (26, 3),
            (30, 3),
        ],
    ),
    (
        "movies",
        [4, 5, 7, 5, 6],
        &[
            (0, 6),
            (5, 4),
            (11, 1),
            (15, 0),
            (19, 1),
            (23, 3),
            (27, 2),
            (30, 1),
        ],
    ),
    (
        "voice",
        [7, 2, 1, 4, 1],
        &[
            (0, -6),
            (4, -4),
            (8, 0),
            (12, 3),
            (17, 4),
            (21, 3),
            (25, 1),
            (30, -1),
        ],
    ),
    (
        "volume",
        [3, 4, 4, 7, 7],
        &[(0, 5), (4, 4), (10, 1), (15, -1), (20, 0), (25, 2), (30, 4)],
    ),
    (
        "gaming",
        [4, 3, 6, 6, 5],
        &[
            (0, 5),
            (5, 3),
            (10, 0),
            (15, -1),
            (20, 2),
            (24, 4),
            (28, 3),
            (30, 2),
        ],
    ),
    (
        "classic",
        [4, 5, 5, 3, 4],
        &[(0, 3), (5, 2), (11, 1), (16, 2), (21, 3), (26, 2), (30, 3)],
    ),
    (
        "light",
        [2, 2, 2, 2, 2],
        &[(0, 1), (8, 1), (15, 1), (23, 1), (30, 1)],
    ),
    (
        "bass",
        [2, 3, 5, 4, 9],
        &[(0, 9), (4, 7), (9, 4), (14, 1), (20, 0), (25, -1), (30, 0)],
    ),
    (
        "streaming",
        [4, 4, 5, 4, 5],
        &[
            (0, 4),
            (5, 3),
            (10, 0),
            (15, -1),
            (19, 2),
            (23, 4),
            (27, 2),
            (30, 1),
        ],
    ),
    (
        "tv",
        [5, 5, 4, 5, 4],
        &[
            (0, 5),
            (5, 4),
            (10, 1),
            (15, -2),
            (20, 1),
            (24, 3),
            (28, 2),
            (30, 1),
        ],
    ),
    (
        "transcription",
        [8, 1, 0, 2, 0],
        &[
            (0, -7),
            (4, -5),
            (8, -2),
            (12, 2),
            (17, 4),
            (21, 4),
            (25, 1),
            (30, -2),
        ],
    ),
];

/// The EQ arrays `FACTORY_PRESETS` produces in the TypeScript, generated with
/// `node --experimental-strip-types -e "import {FACTORY_PRESETS} from
/// './src/data.ts'; ..."` against the same commit these anchors came from.
const EXPECTED: [(&str, [i32; 31]); 13] = [
    (
        "default",
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0,
        ],
    ),
    (
        "general",
        [
            1, 1, 2, 2, 2, 2, 2, 1, 1, 1, 1, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2,
            1, 1,
        ],
    ),
    (
        "music",
        [
            4, 4, 4, 3, 3, 3, 3, 2, 2, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 3,
            3, 3,
        ],
    ),
    (
        "movies",
        [
            6, 6, 5, 5, 4, 4, 4, 3, 3, 2, 2, 1, 1, 1, 0, 0, 0, 1, 1, 1, 2, 2, 3, 3, 3, 3, 2, 2, 2,
            1, 1,
        ],
    ),
    (
        "voice",
        [
            -6, -5, -5, -4, -4, -3, -2, -1, 0, 1, 2, 2, 3, 3, 3, 4, 4, 4, 4, 4, 3, 3, 3, 2, 2, 1,
            1, 0, 0, -1, -1,
        ],
    ),
    (
        "volume",
        [
            5, 5, 5, 4, 4, 4, 3, 3, 2, 2, 1, 1, 0, 0, -1, -1, -1, -1, 0, 0, 0, 0, 1, 1, 2, 2, 2, 3,
            3, 4, 4,
        ],
    ),
    (
        "gaming",
        [
            5, 5, 4, 4, 3, 3, 2, 2, 1, 1, 0, 0, 0, -1, -1, -1, 0, 0, 1, 1, 2, 3, 3, 4, 4, 4, 4, 3,
            3, 3, 2,
        ],
    ),
    (
        "classic",
        [
            3, 3, 3, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 2, 2, 2, 2, 3,
            3, 3,
        ],
    ),
    (
        "light",
        [
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            1, 1,
        ],
    ),
    (
        "bass",
        [
            9, 9, 8, 8, 7, 6, 6, 5, 5, 4, 3, 3, 2, 2, 1, 1, 1, 1, 0, 0, 0, 0, 0, -1, -1, -1, -1,
            -1, 0, 0, 0,
        ],
    ),
    (
        "streaming",
        [
            4, 4, 4, 3, 3, 3, 2, 2, 1, 1, 0, 0, 0, -1, -1, -1, 0, 1, 1, 2, 3, 3, 4, 4, 4, 3, 3, 2,
            2, 1, 1,
        ],
    ),
    (
        "tv",
        [
            5, 5, 5, 4, 4, 4, 3, 3, 2, 2, 1, 0, 0, -1, -1, -2, -1, -1, 0, 0, 1, 2, 2, 3, 3, 3, 3,
            2, 2, 2, 1,
        ],
    ),
    (
        "transcription",
        [
            -7, -6, -6, -5, -5, -4, -3, -3, -2, -1, 0, 1, 2, 2, 3, 3, 4, 4, 4, 4, 4, 4, 3, 3, 2, 1,
            0, 0, -1, -1, -2,
        ],
    ),
];

#[test]
fn factory_curves_match_the_typescript_arrays() {
    for ((id, _effects, anchors), (expected_id, expected_eq)) in
        FACTORY_PRESETS.iter().zip(EXPECTED.iter())
    {
        assert_eq!(id, expected_id, "preset order matches the TS array");
        assert_eq!(&curve(anchors), expected_eq, "preset `{id}` curve");
    }
}

#[test]
fn flat_system_preset_is_the_two_anchor_flat_curve() {
    assert_eq!(curve(&[(0, 0), (30, 0)]), vec![0; 31]);
}

#[test]
fn curve_clamps_to_the_eq_range() {
    // Anchors beyond +-12 dB clamp after rounding, exactly like the TS map.
    assert_eq!(curve(&[(0, 40), (30, -40)])[0], 12);
    assert_eq!(curve(&[(0, 40), (30, -40)])[30], -12);
}
