//! Arena death recap card: one line on why the player died and three drawn icons with
//! numbers (who saw whom first, crosshair placement at first sight, first-shot time or
//! moving shots). The data comes from `aim_world::arena::DeathRecap`; this only draws it.

use crate::ui::atoms::{BAD, GOOD, WARN, big};
use aim_world::arena::{DeathCause, DeathRecap, RECAP_PLACEMENT_DEG, RECAP_SLOW_SHOT_SECS};
use bevy_egui::egui::{self, Align2, Color32, Pos2, Sense, Stroke, Vec2};

/// Crosshair error at first sight under which placement shows as good, degrees: the
/// arena's pre-aim tolerance.
const PLACED_DEG: f32 = 5.0;
/// First-shot time under which it shows as quick, seconds (a design choice: about a
/// human visual reaction plus a few frames).
const QUICK_SHOT_SECS: f32 = 0.3;

/// The drawn icon of a chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Who saw whom first.
    Eye,
    /// Crosshair placement.
    Crosshair,
    /// First-shot time.
    Clock,
    /// Shots fired while moving.
    Run,
}

/// One icon with its number.
#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub icon: Icon,
    pub text: String,
    pub color: Color32,
}

/// What the card shows.
#[derive(Debug, Clone, PartialEq)]
pub struct RecapView {
    pub headline: String,
    pub chips: Vec<Chip>,
    /// The small line under the chips: killer skill, distance, damage both ways.
    pub detail: String,
}

fn ms(secs: f32) -> String {
    format!("{:.0} ms", secs * 1000.0)
}

fn headline(r: &DeathRecap) -> String {
    match r.cause {
        DeathCause::MovingShots => format!(
            "Shooting while moving ({} of {} shots)",
            r.moving_shots, r.shots
        ),
        DeathCause::Placement => format!(
            "Crosshair {:.0}° off when it appeared",
            r.crosshair_error_deg.unwrap_or_default()
        ),
        DeathCause::SlowShot => match r.first_shot_delay_secs {
            Some(d) => format!("First shot {} after seeing it", ms(d)),
            None => "Never fired after seeing it".into(),
        },
        DeathCause::Unseen => "Shot by an enemy you never saw".into(),
        DeathCause::Deadline => "Too slow: it lived past the deadline".into(),
        DeathCause::Outaimed => "Placed and quick: lost the aim duel".into(),
    }
}

/// Green, yellow or red for a value where lower is better.
fn grade(value: f32, good: f32, bad: f32) -> Color32 {
    if value < good {
        GOOD
    } else if value < bad {
        WARN
    } else {
        BAD
    }
}

/// The card for a recap.
pub fn view(r: &DeathRecap) -> RecapView {
    let eye = match (r.crosshair_error_deg.is_some(), r.saw_first) {
        (false, _) => ("Unseen".to_string(), BAD),
        (true, true) => ("Saw first".to_string(), GOOD),
        (true, false) => ("Seen first".to_string(), BAD),
    };
    let mut chips = vec![Chip {
        icon: Icon::Eye,
        text: eye.0,
        color: eye.1,
    }];
    if let Some(e) = r.crosshair_error_deg {
        chips.push(Chip {
            icon: Icon::Crosshair,
            text: format!("{e:.0}°"),
            color: grade(e, PLACED_DEG, RECAP_PLACEMENT_DEG),
        });
    }
    if r.cause == DeathCause::MovingShots {
        chips.push(Chip {
            icon: Icon::Run,
            text: format!("{}/{} moving", r.moving_shots, r.shots),
            color: BAD,
        });
    } else if r.crosshair_error_deg.is_some() {
        chips.push(match r.first_shot_delay_secs {
            Some(d) => Chip {
                icon: Icon::Clock,
                text: ms(d),
                color: grade(d, QUICK_SHOT_SECS, RECAP_SLOW_SHOT_SECS),
            },
            None => Chip {
                icon: Icon::Clock,
                text: "no shot".into(),
                color: BAD,
            },
        });
    }
    RecapView {
        headline: headline(r),
        chips,
        detail: format!(
            "{} bot · {:.0} m · dealt {:.0} / took {:.0}",
            r.killer_skill, r.distance_m, r.damage_dealt, r.damage_taken
        ),
    }
}

/// Paint `icon` in a square of side `size` centered at `c`.
fn paint_icon(p: &egui::Painter, icon: Icon, c: Pos2, size: f32, color: Color32) {
    let r = size / 2.0;
    let stroke = Stroke::new(1.6, color);
    match icon {
        Icon::Eye => {
            let lid = |sign: f32| {
                (0..=12)
                    .map(|i| {
                        let t = i as f32 / 12.0 * 2.0 - 1.0;
                        c + Vec2::new(t * r, sign * (1.0 - t * t) * r * 0.55)
                    })
                    .collect::<Vec<_>>()
            };
            p.add(egui::Shape::line(lid(-1.0), stroke));
            p.add(egui::Shape::line(lid(1.0), stroke));
            p.circle_filled(c, r * 0.28, color);
        }
        Icon::Crosshair => {
            p.circle_stroke(c, r * 0.65, stroke);
            for d in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
                p.line_segment([c + d * r * 0.35, c + d * r], stroke);
            }
        }
        Icon::Clock => {
            p.circle_stroke(c, r * 0.85, stroke);
            p.line_segment([c, c + Vec2::new(0.0, -r * 0.6)], stroke);
            p.line_segment([c, c + Vec2::new(r * 0.45, 0.0)], stroke);
        }
        Icon::Run => {
            // Three speed lines and an arrow head: motion to the right.
            for (i, y) in [-0.5f32, 0.0, 0.5].into_iter().enumerate() {
                let x0 = -r + i as f32 * r * 0.2;
                p.line_segment(
                    [c + Vec2::new(x0, y * r), c + Vec2::new(r * 0.3, y * r)],
                    stroke,
                );
            }
            p.line_segment(
                [c + Vec2::new(r * 0.3, -r * 0.6), c + Vec2::new(r, 0.0)],
                stroke,
            );
            p.line_segment(
                [c + Vec2::new(r * 0.3, r * 0.6), c + Vec2::new(r, 0.0)],
                stroke,
            );
        }
    }
}

fn chip(ui: &mut egui::Ui, c: &Chip) {
    ui.horizontal(|ui| {
        let size = 18.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        paint_icon(ui.painter(), c.icon, rect.center(), size, c.color);
        ui.label(big(&c.text, 17.0).color(c.color));
    });
}

/// Draw the card under the death screen.
pub fn draw(ctx: &egui::Context, view: &RecapView) {
    egui::Area::new(egui::Id::new("death-recap"))
        .anchor(Align2::CENTER_CENTER, [0.0, 215.0])
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(170))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(big(&view.headline, 20.0).color(WARN));
                        ui.horizontal(|ui| {
                            for (i, c) in view.chips.iter().enumerate() {
                                if i > 0 {
                                    ui.add_space(14.0);
                                }
                                chip(ui, c);
                            }
                        });
                        ui.weak(&view.detail);
                    });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recap(cause: DeathCause) -> DeathRecap {
        DeathRecap {
            at_secs: 4.0,
            killer_id: 2,
            killer_skill: "veteran",
            distance_m: 12.4,
            saw_first: false,
            crosshair_error_deg: Some(18.2),
            first_shot_delay_secs: Some(0.42),
            shots: 4,
            moving_shots: 3,
            damage_dealt: 40.0,
            damage_taken: 160.0,
            cause,
        }
    }

    #[test]
    fn the_card_leads_with_the_cause_and_its_number() {
        let v = view(&recap(DeathCause::Placement));
        assert_eq!(v.headline, "Crosshair 18° off when it appeared");
        let icons: Vec<Icon> = v.chips.iter().map(|c| c.icon).collect();
        assert_eq!(icons, [Icon::Eye, Icon::Crosshair, Icon::Clock]);
        assert_eq!(v.chips[0].text, "Seen first");
        assert_eq!(v.chips[1].color, BAD);
        assert_eq!(v.chips[2].text, "420 ms");
        assert_eq!(v.chips[2].color, WARN);
        assert_eq!(v.detail, "veteran bot · 12 m · dealt 40 / took 160");
    }

    #[test]
    fn moving_shots_replace_the_clock_and_unseen_killers_show_one_chip() {
        let v = view(&recap(DeathCause::MovingShots));
        assert_eq!(v.headline, "Shooting while moving (3 of 4 shots)");
        assert_eq!(v.chips[2].icon, Icon::Run);
        let unseen = DeathRecap {
            crosshair_error_deg: None,
            first_shot_delay_secs: None,
            ..recap(DeathCause::Unseen)
        };
        let v = view(&unseen);
        assert_eq!(v.chips.len(), 1);
        assert_eq!(v.chips[0].text, "Unseen");
    }
}
