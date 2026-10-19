//! Arena kill feed (top right) and the multi-kill / streak banner.

use crate::ui::atoms::{GOOD, WARN, big, pop};
use bevy_egui::egui::{self, Align2, RichText};

/// How long a kill stays in the feed, seconds.
pub const KILL_FEED_SECS: f32 = 4.0;
/// Most kills the feed shows at once; older ones drop off.
pub const KILL_FEED_MAX: usize = 5;
/// Kills closer together than this chain into a multi-kill, seconds. A design choice, not
/// a VALORANT value: about the time to re-aim and kill the next bot in a deathmatch.
pub const MULTI_KILL_WINDOW_SECS: f32 = 1.5;
/// How long the banner stays on screen, seconds.
pub const BANNER_SECS: f32 = 1.6;

/// The feed line of a kill: "You ▶ Bot 3", with " [HS]" for a headshot.
pub fn kill_line(bot_id: u64, headshot: bool) -> String {
    let hs = if headshot { " [HS]" } else { "" };
    format!("You ▶ Bot {bot_id}{hs}")
}

/// The name of `count` kills in a row within the multi-kill window, from two on.
pub fn multi_kill_name(count: u32) -> Option<String> {
    match count {
        0 | 1 => None,
        2 => Some("Double".to_owned()),
        3 => Some("Triple".to_owned()),
        4 => Some("Quadra".to_owned()),
        n => Some(format!("Multi ×{n}")),
    }
}

/// The name of a kill streak (kills since the last death) at its milestones: 3, 5, then
/// every 5.
pub fn streak_name(streak: u32) -> Option<String> {
    (streak == 3 || (streak >= 5 && streak.is_multiple_of(5))).then(|| format!("{streak} streak"))
}

/// The banner for a kill that makes `multi` quick kills and a streak of `streak`. A
/// multi-kill wins over a streak milestone reached by the same kill.
pub fn banner_text(multi: u32, streak: u32) -> Option<String> {
    multi_kill_name(multi).or_else(|| streak_name(streak))
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    line: String,
    headshot: bool,
    age: f32,
}

/// Recent kills, the multi-kill chain and the banner on screen.
#[derive(Debug, Default, Clone)]
pub struct KillFeed {
    entries: Vec<Entry>,
    /// Kills in the current chain, and seconds since its last kill.
    multi: u32,
    since_kill: f32,
    banner: Option<(String, f32)>,
}

impl KillFeed {
    /// A kill of bot `bot_id`, with the player's streak after it.
    pub fn on_kill(&mut self, bot_id: u64, headshot: bool, streak: u32) {
        self.multi = if self.multi > 0 && self.since_kill <= MULTI_KILL_WINDOW_SECS {
            self.multi + 1
        } else {
            1
        };
        self.since_kill = 0.0;
        if let Some(text) = banner_text(self.multi, streak) {
            self.banner = Some((text, 0.0));
        }
        self.entries.push(Entry {
            line: kill_line(bot_id, headshot),
            headshot,
            age: 0.0,
        });
        if self.entries.len() > KILL_FEED_MAX {
            self.entries.remove(0);
        }
    }

    /// A death ends the multi-kill chain.
    pub fn on_death(&mut self) {
        self.multi = 0;
    }

    /// Age everything by `dt` seconds.
    pub fn advance(&mut self, dt: f32) {
        self.since_kill += dt;
        for e in &mut self.entries {
            e.age += dt;
        }
        self.entries.retain(|e| e.age < KILL_FEED_SECS);
        if let Some((_, age)) = &mut self.banner {
            *age += dt;
        }
        self.banner.take_if(|(_, age)| *age >= BANNER_SECS);
    }

    /// Feed lines, newest first, with whether each was a headshot and its age.
    pub fn lines(&self) -> impl Iterator<Item = (&str, bool, f32)> {
        self.entries
            .iter()
            .rev()
            .map(|e| (e.line.as_str(), e.headshot, e.age))
    }

    /// The banner on screen and its age.
    pub fn banner(&self) -> Option<(&str, f32)> {
        self.banner.as_ref().map(|(t, a)| (t.as_str(), *a))
    }
}

/// The feed, top right; each line fades out over its last second.
pub fn draw_feed(ctx: &egui::Context, feed: &KillFeed) {
    egui::Area::new(egui::Id::new("kill-feed"))
        .anchor(Align2::RIGHT_TOP, [-24.0, 24.0])
        .interactable(false)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                for (line, headshot, age) in feed.lines() {
                    let alpha = (KILL_FEED_SECS - age).clamp(0.0, 1.0);
                    let color = if headshot { WARN } else { GOOD };
                    ui.label(
                        RichText::new(line)
                            .strong()
                            .color(color.gamma_multiply(alpha)),
                    );
                }
            });
        });
}

/// The multi-kill / streak banner, popping in above the crosshair.
pub fn draw_banner(ctx: &egui::Context, feed: &KillFeed) {
    let Some((text, age)) = feed.banner() else {
        return;
    };
    let Some((scale, alpha)) = pop(age, BANNER_SECS) else {
        return;
    };
    egui::Area::new(egui::Id::new("streak-banner"))
        .anchor(Align2::CENTER_CENTER, [0.0, -150.0])
        .interactable(false)
        .show(ctx, |ui| {
            ui.label(big(text.to_uppercase(), 34.0 * scale).color(WARN.gamma_multiply(alpha)));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_lines_name_the_bot_and_mark_headshots() {
        assert_eq!(kill_line(3, true), "You ▶ Bot 3 [HS]");
        assert_eq!(kill_line(12, false), "You ▶ Bot 12");
    }

    #[test]
    fn multi_kills_and_streaks_have_names() {
        assert_eq!(multi_kill_name(1), None);
        assert_eq!(multi_kill_name(2).as_deref(), Some("Double"));
        assert_eq!(multi_kill_name(3).as_deref(), Some("Triple"));
        assert_eq!(multi_kill_name(4).as_deref(), Some("Quadra"));
        assert_eq!(multi_kill_name(6).as_deref(), Some("Multi ×6"));
        assert_eq!(streak_name(2), None);
        assert_eq!(streak_name(3).as_deref(), Some("3 streak"));
        assert_eq!(streak_name(4), None);
        assert_eq!(streak_name(5).as_deref(), Some("5 streak"));
        assert_eq!(streak_name(7), None);
        assert_eq!(streak_name(10).as_deref(), Some("10 streak"));
        assert_eq!(banner_text(1, 1), None);
        assert_eq!(banner_text(1, 3).as_deref(), Some("3 streak"));
        assert_eq!(banner_text(3, 3).as_deref(), Some("Triple"));
    }

    #[test]
    fn quick_kills_chain_and_slow_ones_restart() {
        let mut feed = KillFeed::default();
        feed.on_kill(1, false, 1);
        assert_eq!(feed.banner(), None);
        feed.advance(0.5);
        feed.on_kill(2, true, 2);
        assert_eq!(feed.banner(), Some(("Double", 0.0)));
        feed.advance(MULTI_KILL_WINDOW_SECS + 0.1);
        // Too slow for a triple, but the third kill without dying is a streak.
        feed.on_kill(3, false, 3);
        assert_eq!(feed.banner(), Some(("3 streak", 0.0)));
        feed.advance(0.2);
        feed.on_death();
        feed.on_kill(4, false, 1);
        // The banner of the streak stays; a death leaves no chain to extend.
        assert_eq!(feed.banner(), Some(("3 streak", 0.2)));
        feed.advance(0.2);
        feed.on_kill(5, false, 2);
        assert_eq!(feed.banner(), Some(("Double", 0.0)));
    }

    #[test]
    fn the_feed_keeps_recent_kills_newest_first() {
        let mut feed = KillFeed::default();
        for id in 1..=(KILL_FEED_MAX as u64 + 2) {
            feed.on_kill(id, id == 7, id as u32);
            feed.advance(0.1);
        }
        let lines: Vec<_> = feed.lines().map(|(l, hs, _)| (l.to_owned(), hs)).collect();
        assert_eq!(lines.len(), KILL_FEED_MAX);
        assert_eq!(lines[0], ("You ▶ Bot 7 [HS]".to_owned(), true));
        let mut feed = KillFeed::default();
        feed.on_kill(1, false, 1);
        feed.advance(KILL_FEED_SECS);
        assert_eq!(feed.lines().count(), 0);
        feed.advance(BANNER_SECS);
        assert_eq!(feed.banner(), None);
    }
}
