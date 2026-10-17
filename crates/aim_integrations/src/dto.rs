//! Serde types for the subset of Riot's VAL-MATCH-V1 `MatchDto` that the analysis reads.
//!
//! Source: docs/research/riot-match-api.md. Parsing is tolerant on purpose: unknown fields
//! are ignored, and missing or `null` fields fall back to their default, because the
//! player supplies the file and Riot may add fields at any time.

use serde::{Deserialize, Deserializer};

/// Treats an explicit JSON `null` like a missing field.
fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// One match as returned by `GET /val/match/v1/matches/{matchId}`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchDto {
    #[serde(deserialize_with = "null_default")]
    pub match_info: MatchInfoDto,
    #[serde(deserialize_with = "null_default")]
    pub players: Vec<PlayerDto>,
    #[serde(deserialize_with = "null_default")]
    pub teams: Vec<TeamDto>,
    #[serde(deserialize_with = "null_default")]
    pub round_results: Vec<RoundResultDto>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchInfoDto {
    #[serde(deserialize_with = "null_default")]
    pub match_id: String,
    #[serde(deserialize_with = "null_default")]
    pub map_id: String,
    #[serde(deserialize_with = "null_default")]
    pub game_length_millis: u64,
    #[serde(deserialize_with = "null_default")]
    pub queue_id: String,
    #[serde(deserialize_with = "null_default")]
    pub is_completed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerDto {
    #[serde(deserialize_with = "null_default")]
    pub puuid: String,
    #[serde(deserialize_with = "null_default")]
    pub game_name: String,
    #[serde(deserialize_with = "null_default")]
    pub tag_line: String,
    #[serde(deserialize_with = "null_default")]
    pub team_id: String,
    #[serde(deserialize_with = "null_default")]
    pub character_id: String,
    #[serde(deserialize_with = "null_default")]
    pub stats: PlayerStatsDto,
}

/// Match totals for one player.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerStatsDto {
    #[serde(deserialize_with = "null_default")]
    pub score: u32,
    #[serde(deserialize_with = "null_default")]
    pub rounds_played: u32,
    #[serde(deserialize_with = "null_default")]
    pub kills: u32,
    #[serde(deserialize_with = "null_default")]
    pub deaths: u32,
    #[serde(deserialize_with = "null_default")]
    pub assists: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TeamDto {
    #[serde(deserialize_with = "null_default")]
    pub team_id: String,
    #[serde(deserialize_with = "null_default")]
    pub won: bool,
    #[serde(deserialize_with = "null_default")]
    pub rounds_played: u32,
    #[serde(deserialize_with = "null_default")]
    pub rounds_won: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RoundResultDto {
    #[serde(deserialize_with = "null_default")]
    pub round_num: u32,
    #[serde(deserialize_with = "null_default")]
    pub winning_team: String,
    #[serde(deserialize_with = "null_default")]
    pub player_stats: Vec<PlayerRoundStatsDto>,
}

/// What one player did in one round. Each kill is listed under its killer.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerRoundStatsDto {
    #[serde(deserialize_with = "null_default")]
    pub puuid: String,
    #[serde(deserialize_with = "null_default")]
    pub kills: Vec<KillDto>,
    #[serde(deserialize_with = "null_default")]
    pub damage: Vec<DamageDto>,
    #[serde(deserialize_with = "null_default")]
    pub economy: EconomyDto,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KillDto {
    #[serde(deserialize_with = "null_default")]
    pub time_since_round_start_millis: u64,
    #[serde(deserialize_with = "null_default")]
    pub killer: String,
    #[serde(deserialize_with = "null_default")]
    pub victim: String,
    /// Absent when Riot did not record it; the death distance is then unknown.
    #[serde(deserialize_with = "null_default")]
    pub victim_location: Option<LocationDto>,
    #[serde(deserialize_with = "null_default")]
    pub assistants: Vec<String>,
    #[serde(deserialize_with = "null_default")]
    pub player_locations: Vec<PlayerLocationDto>,
    #[serde(deserialize_with = "null_default")]
    pub finishing_damage: FinishingDamageDto,
}

/// A 2D map position in engine units (assumed centimeters; there is no height).
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct LocationDto {
    #[serde(deserialize_with = "null_default")]
    pub x: f64,
    #[serde(deserialize_with = "null_default")]
    pub y: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlayerLocationDto {
    #[serde(deserialize_with = "null_default")]
    pub puuid: String,
    #[serde(deserialize_with = "null_default")]
    pub view_radians: f64,
    #[serde(deserialize_with = "null_default")]
    pub location: Option<LocationDto>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FinishingDamageDto {
    /// `Weapon`, `Ability`, `Bomb`, ...
    #[serde(deserialize_with = "null_default")]
    pub damage_type: String,
    /// A weapon or ability id (usually a UUID).
    #[serde(deserialize_with = "null_default")]
    pub damage_item: String,
    #[serde(deserialize_with = "null_default")]
    pub is_secondary_fire_mode: bool,
}

/// Damage one player dealt to one receiver in a round, with hit counts per body zone.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct DamageDto {
    #[serde(deserialize_with = "null_default")]
    pub receiver: String,
    #[serde(deserialize_with = "null_default")]
    pub damage: u32,
    #[serde(deserialize_with = "null_default")]
    pub legshots: u32,
    #[serde(deserialize_with = "null_default")]
    pub bodyshots: u32,
    #[serde(deserialize_with = "null_default")]
    pub headshots: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EconomyDto {
    #[serde(deserialize_with = "null_default")]
    pub loadout_value: u32,
    #[serde(deserialize_with = "null_default")]
    pub weapon: String,
    #[serde(deserialize_with = "null_default")]
    pub armor: String,
    #[serde(deserialize_with = "null_default")]
    pub remaining: u32,
    #[serde(deserialize_with = "null_default")]
    pub spent: u32,
}
