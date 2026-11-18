//! The Rust port of `src/data.ts`.
//!
//! The four exported functions are character-for-character ports of the
//! TypeScript, including JavaScript's number-formatting semantics
//! (`String(x)` switches to exponent notation outside `-6 < n <= 21`,
//! `toFixed` rounds exact ties up, and `Math.round` rounds half towards
//! +infinity). Those semantics are the whole reason this harness exists:
//! `binding.rs` reimplements the same functions for the Slint UI, and the
//! recorded cases prove the port agrees with the original TS byte for byte.
//!
//! The harness only wires the four exports; the factory-preset curve
//! generator lives in `tests/factory_presets.rs` so `refresh` does not wire
//! it.

#![forbid(unsafe_code)]

/// The six fixed accents (`src/data.ts:18`), in array order.
const ACCENTS: [(&str, &str, &str); 6] = [
    ("rose", "Rose", "#f2486f"),
    ("blue", "Blue", "#3b82f6"),
    ("green", "Green", "#2fa572"),
    ("teal", "Teal", "#12a8a0"),
    ("amber", "Amber", "#d4922a"),
    ("violet", "Violet", "#8b7cf6"),
];

/// Port of `formatFrequency` (`src/data.ts:51`).
pub fn format_frequency(frequency: f64, compact: Option<bool>) -> Result<String, String> {
    let compact = compact.unwrap_or(false);
    if frequency >= 1000.0 {
        let value = frequency / 1000.0;
        // Number.isInteger: finite and equal to its floor, so +Infinity is
        // not an integer but every finite integral double is.
        let digits = if value.is_finite() && value == value.trunc() {
            0
        } else {
            1
        };
        let suffix = if compact { "k" } else { " kHz" };
        Ok(format!("{}{suffix}", js_to_fixed(value, digits)))
    } else {
        let suffix = if compact { "" } else { " Hz" };
        Ok(format!("{}{suffix}", js_number_to_string(frequency)))
    }
}

/// Port of `formatGain` (`src/data.ts:60`).
pub fn format_gain(value: f64) -> Result<String, String> {
    // Math.round(value * 10) / 10: the multiply is the JS float multiply, the
    // round is JS Math.round (floor(x + 0.5) in exact arithmetic, half
    // towards +infinity), and the divide is the JS float divide.
    let scaled = value * 10.0;
    let rounded = js_math_round(scaled) / 10.0;
    if rounded == 0.0 {
        // `rounded === 0` also catches -0.
        return Ok("0".into());
    }
    let mut body = js_to_fixed(rounded, 1);
    // `.replace(/\.0$/, '')` — the one-decimal form of a whole number.
    if body.ends_with(".0") {
        body.truncate(body.len() - 2);
    }
    Ok(if rounded > 0.0 {
        format!("+{body}")
    } else {
        body
    })
}

/// Port of `getAccent` (`src/data.ts:260`).
pub fn get_accent(id: String) -> Result<Option<serde_json::Value>, String> {
    Ok(ACCENTS
        .iter()
        .find(|(accent_id, _, _)| *accent_id == id)
        .map(|(accent_id, label, value)| {
            serde_json::json!({ "id": accent_id, "label": label, "value": value })
        }))
}

/// Port of `accentIdFor` (`src/data.ts:264`).
pub fn accent_id_for(value: String) -> Result<String, String> {
    let lowered = value.to_lowercase();
    for (accent_id, _, hex) in ACCENTS {
        if hex.to_lowercase() == lowered {
            return Ok(accent_id.to_string());
        }
    }
    Ok("custom".into())
}

/// `String(x)` per ECMA-262 Number::toString: shortest digits that roundtrip,
/// fixed notation while the decimal point sits in `-6 < n <= 21`, exponential
/// `d.ddde±xx` otherwise. Rust's `Display` for f64 already produces the same
/// shortest-roundtrip digits, just always in fixed notation, so the digits
/// are recovered from it and only the layout rule is reimplemented.
fn js_number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == 0.0 {
        // Covers -0: the template prints it without a sign.
        return "0".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let (digits, point) = shortest_digits(x.abs());
    layout_digits(sign, &digits, point)
}

/// Shortest-roundtrip digit string of a positive finite double plus the
/// decimal-point position `n` (digits before the point; may be negative),
/// i.e. the value is `digits * 10^(n - digits.len())`.
fn shortest_digits(x: f64) -> (String, i64) {
    // Rust never uses exponent notation for Display and never leaves a bare
    // trailing ".0", so the digits are exactly the shortest roundtrip set.
    let text = format!("{x}");
    let (int_part, frac_part) = match text.split_once('.') {
        Some((int_part, frac_part)) => (int_part, frac_part),
        None => (text.as_str(), ""),
    };
    let int_len = int_part.len() as i64;
    let combined: String = format!("{int_part}{frac_part}");
    let leading = combined.len() - combined.trim_start_matches('0').len();
    let stripped = combined.trim_start_matches('0');
    let trailing = stripped.len() - stripped.trim_end_matches('0').len();
    let digits = stripped[..stripped.len() - trailing].to_string();
    // value = digits * 10^(n - digits.len()); stripping a leading zeros from
    // the fixed form keeps n at `int_len - a`.
    let n = int_len - leading as i64;
    (digits, n)
}

/// Lay the digit string out per the `n` window rules of Number::toString:
/// whole numbers up to 21 digits stay fixed, the point may sit anywhere from
/// inside the digits down to `n == -5` ("0.000ddd"), and everything outside
/// that window goes exponential.
fn layout_digits(sign: &str, digits: &str, n: i64) -> String {
    let k = digits.len() as i64;
    if k <= n && n <= 21 {
        // Whole number: pad with n - k zeros.
        format!("{sign}{digits}{}", "0".repeat((n - k) as usize))
    } else if n > 0 && n <= 21 {
        // Point inside the digit string (n < k here; the first branch took
        // every n >= k within the window).
        let n = n as usize;
        format!("{sign}{}.{}", &digits[..n], &digits[n..])
    } else if n > -6 && n <= 0 {
        // Point before the first digit but inside the fixed window.
        format!("{sign}0.{}{}", "0".repeat((-n) as usize), digits)
    } else {
        // Exponential: d[.ddd]e±xx, no zero padding, sign always shown.
        let exponent = n - 1;
        let (head, tail) = digits.split_at(1);
        let mantissa = if tail.is_empty() {
            head.to_string()
        } else {
            format!("{head}.{tail}")
        };
        if exponent >= 0 {
            format!("{sign}{mantissa}e+{exponent}")
        } else {
            format!("{sign}{mantissa}e{exponent}")
        }
    }
}

/// Exact decomposition of a finite double into `(mantissa, exponent)` with
/// value = mantissa × 2^exponent (mantissa signed, no fractional bits).
fn exact_parts(x: f64) -> (i128, i64) {
    let bits = x.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i64;
    let fraction = (bits & ((1u64 << 52) - 1)) as i128;
    let mantissa = if raw_exponent == 0 {
        fraction
    } else {
        fraction | (1i128 << 52)
    };
    let signed = if x < 0.0 { -mantissa } else { mantissa };
    (signed, raw_exponent - 1075)
}

/// JS `Math.round(x)` = floor(x + 0.5) computed in exact arithmetic: the
/// naive `(x + 0.5).floor()` double-rounds when the add crosses an integer
/// boundary (x = 0.49999999999999994 must stay 0, not become 1).
fn js_math_round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x; // NaN, ±Infinity, ±0 pass through per the spec.
    }
    let (mantissa, exponent) = exact_parts(x);
    if exponent >= 0 {
        // x is an integer already; x + 0.5 floors back to x.
        return x;
    }
    // floor((m + 2^(-e-1)) / 2^(-e)): div_euclid is floor division for a
    // positive divisor, which is what the negative-half cases need. For
    // exponents below -126 the value is far below 0.5, so the clamped
    // division still lands on 0 exactly like the unclamped one.
    let denominator = 1i128 << (-exponent).min(126);
    let half = denominator >> 1;
    ((mantissa + half).div_euclid(denominator)) as f64
}

/// `Number.prototype.toFixed(digits)` for `digits` 0 or 1 (the only values
/// `data.ts` passes). Ties round up ("pick the larger n"), magnitudes of 1e21
/// or more fall back to `String(x)`, and NaN/infinities keep their names.
fn js_to_fixed(x: f64, digits: u32) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return match digits {
            0 => "0".into(),
            _ => "0.0".into(),
        };
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let magnitude = x.abs();
    if magnitude >= 1e21 {
        return format!("{sign}{}", js_number_to_string(magnitude));
    }
    // n is the integer with n / 10^d closest to the magnitude, larger on an
    // exact tie — computed on the EXACT double (mantissa × 2^e), never on a
    // rounded float product: 3.15's double is 3.14999…, so toFixed(1) is
    // "3.1", while multiplying first rounds 3.15 × 10 up to exactly 31.5.
    let (mantissa, exponent) = exact_parts(magnitude);
    let scaled = (mantissa as u128) * 10u128.pow(digits);
    let n: u128 = if exponent >= 0 {
        // Integral magnitude below the 1e21 fallback: exact product. The
        // largest shift here is 69 and mantissa fits 54 bits, so the product
        // stays inside u128.
        scaled << exponent
    } else {
        let shift = (-exponent).min(126);
        let denominator = 1u128 << shift;
        let quotient = scaled / denominator;
        let remainder = scaled % denominator;
        if remainder * 2 >= denominator {
            quotient + 1
        } else {
            quotient
        }
    };
    let whole = n.to_string();
    if digits == 0 {
        return format!("{sign}{whole}");
    }
    // Place the point one digit from the right, zero-padding the left so
    // 0.05 becomes "0.1" and 13 becomes "13.0".
    let padded = format!("{whole:0>width$}", width = digits as usize + 1);
    let split = padded.len() - digits as usize;
    format!("{sign}{}.{}", &padded[..split], &padded[split..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_gains_are_bare_zero() {
        assert_eq!(format_gain(0.0).unwrap(), "0");
        assert_eq!(format_gain(-0.0).unwrap(), "0");
    }

    #[test]
    fn whole_gains_drop_the_decimal() {
        assert_eq!(format_gain(10.0).unwrap(), "+10");
        assert_eq!(format_gain(-12.0).unwrap(), "-12");
    }

    #[test]
    fn eq_frequencies_format_like_the_captions() {
        // The static EQ_FREQUENCY_CAPTIONS array in binding.rs must equal
        // what the ported formatter generates for the 31 band frequencies.
        let frequencies = [
            20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0,
            400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0,
            5000.0, 6300.0, 8000.0, 10000.0, 12500.0, 16000.0, 20000.0,
        ];
        let captions: Vec<String> = frequencies
            .iter()
            .map(|f| format_frequency(*f, Some(false)).unwrap())
            .collect();
        assert_eq!(captions[0], "20 Hz");
        assert_eq!(captions[17], "1 kHz");
        assert_eq!(captions[18], "1.3 kHz");
        assert_eq!(captions[22], "3.1 kHz");
        assert_eq!(captions[30], "20 kHz");
    }
}
