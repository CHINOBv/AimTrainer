//! Post-match analysis from official match data the player provides (offline import).
//!
//! The input is a Riot VAL-MATCH-V1 `MatchDto` JSON file. This crate makes no network
//! calls, ships no API key and never reads the game process; it only analyzes a finished
//! match. See docs/match-analysis.md.

mod analysis;
mod dto;
mod insight;
mod report;
mod select;

pub use analysis::{
    Analysis, CLOSE_MAX_M, MID_MAX_M, OpeningDuels, Range, RangeCounts, UNITS_PER_METER, WeaponUse,
};
pub use dto::{
    DamageDto, EconomyDto, FinishingDamageDto, KillDto, LocationDto, MatchDto, MatchInfoDto,
    PlayerDto, PlayerLocationDto, PlayerRoundStatsDto, PlayerStatsDto, RoundResultDto, TeamDto,
};
pub use insight::{
    Insight, LOW_HEADSHOT_RATE, MIN_DEATHS, MIN_HITS, MIN_OPENING_DUELS, OPENING_LOST_SHARE,
    RANGE_DEATH_SHARE, recommend,
};
pub use report::{MatchReport, PlayerSummary, RIOT_NOTICE};
pub use select::PlayerSelector;

/// Why a match could not be analyzed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid match JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid player selector {0:?}: use a PUUID or gameName#tagLine")]
    InvalidSelector(String),
    #[error("player {0} is not in this match")]
    PlayerNotFound(String),
}

/// Parses a `MatchDto` JSON document.
pub fn parse_match(json: &str) -> Result<MatchDto, Error> {
    Ok(serde_json::from_str(json)?)
}

/// Analyzes one player's match and suggests drills.
pub fn analyze(match_dto: &MatchDto, player: &PlayerSelector) -> Result<MatchReport, Error> {
    let found = player.find(match_dto)?;
    let stats = &found.stats;
    let analysis = Analysis::compute(
        match_dto,
        &found.puuid,
        stats.kills,
        stats.deaths,
        stats.assists,
    );
    let insights = recommend(&analysis);
    let team_won = match_dto
        .teams
        .iter()
        .find(|t| t.team_id == found.team_id)
        .map(|t| t.won);
    Ok(MatchReport {
        match_id: match_dto.match_info.match_id.clone(),
        map_id: match_dto.match_info.map_id.clone(),
        rounds: match_dto.round_results.len(),
        team_won,
        player: PlayerSummary {
            puuid: found.puuid.clone(),
            game_name: found.game_name.clone(),
            tag_line: found.tag_line.clone(),
            team_id: found.team_id.clone(),
            character_id: found.character_id.clone(),
        },
        analysis,
        insights,
    })
}

/// Parses `json` and analyzes the player given as a PUUID or `gameName#tagLine`.
pub fn analyze_json(json: &str, player: &str) -> Result<MatchReport, Error> {
    let selector = PlayerSelector::parse(player)?;
    analyze(&parse_match(json)?, &selector)
}
