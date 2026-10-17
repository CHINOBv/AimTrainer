//! Shareable player profile code: sensitivity, DPI, ADS multiplier and crosshair in one
//! line, to move a setup between PCs or share it with a friend.
//!
//! Format (version 1): `CAT1|<game>|<sens>|<dpi>|<ads multiplier>|<VALORANT crosshair code>`.
//! `|` separates fields because crosshair codes already use `;`.

use crate::crosshair::{Crosshair, code};
use crate::sensitivity::{Game, Sensitivity};
use serde::Deserialize;
use thiserror::Error;

const PREFIX: &str = "CAT1";

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub sensitivity: Sensitivity,
    pub ads_multiplier: f64,
    pub crosshair: Crosshair,
}

#[derive(Debug, Error, PartialEq)]
pub enum ProfileError {
    #[error("not a profile code (it should start with {PREFIX}|)")]
    NotAProfile,
    #[error("profile code needs 6 fields separated by |")]
    FieldCount,
    #[error("unknown game '{0}'")]
    Game(String),
    #[error("invalid {field}: '{value}'")]
    Number { field: &'static str, value: String },
    #[error("crosshair: {0}")]
    Crosshair(String),
}

fn game_id(game: Game) -> String {
    match toml::Value::try_from(game) {
        Ok(toml::Value::String(s)) => s,
        _ => format!("{game:?}").to_lowercase(),
    }
}

fn parse_game(id: &str) -> Result<Game, ProfileError> {
    Game::deserialize(toml::Value::String(id.to_owned()))
        .map_err(|_| ProfileError::Game(id.to_owned()))
}

impl Profile {
    pub fn encode(&self) -> String {
        let s = &self.sensitivity;
        format!(
            "{PREFIX}|{}|{}|{}|{}|{}",
            game_id(s.game),
            s.value,
            s.dpi,
            self.ads_multiplier,
            code::encode(&self.crosshair)
        )
    }

    pub fn decode(text: &str) -> Result<Self, ProfileError> {
        let text = text.trim();
        let fields: Vec<&str> = text.splitn(6, '|').collect();
        if fields.first() != Some(&PREFIX) {
            return Err(ProfileError::NotAProfile);
        }
        let [_, game, sens, dpi, ads, crosshair] = fields[..] else {
            return Err(ProfileError::FieldCount);
        };
        let number = |field: &'static str, v: &str| -> Result<f64, ProfileError> {
            v.parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() && *x > 0.0)
                .ok_or(ProfileError::Number {
                    field,
                    value: v.to_owned(),
                })
        };
        let dpi = dpi
            .parse::<u32>()
            .ok()
            .filter(|d| *d > 0)
            .ok_or(ProfileError::Number {
                field: "dpi",
                value: dpi.to_owned(),
            })?;
        let sensitivity = Sensitivity {
            game: parse_game(game)?,
            value: number("sensitivity", sens)?,
            dpi,
        };
        Ok(Self {
            sensitivity,
            ads_multiplier: number("ads multiplier", ads)?,
            crosshair: code::parse(crosshair)
                .map_err(|e| ProfileError::Crosshair(e.to_string()))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Profile {
        Profile {
            sensitivity: Sensitivity::valorant(0.35, 1600),
            ads_multiplier: 0.87,
            crosshair: code::parse("0;P;c;5;h;0;f;0;0l;4;0o;2;0a;1;0f;0;1b;0").unwrap(),
        }
    }

    #[test]
    fn round_trips_every_game_and_the_crosshair() {
        for game in Game::ALL {
            let mut p = sample();
            p.sensitivity = p.sensitivity.convert_to(game);
            let code = p.encode();
            assert!(code.starts_with("CAT1|"), "{code}");
            assert_eq!(Profile::decode(&code), Ok(p), "{code}");
        }
        assert_eq!(
            sample().encode(),
            "CAT1|valorant|0.35|1600|0.87|0;P;c;5;h;0;f;0;0l;4;0o;2;0a;1;0f;0;1b;0"
        );
    }

    #[test]
    fn rejects_bad_codes_with_a_reason() {
        assert_eq!(Profile::decode("hello"), Err(ProfileError::NotAProfile));
        assert_eq!(
            Profile::decode("CAT1|valorant|0.4"),
            Err(ProfileError::FieldCount)
        );
        assert_eq!(
            Profile::decode("CAT1|quake|0.4|800|1|0"),
            Err(ProfileError::Game("quake".into()))
        );
        assert!(matches!(
            Profile::decode("CAT1|valorant|-1|800|1|0"),
            Err(ProfileError::Number {
                field: "sensitivity",
                ..
            })
        ));
        assert!(matches!(
            Profile::decode("CAT1|valorant|0.4|0|1|0"),
            Err(ProfileError::Number { field: "dpi", .. })
        ));
        assert!(matches!(
            Profile::decode("CAT1|valorant|0.4|800|1|x;P"),
            Err(ProfileError::Crosshair(_))
        ));
        assert!(
            Profile::decode("  CAT1|valorant|0.4|800|1|0  ").is_ok(),
            "surrounding whitespace is fine"
        );
    }
}
