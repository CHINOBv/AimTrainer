//! Crosshair model shared by rendering and VALORANT profile-code import/export.
//!
//! The model mirrors VALORANT's *Primary* crosshair. Defaults are VALORANT's factory
//! defaults (`docs/research/crosshair-codes.md`). Lengths, thicknesses and offsets are
//! whole screen pixels; VALORANT documents no reference resolution, so the trainer
//! draws them 1:1 in physical pixels. Movement and firing error flags are kept as data
//! so codes round-trip, but the trainer does not animate them.

pub mod code;
pub mod preset;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::RangeInclusive;
use std::str::FromStr;
use thiserror::Error;

/// An 8-bit RGBA color, serialized as `RRGGBBAA` hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rgba {
    /// Red channel, 0-255.
    pub r: u8,
    /// Green channel, 0-255.
    pub g: u8,
    /// Blue channel, 0-255.
    pub b: u8,
    /// Alpha channel, 0-255 (255 = opaque).
    pub a: u8,
}

impl Rgba {
    /// Opaque white.
    pub const WHITE: Rgba = Rgba::opaque(0xFF, 0xFF, 0xFF);

    /// An opaque color from red, green and blue.
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xFF }
    }

    /// Hex as VALORANT writes it: `RRGGBBAA`, uppercase.
    pub fn to_hex(self) -> String {
        format!("{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
    }
}

/// A string that is not a valid hex color; holds the rejected text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("expected an RRGGBB or RRGGBBAA hex color, got {0:?}")]
pub struct HexColorError(pub String);

impl FromStr for Rgba {
    type Err = HexColorError;

    /// Parses `RRGGBBAA` or `RRGGBB` (alpha `FF`), case-insensitive.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || HexColorError(s.to_owned());
        if !(s.len() == 6 || s.len() == 8) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let byte = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| err());
        Ok(Self {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if s.len() == 8 { byte(6)? } else { 0xFF },
        })
    }
}

impl TryFrom<String> for Rgba {
    type Error = HexColorError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Rgba> for String {
    fn from(c: Rgba) -> Self {
        c.to_hex()
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// VALORANT's preset colors, indexed by the code's `c` key (0–7).
pub const PRESET_COLORS: [(&str, Rgba); 8] = [
    ("White", Rgba::opaque(0xFF, 0xFF, 0xFF)),
    ("Green", Rgba::opaque(0x00, 0xFF, 0x00)),
    ("Yellow green", Rgba::opaque(0x7F, 0xFF, 0x00)),
    ("Green yellow", Rgba::opaque(0xDF, 0xFF, 0x00)),
    ("Yellow", Rgba::opaque(0xFF, 0xFF, 0x00)),
    ("Cyan", Rgba::opaque(0x00, 0xFF, 0xFF)),
    ("Pink", Rgba::opaque(0xFF, 0x00, 0xFF)),
    ("Red", Rgba::opaque(0xFF, 0x00, 0x00)),
];

/// Color index meaning "use [`Crosshair::custom_color`]".
pub const CUSTOM_COLOR_INDEX: u8 = 8;

/// Outline thickness range VALORANT allows.
pub const OUTLINE_THICKNESS_PX: RangeInclusive<u8> = 1..=6;
/// Center dot thickness range VALORANT allows.
pub const DOT_THICKNESS_PX: RangeInclusive<u8> = 1..=6;
/// Line thickness maximum (inner and outer).
pub const LINE_THICKNESS_MAX_PX: u8 = 10;
/// Movement and firing error multiplier maximum.
pub const ERROR_MULTIPLIER_MAX: f64 = 3.0;

/// Slider maxima that differ between inner and outer lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineLimits {
    /// Longest arm length allowed, in pixels.
    pub max_length_px: u8,
    /// Largest center gap allowed, in pixels.
    pub max_offset_px: u8,
}

impl LineLimits {
    /// Limits for the inner lines.
    pub const INNER: LineLimits = LineLimits {
        max_length_px: 20,
        max_offset_px: 20,
    };
    /// Limits for the outer lines.
    pub const OUTER: LineLimits = LineLimits {
        max_length_px: 10,
        max_offset_px: 40,
    };
}

/// One set of four lines (inner or outer).
///
/// Deserializing needs every field: inner and outer lines have different defaults, so a
/// partial table has no single sensible default to fall back to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Lines {
    /// Whether this set of lines is drawn.
    pub show: bool,
    /// Line thickness in pixels.
    pub thickness_px: u8,
    /// Horizontal arm length (and vertical too unless `separate_vertical`).
    pub length_px: u8,
    /// Whether the vertical arms use `vertical_length_px`.
    pub separate_vertical: bool,
    /// Vertical arm length, used only when `separate_vertical` is set.
    pub vertical_length_px: u8,
    /// Gap from the center to where each arm starts.
    pub offset_px: u8,
    /// 0–1.
    pub opacity: f64,
    /// Whether the lines react to movement.
    pub movement_error: bool,
    /// Scale of the movement reaction.
    pub movement_error_multiplier: f64,
    /// Whether the lines react to firing.
    pub firing_error: bool,
    /// Scale of the firing reaction.
    pub firing_error_multiplier: f64,
}

impl Lines {
    /// VALORANT's inner-line defaults.
    pub const INNER: Lines = Lines {
        show: true,
        thickness_px: 2,
        length_px: 6,
        separate_vertical: false,
        vertical_length_px: 6,
        offset_px: 3,
        opacity: 0.8,
        movement_error: false,
        movement_error_multiplier: 1.0,
        firing_error: true,
        firing_error_multiplier: 1.0,
    };

    /// VALORANT's outer-line defaults.
    pub const OUTER: Lines = Lines {
        show: true,
        thickness_px: 2,
        length_px: 2,
        separate_vertical: false,
        vertical_length_px: 2,
        offset_px: 10,
        opacity: 0.35,
        movement_error: true,
        movement_error_multiplier: 1.0,
        firing_error: true,
        firing_error_multiplier: 1.0,
    };

    /// Length of the up and down arms.
    pub fn effective_vertical_length_px(&self) -> u8 {
        if self.separate_vertical {
            self.vertical_length_px
        } else {
            self.length_px
        }
    }

    /// Whether the lines draw anything.
    pub fn is_visible(&self) -> bool {
        self.show && self.thickness_px > 0 && self.opacity > 0.0
    }
}

/// VALORANT's primary crosshair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crosshair {
    /// 0–7 = [`PRESET_COLORS`], 8 = [`CUSTOM_COLOR_INDEX`].
    pub color_index: u8,
    /// Used when `color_index` is custom; kept otherwise so codes round-trip.
    pub custom_color: Rgba,
    /// Whether the outlines are drawn.
    pub outlines: bool,
    /// Outline thickness in pixels.
    pub outline_thickness_px: u8,
    /// 0–1.
    pub outline_opacity: f64,
    /// Whether the center dot is drawn.
    pub center_dot: bool,
    /// Center dot thickness in pixels.
    pub dot_thickness_px: u8,
    /// 0–1.
    pub dot_opacity: f64,
    /// Whether the crosshair fades as firing error grows.
    pub fade_with_firing_error: bool,
    /// Whether the crosshair is shown to spectators.
    pub show_spectated: bool,
    /// Whether firing error uses its own offset instead of the line offset.
    pub override_firing_offset: bool,
    /// The inner lines.
    pub inner: Lines,
    /// The outer lines.
    pub outer: Lines,
}

impl Crosshair {
    /// VALORANT's factory crosshair.
    pub const VALORANT_DEFAULT: Crosshair = Crosshair {
        color_index: 0,
        custom_color: Rgba::WHITE,
        outlines: true,
        outline_thickness_px: 1,
        outline_opacity: 0.5,
        center_dot: false,
        dot_thickness_px: 2,
        dot_opacity: 1.0,
        fade_with_firing_error: true,
        show_spectated: true,
        override_firing_offset: false,
        inner: Lines::INNER,
        outer: Lines::OUTER,
    };

    /// The color lines and the dot are drawn with (before per-element opacity).
    pub fn color(&self) -> Rgba {
        PRESET_COLORS
            .get(usize::from(self.color_index))
            .map_or(self.custom_color, |&(_, c)| c)
    }
}

impl Default for Crosshair {
    fn default() -> Self {
        Self::VALORANT_DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_parse_and_print() {
        let c: Rgba = "ef92bfff".parse().unwrap();
        assert_eq!(
            c,
            Rgba {
                r: 0xEF,
                g: 0x92,
                b: 0xBF,
                a: 0xFF
            }
        );
        assert_eq!(c.to_hex(), "EF92BFFF");
        assert_eq!("00FF00".parse::<Rgba>().unwrap(), Rgba::opaque(0, 255, 0));
        assert!("12345".parse::<Rgba>().is_err());
        assert!("GGGGGGGG".parse::<Rgba>().is_err());
        assert!("ÿÿÿ".parse::<Rgba>().is_err());
    }

    #[test]
    fn preset_index_wins_over_a_stale_custom_color() {
        let mut ch = Crosshair {
            color_index: 5,
            custom_color: Rgba::opaque(1, 2, 3),
            ..Crosshair::default()
        };
        assert_eq!(ch.color(), Rgba::opaque(0, 255, 255));
        ch.color_index = CUSTOM_COLOR_INDEX;
        assert_eq!(ch.color(), Rgba::opaque(1, 2, 3));
    }

    #[test]
    fn separate_vertical_length_applies_only_when_enabled() {
        let mut l = Lines {
            vertical_length_px: 9,
            ..Lines::INNER
        };
        assert_eq!(l.effective_vertical_length_px(), 6);
        l.separate_vertical = true;
        assert_eq!(l.effective_vertical_length_px(), 9);
    }

    #[test]
    fn serde_round_trips_through_toml() {
        let ch = Crosshair {
            color_index: CUSTOM_COLOR_INDEX,
            custom_color: Rgba {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            ..Crosshair::default()
        };
        let text = toml::to_string(&ch).unwrap();
        assert!(text.contains("custom_color = \"01020304\""), "{text}");
        assert_eq!(toml::from_str::<Crosshair>(&text).unwrap(), ch);
        assert_eq!(
            toml::from_str::<Crosshair>("").unwrap(),
            Crosshair::default()
        );
    }
}
