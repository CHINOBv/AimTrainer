//! "Coach's take" on a finished run and "Ask the coach why" on one mistake: a compact,
//! byte-capped digest of numbers with stable metric keys ([`RunDigest`], [`MistakeDigest`]),
//! the prompt sent with it, and a strict reader of the model's JSON reply.
//!
//! The model must cite the metric key behind every number it writes. A reply is accepted
//! only when it is valid JSON within the length limits, every cited key exists in the
//! digest, every number in its text matches a cited value (within the rounding of the
//! number as written) and a suggested drill is one of the scenario ids offered. One model
//! call, no tools and no retries: a rejected reply is reported, never silently re-asked.
//!
//! A digest holds numbers only. It never carries crosshair paths, trace sample indices,
//! target ids, seeds, file paths or names.

use std::fmt::Write as _;

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use super::{CUE_RULE, ChatModel, Message, ModelError, NO_GAME_RULE, ROLE};
use crate::analytics::RunAnalysis;
use crate::analytics::mistakes::Mistake;
use crate::scenario::{Category, RunResult, ScenarioDef};
use crate::training::progress::RunRecord;

/// Upper bound of a digest's JSON, in bytes.
pub const DIGEST_MAX_BYTES: usize = 2 * 1024;
/// Most bullets in a Coach's take.
pub const MAX_BULLETS: usize = 3;
/// Longest bullet, in characters.
pub const BULLET_MAX_CHARS: usize = 160;
/// Longest drill reason, in characters.
pub const REASON_MAX_CHARS: usize = 120;
/// Longest "why" of a mistake, in characters.
pub const WHY_MAX_CHARS: usize = 200;
/// Longest cue of a mistake, in characters.
pub const CUE_MAX_CHARS: usize = 80;
/// Most earlier runs of the same scenario summarized in a digest.
pub const MAX_PREVIOUS_RUNS: usize = 5;
/// Most scenario ids offered as drills.
pub const MAX_DRILL_IDS: usize = 40;

/// Named numbers in a fixed order, serialized as a JSON object `{key: value}`. Values are
/// already rounded to the precision they are shown with.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metrics(pub Vec<(String, f64)>);

impl Metrics {
    /// The value of `key`, if present.
    pub fn get(&self, key: &str) -> Option<f64> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
    }

    /// Add `key` rounded to `decimals`, unless the value is missing or not finite.
    fn push(&mut self, key: &str, value: Option<f32>, decimals: i32) {
        if let Some(v) = value.filter(|v| v.is_finite()) {
            self.0.push((key.to_owned(), round(f64::from(v), decimals)));
        }
    }

    /// Add a share (0-1) as a percentage with one decimal.
    fn push_pct(&mut self, key: &str, share: Option<f32>) {
        self.push(key, share.map(|s| s * 100.0), 1);
    }

    /// Add seconds as whole milliseconds.
    fn push_ms(&mut self, key: &str, secs: Option<f32>) {
        self.push(key, secs.map(|s| s * 1000.0), 0);
    }
}

impl Serialize for Metrics {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

fn round(v: f64, decimals: i32) -> f64 {
    let p = 10f64.powi(decimals);
    (v * p).round() / p
}

/// What the coach is told about one finished run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunDigest {
    /// The scenario played (its run key).
    pub scenario_id: String,
    /// Its category.
    pub category: Category,
    /// The run's numbers, most important first.
    pub metrics: Metrics,
}

impl RunDigest {
    /// A digest of `metrics`, dropping metrics from the end until its JSON fits
    /// [`DIGEST_MAX_BYTES`]. What this returns is exactly what is sent and validated.
    pub fn new(scenario_id: &str, category: Category, metrics: Metrics) -> Self {
        let mut digest = Self {
            scenario_id: scenario_id.to_owned(),
            category,
            metrics,
        };
        while digest.to_prompt_json().len() > DIGEST_MAX_BYTES && digest.metrics.0.pop().is_some() {
        }
        digest
    }

    /// The digest of a run: its result, the analysis aggregates and, from `previous` (the
    /// earlier runs of the same scenario, oldest first; see [`previous_runs`]), its score
    /// against the last [`MAX_PREVIOUS_RUNS`] of them.
    pub fn build(analysis: &RunAnalysis, result: &RunResult, previous: &[RunRecord]) -> Self {
        let mut m = Metrics::default();
        m.push("score", Some(result.score), 1);
        m.push_pct("accuracy_pct", Some(result.accuracy));
        m.push("kills", Some(result.kills as f32), 0);
        m.push("kills_per_min", Some(result.kills_per_min), 1);
        m.push_ms("avg_time_to_kill_ms", result.avg_time_to_kill);
        m.push_pct("headshot_pct", Some(result.headshot_rate));
        if result.moving_shots > 0 {
            m.push("moving_shots", Some(result.moving_shots as f32), 0);
        }
        if result.expired > 0 {
            m.push("expired_targets", Some(result.expired as f32), 0);
        }
        m.push("duration_s", Some(result.duration_secs), 1);
        if let Some(f) = analysis.flick_summary().filter(|f| f.flicks > 0) {
            m.push("flick.count", Some(f.flicks as f32), 0);
            m.push_pct("flick.hit_pct", Some(f.hit_rate));
            m.push_ms("flick.median_reaction_ms", f.median_reaction_secs);
            m.push_ms("flick.median_movement_ms", f.median_movement_secs);
            m.push_pct("flick.overshoot_pct", Some(f.overshoot_rate));
            m.push_pct("flick.undershoot_pct", Some(f.undershoot_rate));
            m.push("flick.mean_corrections", Some(f.mean_corrections), 2);
            m.push_ms("flick.median_settle_ms", f.median_settle_secs);
            m.push_ms("flick.median_click_delay_ms", f.median_click_delay_secs);
            m.push(
                "flick.median_peak_speed_deg_s",
                f.median_peak_speed_deg_per_sec,
                0,
            );
            m.push("flick.median_path_efficiency", f.median_path_efficiency, 2);
            m.push("flick.throughput_bits_s", f.throughput_bits_per_sec, 2);
        }
        if let Some(t) = analysis.tracking_summary() {
            m.push("tracking.hold_s", Some(t.hold_secs), 1);
            m.push_pct("tracking.on_target_pct", Some(t.on_target_ratio));
            m.push("tracking.median_error_deg", t.median_error_deg, 2);
            m.push_pct("tracking.lag_pct", t.lag_ratio);
            m.push_pct("tracking.speed_match_pct", t.speed_match_ratio);
            m.push_pct("tracking.speed_under_pct", t.speed_under_ratio);
            m.push_pct("tracking.speed_over_pct", t.speed_over_ratio);
            m.push_pct("tracking.edge_pct", t.edge_ratio);
            m.push_ms(
                "tracking.median_reversal_ms",
                t.median_reversal_response_secs,
            );
        }
        if let Some(s) = analysis.moving.as_ref().map(|a| a.summary())
            && s.shots > 0
        {
            m.push("moving.shots", Some(s.shots as f32), 0);
            m.push_pct("moving.accurate_shot_pct", s.accurate_share);
            m.push("moving.median_shot_speed_mps", s.median_shot_speed, 2);
            m.push_pct("moving.stopped_hit_pct", s.stopped_hit_rate);
            m.push_pct("moving.moving_hit_pct", s.moving_hit_rate);
            m.push_pct("moving.counter_strafe_pct", s.counter_strafe_share);
            m.push_ms("moving.median_stop_to_shot_ms", s.median_stop_to_shot_secs);
        }
        if let Some(p) = &analysis.potential {
            m.push("potential.points_gain", Some(p.gain.points), 1);
            m.push("potential.score", Some(p.potential_score()), 1);
        }
        let recent = &previous[previous.len().saturating_sub(MAX_PREVIOUS_RUNS)..];
        if let Some(last) = recent.last() {
            let best = recent.iter().map(|r| r.score).fold(f32::MIN, f32::max);
            let mean = recent.iter().map(|r| r.score).sum::<f32>() / recent.len() as f32;
            m.push("history.previous_runs", Some(recent.len() as f32), 0);
            m.push("history.previous_score", Some(last.score), 1);
            m.push("history.recent_best_score", Some(best), 1);
            m.push("history.recent_mean_score", Some(mean), 1);
            m.push(
                "history.score_vs_previous",
                Some(result.score - last.score),
                1,
            );
            m.push("history.score_vs_recent_mean", Some(result.score - mean), 1);
        }
        Self::new(&result.scenario_id, analysis.category, m)
    }

    /// The digest as JSON, exactly as it is sent.
    pub fn to_prompt_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// The earlier runs of `result`'s scenario in `records` (oldest first), without `result`
/// itself: the history is written before the results screen shows, so the newest record
/// with the same score and length is the run being debriefed.
pub fn previous_runs(records: &[RunRecord], result: &RunResult) -> Vec<RunRecord> {
    let mut runs: Vec<RunRecord> = records
        .iter()
        .filter(|r| r.scenario == result.scenario_id)
        .cloned()
        .collect();
    if runs
        .last()
        .is_some_and(|r| r.score == result.score && r.duration_secs == result.duration_secs)
    {
        runs.pop();
    }
    runs
}

/// What the coach is told about one flick mistake.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MistakeDigest {
    /// The scenario played.
    pub scenario_id: String,
    /// Its category.
    pub category: Category,
    /// What went wrong, in words (for example "Overflick").
    pub kind: String,
    /// Whether the shot hit anyway.
    pub hit: bool,
    /// The mistake's numbers.
    pub metrics: Metrics,
}

impl MistakeDigest {
    /// The digest of `mistake`: its kind and measured amounts, never its paths, sample
    /// indices or target id.
    pub fn build(mistake: &Mistake, scenario_id: &str, category: Category) -> Self {
        let mut m = Metrics::default();
        m.push("distance_deg", Some(mistake.distance_deg), 2);
        m.push("target_radius_deg", Some(mistake.radius_deg), 2);
        m.push("direction_right_deg", Some(mistake.direction_deg.0), 2);
        m.push("direction_up_deg", Some(mistake.direction_deg.1), 2);
        m.push("endpoint_error_radii", mistake.error_radii, 2);
        m.push("edge_error_deg", mistake.edge_error_deg, 2);
        m.push_ms("delay_ms", mistake.delay_secs);
        m.push("corrections", Some(mistake.corrections as f32), 0);
        Self {
            scenario_id: scenario_id.to_owned(),
            category,
            kind: mistake.kind.label().to_owned(),
            hit: mistake.hit,
            metrics: m,
        }
    }

    /// The digest as JSON, exactly as it is sent.
    pub fn to_prompt_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// One point of a Coach's take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bullet {
    /// The point, in one sentence.
    pub text: String,
    /// The metric keys behind its numbers.
    #[serde(default)]
    pub cites: Vec<String>,
}

/// A drill the coach suggests, from the scenario ids it was offered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrillPick {
    /// A scenario id from the offered list.
    pub scenario_id: String,
    /// Why this drill.
    pub reason: String,
}

/// The coach's take on a run, validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Debrief {
    /// One to [`MAX_BULLETS`] points, most important first.
    pub bullets: Vec<Bullet>,
    /// An optional drill.
    #[serde(default)]
    pub drill: Option<DrillPick>,
}

/// The coach's explanation of one mistake, validated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MistakeWhy {
    /// Why it happened.
    pub why: String,
    /// One external-focus cue to fix it.
    pub cue: String,
    /// The metric keys behind its numbers.
    #[serde(default)]
    pub cites: Vec<String>,
}

/// Why a debrief failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DebriefError {
    /// The model call failed.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// The model answered, but the reply broke a rule; the reason is for the player.
    #[error("the coach's reply was rejected: {0}")]
    Rejected(String),
}

/// Instructions shared by both debriefs.
fn system_prompt() -> String {
    format!(
        "{ROLE} You review numbers from the trainer's analysis. Use only the numbers you are \
         given and cite the metric key of every number you write; write no other numbers \
         (spell counts out in words). {CUE_RULE} Never invent VALORANT constants. \
         {NO_GAME_RULE} Answer with JSON only, with no text around it."
    )
}

/// The ids offered as drills: at most [`MAX_DRILL_IDS`] of `ids`.
fn offered(ids: &[String]) -> &[String] {
    &ids[..ids.len().min(MAX_DRILL_IDS)]
}

/// The messages [`run_debrief`] sends.
pub fn debrief_messages(digest: &RunDigest, drill_ids: &[String]) -> Vec<Message> {
    let ids = offered(drill_ids);
    let mut user = format!("Run digest (JSON):\n{}\n\n", digest.to_prompt_json());
    if ids.is_empty() {
        user.push_str("No drills are offered; leave out \"drill\".\n\n");
    } else {
        let _ = write!(
            user,
            "Drills you may suggest (scenario ids): {}\n\n",
            ids.join(", ")
        );
    }
    let _ = write!(
        user,
        "Reply with this JSON: {{\"bullets\":[{{\"text\":\"...\",\"cites\":[\"metric key\"]}}],\
         \"drill\":{{\"scenario_id\":\"...\",\"reason\":\"...\"}}}}. Give one to three bullets, \
         most important first, each at most {BULLET_MAX_CHARS} characters, with at least one \
         cite; every number in a bullet must be the value of a key in its cites. Use at most \
         one external-focus cue in total. \"drill\" is optional: leave it out or pick one id \
         from the list, with a reason of at most {REASON_MAX_CHARS} characters."
    );
    vec![
        Message::System {
            content: system_prompt(),
        },
        Message::User { content: user },
    ]
}

/// The messages [`run_mistake_why`] sends.
pub fn mistake_messages(digest: &MistakeDigest) -> Vec<Message> {
    let user = format!(
        "Mistake digest (JSON):\n{}\n\nExplain why this mistake happened and give one \
         external-focus cue to fix it. Reply with this JSON: {{\"why\":\"...\",\"cue\":\"...\",\
         \"cites\":[\"metric key\"]}}. \"why\" at most {WHY_MAX_CHARS} characters, \"cue\" at \
         most {CUE_MAX_CHARS}, at least one cite; every number you write must be the value of \
         a key in cites.",
        digest.to_prompt_json()
    );
    vec![
        Message::System {
            content: system_prompt(),
        },
        Message::User { content: user },
    ]
}

/// Ask `model` once for its take on the run in `digest`, offering `drill_ids` (at most
/// [`MAX_DRILL_IDS`]), without tools and without retrying.
pub fn run_debrief(
    model: &mut dyn ChatModel,
    digest: &RunDigest,
    drill_ids: &[String],
) -> Result<Debrief, DebriefError> {
    let text = ask(model, &debrief_messages(digest, drill_ids))?;
    parse_debrief(&text, digest, drill_ids).map_err(DebriefError::Rejected)
}

/// Ask `model` once why the mistake in `digest` happened, without tools and without
/// retrying.
pub fn run_mistake_why(
    model: &mut dyn ChatModel,
    digest: &MistakeDigest,
) -> Result<MistakeWhy, DebriefError> {
    let text = ask(model, &mistake_messages(digest))?;
    parse_mistake_why(&text, digest).map_err(DebriefError::Rejected)
}

fn ask(model: &mut dyn ChatModel, messages: &[Message]) -> Result<String, DebriefError> {
    let reply = model.complete(messages, &[])?;
    if !reply.tool_calls.is_empty() {
        return Err(DebriefError::Rejected(
            "the model asked for tools; none are offered here".to_owned(),
        ));
    }
    Ok(reply.text)
}

/// Read and validate a Coach's take reply against the `digest` and `drill_ids` it was
/// sent with. The error says which rule the reply broke.
pub fn parse_debrief(
    reply: &str,
    digest: &RunDigest,
    drill_ids: &[String],
) -> Result<Debrief, String> {
    let debrief: Debrief = serde_json::from_str(extract_json(reply))
        .map_err(|e| format!("the reply is not the expected JSON ({e})"))?;
    validate_debrief(&debrief, &digest.metrics, offered(drill_ids))?;
    Ok(debrief)
}

/// Read and validate an "Ask the coach why" reply against the `digest` it was sent with.
pub fn parse_mistake_why(reply: &str, digest: &MistakeDigest) -> Result<MistakeWhy, String> {
    let why: MistakeWhy = serde_json::from_str(extract_json(reply))
        .map_err(|e| format!("the reply is not the expected JSON ({e})"))?;
    validate_why(&why, &digest.metrics)?;
    Ok(why)
}

/// The JSON object in `reply`: inside a fenced code block if there is one, from its first
/// `{` to its last `}`.
fn extract_json(reply: &str) -> &str {
    let mut text = reply.trim();
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let end = after.find("```").unwrap_or(after.len());
        text = &after[..end];
    }
    match (text.find('{'), text.rfind('}')) {
        (Some(a), Some(b)) if a < b => &text[a..=b],
        _ => text.trim(),
    }
}

fn validate_debrief(debrief: &Debrief, metrics: &Metrics, ids: &[String]) -> Result<(), String> {
    let count = debrief.bullets.len();
    if !(1..=MAX_BULLETS).contains(&count) {
        return Err(format!(
            "it has {count} bullets; one to {MAX_BULLETS} are allowed"
        ));
    }
    let mut all_cited = Vec::new();
    for bullet in &debrief.bullets {
        check_text("a bullet", &bullet.text, BULLET_MAX_CHARS)?;
        let cited = cited_values(&bullet.cites, metrics)?;
        check_numbers(&bullet.text, &cited)?;
        all_cited.extend(cited);
    }
    if let Some(drill) = &debrief.drill {
        if !ids.contains(&drill.scenario_id) {
            return Err(format!(
                "drill {:?} is not one of the offered scenarios",
                drill.scenario_id
            ));
        }
        check_text("the drill reason", &drill.reason, REASON_MAX_CHARS)?;
        check_numbers(&drill.reason, &all_cited)?;
    }
    Ok(())
}

fn validate_why(why: &MistakeWhy, metrics: &Metrics) -> Result<(), String> {
    check_text("the explanation", &why.why, WHY_MAX_CHARS)?;
    check_text("the cue", &why.cue, CUE_MAX_CHARS)?;
    let cited = cited_values(&why.cites, metrics)?;
    check_numbers(&why.why, &cited)?;
    check_numbers(&why.cue, &cited)
}

/// `text` is not blank and at most `max` characters long.
fn check_text(what: &str, text: &str, max: usize) -> Result<(), String> {
    let len = text.chars().count();
    if text.trim().is_empty() {
        return Err(format!("{what} is empty"));
    }
    if len > max {
        return Err(format!(
            "{what} has {len} characters; at most {max} are allowed"
        ));
    }
    Ok(())
}

/// The values of `cites`, which must be at least one key, every one in `metrics`.
fn cited_values(cites: &[String], metrics: &Metrics) -> Result<Vec<f64>, String> {
    if cites.is_empty() {
        return Err("a point has no cite; every point must cite a metric key".to_owned());
    }
    cites
        .iter()
        .map(|key| {
            metrics
                .get(key)
                .ok_or_else(|| format!("it cites {key:?}, which is not in the digest"))
        })
        .collect()
}

/// Every number written in `text` equals one of `cited` (sign aside, since "fell 12" and
/// "-12" say the same), within the rounding of the number as written: "62%" matches 62.4,
/// "62.4" matches 62.4, "63" does not.
fn check_numbers(text: &str, cited: &[f64]) -> Result<(), String> {
    for (value, decimals) in numbers(text) {
        let tolerance = 0.5 * 10f64.powi(-decimals) + 1e-9;
        if !cited.iter().any(|c| (c.abs() - value).abs() <= tolerance) {
            let written = format!("{value:.prec$}", prec = decimals as usize);
            return Err(format!(
                "{written} in {text:?} is not the value of a cited metric"
            ));
        }
    }
    Ok(())
}

/// The numbers written in `text` with their decimal places, ignoring digits glued to a
/// preceding letter or underscore ("M2", "p90", "flick_2"), which name things.
fn numbers(text: &str) -> Vec<(f64, i32)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let glued =
            start > 0 && (bytes[start - 1].is_ascii_alphabetic() || bytes[start - 1] == b'_');
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let mut decimals = 0;
        if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
            i += 1;
            let fraction = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            decimals = (i - fraction) as i32;
        }
        if !glued && let Ok(value) = text[start..i].parse::<f64>() {
            found.push((value, decimals));
        }
    }
    found
}

/// Scenario ids to offer as drills: `category`'s scenarios first, then the rest, at most
/// [`MAX_DRILL_IDS`].
pub fn drill_candidates(scenarios: &[ScenarioDef], category: Category) -> Vec<String> {
    let same = scenarios.iter().filter(|d| d.category == category);
    let other = scenarios.iter().filter(|d| d.category != category);
    same.chain(other)
        .map(|d| d.id.clone())
        .take(MAX_DRILL_IDS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::analyze;
    use crate::analytics::mistakes::{MistakeKind, PathPoint};
    use crate::analytics::test_support;
    use crate::coach::{Reply, ToolSpec};
    use crate::sim::AimerProfile;

    /// A model that answers one scripted text and records what it was sent.
    struct FakeModel {
        reply: String,
        seen: Vec<Vec<Message>>,
    }

    impl FakeModel {
        fn new(reply: &str) -> Self {
            Self {
                reply: reply.to_owned(),
                seen: Vec::new(),
            }
        }
    }

    impl ChatModel for FakeModel {
        fn complete(
            &mut self,
            messages: &[Message],
            tools: &[ToolSpec],
        ) -> Result<Reply, ModelError> {
            assert!(tools.is_empty(), "debriefs offer no tools");
            self.seen.push(messages.to_vec());
            Ok(Reply {
                text: self.reply.clone(),
                tool_calls: Vec::new(),
            })
        }
    }

    fn digest() -> RunDigest {
        RunDigest::new(
            "micro-adjust",
            Category::Microcorrection,
            Metrics(vec![
                ("accuracy_pct".into(), 62.4),
                ("flick.overshoot_pct".into(), 38.0),
                ("flick.median_reaction_ms".into(), 412.0),
                ("history.score_vs_previous".into(), -12.5),
            ]),
        )
    }

    fn ids() -> Vec<String> {
        vec!["micro-adjust".into(), "flick-wide".into()]
    }

    fn ask(reply: &str) -> Result<Debrief, DebriefError> {
        run_debrief(&mut FakeModel::new(reply), &digest(), &ids())
    }

    fn rejected(result: Result<Debrief, DebriefError>, needle: &str) {
        match result {
            Err(DebriefError::Rejected(reason)) => {
                assert!(reason.contains(needle), "{needle:?} not in {reason:?}");
            }
            other => panic!("expected a rejection mentioning {needle:?}, got {other:?}"),
        }
    }

    const VALID: &str = r#"{"bullets":[
        {"text":"You overshot 38% of flicks; land on the near edge first.","cites":["flick.overshoot_pct"]},
        {"text":"Accuracy 62.4% with reactions near 412 ms.","cites":["accuracy_pct","flick.median_reaction_ms"]}
    ],"drill":{"scenario_id":"flick-wide","reason":"Overshoots at 38% need wider stops."}}"#;

    #[test]
    fn a_valid_reply_is_accepted_from_one_tool_free_call() {
        let mut model = FakeModel::new(VALID);
        let debrief = run_debrief(&mut model, &digest(), &ids()).expect("valid");
        assert_eq!(debrief.bullets.len(), 2);
        assert_eq!(debrief.drill.unwrap().scenario_id, "flick-wide");
        assert_eq!(model.seen.len(), 1, "one call, no retries");
        let sent = &model.seen[0];
        assert!(matches!(sent[0], Message::System { .. }));
        let Message::User { content } = &sent[1] else {
            panic!("the digest goes in the user message");
        };
        assert!(content.contains(&digest().to_prompt_json()), "{content}");
        assert!(content.contains("flick-wide"), "the offered ids are listed");
    }

    #[test]
    fn a_fenced_json_block_and_rounded_or_signed_numbers_are_accepted() {
        let reply = "Here you go:\n```json\n{\"bullets\":[{\"text\":\"Score fell 12.5 \
                     points; accuracy was about 62%.\",\"cites\":[\"history.score_vs_previous\",\
                     \"accuracy_pct\"]}]}\n```";
        let debrief = ask(reply).expect("fenced block");
        assert_eq!(debrief.drill, None);
    }

    #[test]
    fn a_number_without_a_cite_is_rejected() {
        rejected(
            ask(r#"{"bullets":[{"text":"You overshot 38% of flicks.","cites":["accuracy_pct"]}]}"#),
            "38",
        );
    }

    #[test]
    fn a_wrong_number_is_rejected() {
        rejected(
            ask(r#"{"bullets":[{"text":"Accuracy was 71%.","cites":["accuracy_pct"]}]}"#),
            "71",
        );
    }

    #[test]
    fn an_unknown_cite_is_rejected() {
        rejected(
            ask(r#"{"bullets":[{"text":"Aim lower.","cites":["crosshair_height"]}]}"#),
            "crosshair_height",
        );
    }

    #[test]
    fn a_bullet_without_cites_is_rejected() {
        rejected(
            ask(r#"{"bullets":[{"text":"Aim lower.","cites":[]}]}"#),
            "cite",
        );
    }

    #[test]
    fn an_unknown_scenario_id_is_rejected() {
        rejected(
            ask(
                r#"{"bullets":[{"text":"Accuracy 62.4%.","cites":["accuracy_pct"]}],
                "drill":{"scenario_id":"aim-lab-gridshot","reason":"Warm up."}}"#,
            ),
            "aim-lab-gridshot",
        );
    }

    #[test]
    fn too_many_or_no_bullets_are_rejected() {
        let b = r#"{"text":"Accuracy 62.4%.","cites":["accuracy_pct"]}"#;
        rejected(
            ask(&format!(r#"{{"bullets":[{b},{b},{b},{b}]}}"#)),
            "bullets",
        );
        rejected(ask(r#"{"bullets":[]}"#), "bullets");
    }

    #[test]
    fn overlong_text_is_rejected() {
        let long = "a".repeat(BULLET_MAX_CHARS + 1);
        rejected(
            ask(&format!(
                r#"{{"bullets":[{{"text":"{long}","cites":["accuracy_pct"]}}]}}"#
            )),
            "characters",
        );
        let reason = "b".repeat(REASON_MAX_CHARS + 1);
        rejected(
            ask(&format!(
                r#"{{"bullets":[{{"text":"Fine.","cites":["accuracy_pct"]}}],
                "drill":{{"scenario_id":"flick-wide","reason":"{reason}"}}}}"#
            )),
            "characters",
        );
    }

    #[test]
    fn a_number_in_the_drill_reason_must_be_cited_too() {
        rejected(
            ask(
                r#"{"bullets":[{"text":"Accuracy 62.4%.","cites":["accuracy_pct"]}],
                "drill":{"scenario_id":"flick-wide","reason":"Play it 5 times."}}"#,
            ),
            "5",
        );
    }

    #[test]
    fn prose_and_tool_calls_are_rejected() {
        rejected(ask("You did great, keep going!"), "JSON");
        struct ToolCaller;
        impl ChatModel for ToolCaller {
            fn complete(&mut self, _: &[Message], _: &[ToolSpec]) -> Result<Reply, ModelError> {
                Ok(Reply {
                    text: String::new(),
                    tool_calls: vec![crate::coach::ToolCall {
                        id: "1".into(),
                        name: "list_stats".into(),
                        arguments: serde_json::json!({}),
                    }],
                })
            }
        }
        rejected(run_debrief(&mut ToolCaller, &digest(), &ids()), "tools");
    }

    #[test]
    fn digits_inside_words_and_keys_are_not_numbers() {
        let reply = r#"{"bullets":[{"text":"Your M2 overshoots sit at 38%.","cites":["flick.overshoot_pct"]}]}"#;
        assert!(ask(reply).is_ok());
    }

    fn mistake() -> Mistake {
        let path = vec![
            PathPoint {
                time_secs: 0.0,
                offset_deg: (-9.0, 1.0),
            },
            PathPoint {
                time_secs: 0.2,
                offset_deg: (0.8, 0.0),
            },
        ];
        Mistake {
            kind: MistakeKind::Overflick,
            flick_index: 7,
            target_id: 4242,
            radius_deg: 0.45,
            distance_deg: 9.06,
            direction_deg: (9.0, -1.0),
            segment_start: 1111,
            focus_start: 1222,
            error_at: 1333,
            focus_end: 1444,
            segment_end: 1555,
            error_radii: Some(1.78),
            edge_error_deg: Some(0.35),
            delay_secs: None,
            corrections: 2,
            hit: false,
            tip: "secret tip text".into(),
            player: path.clone(),
            corrected: path,
        }
    }

    #[test]
    fn a_mistake_digest_has_numbers_only_and_no_paths_indices_or_ids() {
        let d = MistakeDigest::build(&mistake(), "micro-adjust", Category::Microcorrection);
        let json = d.to_prompt_json();
        for banned in [
            "player",
            "corrected",
            "offset",
            "segment",
            "focus",
            "error_at",
            "flick_index",
            "target_id",
            "4242",
            "1111",
            "1333",
            "1555",
            "seed",
            "tip",
            "path",
        ] {
            assert!(!json.contains(banned), "{banned:?} leaked: {json}");
        }
        assert_eq!(d.metrics.get("edge_error_deg"), Some(0.35));
        assert_eq!(d.metrics.get("corrections"), Some(2.0));
        assert_eq!(d.kind, "Overflick");
    }

    #[test]
    fn a_run_digest_holds_no_paths_names_or_seeds_and_fits_the_cap() {
        let rec = test_support::record("micro-adjust", AimerProfile::PRESETS[1], 4);
        let analysis = analyze(&rec);
        let result = RunResult {
            scenario_id: "micro-adjust".into(),
            score: 4321.0,
            accuracy: 0.624,
            kills: 40,
            kills_per_min: 40.0,
            avg_time_to_kill: Some(0.61),
            headshot_rate: 0.5,
            moving_shots: 0,
            expired: 0,
            duration_secs: 60.0,
        };
        let previous = [
            RunRecord {
                timestamp: 1,
                scenario: "micro-adjust".into(),
                score: 4000.0,
                duration_secs: 60.0,
            },
            RunRecord {
                timestamp: 2,
                scenario: "micro-adjust".into(),
                score: 4333.5,
                duration_secs: 60.0,
            },
        ];
        let d = RunDigest::build(&analysis, &result, &previous);
        let json = d.to_prompt_json();
        assert!(json.len() <= DIGEST_MAX_BYTES, "{} bytes", json.len());
        assert_eq!(d.metrics.get("accuracy_pct"), Some(62.4));
        assert_eq!(d.metrics.get("history.score_vs_previous"), Some(-12.5));
        assert!(d.metrics.get("flick.overshoot_pct").is_some());
        for banned in ["seed", "player", "corrected", "target_id", "/", "\\"] {
            assert!(!json.contains(banned), "{banned:?} leaked: {json}");
        }
    }

    #[test]
    fn the_byte_cap_drops_metrics_from_the_end() {
        let many = (0..500)
            .map(|i| (format!("metric_number_{i}"), f64::from(i) * 1.5))
            .collect();
        let d = RunDigest::new("micro-adjust", Category::Flicking, Metrics(many));
        assert!(d.to_prompt_json().len() <= DIGEST_MAX_BYTES);
        assert_eq!(
            d.metrics.0[0].0, "metric_number_0",
            "the first metrics stay"
        );
        assert!(d.metrics.0.len() < 500);
    }

    #[test]
    fn the_run_being_debriefed_is_not_its_own_previous_run() {
        let result = RunResult {
            scenario_id: "a".into(),
            score: 10.0,
            accuracy: 1.0,
            kills: 1,
            kills_per_min: 1.0,
            avg_time_to_kill: None,
            headshot_rate: 0.0,
            moving_shots: 0,
            expired: 0,
            duration_secs: 30.0,
        };
        let rec = |ts, scenario: &str, score| RunRecord {
            timestamp: ts,
            scenario: scenario.into(),
            score,
            duration_secs: 30.0,
        };
        let records = [rec(1, "a", 8.0), rec(2, "b", 50.0), rec(3, "a", 10.0)];
        let prev = previous_runs(&records, &result);
        assert_eq!(prev, vec![rec(1, "a", 8.0)]);
    }

    #[test]
    fn a_mistake_why_is_validated_like_a_take() {
        let d = MistakeDigest::build(&mistake(), "micro-adjust", Category::Microcorrection);
        let ok = r#"{"why":"You passed the far edge by 0.35 degrees, then needed two corrections.","cue":"Stop on the near edge.","cites":["edge_error_deg"]}"#;
        let mut model = FakeModel::new(ok);
        let why = run_mistake_why(&mut model, &d).expect("valid");
        assert_eq!(why.cue, "Stop on the near edge.");
        let Message::User { content } = &model.seen[0][1] else {
            panic!("user message");
        };
        assert!(content.contains(&d.to_prompt_json()));

        let wrong = r#"{"why":"You passed the edge by 2 degrees.","cue":"Stop early.","cites":["edge_error_deg"]}"#;
        let err = run_mistake_why(&mut FakeModel::new(wrong), &d).unwrap_err();
        assert!(
            matches!(err, DebriefError::Rejected(ref r) if r.contains('2')),
            "{err}"
        );
        let long_cue = format!(
            r#"{{"why":"Too far.","cue":"{}","cites":["edge_error_deg"]}}"#,
            "c".repeat(CUE_MAX_CHARS + 1)
        );
        let err = run_mistake_why(&mut FakeModel::new(&long_cue), &d).unwrap_err();
        assert!(
            matches!(err, DebriefError::Rejected(ref r) if r.contains("characters")),
            "{err}"
        );
        let uncited = r#"{"why":"Too far.","cue":"Stop early.","cites":["nope"]}"#;
        assert!(run_mistake_why(&mut FakeModel::new(uncited), &d).is_err());
    }

    #[test]
    fn drill_candidates_put_the_category_first_within_the_cap() {
        let all = crate::scenario::builtin::all();
        let ids = drill_candidates(&all, Category::Tracking);
        assert!(ids.len() <= MAX_DRILL_IDS);
        let first = all.iter().find(|d| d.id == ids[0]).unwrap();
        assert_eq!(first.category, Category::Tracking);
    }
}
