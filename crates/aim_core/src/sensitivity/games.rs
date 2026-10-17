//! Data-driven game profiles: yaw constant and field-of-view model per game.
//! Every value and its confidence tag is listed in `docs/valorant-mechanics.md`
//! (research notes in `docs/research/sensitivity-games.md`).

use serde::{Deserialize, Serialize};

/// Games whose yaw constant (degrees per count at sensitivity 1.0) is known.
///
/// The serialized names are stored in user settings, so they never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Game {
    Valorant,
    Cs2,
    Apex,
    Overwatch2,
    Fortnite,
    CallOfDuty,
    Destiny2,
    RainbowSixSiege,
    Deadlock,
}

/// Which screen axis a game's FOV setting measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FovAxis {
    Horizontal,
    Vertical,
}

/// How a game turns its FOV setting into the view on the player's monitor.
///
/// Every game in the converter is Hor+: the vertical FOV is locked and the
/// horizontal FOV grows with the aspect ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FovModel {
    /// Axis the in-game FOV value refers to.
    pub axis: FovAxis,
    /// Aspect ratio (width / height) at which the in-game value is measured.
    /// Only meaningful for [`FovAxis::Horizontal`].
    pub reference_aspect: f64,
    /// Default in-game FOV value, in degrees.
    pub default_deg: f64,
    /// Whether the player can change the FOV in game.
    pub adjustable: bool,
    /// Displays wider than this are pillarboxed to it (VALORANT caps at 16:9).
    pub max_aspect: Option<f64>,
}

/// Everything the converter knows about one game.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameProfile {
    pub display_name: &'static str,
    /// Degrees per count at sensitivity 1.0.
    pub yaw_deg_per_count: f64,
    /// `None` when no reliable FOV model is documented; only 360° matching works then.
    pub fov: Option<FovModel>,
    /// What the sensitivity value means in game, shown next to the input.
    pub value_note: &'static str,
}

const ASPECT_16_9: f64 = 16.0 / 9.0;
const ASPECT_4_3: f64 = 4.0 / 3.0;

const fn horizontal(reference_aspect: f64, default_deg: f64, adjustable: bool) -> FovModel {
    FovModel {
        axis: FovAxis::Horizontal,
        reference_aspect,
        default_deg,
        adjustable,
        max_aspect: None,
    }
}

const VALORANT: GameProfile = GameProfile {
    display_name: "VALORANT",
    yaw_deg_per_count: 0.07,
    fov: Some(FovModel {
        max_aspect: Some(ASPECT_16_9),
        ..horizontal(ASPECT_16_9, 103.0, false)
    }),
    value_note: "Sensitivity",
};

const CS2: GameProfile = GameProfile {
    display_name: "Counter-Strike 2",
    yaw_deg_per_count: 0.022,
    fov: Some(horizontal(ASPECT_4_3, 90.0, false)),
    value_note: "sensitivity",
};

const APEX: GameProfile = GameProfile {
    display_name: "Apex Legends",
    yaw_deg_per_count: 0.022,
    fov: Some(horizontal(ASPECT_4_3, 90.0, true)),
    value_note: "Mouse sensitivity; FOV is the in-game slider (cl_fovScale × 70)",
};

const OVERWATCH2: GameProfile = GameProfile {
    display_name: "Overwatch 2",
    yaw_deg_per_count: 0.0066,
    fov: Some(horizontal(ASPECT_16_9, 103.0, true)),
    value_note: "Sensitivity",
};

const FORTNITE: GameProfile = GameProfile {
    display_name: "Fortnite",
    yaw_deg_per_count: 0.005555,
    fov: Some(horizontal(ASPECT_16_9, 80.0, false)),
    value_note: "X/Y sensitivity in % (e.g. 6.5); FOV is approximate",
};

const CALL_OF_DUTY: GameProfile = GameProfile {
    display_name: "Call of Duty (MW2019–BO6)",
    yaw_deg_per_count: 0.0066,
    fov: Some(horizontal(ASPECT_16_9, 80.0, true)),
    value_note: "Mouse sensitivity",
};

const DESTINY2: GameProfile = GameProfile {
    display_name: "Destiny 2",
    yaw_deg_per_count: 0.0066,
    fov: Some(horizontal(ASPECT_16_9, 85.0, true)),
    value_note: "Look sensitivity",
};

const RAINBOW_SIX_SIEGE: GameProfile = GameProfile {
    display_name: "Rainbow Six Siege",
    yaw_deg_per_count: 0.00572958,
    fov: Some(FovModel {
        axis: FovAxis::Vertical,
        reference_aspect: ASPECT_16_9,
        default_deg: 60.0,
        adjustable: true,
        max_aspect: None,
    }),
    value_note: "Hip-fire slider with the default MouseSensitivityMultiplierUnit 0.02",
};

const DEADLOCK: GameProfile = GameProfile {
    display_name: "Deadlock",
    yaw_deg_per_count: 0.044,
    fov: None,
    value_note: "Sensitivity; FOV model unknown, 360° matching only",
};

impl Game {
    pub const ALL: [Game; 9] = [
        Game::Valorant,
        Game::Cs2,
        Game::Apex,
        Game::Overwatch2,
        Game::Fortnite,
        Game::CallOfDuty,
        Game::Destiny2,
        Game::RainbowSixSiege,
        Game::Deadlock,
    ];

    /// The game's documented data.
    pub fn profile(self) -> &'static GameProfile {
        match self {
            Game::Valorant => &VALORANT,
            Game::Cs2 => &CS2,
            Game::Apex => &APEX,
            Game::Overwatch2 => &OVERWATCH2,
            Game::Fortnite => &FORTNITE,
            Game::CallOfDuty => &CALL_OF_DUTY,
            Game::Destiny2 => &DESTINY2,
            Game::RainbowSixSiege => &RAINBOW_SIX_SIEGE,
            Game::Deadlock => &DEADLOCK,
        }
    }

    /// Degrees per count at sensitivity 1.0.
    pub fn yaw_deg_per_count(self) -> f64 {
        self.profile().yaw_deg_per_count
    }

    pub fn display_name(self) -> &'static str {
        self.profile().display_name
    }

    /// FOV model, if one is documented.
    pub fn fov_model(self) -> Option<FovModel> {
        self.profile().fov
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_names_stay_backward_compatible() {
        #[derive(Serialize, Deserialize)]
        struct Wrap {
            game: Game,
        }
        let names: Vec<String> = Game::ALL
            .iter()
            .map(|&game| {
                toml::to_string(&Wrap { game })
                    .unwrap()
                    .trim()
                    .trim_start_matches("game = ")
                    .trim_matches('"')
                    .to_owned()
            })
            .collect();
        assert_eq!(
            names,
            [
                "valorant",
                "cs2",
                "apex",
                "overwatch2",
                "fortnite",
                "call_of_duty",
                "destiny2",
                "rainbow_six_siege",
                "deadlock"
            ]
        );
        let old: Wrap = toml::from_str("game = \"overwatch2\"").unwrap();
        assert_eq!(old.game, Game::Overwatch2);
    }

    #[test]
    fn original_yaw_constants_are_unchanged() {
        assert_eq!(Game::Valorant.yaw_deg_per_count(), 0.07);
        assert_eq!(Game::Cs2.yaw_deg_per_count(), 0.022);
        assert_eq!(Game::Apex.yaw_deg_per_count(), 0.022);
        assert_eq!(Game::Overwatch2.yaw_deg_per_count(), 0.0066);
    }

    #[test]
    fn every_game_has_a_positive_yaw_and_a_sane_fov() {
        for game in Game::ALL {
            assert!(game.yaw_deg_per_count() > 0.0, "{game:?}");
            if let Some(fov) = game.fov_model() {
                assert!(fov.default_deg > 0.0 && fov.default_deg < 180.0, "{game:?}");
                assert!(fov.reference_aspect > 0.0, "{game:?}");
            }
        }
    }
}
