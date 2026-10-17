//! Mouse sensitivity math. Everything is expressed as degrees of rotation per mouse count,
//! which is the only quantity that transfers exactly between games.

use serde::{Deserialize, Serialize};

const CM_PER_INCH: f64 = 2.54;

/// Games whose yaw constant (degrees per count at sensitivity 1.0) is known.
/// Sources are listed in `docs/valorant-mechanics.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Game {
    Valorant,
    Cs2,
    Apex,
    Overwatch2,
}

impl Game {
    pub const ALL: [Game; 4] = [Game::Valorant, Game::Cs2, Game::Apex, Game::Overwatch2];

    /// Degrees per count at sensitivity 1.0.
    pub fn yaw_deg_per_count(self) -> f64 {
        match self {
            Game::Valorant => 0.07,
            Game::Cs2 | Game::Apex => 0.022,
            Game::Overwatch2 => 0.0066,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Game::Valorant => "VALORANT",
            Game::Cs2 => "Counter-Strike 2",
            Game::Apex => "Apex Legends",
            Game::Overwatch2 => "Overwatch 2",
        }
    }
}

/// The player's mouse setup expressed in a given game's terms.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sensitivity {
    pub game: Game,
    pub value: f64,
    pub dpi: u32,
}

impl Sensitivity {
    pub fn valorant(value: f64, dpi: u32) -> Self {
        Self {
            game: Game::Valorant,
            value,
            dpi,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.value.is_finite() && self.value > 0.0 && self.dpi > 0
    }

    /// Degrees rotated per mouse count (hip-fire).
    pub fn deg_per_count(&self) -> f64 {
        self.game.yaw_deg_per_count() * self.value
    }

    /// Effective DPI (`dpi * sens`). Only comparable between setups of the same game.
    pub fn edpi(&self) -> f64 {
        self.dpi as f64 * self.value
    }

    /// Physical mouse travel for a full 360° turn.
    pub fn cm_per_360(&self) -> f64 {
        360.0 / (self.deg_per_count() * self.dpi as f64) * CM_PER_INCH
    }

    pub fn inches_per_360(&self) -> f64 {
        self.cm_per_360() / CM_PER_INCH
    }

    /// Same physical cm/360 expressed in another game (360-distance match).
    pub fn convert_to(&self, game: Game) -> Self {
        let value = self.deg_per_count() / game.yaw_deg_per_count();
        Self {
            game,
            value,
            dpi: self.dpi,
        }
    }

    /// Same cm/360 at a different DPI.
    pub fn with_dpi(&self, dpi: u32) -> Self {
        Self {
            game: self.game,
            value: self.value * self.dpi as f64 / dpi as f64,
            dpi,
        }
    }

    /// Sensitivity value that produces `cm` per 360 in `game` at `dpi`.
    pub fn from_cm_per_360(game: Game, cm: f64, dpi: u32) -> Self {
        let deg_per_count = 360.0 * CM_PER_INCH / (cm * dpi as f64);
        Self {
            game,
            value: deg_per_count / game.yaw_deg_per_count(),
            dpi,
        }
    }

    /// Degrees per count while zoomed, using VALORANT's scoped model:
    /// a multiplier of 1.0 keeps a 100% horizontal monitor match, i.e. the
    /// rotation is divided by the zoom factor.
    pub fn zoomed_deg_per_count(&self, zoom: f64, multiplier: f64) -> f64 {
        self.deg_per_count() * multiplier / zoom
    }
}

impl Default for Sensitivity {
    fn default() -> Self {
        // A common VALORANT pro-range setup (~41 cm/360).
        Self::valorant(0.4, 800)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn valorant_cm_per_360_matches_reference_values() {
        // 800 DPI @ 0.314 ≈ 52 cm (sensiconverter / mousesensitivityconverters).
        let s = Sensitivity::valorant(0.314, 800);
        assert!(close(s.cm_per_360(), 52.0018, 0.001), "{}", s.cm_per_360());
        // 1600 DPI @ 0.2 → 914.4 / (0.07*0.2*1600) = 40.82 cm.
        assert!(close(
            Sensitivity::valorant(0.2, 1600).cm_per_360(),
            40.821,
            0.001
        ));
    }

    #[test]
    fn edpi_is_dpi_times_sens() {
        assert!(close(Sensitivity::valorant(0.35, 800).edpi(), 280.0, 1e-9));
    }

    #[test]
    fn converts_valorant_to_cs2_with_the_3_1818_factor() {
        let cs = Sensitivity::valorant(0.5, 800).convert_to(Game::Cs2);
        assert!(close(cs.value, 0.5 * 0.07 / 0.022, 1e-9));
        assert!(close(cs.value / 0.5, 3.1818, 1e-4));
        assert!(close(
            cs.cm_per_360(),
            Sensitivity::valorant(0.5, 800).cm_per_360(),
            1e-9
        ));
    }

    #[test]
    fn cm_per_360_round_trips() {
        let s = Sensitivity::from_cm_per_360(Game::Valorant, 45.0, 1600);
        assert!(close(s.cm_per_360(), 45.0, 1e-9));
    }

    #[test]
    fn changing_dpi_preserves_cm_per_360() {
        let s = Sensitivity::valorant(0.4, 800);
        assert!(close(s.with_dpi(1600).cm_per_360(), s.cm_per_360(), 1e-9));
        assert!(close(s.with_dpi(1600).value, 0.2, 1e-12));
    }

    #[test]
    fn zoomed_rotation_divides_by_zoom_at_multiplier_one() {
        let s = Sensitivity::valorant(0.4, 800);
        assert!(close(
            s.zoomed_deg_per_count(1.25, 1.0),
            0.028 / 1.25,
            1e-12
        ));
    }
}
