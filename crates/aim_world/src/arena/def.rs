//! Arena definition: the TOML format, loading and semantic validation.

use crate::map::{MapDef, SpawnKind};
use aim_core::games::valorant::Weapon;
use aim_core::math::Range;
use aim_core::scenario::ValidationError;
use aim_core::sim::AimerProfile;
use serde::{Deserialize, Serialize};

/// Most bots a deathmatch arena may ask for.
pub const MAX_BOTS: u32 = 16;
/// Most attackers a site objective may send in one run.
pub const MAX_ATTACKERS: u32 = 64;

/// An arena: an activity played on a map against bots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaDef {
    /// Unique, stable identifier (`[a-z0-9_-]`); equals the file name without `.toml`.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Id of the map the arena is played on.
    pub map: String,
    pub duration_secs: f32,
    /// The player's weapon: fire rate, spread, damage and movement profile.
    pub weapon: Weapon,
    pub mode: ArenaMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArenaMode {
    /// Bots roam the map's waypoints, stop to fight when they see the player and respawn
    /// after `respawn_secs`.
    Deathmatch {
        bots: u32,
        /// One of the `sim::AimerProfile::PRESETS` names.
        bot_skill: String,
        respawn_secs: f32,
        bots_shoot_back: bool,
        /// Bots strafe (ADAD) while they fight and only fire when slow enough to be
        /// accurate. Off by default: bots stop to fight.
        #[serde(default)]
        bot_strafe: bool,
        /// Turns the deathmatch into a site hold: bots push toward a site instead of
        /// roaming, and the run is lost if one of them captures it.
        #[serde(default)]
        objective: Option<SiteObjective>,
        /// Bots land a sure shot on a player who has not killed them within a deadline.
        #[serde(default)]
        deadline: Option<ShootBackDeadline>,
        /// Bots react to the player: they copy or oppose the player's strafe and juke when
        /// the crosshair gets close or the player fires. Reactive bots strafe while they
        /// fight, as with `bot_strafe`.
        #[serde(default)]
        reactive: Option<ReactiveBots>,
        /// Conditional behavior of roaming bots: the first rule whose conditions all hold
        /// overrides the bot's behavior and speed this frame. Not with a site objective.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rules: Vec<BehaviorRule>,
    },
    /// One bot per angle marker, in order. Each bot waits hidden, peeks when the player can
    /// see the marker, holds for `hold_secs` and hides again until it is killed.
    AngleClear {
        /// Angle marker ids of the map, cleared in this order.
        angles: Vec<String>,
        hold_secs: f32,
        /// Seconds from the bot seeing the player to the start of its flick.
        bot_reaction_secs: Range,
        bots_shoot_back: bool,
        /// Delay from the player seeing the angle to the bot peeking it, seconds. Default
        /// [`crate::arena::PEEK_DELAY_SECS`]. A short delay with a short `hold_secs` makes
        /// jiggle peeks.
        #[serde(default)]
        peek_delay_secs: Option<Range>,
        /// When set, a bot life counts as pre-aimed if the crosshair was within this many
        /// degrees of its head at first sight, and each one scores
        /// [`crate::arena::PRE_AIM_POINTS`].
        #[serde(default)]
        pre_aim_tolerance_deg: Option<f32>,
        /// Bots land a sure shot on a player who has not killed them within a deadline.
        #[serde(default)]
        deadline: Option<ShootBackDeadline>,
    },
}

/// Most behavior rules an arena may list.
pub const MAX_RULES: usize = 16;

/// One conditional behavior of roaming deathmatch bots: when every condition set in `when`
/// holds (an empty `when` always holds), the bot uses `behavior` and moves at
/// `speed_multiplier` times its normal speed. Rules are checked in order every frame and the
/// first match wins. See `docs/maps.md`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorRule {
    #[serde(default)]
    pub when: RuleConditions,
    /// The behavior to switch to; `None` keeps the normal one (only the speed changes).
    #[serde(default)]
    pub behavior: Option<BotBehavior>,
    /// Scales the bot's run speed and acceleration, in (0, 2]. Default 1.
    #[serde(default)]
    pub speed_multiplier: Option<f32>,
}

/// Conditions of a [`BehaviorRule`]. Unset conditions are ignored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleConditions {
    /// Health below this fraction of [`crate::arena::BOT_HEALTH`], in (0, 1].
    #[serde(default)]
    pub health_below: Option<f32>,
    /// The player hit the bot within the last this many seconds.
    #[serde(default)]
    pub damaged_within_secs: Option<f32>,
    /// The bot has been alive (since its spawn) at least this many seconds.
    #[serde(default)]
    pub alive_over_secs: Option<f32>,
    /// Whether the bot has line of sight to the player's head (`MapDef::visible_head`).
    #[serde(default)]
    pub sees_player: Option<bool>,
    /// Horizontal distance to the player below this many meters.
    #[serde(default)]
    pub distance_below_m: Option<f32>,
    /// Horizontal distance to the player above this many meters.
    #[serde(default)]
    pub distance_above_m: Option<f32>,
    /// The player moves faster than this, m/s.
    #[serde(default)]
    pub player_speed_above: Option<f32>,
    /// The player moves slower than this, m/s.
    #[serde(default)]
    pub player_speed_below: Option<f32>,
}

/// The behavior a [`BehaviorRule`] switches a roaming bot to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotBehavior {
    /// Stand still and hold the angle: no roaming, no strafing, no falling back; fight from
    /// where it stands.
    Hold,
    /// Walk to where it last saw the player (or was shot from) until it sees them again,
    /// then stop to fight.
    Peek,
    /// Strafe (ADAD) while it fights instead of falling back when shot.
    Strafe,
    /// Fall back to cover out of sight of where it last saw the player and stay hidden
    /// until it sees the player again; with no cover in reach it holds its ground.
    Retreat,
}

fn validate_rules(rules: &[BehaviorRule], push: &mut impl FnMut(&'static str, String)) {
    if rules.len() > MAX_RULES {
        push("mode.rules", format!("at most {MAX_RULES} rules"));
    }
    for (i, r) in rules.iter().enumerate() {
        let mut bad = |what: &str, need: &str| {
            push("mode.rules", format!("rule {}: {what} {need}", i + 1));
        };
        if r.behavior.is_none() && r.speed_multiplier.is_none() {
            bad("needs", "a behavior or a speed_multiplier");
        }
        if let Some(m) = r.speed_multiplier
            && !(m.is_finite() && m > 0.0 && m <= 2.0)
        {
            bad("speed_multiplier", "must be in (0, 2]");
        }
        let w = &r.when;
        if let Some(h) = w.health_below
            && !(h.is_finite() && h > 0.0 && h <= 1.0)
        {
            bad("when.health_below", "must be in (0, 1]");
        }
        if let Some(s) = w.damaged_within_secs
            && !(s.is_finite() && s > 0.0)
        {
            bad("when.damaged_within_secs", "must be > 0");
        }
        let non_negative = [
            ("when.alive_over_secs", w.alive_over_secs),
            ("when.distance_below_m", w.distance_below_m),
            ("when.distance_above_m", w.distance_above_m),
            ("when.player_speed_above", w.player_speed_above),
            ("when.player_speed_below", w.player_speed_below),
        ];
        for (name, v) in non_negative {
            if let Some(v) = v
                && !(v.is_finite() && v >= 0.0)
            {
                bad(name, "must be >= 0");
            }
        }
    }
}

/// Reactions of deathmatch bots to the player while they fight. Every reaction changes the
/// bot's strafe (ADAD), so reactive bots always strafe while they fight. See `docs/maps.md`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactiveBots {
    /// Chance, at each strafe-key decision, that the bot strafes the same world direction
    /// as the player is strafing (copies the player's ADAD).
    #[serde(default)]
    pub mirror_strafe_chance: f32,
    /// Chance, at each strafe-key decision, that the bot strafes against the player's
    /// direction. `mirror_strafe_chance + oppose_strafe_chance` is at most 1.
    #[serde(default)]
    pub oppose_strafe_chance: f32,
    /// When the player's crosshair comes within this many degrees of the bot's head, the
    /// bot jukes (switches strafe direction) after its reaction time.
    #[serde(default)]
    pub dodge_within_deg: Option<f32>,
    /// Chance of a juke each time the crosshair comes that close. Default 1.
    #[serde(default = "always")]
    pub dodge_chance: f32,
    /// A bot that sees the player fire jukes after its reaction time.
    #[serde(default)]
    pub react_to_shots: bool,
}

fn always() -> f32 {
    1.0
}

fn validate_reactive(r: &ReactiveBots, push: &mut impl FnMut(&'static str, String)) {
    let chance = |c: f32| c.is_finite() && (0.0..=1.0).contains(&c);
    if !chance(r.mirror_strafe_chance) {
        push(
            "mode.reactive.mirror_strafe_chance",
            "must be in [0, 1]".into(),
        );
    }
    if !chance(r.oppose_strafe_chance) {
        push(
            "mode.reactive.oppose_strafe_chance",
            "must be in [0, 1]".into(),
        );
    }
    if r.mirror_strafe_chance + r.oppose_strafe_chance > 1.0 {
        push(
            "mode.reactive",
            "mirror_strafe_chance + oppose_strafe_chance must be at most 1".into(),
        );
    }
    if let Some(d) = r.dodge_within_deg
        && !(d.is_finite() && d > 0.0 && d < 90.0)
    {
        push(
            "mode.reactive.dodge_within_deg",
            "must be in (0, 90)".into(),
        );
    }
    if !(chance(r.dodge_chance) && r.dodge_chance > 0.0) {
        push("mode.reactive.dodge_chance", "must be in (0, 1]".into());
    }
    let strafes = r.mirror_strafe_chance > 0.0 || r.oppose_strafe_chance > 0.0;
    if !(strafes || r.dodge_within_deg.is_some() || r.react_to_shots) {
        push(
            "mode.reactive",
            "needs a strafe chance, dodge_within_deg or react_to_shots".into(),
        );
    }
}

/// Duel pressure: once the player first sees a bot life, they have `secs` to kill it. If
/// they do not, the bot lands one sure shot the first moment it has line of sight to the
/// targetable player: `damage` to the player and `penalty_points` off the score. Once per
/// bot life, independent of `bots_shoot_back`, never in practice. See `docs/maps.md`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShootBackDeadline {
    /// Seconds from the player first seeing a bot life to its sure shot.
    pub secs: f32,
    /// Damage the sure shot deals to the player.
    #[serde(default)]
    pub damage: f32,
    /// Points the sure shot costs.
    #[serde(default)]
    pub penalty_points: f32,
}

/// A game-like objective on top of a deathmatch: defend a site against a fixed number of
/// attackers. See `docs/maps.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteObjective {
    /// Waypoint id of the map that marks the center of the site.
    pub site: String,
    /// Radius of the site around its waypoint, meters.
    pub site_radius: f32,
    /// Seconds a living attacker must stay on the site, uninterrupted, to capture it.
    pub capture_secs: f32,
    /// Attackers sent in the run, at most `bots` alive at once. Killing every one of them
    /// holds the site and ends the run early.
    pub attackers: u32,
}

/// Suffix of the run key of a practice run (bots never shoot), so its scores never mix
/// with normal runs of the same arena.
pub const PRACTICE_SUFFIX: &str = "+practice";

/// `key` without the practice suffix, and whether it had one.
pub fn split_practice(key: &str) -> (&str, bool) {
    match key.strip_suffix(PRACTICE_SUFFIX) {
        Some(base) => (base, true),
        None => (key, false),
    }
}

impl ArenaMode {
    pub(super) fn set_bots_shoot_back(&mut self, on: bool) {
        match self {
            Self::Deathmatch {
                bots_shoot_back, ..
            }
            | Self::AngleClear {
                bots_shoot_back, ..
            } => *bots_shoot_back = on,
        }
    }

    pub fn bots_shoot_back(&self) -> bool {
        match self {
            Self::Deathmatch {
                bots_shoot_back, ..
            }
            | Self::AngleClear {
                bots_shoot_back, ..
            } => *bots_shoot_back,
        }
    }

    /// The shoot-back deadline, if the arena sets one.
    pub fn deadline(&self) -> Option<ShootBackDeadline> {
        match self {
            Self::Deathmatch { deadline, .. } | Self::AngleClear { deadline, .. } => *deadline,
        }
    }
}

fn validate_deadline(d: &ShootBackDeadline, push: &mut impl FnMut(&'static str, String)) {
    if !(d.secs.is_finite() && d.secs > 0.0) {
        push("mode.deadline.secs", "must be > 0".into());
    }
    if !(d.damage.is_finite() && d.damage >= 0.0) {
        push("mode.deadline.damage", "must be >= 0".into());
    }
    if !(d.penalty_points.is_finite() && d.penalty_points >= 0.0) {
        push("mode.deadline.penalty_points", "must be >= 0".into());
    }
    if d.damage == 0.0 && d.penalty_points == 0.0 {
        push(
            "mode.deadline",
            "needs damage or penalty_points above 0".into(),
        );
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ArenaError {
    #[error("invalid arena TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid arena: {}", join(.0))]
    Invalid(Vec<ValidationError>),
}

fn join(errs: &[ValidationError]) -> String {
    errs.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// The aimer preset with this name.
pub fn bot_profile(name: &str) -> Option<AimerProfile> {
    AimerProfile::PRESETS.into_iter().find(|p| p.name == name)
}

impl ArenaDef {
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    /// Parse and validate the definition on its own (without its map).
    pub fn load(src: &str) -> Result<Self, ArenaError> {
        let def = Self::from_toml(src)?;
        let errs = def.validate();
        if errs.is_empty() {
            Ok(def)
        } else {
            Err(ArenaError::Invalid(errs))
        }
    }

    /// Checks that need only the definition. Returns every problem found.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errs = Vec::new();
        let mut push = |field: &'static str, message: String| {
            errs.push(ValidationError { field, message });
        };
        if !valid_id(&self.id) {
            push(
                "id",
                "must be non-empty and only contain a-z, 0-9, '-' or '_'".into(),
            );
        }
        if self.name.trim().is_empty() {
            push("name", "must not be empty".into());
        }
        if !valid_id(&self.map) {
            push("map", "must be a valid map id".into());
        }
        if !(self.duration_secs.is_finite() && self.duration_secs > 0.0) {
            push("duration_secs", "must be > 0".into());
        }
        match &self.mode {
            ArenaMode::Deathmatch {
                bots,
                bot_skill,
                respawn_secs,
                objective,
                reactive,
                rules,
                ..
            } => {
                if let Some(r) = reactive {
                    validate_reactive(r, &mut push);
                }
                validate_rules(rules, &mut push);
                if !rules.is_empty() && objective.is_some() {
                    push(
                        "mode.rules",
                        "behavior rules do not apply to site attackers; drop [mode.objective] or the rules".into(),
                    );
                }
                if !(1..=MAX_BOTS).contains(bots) {
                    push("mode.bots", format!("must be in 1..={MAX_BOTS}"));
                }
                if bot_profile(bot_skill).is_none() {
                    let names: Vec<&str> = AimerProfile::PRESETS.iter().map(|p| p.name).collect();
                    push(
                        "mode.bot_skill",
                        format!("'{bot_skill}' is not one of {}", names.join(", ")),
                    );
                }
                if !(respawn_secs.is_finite() && *respawn_secs >= 0.0) {
                    push("mode.respawn_secs", "must be >= 0".into());
                }
                if let Some(o) = objective {
                    if !valid_id(&o.site) {
                        push("mode.objective.site", "must be a valid waypoint id".into());
                    }
                    if !(o.site_radius.is_finite() && o.site_radius > 0.0) {
                        push("mode.objective.site_radius", "must be > 0".into());
                    }
                    if !(o.capture_secs.is_finite() && o.capture_secs > 0.0) {
                        push("mode.objective.capture_secs", "must be > 0".into());
                    }
                    if !(1..=MAX_ATTACKERS).contains(&o.attackers) {
                        push(
                            "mode.objective.attackers",
                            format!("must be in 1..={MAX_ATTACKERS}"),
                        );
                    }
                }
            }
            ArenaMode::AngleClear {
                angles,
                hold_secs,
                bot_reaction_secs,
                peek_delay_secs,
                pre_aim_tolerance_deg,
                ..
            } => {
                if angles.is_empty() {
                    push("mode.angles", "needs at least one angle marker id".into());
                }
                if !(hold_secs.is_finite() && *hold_secs > 0.0) {
                    push("mode.hold_secs", "must be > 0".into());
                }
                if !(bot_reaction_secs.is_valid() && bot_reaction_secs.min >= 0.0) {
                    push(
                        "mode.bot_reaction_secs",
                        "must be [min, max] with 0 <= min <= max".into(),
                    );
                }
                if let Some(r) = peek_delay_secs
                    && !(r.is_valid() && r.min >= 0.0)
                {
                    push(
                        "mode.peek_delay_secs",
                        "must be [min, max] with 0 <= min <= max".into(),
                    );
                }
                if let Some(t) = pre_aim_tolerance_deg
                    && !(t.is_finite() && *t > 0.0 && *t < 180.0)
                {
                    push("mode.pre_aim_tolerance_deg", "must be in (0, 180)".into());
                }
            }
        }
        if let Some(d) = self.mode.deadline() {
            validate_deadline(&d, &mut push);
        }
        errs
    }

    /// [`Self::validate`] plus the checks against the map the arena is played on.
    pub fn validate_with_map(&self, map: &MapDef) -> Vec<ValidationError> {
        let mut errs = self.validate();
        let mut push = |field: &'static str, message: String| {
            errs.push(ValidationError { field, message });
        };
        if map.id != self.map {
            push(
                "map",
                format!("arena wants map '{}', got '{}'", self.map, map.id),
            );
        }
        match &self.mode {
            ArenaMode::Deathmatch { objective, .. } => {
                if map.spawns_of(SpawnKind::Bot).next().is_none() {
                    push("map", "deathmatch needs at least one bot spawn".into());
                }
                if let Some(o) = objective
                    && !map.waypoints.iter().any(|w| w.id == o.site)
                {
                    push(
                        "mode.objective.site",
                        format!("map '{}' has no waypoint '{}'", map.id, o.site),
                    );
                }
            }
            ArenaMode::AngleClear { angles, .. } => {
                for id in angles {
                    if !map.angles.iter().any(|a| &a.id == id) {
                        push(
                            "mode.angles",
                            format!("map '{}' has no angle '{id}'", map.id),
                        );
                    }
                }
            }
        }
        errs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROKEN: &str = r#"
id = "Bad"
name = ""
map = "training-yard"
duration_secs = 0.0
weapon = "vandal"

[mode]
kind = "deathmatch"
bots = 0
bot_skill = "godlike"
respawn_secs = -1.0
bots_shoot_back = true
"#;

    #[test]
    fn validation_reports_every_problem() {
        let errs = ArenaDef::from_toml(BROKEN).unwrap().validate();
        let fields: Vec<&str> = errs.iter().map(|e| e.field).collect();
        for f in [
            "id",
            "name",
            "duration_secs",
            "mode.bots",
            "mode.bot_skill",
            "mode.respawn_secs",
        ] {
            assert!(fields.contains(&f), "missing {f}: {errs:?}");
        }
        assert!(matches!(
            ArenaDef::load(BROKEN),
            Err(ArenaError::Invalid(_))
        ));
    }

    #[test]
    fn unknown_fields_are_rejected_in_the_mode_too() {
        let src = BROKEN.replace(
            "bots_shoot_back = true",
            "bots_shoot_back = true\nlives = 3",
        );
        assert!(matches!(ArenaDef::load(&src), Err(ArenaError::Parse(_))));
    }

    #[test]
    fn new_options_default_off_and_are_validated() {
        let def = crate::arena::builtin::get("yard-deathmatch").unwrap();
        assert!(matches!(
            def.mode,
            ArenaMode::Deathmatch {
                bot_strafe: false,
                objective: None,
                ..
            }
        ));

        let src = r#"
id = "hold"
name = "Hold"
map = "training-yard"
duration_secs = 30.0
weapon = "vandal"

[mode]
kind = "deathmatch"
bots = 2
bot_skill = "contender"
respawn_secs = 1.0
bots_shoot_back = true
bot_strafe = true

[mode.objective]
site = "Not An Id"
site_radius = 0.0
capture_secs = -1.0
attackers = 0
"#;
        let errs = ArenaDef::from_toml(src).unwrap().validate();
        let fields: Vec<&str> = errs.iter().map(|e| e.field).collect();
        for f in [
            "mode.objective.site",
            "mode.objective.site_radius",
            "mode.objective.capture_secs",
            "mode.objective.attackers",
        ] {
            assert!(fields.contains(&f), "missing {f}: {errs:?}");
        }
        let typo = src.replace("attackers = 0", "attackers = 3\nbombs = 1");
        assert!(matches!(ArenaDef::load(&typo), Err(ArenaError::Parse(_))));

        let fixed = src
            .replace("\"Not An Id\"", "\"nowhere\"")
            .replace("site_radius = 0.0", "site_radius = 2.0")
            .replace("capture_secs = -1.0", "capture_secs = 3.0")
            .replace("attackers = 0", "attackers = 4");
        let def = ArenaDef::load(&fixed).unwrap();
        let map = crate::builtin::get("training-yard").unwrap();
        let errs = def.validate_with_map(&map);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].message.contains("no waypoint 'nowhere'"));
    }

    #[test]
    fn angle_clear_options_are_validated() {
        let src = r#"
id = "a"
name = "A"
map = "angles-lab"
duration_secs = 30.0
weapon = "vandal"

[mode]
kind = "angle_clear"
angles = ["doorway"]
hold_secs = 1.0
bot_reaction_secs = [0.3, 0.5]
bots_shoot_back = false
peek_delay_secs = [0.5, 0.1]
pre_aim_tolerance_deg = 0.0
"#;
        let errs = ArenaDef::from_toml(src).unwrap().validate();
        let fields: Vec<&str> = errs.iter().map(|e| e.field).collect();
        assert_eq!(
            fields,
            ["mode.peek_delay_secs", "mode.pre_aim_tolerance_deg"],
            "{errs:?}"
        );
    }

    #[test]
    fn angle_ids_must_exist_on_the_map() {
        let src = r#"
id = "a"
name = "A"
map = "angles-lab"
duration_secs = 30.0
weapon = "vandal"

[mode]
kind = "angle_clear"
angles = ["doorway", "nowhere"]
hold_secs = 1.0
bot_reaction_secs = [0.3, 0.5]
bots_shoot_back = false
"#;
        let def = ArenaDef::load(src).unwrap();
        let map = crate::builtin::get("angles-lab").unwrap();
        let errs = def.validate_with_map(&map);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].message.contains("nowhere"));
    }
}
