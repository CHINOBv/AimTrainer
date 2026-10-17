//! VALORANT crosshair profile codes (`0;P;c;5;h;0;…`).
//!
//! Format and defaults: `docs/research/crosshair-codes.md`. Only the *Primary* section
//! maps onto [`Crosshair`]. The General (`0`), ADS (`A`) and Sniper (`S`) sections are
//! read tolerantly and **ignored**: their pairs are skipped without validation and
//! [`encode`] never writes them, so an exported code leaves ADS at its default
//! ("copy primary crosshair") when imported into VALORANT.

use super::{
    Crosshair, DOT_THICKNESS_PX, ERROR_MULTIPLIER_MAX, LINE_THICKNESS_MAX_PX, LineLimits, Lines,
    OUTLINE_THICKNESS_PX,
};
use thiserror::Error;

/// Why a profile code could not be parsed.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CodeError {
    /// The code contains nothing.
    #[error("the crosshair code is empty")]
    Empty,
    /// The code does not start with the `0;` header.
    #[error("a VALORANT crosshair code starts with \"0;\"")]
    MissingHeader,
    /// A key's value is not a number or flag it accepts.
    #[error("invalid value {value:?} for key {key:?}")]
    InvalidValue {
        /// The offending key.
        key: String,
        /// The rejected value text.
        value: String,
    },
    /// A numeric value lies outside the range VALORANT allows.
    #[error("value {value} for key {key:?} is outside {min}–{max}")]
    OutOfRange {
        /// The offending key.
        key: String,
        /// The rejected value.
        value: f64,
        /// Smallest allowed value.
        min: f64,
        /// Largest allowed value.
        max: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Primary,
    Ads,
    Sniper,
}

/// Decodes a profile code into the primary crosshair, starting from VALORANT's defaults.
///
/// Unknown keys and a truncated final key are skipped. Malformed or out-of-range values
/// of known Primary keys are errors.
pub fn parse(code: &str) -> Result<Crosshair, CodeError> {
    let code = code.trim();
    if code.is_empty() {
        return Err(CodeError::Empty);
    }
    let mut tokens = code.split(';').map(str::trim).filter(|t| !t.is_empty());
    if tokens.next() != Some("0") {
        return Err(CodeError::MissingHeader);
    }
    let mut crosshair = Crosshair::default();
    let mut section = Section::General;
    while let Some(key) = tokens.next() {
        section = match key {
            "P" => Section::Primary,
            "A" => Section::Ads,
            "S" => Section::Sniper,
            _ => {
                let Some(value) = tokens.next() else {
                    break; // Truncated final key.
                };
                if section == Section::Primary {
                    apply_primary(&mut crosshair, key, value)?;
                }
                continue;
            }
        };
    }
    Ok(crosshair)
}

fn invalid(key: &str, value: &str) -> CodeError {
    CodeError::InvalidValue {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

fn number(key: &str, value: &str, min: f64, max: f64) -> Result<f64, CodeError> {
    let n: f64 = value
        .parse()
        .ok()
        .filter(|n: &f64| n.is_finite())
        .ok_or_else(|| invalid(key, value))?;
    if (min..=max).contains(&n) {
        Ok(n)
    } else {
        Err(CodeError::OutOfRange {
            key: key.to_owned(),
            value: n,
            min,
            max,
        })
    }
}

fn whole(key: &str, value: &str, min: u8, max: u8) -> Result<u8, CodeError> {
    let n = number(key, value, f64::from(min), f64::from(max))?;
    if n.fract() == 0.0 {
        Ok(n as u8)
    } else {
        Err(invalid(key, value))
    }
}

fn flag(key: &str, value: &str) -> Result<bool, CodeError> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(invalid(key, value)),
    }
}

fn apply_primary(ch: &mut Crosshair, key: &str, value: &str) -> Result<(), CodeError> {
    match key {
        "c" => ch.color_index = whole(key, value, 0, 8)?,
        "u" => ch.custom_color = value.parse().map_err(|_| invalid(key, value))?,
        "h" => ch.outlines = flag(key, value)?,
        "t" => {
            ch.outline_thickness_px = whole(
                key,
                value,
                *OUTLINE_THICKNESS_PX.start(),
                *OUTLINE_THICKNESS_PX.end(),
            )?
        }
        "o" => ch.outline_opacity = number(key, value, 0.0, 1.0)?,
        "d" => ch.center_dot = flag(key, value)?,
        "z" => {
            ch.dot_thickness_px = whole(
                key,
                value,
                *DOT_THICKNESS_PX.start(),
                *DOT_THICKNESS_PX.end(),
            )?
        }
        "a" => ch.dot_opacity = number(key, value, 0.0, 1.0)?,
        "f" => ch.fade_with_firing_error = flag(key, value)?,
        "s" => ch.show_spectated = flag(key, value)?,
        "m" => ch.override_firing_offset = flag(key, value)?,
        _ => {
            let mut chars = key.chars();
            let (Some(prefix), Some(field), None) = (chars.next(), chars.next(), chars.next())
            else {
                return Ok(()); // Unknown key.
            };
            match prefix {
                '0' => apply_lines(&mut ch.inner, LineLimits::INNER, key, field, value)?,
                '1' => apply_lines(&mut ch.outer, LineLimits::OUTER, key, field, value)?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn apply_lines(
    lines: &mut Lines,
    limits: LineLimits,
    key: &str,
    field: char,
    value: &str,
) -> Result<(), CodeError> {
    match field {
        'b' => lines.show = flag(key, value)?,
        't' => lines.thickness_px = whole(key, value, 0, LINE_THICKNESS_MAX_PX)?,
        'l' => lines.length_px = whole(key, value, 0, limits.max_length_px)?,
        'v' => lines.vertical_length_px = whole(key, value, 0, limits.max_length_px)?,
        'g' => lines.separate_vertical = flag(key, value)?,
        'o' => lines.offset_px = whole(key, value, 0, limits.max_offset_px)?,
        'a' => lines.opacity = number(key, value, 0.0, 1.0)?,
        'm' => lines.movement_error = flag(key, value)?,
        's' => lines.movement_error_multiplier = number(key, value, 0.0, ERROR_MULTIPLIER_MAX)?,
        'f' => lines.firing_error = flag(key, value)?,
        'e' => lines.firing_error_multiplier = number(key, value, 0.0, ERROR_MULTIPLIER_MAX)?,
        _ => {} // Unknown key.
    }
    Ok(())
}

/// Encodes the primary crosshair as a profile code, writing only values that differ from
/// VALORANT's defaults (the factory crosshair encodes as `"0"`). Decimals use at most 3
/// places, as VALORANT's own exports do.
pub fn encode(ch: &Crosshair) -> String {
    let d = Crosshair::default();
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut put = |key: &str, changed: bool, value: String| {
        if changed {
            pairs.push((key.to_owned(), value));
        }
    };
    put(
        "c",
        ch.color_index != d.color_index,
        ch.color_index.to_string(),
    );
    put(
        "u",
        ch.custom_color != d.custom_color,
        ch.custom_color.to_hex(),
    );
    put("h", ch.outlines != d.outlines, bit(ch.outlines));
    put(
        "t",
        ch.outline_thickness_px != d.outline_thickness_px,
        ch.outline_thickness_px.to_string(),
    );
    put(
        "o",
        ch.outline_opacity != d.outline_opacity,
        decimal(ch.outline_opacity),
    );
    put("d", ch.center_dot != d.center_dot, bit(ch.center_dot));
    put(
        "z",
        ch.dot_thickness_px != d.dot_thickness_px,
        ch.dot_thickness_px.to_string(),
    );
    put(
        "a",
        ch.dot_opacity != d.dot_opacity,
        decimal(ch.dot_opacity),
    );
    put(
        "f",
        ch.fade_with_firing_error != d.fade_with_firing_error,
        bit(ch.fade_with_firing_error),
    );
    put(
        "s",
        ch.show_spectated != d.show_spectated,
        bit(ch.show_spectated),
    );
    put(
        "m",
        ch.override_firing_offset != d.override_firing_offset,
        bit(ch.override_firing_offset),
    );
    encode_lines(&mut put, '0', &ch.inner, &d.inner);
    encode_lines(&mut put, '1', &ch.outer, &d.outer);

    if pairs.is_empty() {
        return "0".to_owned();
    }
    let mut out = String::from("0;P");
    for (key, value) in pairs {
        out.push(';');
        out.push_str(&key);
        out.push(';');
        out.push_str(&value);
    }
    out
}

fn encode_lines(put: &mut impl FnMut(&str, bool, String), prefix: char, l: &Lines, d: &Lines) {
    let mut field = |name: char, changed: bool, value: String| {
        put(&format!("{prefix}{name}"), changed, value);
    };
    field('b', l.show != d.show, bit(l.show));
    field(
        't',
        l.thickness_px != d.thickness_px,
        l.thickness_px.to_string(),
    );
    field('l', l.length_px != d.length_px, l.length_px.to_string());
    field(
        'v',
        l.vertical_length_px != d.vertical_length_px,
        l.vertical_length_px.to_string(),
    );
    field(
        'g',
        l.separate_vertical != d.separate_vertical,
        bit(l.separate_vertical),
    );
    field('o', l.offset_px != d.offset_px, l.offset_px.to_string());
    field('a', l.opacity != d.opacity, decimal(l.opacity));
    field(
        'm',
        l.movement_error != d.movement_error,
        bit(l.movement_error),
    );
    field(
        's',
        l.movement_error_multiplier != d.movement_error_multiplier,
        decimal(l.movement_error_multiplier),
    );
    field('f', l.firing_error != d.firing_error, bit(l.firing_error));
    field(
        'e',
        l.firing_error_multiplier != d.firing_error_multiplier,
        decimal(l.firing_error_multiplier),
    );
}

fn bit(on: bool) -> String {
    if on { "1" } else { "0" }.to_owned()
}

/// At most 3 decimals, without trailing zeros (`0.5`, `1`, `0.35`).
fn decimal(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0" } else { s }.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crosshair::{CUSTOM_COLOR_INDEX, Rgba};

    const VECTOR_1: &str = "0;P;c;5;h;0;f;0;0l;4;0o;2;0a;1;0f;0;1b;0";
    const VECTOR_2: &str =
        "0;p;0;s;1;P;c;5;h;0;0l;4;0o;2;0a;1;1b;0;A;c;7;u;EF92BFFF;o;1;d;1;f;0;s;0;0b;0;1b;0;S;o;1";
    const VECTOR_3: &str = "0;P;d;1;0l;10;0o;2;1b;0";

    #[test]
    fn vector_1_decodes_the_documented_fields() {
        let ch = parse(VECTOR_1).unwrap();
        assert_eq!(ch.color_index, 5);
        assert_eq!(ch.color(), Rgba::opaque(0, 255, 255));
        assert!(!ch.outlines);
        assert!(!ch.fade_with_firing_error);
        assert_eq!(ch.inner.length_px, 4);
        assert_eq!(ch.inner.offset_px, 2);
        assert_eq!(ch.inner.opacity, 1.0);
        assert!(!ch.inner.firing_error);
        assert!(!ch.outer.show);
        // Untouched fields keep VALORANT's defaults.
        assert_eq!(ch.inner.thickness_px, 2);
        assert_eq!(ch.outline_opacity, 0.5);
        assert_eq!(ch.outer.offset_px, 10);
    }

    #[test]
    fn vector_2_reads_only_the_primary_section() {
        let ch = parse(VECTOR_2).unwrap();
        assert_eq!(ch.color_index, 5);
        assert!(!ch.outlines);
        assert_eq!(ch.inner.length_px, 4);
        assert_eq!(ch.inner.offset_px, 2);
        assert_eq!(ch.inner.opacity, 1.0);
        assert!(!ch.outer.show);
        // ADS and Sniper values (c=7, custom color, dot, inner lines off) do not leak.
        assert_eq!(ch.custom_color, Rgba::WHITE);
        assert!(!ch.center_dot);
        assert!(ch.inner.show);
        assert!(ch.fade_with_firing_error);
        assert!(ch.show_spectated);
        assert_eq!(encode(&ch), "0;P;c;5;h;0;0l;4;0o;2;0a;1;1b;0");
    }

    #[test]
    fn vector_3_is_the_default_plus_a_dot() {
        let ch = parse(VECTOR_3).unwrap();
        assert!(ch.center_dot);
        assert_eq!(ch.inner.length_px, 10);
        assert_eq!(ch.inner.offset_px, 2);
        assert!(!ch.outer.show);
        assert_eq!(ch.color_index, 0);
        assert!(ch.outlines);
    }

    #[test]
    fn canonical_codes_round_trip_exactly() {
        for code in [VECTOR_1, VECTOR_3, "0"] {
            assert_eq!(encode(&parse(code).unwrap()), code);
        }
    }

    #[test]
    fn defaults_encode_as_zero() {
        assert_eq!(encode(&Crosshair::default()), "0");
        assert_eq!(parse("0").unwrap(), Crosshair::default());
    }

    #[test]
    fn every_field_round_trips() {
        let lines = |l: Lines| Lines {
            show: !l.show,
            thickness_px: 4,
            length_px: 7,
            separate_vertical: true,
            vertical_length_px: 9,
            offset_px: 5,
            opacity: 0.123,
            movement_error: !l.movement_error,
            movement_error_multiplier: 2.5,
            firing_error: !l.firing_error,
            firing_error_multiplier: 0.25,
        };
        let ch = Crosshair {
            color_index: CUSTOM_COLOR_INDEX,
            custom_color: Rgba {
                r: 0xEF,
                g: 0x92,
                b: 0xBF,
                a: 0x80,
            },
            outlines: false,
            outline_thickness_px: 3,
            outline_opacity: 0.75,
            center_dot: true,
            dot_thickness_px: 4,
            dot_opacity: 0.6,
            fade_with_firing_error: false,
            show_spectated: false,
            override_firing_offset: true,
            inner: lines(Lines::INNER),
            outer: lines(Lines::OUTER),
        };
        let code = encode(&ch);
        assert_eq!(parse(&code).unwrap(), ch, "{code}");
        assert!(code.contains(";u;EF92BF80;"), "{code}");
    }

    #[test]
    fn decimals_use_at_most_three_places() {
        assert_eq!(decimal(0.5), "0.5");
        assert_eq!(decimal(1.0), "1");
        assert_eq!(decimal(0.35), "0.35");
        assert_eq!(decimal(0.12345), "0.123");
        assert_eq!(decimal(0.0), "0");
    }

    #[test]
    fn unknown_keys_are_skipped() {
        let ch = parse("0;P;q;7;0x;5;9z;1;longkey;3;c;1").unwrap();
        assert_eq!(ch.color_index, 1);
    }

    #[test]
    fn a_truncated_final_key_is_tolerated() {
        let ch = parse("0;P;c;5;h").unwrap();
        assert_eq!(ch.color_index, 5);
        assert!(ch.outlines);
        assert_eq!(parse("0;P;c;5;").unwrap().color_index, 5);
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(parse("  0;P;c;5 \n").unwrap().color_index, 5);
    }

    #[test]
    fn bad_values_are_errors() {
        let invalid = |key: &str, value: &str| CodeError::InvalidValue {
            key: key.to_owned(),
            value: value.to_owned(),
        };
        assert_eq!(parse("0;P;c;abc"), Err(invalid("c", "abc")));
        assert_eq!(parse("0;P;0a;NaN"), Err(invalid("0a", "NaN")));
        assert_eq!(parse("0;P;0t;2.5"), Err(invalid("0t", "2.5")));
        assert_eq!(parse("0;P;h;2"), Err(invalid("h", "2")));
        assert_eq!(parse("0;P;u;XYZ"), Err(invalid("u", "XYZ")));
        assert_eq!(
            parse("0;P;c;9"),
            Err(CodeError::OutOfRange {
                key: "c".to_owned(),
                value: 9.0,
                min: 0.0,
                max: 8.0
            })
        );
        assert!(matches!(
            parse("0;P;1o;41"),
            Err(CodeError::OutOfRange { .. })
        ));
        assert!(parse("0;P;1o;40").is_ok());
    }

    #[test]
    fn ignored_sections_are_not_validated() {
        assert!(parse("0;p;zz;A;c;nonsense;S;o;x").is_ok());
    }

    #[test]
    fn codes_without_the_header_are_rejected() {
        assert_eq!(parse(""), Err(CodeError::Empty));
        assert_eq!(parse("   "), Err(CodeError::Empty));
        assert_eq!(parse("P;c;5"), Err(CodeError::MissingHeader));
        assert_eq!(parse("hello"), Err(CodeError::MissingHeader));
    }

    #[test]
    fn every_prefix_of_a_real_code_parses_without_panicking() {
        for end in 0..=VECTOR_2.len() {
            let _ = parse(&VECTOR_2[..end]);
        }
    }
}
