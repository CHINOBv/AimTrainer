//! The analysis result for one player and its plain-text rendering.

use crate::analysis::Analysis;
use crate::insight::Insight;

/// Riot's required legal notice for third-party products (docs/match-analysis.md).
pub const RIOT_NOTICE: &str = "CustomAimTrainer isn't endorsed by Riot Games and doesn't reflect the views or opinions of Riot Games or anyone officially involved in producing or managing Riot Games properties. Riot Games, and all associated properties are trademarks or registered trademarks of Riot Games, Inc.";

/// Who was analyzed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSummary {
    pub puuid: String,
    pub game_name: String,
    pub tag_line: String,
    pub team_id: String,
    pub character_id: String,
}

/// Post-match report for one player.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchReport {
    pub match_id: String,
    pub map_id: String,
    pub rounds: usize,
    /// `None` when the player's team is not in `teams`.
    pub team_won: Option<bool>,
    pub player: PlayerSummary,
    pub analysis: Analysis,
    pub insights: Vec<Insight>,
    /// The match's duels rebuilt as a drill (see `rebuild`), when enough deaths have a
    /// known distance.
    pub rebuilt_drill: Option<aim_core::scenario::ScenarioDef>,
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |v| format!("{:.1}%", v * 100.0))
}

impl MatchReport {
    /// Renders the report for a terminal.
    pub fn to_text(&self) -> String {
        let a = &self.analysis;
        let mut out = String::new();
        let mut line = |text: String| {
            out.push_str(&text);
            out.push('\n');
        };
        let result = match self.team_won {
            Some(true) => "won",
            Some(false) => "lost",
            None => "unknown",
        };
        line(format!(
            "Match {} on {} ({} rounds, {result})",
            self.match_id, self.map_id, self.rounds
        ));
        line(format!(
            "Player {}#{} ({})",
            self.player.game_name, self.player.tag_line, self.player.puuid
        ));
        line(String::new());
        let kd = a
            .kd_ratio()
            .map_or_else(|| "n/a".to_owned(), |v| format!("{v:.2}"));
        line(format!(
            "K/D/A: {}/{}/{} (K/D {kd})",
            a.kills, a.deaths, a.assists
        ));
        line(format!(
            "Headshot rate of hits: {} ({} head, {} body, {} legs; shots fired are not in the data)",
            percent(a.headshot_rate()),
            a.headshots,
            a.bodyshots,
            a.legshots
        ));
        for (label, counts) in [("Kills", a.kills_by_range), ("Deaths", a.deaths_by_range)] {
            line(format!(
                "{label} by range (approx.): close <10 m {}, mid 10-25 m {}, long >25 m {}, unknown {}",
                counts.close, counts.mid, counts.long, counts.unknown
            ));
        }
        line(format!(
            "Opening duels: won {}, lost {}",
            a.opening_duels.won, a.opening_duels.lost
        ));
        let first_kill = a
            .avg_first_kill_secs
            .map_or_else(|| "n/a".to_owned(), |s| format!("{s:.1} s"));
        line(format!(
            "Average time to first kill in a round: {first_kill}"
        ));
        if !a.weapon_usage.is_empty() {
            line("Kills by damage source:".to_owned());
            for weapon in &a.weapon_usage {
                line(format!(
                    "  {} {}: {}",
                    weapon.damage_type, weapon.item, weapon.kills
                ));
            }
        }
        line(String::new());
        if self.insights.is_empty() {
            line("No rule fired: no clear weakness in this match.".to_owned());
        } else {
            line("Suggested drills (provisional thresholds):".to_owned());
            for insight in &self.insights {
                line(format!(
                    "- [{}] {} = {:.2} (threshold {:.2}). {} Drills: {}",
                    insight.rule,
                    insight.metric,
                    insight.value,
                    insight.threshold,
                    insight.reason,
                    insight.drills.join(", ")
                ));
            }
        }
        line(String::new());
        match &self.rebuilt_drill {
            Some(d) => line(format!("Rebuilt drill '{}': {}", d.id, d.description)),
            None => line(format!(
                "No rebuilt drill: it needs {} deaths with a known distance.",
                crate::insight::MIN_DEATHS
            )),
        }
        line(String::new());
        line(RIOT_NOTICE.to_owned());
        out
    }
}
