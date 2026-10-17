//! Per-player numbers computed from one match.
//!
//! The data has hits but no shots fired, so nothing here is an accuracy figure.

use std::collections::BTreeMap;

use crate::dto::{KillDto, LocationDto, MatchDto};

/// Engine units per meter. Riot does not document the unit; Unreal Engine uses
/// centimeters, so distances are approximate (see docs/match-analysis.md).
pub const UNITS_PER_METER: f64 = 100.0;
/// Below this 2D distance an engagement is close range.
pub const CLOSE_MAX_M: f64 = 10.0;
/// Above this 2D distance an engagement is long range; in between is mid range.
pub const MID_MAX_M: f64 = 25.0;

/// Engagement distance class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    /// Under 10 m.
    Close,
    /// 10 m to 25 m inclusive.
    Mid,
    /// Over 25 m.
    Long,
}

impl Range {
    pub fn of(distance_m: f64) -> Self {
        if distance_m < CLOSE_MAX_M {
            Self::Close
        } else if distance_m <= MID_MAX_M {
            Self::Mid
        } else {
            Self::Long
        }
    }
}

/// Engagements per distance class. `unknown` counts kills without both positions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RangeCounts {
    pub close: u32,
    pub mid: u32,
    pub long: u32,
    pub unknown: u32,
}

impl RangeCounts {
    /// Engagements with a known distance.
    pub fn known(&self) -> u32 {
        self.close + self.mid + self.long
    }

    fn add(&mut self, distance_m: Option<f64>) {
        match distance_m.map(Range::of) {
            Some(Range::Close) => self.close += 1,
            Some(Range::Mid) => self.mid += 1,
            Some(Range::Long) => self.long += 1,
            None => self.unknown += 1,
        }
    }
}

/// The first kill of each round in which the player was the killer or the victim.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpeningDuels {
    pub won: u32,
    pub lost: u32,
}

impl OpeningDuels {
    pub fn total(&self) -> u32 {
        self.won + self.lost
    }
}

/// Kills the player finished with one damage source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaponUse {
    /// `Weapon`, `Ability`, `Bomb`, ... as Riot reports it.
    pub damage_type: String,
    /// Riot's item id (usually a UUID); names are not in the match data.
    pub item: String,
    pub kills: u32,
}

/// What one player did in one match.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Analysis {
    /// Match totals from `players[].stats`.
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    /// Hits dealt per body zone, summed over every round.
    pub headshots: u32,
    pub bodyshots: u32,
    pub legshots: u32,
    /// 2D killer-to-victim distance of each kill and death, in meters, in match order.
    /// `None` when a position is missing.
    pub kill_distances_m: Vec<Option<f64>>,
    pub death_distances_m: Vec<Option<f64>>,
    pub kills_by_range: RangeCounts,
    pub deaths_by_range: RangeCounts,
    pub opening_duels: OpeningDuels,
    /// Sorted by kills, most first, then by item id.
    pub weapon_usage: Vec<WeaponUse>,
    /// Mean time from round start to the player's first kill, over rounds with a kill.
    pub avg_first_kill_secs: Option<f64>,
}

impl Analysis {
    /// Hits dealt in any zone.
    pub fn hits(&self) -> u32 {
        self.headshots + self.bodyshots + self.legshots
    }

    /// Share of hits that were headshots, in 0..=1. This is not accuracy: misses are
    /// not in the data.
    pub fn headshot_rate(&self) -> Option<f64> {
        let hits = self.hits();
        (hits > 0).then(|| f64::from(self.headshots) / f64::from(hits))
    }

    pub fn kd_ratio(&self) -> Option<f64> {
        (self.deaths > 0).then(|| f64::from(self.kills) / f64::from(self.deaths))
    }

    pub(crate) fn compute(
        match_dto: &MatchDto,
        puuid: &str,
        kills: u32,
        deaths: u32,
        assists: u32,
    ) -> Self {
        let mut analysis = Self {
            kills,
            deaths,
            assists,
            ..Self::default()
        };
        let mut weapons: BTreeMap<(String, String), u32> = BTreeMap::new();
        let mut first_kill_secs = Vec::new();

        for round in &match_dto.round_results {
            for stats in round.player_stats.iter().filter(|s| s.puuid == puuid) {
                for hit in &stats.damage {
                    analysis.headshots += hit.headshots;
                    analysis.bodyshots += hit.bodyshots;
                    analysis.legshots += hit.legshots;
                }
            }

            let round_kills: Vec<&KillDto> =
                round.player_stats.iter().flat_map(|s| &s.kills).collect();

            let opening = round_kills
                .iter()
                .min_by_key(|k| k.time_since_round_start_millis);
            if let Some(opening) = opening {
                if opening.killer == puuid {
                    analysis.opening_duels.won += 1;
                } else if opening.victim == puuid {
                    analysis.opening_duels.lost += 1;
                }
            }

            let mut own_kills: Vec<&KillDto> = round_kills
                .iter()
                .copied()
                .filter(|k| k.killer == puuid)
                .collect();
            own_kills.sort_by_key(|k| k.time_since_round_start_millis);
            if let Some(first) = own_kills.first() {
                first_kill_secs.push(first.time_since_round_start_millis as f64 / 1000.0);
            }
            for kill in own_kills {
                let distance = kill_distance_m(kill);
                analysis.kill_distances_m.push(distance);
                analysis.kills_by_range.add(distance);
                let key = (
                    kill.finishing_damage.damage_type.clone(),
                    kill.finishing_damage.damage_item.clone(),
                );
                *weapons.entry(key).or_default() += 1;
            }

            let mut own_deaths: Vec<&KillDto> = round_kills
                .into_iter()
                .filter(|k| k.victim == puuid)
                .collect();
            own_deaths.sort_by_key(|k| k.time_since_round_start_millis);
            for death in own_deaths {
                let distance = kill_distance_m(death);
                analysis.death_distances_m.push(distance);
                analysis.deaths_by_range.add(distance);
            }
        }

        let mut usage: Vec<WeaponUse> = weapons
            .into_iter()
            .map(|((damage_type, item), kills)| WeaponUse {
                damage_type,
                item,
                kills,
            })
            .collect();
        usage.sort_by(|a, b| b.kills.cmp(&a.kills).then_with(|| a.item.cmp(&b.item)));
        analysis.weapon_usage = usage;

        if !first_kill_secs.is_empty() {
            analysis.avg_first_kill_secs =
                Some(first_kill_secs.iter().sum::<f64>() / first_kill_secs.len() as f64);
        }
        analysis
    }
}

/// 2D distance between the killer (from `playerLocations`) and the victim, in meters.
fn kill_distance_m(kill: &KillDto) -> Option<f64> {
    let victim = kill.victim_location?;
    let killer: LocationDto = kill
        .player_locations
        .iter()
        .find(|p| p.puuid == kill.killer)?
        .location?;
    Some((killer.x - victim.x).hypot(killer.y - victim.y) / UNITS_PER_METER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_boundaries_follow_the_documented_buckets() {
        assert_eq!(Range::of(9.99), Range::Close);
        assert_eq!(Range::of(10.0), Range::Mid);
        assert_eq!(Range::of(25.0), Range::Mid);
        assert_eq!(Range::of(25.01), Range::Long);
    }
}
