//! Break prompts: counts continuous play, and shows a non-blocking banner in the menu and on
//! the results screen (never during a run) when a break is due or the weekly volume cap is
//! reached. The policy is `Settings::breaks` (`aim_core::training::ergonomics`).

use crate::game::AppState;
use crate::overlay::Overlay;
use crate::persistence::history::History;
use crate::persistence::settings::Settings;
use crate::ui::organisms::break_banner::{self, BannerAction, BannerModel};
use aim_core::training::ergonomics::{BreakPolicy, minutes_this_week};
use aim_core::training::store::RunStore;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

/// How long "Snooze" postpones a due break, minutes of play (estimate).
pub const SNOOZE_MINUTES: f32 = 10.0;
/// The weekly total is recomputed at least this often, seconds, so runs leave the 7-day
/// window while the trainer stays open.
const WEEKLY_REFRESH_SECS: f64 = 600.0;

pub struct BreaksPage;

impl Plugin for BreaksPage {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayClock>()
            .init_resource::<WeeklyVolume>()
            .add_systems(Update, (track_play, refresh_weekly_volume))
            .add_systems(EguiPrimaryContextPass, banner.run_if(menu_or_results));
    }
}

/// Continuous play since the last break. Only time in a run counts as play; time anywhere
/// else (menu, results, hidden overlay) is idle, and idle time as long as a break resets
/// the count.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct PlayClock {
    /// Seconds played since the last break.
    played_secs: f32,
    /// Seconds since the last run frame.
    idle_secs: f32,
    /// A due break is not shown again before `played_secs` reaches this.
    quiet_until_secs: f32,
}

impl PlayClock {
    /// Advance by `dt` seconds of play (`playing`) or idle time. Idle time of at least
    /// `break_secs` counts as a break and resets the count.
    pub fn tick(&mut self, dt: f32, playing: bool, break_secs: f32) {
        if playing {
            self.played_secs += dt;
            self.idle_secs = 0.0;
        } else {
            self.idle_secs += dt;
            if self.idle_secs >= break_secs {
                self.played_secs = 0.0;
                self.quiet_until_secs = 0.0;
            }
        }
    }

    /// Minutes of continuous play since the last break.
    pub fn minutes_since_break(&self) -> f32 {
        self.played_secs / 60.0
    }

    /// Whether the break banner should show under `policy`.
    pub fn break_due(&self, policy: &BreakPolicy) -> bool {
        policy.break_due(self.minutes_since_break()) && self.played_secs >= self.quiet_until_secs
    }

    /// Hide the banner for another [`SNOOZE_MINUTES`] of play.
    pub fn snooze(&mut self) {
        self.quiet_until_secs = self.played_secs + SNOOZE_MINUTES * 60.0;
    }

    /// Hide the banner until another full `policy.continuous_minutes` of play.
    pub fn dismiss(&mut self, policy: &BreakPolicy) {
        self.quiet_until_secs = self.played_secs + policy.continuous_minutes as f32 * 60.0;
    }
}

/// Minutes trained in the last 7 days, cached; plus whether the cap banner was dismissed.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct WeeklyVolume {
    pub minutes: f32,
    refreshed_at: Option<f64>,
    pub dismissed: bool,
}

/// The banner shows in the menu and on the results screen, never during a run.
fn menu_or_results(state: Res<State<AppState>>) -> bool {
    matches!(state.get(), AppState::Menu | AppState::Results)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn track_play(
    time: Res<Time<Real>>,
    state: Res<State<AppState>>,
    overlay: Res<Overlay>,
    settings: Res<Settings>,
    mut clock: ResMut<PlayClock>,
) {
    let playing = *state.get() == AppState::Playing && overlay.visible;
    let break_secs = settings.breaks.break_minutes as f32 * 60.0;
    clock.tick(time.delta_secs(), playing, break_secs);
}

/// Recompute the weekly total when the history changes, and every
/// [`WEEKLY_REFRESH_SECS`] otherwise. Never per frame.
fn refresh_weekly_volume(
    time: Res<Time<Real>>,
    history: Res<History>,
    mut weekly: ResMut<WeeklyVolume>,
) {
    let now = time.elapsed_secs_f64();
    let stale = weekly
        .refreshed_at
        .is_none_or(|t| now - t >= WEEKLY_REFRESH_SECS);
    if history.is_changed() || stale {
        weekly.minutes = minutes_this_week(history.runs(), now_secs());
        weekly.refreshed_at = Some(now);
    }
}

fn banner(
    mut contexts: EguiContexts,
    overlay: Res<Overlay>,
    settings: Res<Settings>,
    mut clock: ResMut<PlayClock>,
    mut weekly: ResMut<WeeklyVolume>,
) -> Result {
    if !overlay.visible {
        return Ok(());
    }
    let policy = settings.breaks;
    let model = BannerModel {
        break_due: clock
            .break_due(&policy)
            .then(|| clock.minutes_since_break()),
        break_minutes: policy.break_minutes,
        weekly: (policy.weekly_cap_reached(weekly.minutes) && !weekly.dismissed)
            .then_some((weekly.minutes, policy.weekly_cap_minutes)),
    };
    if model.break_due.is_none() && model.weekly.is_none() {
        return Ok(());
    }
    match break_banner::break_banner(contexts.ctx_mut()?, &model) {
        Some(BannerAction::Dismiss) => clock.dismiss(&policy),
        Some(BannerAction::Snooze) => clock.snooze(),
        Some(BannerAction::DismissWeekly) => weekly.dismissed = true,
        None => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: f32 = 60.0;

    fn policy() -> BreakPolicy {
        BreakPolicy::default() // 45 min of play, 5 min breaks
    }

    #[test]
    fn a_break_is_due_after_the_continuous_minutes_of_play() {
        let mut c = PlayClock::default();
        c.tick(44.0 * MIN, true, 5.0 * MIN);
        assert!(!c.break_due(&policy()));
        c.tick(1.0 * MIN, true, 5.0 * MIN);
        assert!(c.break_due(&policy()));
        assert_eq!(c.minutes_since_break(), 45.0);
    }

    #[test]
    fn short_menu_time_keeps_the_count_and_a_break_length_resets_it() {
        let mut c = PlayClock::default();
        c.tick(30.0 * MIN, true, 5.0 * MIN);
        c.tick(4.0 * MIN, false, 5.0 * MIN);
        c.tick(15.0 * MIN, true, 5.0 * MIN);
        assert!(c.break_due(&policy()), "4 min in the menu is not a break");
        c.tick(5.0 * MIN, false, 5.0 * MIN);
        assert_eq!(c.minutes_since_break(), 0.0);
        assert!(!c.break_due(&policy()));
    }

    #[test]
    fn idle_time_never_counts_as_play() {
        let mut c = PlayClock::default();
        c.tick(60.0 * MIN, false, 5.0 * MIN);
        assert_eq!(c.minutes_since_break(), 0.0);
    }

    #[test]
    fn snooze_and_dismiss_hide_the_banner_for_a_while() {
        let mut c = PlayClock::default();
        c.tick(45.0 * MIN, true, 5.0 * MIN);
        c.snooze();
        assert!(!c.break_due(&policy()));
        c.tick(SNOOZE_MINUTES * MIN, true, 5.0 * MIN);
        assert!(c.break_due(&policy()), "due again after the snooze");
        c.dismiss(&policy());
        c.tick(44.0 * MIN, true, 5.0 * MIN);
        assert!(!c.break_due(&policy()));
        c.tick(1.0 * MIN, true, 5.0 * MIN);
        assert!(
            c.break_due(&policy()),
            "due again after another full interval"
        );
        // A real break clears the snooze too.
        c.snooze();
        c.tick(5.0 * MIN, false, 5.0 * MIN);
        c.tick(45.0 * MIN, true, 5.0 * MIN);
        assert!(c.break_due(&policy()));
    }

    #[test]
    fn turning_breaks_off_hides_every_prompt() {
        let off = BreakPolicy {
            enabled: false,
            ..policy()
        };
        let mut c = PlayClock::default();
        c.tick(600.0 * MIN, true, 5.0 * MIN);
        assert!(!c.break_due(&off));
        assert!(!off.weekly_cap_reached(10_000.0));
    }

    #[test]
    fn systems_have_no_conflicting_access() {
        use bevy::ecs::system::assert_is_system;
        assert_is_system(track_play);
        assert_is_system(refresh_weekly_volume);
        assert_is_system::<(), Result, _>(banner);
    }
}
