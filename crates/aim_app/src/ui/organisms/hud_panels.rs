//! In-run overlays: run status, movement accuracy meter, spray meter, countdown, pause
//! window.

use crate::ui::UiAction;
use crate::ui::atoms::{BAD, GOOD, WARN, big, pop};
use crate::ui::organisms::health_bar::{self, HealthBar};
use aim_core::scenario::{RunEnd, RunKey, Session, Tuning};
use bevy_egui::egui::{self, Align2, FontId, RichText};

pub fn run_status(ctx: &egui::Context, s: &Session) {
    let stats = s.stats();
    let accuracy = s.result().accuracy * 100.0;
    let hold = s.def().weapon.fire == aim_core::scenario::FireMode::Hold;
    egui::Area::new(egui::Id::new("hud"))
        .anchor(Align2::CENTER_TOP, [0.0, 16.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(big(format!("{:.1}", s.remaining()), 28.0));
                let key = RunKey::parse(&s.def().id);
                if key.level != 0.0 {
                    ui.small(format!("difficulty {:+.2}", key.level));
                }
                if !key.tuning.is_default() {
                    ui.small(RichText::new("tuned").color(WARN))
                        .on_hover_text(crate::game::tuning_text(&key.tuning));
                }
                ui.label(format!(
                    "{}  ·  score {:.0}  ·  {accuracy:.0}% acc  ·  {} kills",
                    s.def().name,
                    stats.score.max(0.0),
                    stats.kills
                ));
                if hold {
                    ui.label(format!(
                        "on target {:.1}s / firing {:.1}s",
                        stats.on_target_secs, stats.hold_secs
                    ));
                }
                let names: Vec<&str> = s
                    .def()
                    .phases
                    .iter()
                    .flatten()
                    .map(|p| p.name.as_str())
                    .collect();
                if let Some(text) =
                    progress_label(s.phase_index(), &names, s.def().end, stats.kills)
                {
                    ui.label(RichText::new(text).color(WARN));
                }
            });
        });
}

/// The current phase of a multi-phase run and the progress towards a kills goal, if the
/// scenario has either. The timer above still shows the time left before the cap.
fn progress_label(phase: Option<usize>, names: &[&str], end: RunEnd, kills: u32) -> Option<String> {
    let phase = phase.filter(|_| !names.is_empty()).map(|i| {
        let name = names.get(i).copied().unwrap_or_default();
        let head = format!("phase {}/{}", i + 1, names.len());
        if name.is_empty() {
            head
        } else {
            format!("{head} · {name}")
        }
    });
    let goal = match end {
        RunEnd::Kills { kills: goal } => Some(format!("{kills}/{goal} kills")),
        RunEnd::Time | RunEnd::Endless => None,
    };
    match (phase, goal) {
        (Some(p), Some(g)) => Some(format!("{p}  ·  {g}")),
        (p, g) => p.or(g),
    }
}

pub fn movement_meter(ctx: &egui::Context, s: &Session) {
    let limit = s.movement_profile().accuracy_speed_limit();
    let (text, color) = if s.is_accurate() {
        ("ACCURATE", GOOD)
    } else {
        ("MOVING", BAD)
    };
    egui::Area::new(egui::Id::new("speed"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -40.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(text)
                        .color(color)
                        .strong()
                        .font(FontId::proportional(20.0)),
                );
                ui.label(format!(
                    "{:.2} m/s  (accurate ≤ {limit:.2})  ·  moving shots {}",
                    s.player_speed(),
                    s.stats().moving_shots
                ));
            });
        });
}

/// Recoil scenarios only: the bullet index of the burst in progress, a green READY when the
/// next shot is a fully recovered first bullet, and the recovery countdown otherwise.
pub fn spray_meter(ctx: &egui::Context, s: &Session) {
    let (Some(profile), Some(remaining)) = (s.recoil_profile(), s.recoil_recovery_remaining())
    else {
        return;
    };
    let bullet = s.burst_index();
    let protected = profile.protected_bullets;
    egui::Area::new(egui::Id::new("spray"))
        .anchor(Align2::RIGHT_BOTTOM, [-24.0, -24.0])
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                if bullet == 0 {
                    ui.label(
                        RichText::new("READY")
                            .color(GOOD)
                            .strong()
                            .font(FontId::proportional(20.0)),
                    );
                    ui.label("next shot: first bullet");
                } else {
                    let color = if bullet < protected { GOOD } else { BAD };
                    ui.label(
                        RichText::new(format!("BULLET {bullet}"))
                            .color(color)
                            .strong()
                            .font(FontId::proportional(20.0)),
                    );
                    ui.label(format!(
                        "reset in {remaining:.2}s  ·  protected {protected}"
                    ));
                }
                let st = s.stats();
                ui.small(format!(
                    "bursts {}  ·  avg {:.1}  ·  first bullets {:.0}%",
                    st.bursts,
                    st.avg_burst_len(),
                    st.first_bullet_share() * 100.0
                ));
            });
        });
}

/// Countdown before a run, with a reminder of the restart key (`restart`, e.g. "R"). With
/// `animate`, each second's number pops in and fades out.
pub fn countdown(ctx: &egui::Context, secs: f32, restart: &str, animate: bool) {
    let (scale, alpha) = if animate {
        countdown_pop(secs)
    } else {
        (1.0, 1.0)
    };
    egui::Area::new(egui::Id::new("countdown"))
        .anchor(Align2::CENTER_CENTER, [0.0, -80.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    big(format!("{}", secs.ceil() as i32), 56.0 * scale)
                        .color(egui::Color32::WHITE.gamma_multiply(alpha)),
                );
                ui.small(format!("{restart}: restart"));
            });
        });
}

/// The animated countdown number never fades below this opacity.
const COUNTDOWN_MIN_ALPHA: f32 = 0.35;
/// How long the "GO" pop and the end-of-round banner stay, seconds.
pub const GO_SECS: f32 = 0.7;
pub const ROUND_END_SECS: f32 = 1.0;

/// Scale and opacity of the countdown number with `secs` left: each second pops in large
/// and fades towards [`COUNTDOWN_MIN_ALPHA`] as it runs out.
pub fn countdown_pop(secs: f32) -> (f32, f32) {
    let into_second = (secs.ceil() - secs).clamp(0.0, 0.999);
    let (scale, alpha) = pop(into_second, 1.0).unwrap_or((1.0, 1.0));
    (scale, alpha.max(COUNTDOWN_MIN_ALPHA))
}

/// "GO", popping in at the crosshair, `age` seconds after the round started.
pub fn go_pop(ctx: &egui::Context, age: f32) {
    let Some((scale, alpha)) = pop(age, GO_SECS) else {
        return;
    };
    egui::Area::new(egui::Id::new("go-pop"))
        .anchor(Align2::CENTER_CENTER, [0.0, -80.0])
        .interactable(false)
        .show(ctx, |ui| {
            ui.label(big("GO", 64.0 * scale).color(GOOD.gamma_multiply(alpha)));
        });
}

/// The end-of-round banner, `age` seconds after the round ended, with the run's kills and
/// deaths.
pub fn round_end_banner(ctx: &egui::Context, age: f32, kills: u32, deaths: u32) {
    let Some((scale, alpha)) = pop(age, ROUND_END_SECS) else {
        return;
    };
    let alpha = alpha.max(COUNTDOWN_MIN_ALPHA);
    egui::Area::new(egui::Id::new("round-end"))
        .anchor(Align2::CENTER_CENTER, [0.0, -80.0])
        .interactable(false)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(big("ROUND OVER", 48.0 * scale).color(WARN.gamma_multiply(alpha)));
                ui.label(
                    big(format!("{kills} kills · {deaths} deaths"), 20.0)
                        .color(egui::Color32::WHITE.gamma_multiply(alpha)),
                );
            });
        });
}

/// Restart options of a library scenario run's pause window: floor height and target
/// tuning, applied together by "Apply and restart".
pub struct RestartPick<'a> {
    /// Height the run is played at, meters.
    pub current_elevation: f32,
    /// Height being chosen, meters; edited by the picker.
    pub elevation: &'a mut f32,
    /// Tuning the run is played with.
    pub current_tuning: Tuning,
    /// Tuning being chosen; edited by the steppers.
    pub tuning: &'a mut Tuning,
    /// Whether the size knob changes this scenario (spheres; agents keep their size).
    pub size_applies: bool,
}

/// Step of the floor-height picker, meters.
const FLOOR_STEP: f32 = 0.5;

/// Pause window, with a restart button labelled with the restart key (`restart`). With
/// `pick`, it also offers a floor-height picker, target tuning and "Apply and restart".
pub fn pause_window(
    ctx: &egui::Context,
    description: &str,
    restart: &str,
    pick: Option<RestartPick>,
) -> Option<UiAction> {
    let mut action = None;
    egui::Window::new("Paused")
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(description);
            ui.separator();
            if let Some(pick) = pick {
                action = restart_options(ui, pick);
                ui.separator();
            }
            if ui.button("Resume (Esc)").clicked() {
                action = Some(UiAction::Resume);
            }
            if ui
                .button(format!("Restart ({restart})"))
                .on_hover_text("Start this run again as it is; nothing is recorded")
                .clicked()
            {
                action = Some(UiAction::Restart);
            }
            if ui.button("Quit to menu").clicked() {
                action = Some(UiAction::BackToMenu);
            }
        });
    action
}

/// Floor height, target tuning and "Apply and restart".
fn restart_options(ui: &mut egui::Ui, pick: RestartPick) -> Option<UiAction> {
    let RestartPick {
        current_elevation,
        elevation,
        current_tuning,
        tuning,
        size_applies,
    } = pick;
    let range = aim_core::scenario::def::ELEVATION_RANGE;
    ui.horizontal(|ui| {
        ui.label("Floor height");
        if ui.small_button("−").clicked() {
            *elevation = (*elevation - FLOOR_STEP).max(range.min);
        }
        ui.monospace(format!("{:+.1} m", *elevation));
        if ui.small_button("+").clicked() {
            *elevation = (*elevation + FLOOR_STEP).min(range.max);
        }
    });
    ui.small("Your floor relative to the targets' floor.");
    ui.add_space(4.0);
    egui::Grid::new("tuning").num_columns(2).show(ui, |ui| {
        ui.add_enabled_ui(size_applies, |ui| ui.label("Target size"))
            .response
            .on_disabled_hover_text("Agents keep their VALORANT size; use Distance instead.");
        ui.add_enabled_ui(size_applies, |ui| percent_stepper(ui, &mut tuning.size_pct));
        ui.end_row();
        ui.label("Distance");
        percent_stepper(ui, &mut tuning.distance_pct);
        ui.end_row();
        ui.label("Target speed");
        percent_stepper(ui, &mut tuning.speed_pct);
        ui.end_row();
    });
    if !size_applies {
        ui.small("Agents keep their VALORANT size; change Distance instead.");
    }
    ui.small("Tuned runs are saved apart and never move your adaptive level or benchmark.");
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !tuning.is_default(),
                egui::Button::new("Reset to scenario defaults"),
            )
            .clicked()
        {
            *tuning = Tuning::DEFAULT;
        }
        let changed = (*elevation - current_elevation).abs() > 1e-3 || *tuning != current_tuning;
        if ui
            .add_enabled(changed, egui::Button::new("Apply and restart"))
            .clicked()
        {
            action = Some(UiAction::RestartTuned {
                elevation: *elevation,
                tuning: *tuning,
            });
        }
    });
    action
}

/// − value % + stepper over the tuning range.
fn percent_stepper(ui: &mut egui::Ui, pct: &mut u16) {
    ui.horizontal(|ui| {
        if ui.small_button("−").clicked() {
            *pct = pct.saturating_sub(Tuning::STEP_PCT).max(Tuning::MIN_PCT);
        }
        ui.monospace(format!("{:>3}%", *pct));
        if ui.small_button("+").clicked() {
            *pct = (*pct + Tuning::STEP_PCT).min(Tuning::MAX_PCT);
        }
    });
}

/// While the spawn grace lasts, say why the player stands where they do: a respawn after a
/// death (deaths otherwise look like a teleport) or the protected entry spawn.
fn spawn_note(ctx: &egui::Context, s: &aim_world::arena::ArenaSession) {
    let left = s.spawn_protection_secs();
    if left <= 0.0 {
        return;
    }
    egui::Area::new(egui::Id::new("arena-spawn-note"))
        .anchor(Align2::CENTER_CENTER, [0.0, 90.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                if s.stats().deaths > 0 {
                    ui.label(big("You died · respawned", 26.0).color(BAD));
                }
                ui.label(format!("Spawn protection {left:.1} s"));
            });
        });
}

/// What the arena status draws besides the session itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArenaStatusExtras {
    /// The health bar instead of the plain HP number, when it is on.
    pub health_bar: Option<HealthBar>,
    /// The death screen is up, so the plain "You died" note stays hidden.
    pub death_screen: bool,
}

/// Arena status: time, health, kills/deaths, the site objective (when the arena has one)
/// and a respawn note during the spawn grace (unless the death screen is up).
pub fn arena_status(
    ctx: &egui::Context,
    s: &aim_world::arena::ArenaSession,
    extras: &ArenaStatusExtras,
) {
    let stats = s.stats();
    egui::Area::new(egui::Id::new("arena-hud"))
        .anchor(Align2::CENTER_TOP, [0.0, 16.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(big(format!("{:.1}", s.remaining()), 28.0));
                ui.label(format!(
                    "{}  ·  {} kills  ·  {} deaths",
                    s.def().name,
                    stats.kills,
                    stats.deaths
                ));
                if s.is_practice() {
                    ui.colored_label(GOOD, "PRACTICE · bots don't shoot");
                }
                if let Some(o) = s.objective() {
                    let site = if o.capture_progress > 0.0 {
                        RichText::new(format!("SITE CONTESTED {:.0}%", o.capture_progress * 100.0))
                            .color(BAD)
                            .strong()
                    } else {
                        RichText::new("Site held").color(GOOD)
                    };
                    ui.label(site);
                    ui.label(format!(
                        "{} of {} attackers left",
                        o.attackers_left, o.attackers
                    ));
                }
            });
        });
    let health = s.player_health();
    let color = if health > 50.0 { GOOD } else { BAD };
    egui::Area::new(egui::Id::new("arena-health"))
        .anchor(Align2::LEFT_BOTTOM, [24.0, -24.0])
        .show(ctx, |ui| {
            if let Some(bar) = &extras.health_bar {
                health_bar::draw(ui, bar);
            } else {
                ui.label(
                    RichText::new(format!("{health:.0} HP"))
                        .color(color)
                        .strong()
                        .font(FontId::proportional(26.0)),
                );
            }
            if !s.is_accurate() {
                ui.colored_label(BAD, "MOVING");
            }
        });
    if !extras.death_screen {
        spawn_note(ctx, s);
    }
}

/// Distance of an off-screen arrow's tip from the screen edge, points.
const ARROW_INSET: f32 = 28.0;
/// Length of each arm of an off-screen arrow, points.
const ARROW_SIZE: f32 = 14.0;

/// What [`offscreen_arrows`] needs to know about the camera.
pub struct ArrowCamera {
    pub eye: aim_core::math::Vec3,
    pub view: aim_core::math::ViewAngles,
    /// Current vertical FOV (including ADS zoom), degrees.
    pub vfov_deg: f32,
    /// Width / height of the 3D viewport.
    pub aspect: f32,
}

/// Where the off-screen arrows of a scenario point: every live, visible target, at its head
/// for humanoids and its center for spheres.
pub fn scenario_arrow_points(s: &Session) -> Vec<aim_core::math::Vec3> {
    let head = s.dims().head_center_height();
    let humanoid = matches!(
        s.def().targets.shape,
        aim_core::scenario::Shape::Humanoid { .. }
    );
    s.targets()
        .filter(|t| t.health > 0.0)
        .map(|t| {
            let mut p = t.position;
            if humanoid {
                p.y += head;
            }
            p
        })
        .collect()
}

/// Where the off-screen arrows of an arena point: the head of every live bot in the
/// player's line of sight.
pub fn arena_arrow_points(
    bots: impl IntoIterator<Item = aim_world::arena::BotView>,
) -> Vec<aim_core::math::Vec3> {
    bots.into_iter()
        .filter(|b| b.alive && b.visible_to_player)
        .map(|b| b.head())
        .collect()
}

/// A small semi-transparent chevron at the screen edge for every one of `points` outside
/// the current view, pointing towards it.
pub fn offscreen_arrows(
    ctx: &egui::Context,
    points: &[aim_core::math::Vec3],
    cam: &ArrowCamera,
    rgb: [f32; 3],
) {
    let half_v = cam.vfov_deg / 2.0;
    let half_h = aim_core::fov::horizontal_from_vertical(cam.vfov_deg as f64, cam.aspect as f64)
        as f32
        / 2.0;
    let screen = ctx.content_rect();
    let (w, h) = aim_core::fov::valorant_viewport(screen.width() as f64, screen.height() as f64);
    let viewport = egui::Rect::from_center_size(screen.center(), egui::vec2(w as f32, h as f32));
    let [r, g, b] = rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    let stroke = egui::Stroke::new(3.0, egui::Color32::from_rgba_unmultiplied(r, g, b, 150));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("offscreen_arrows"),
    ));
    for &p in points {
        let Some(bearing) = aim_core::fov::offscreen_bearing(cam.view, cam.eye, p, half_h, half_v)
        else {
            continue;
        };
        let tip = edge_point(viewport, bearing, ARROW_INSET);
        let dir = egui::vec2(bearing.cos(), -bearing.sin());
        let side = egui::vec2(-dir.y, dir.x);
        let back = tip - dir * ARROW_SIZE;
        painter.line_segment([back + side * ARROW_SIZE * 0.8, tip], stroke);
        painter.line_segment([back - side * ARROW_SIZE * 0.8, tip], stroke);
    }
}

/// Where a ray from the center of `rect` along `bearing` (radians, 0 = right,
/// counter-clockwise) meets the rectangle shrunk by `inset` on every side.
fn edge_point(rect: egui::Rect, bearing: f32, inset: f32) -> egui::Pos2 {
    let c = rect.center();
    let hx = (rect.width() / 2.0 - inset).max(0.0);
    let hy = (rect.height() / 2.0 - inset).max(0.0);
    let (dx, dy) = (bearing.cos(), -bearing.sin());
    let tx = if dx.abs() > 1e-6 {
        hx / dx.abs()
    } else {
        f32::INFINITY
    };
    let ty = if dy.abs() > 1e-6 {
        hy / dy.abs()
    } else {
        f32::INFINITY
    };
    c + egui::vec2(dx, dy) * tx.min(ty)
}

#[cfg(test)]
mod progress_tests {
    use super::progress_label;
    use aim_core::scenario::RunEnd;

    #[test]
    fn phased_and_kill_goal_runs_show_their_progress() {
        assert_eq!(
            progress_label(Some(1), &["Close trio", "Far peeker"], RunEnd::Time, 4),
            Some("phase 2/2 · Far peeker".to_owned())
        );
        assert_eq!(
            progress_label(Some(0), &["", ""], RunEnd::Time, 0),
            Some("phase 1/2".to_owned())
        );
        assert_eq!(
            progress_label(None, &[], RunEnd::Kills { kills: 30 }, 12),
            Some("12/30 kills".to_owned())
        );
        assert_eq!(
            progress_label(
                Some(0),
                &["Warm-up", "Race"],
                RunEnd::Kills { kills: 30 },
                3
            ),
            Some("phase 1/2 · Warm-up  ·  3/30 kills".to_owned())
        );
        assert_eq!(progress_label(None, &[], RunEnd::Time, 5), None);
        assert_eq!(progress_label(None, &[], RunEnd::Endless, 5), None);
    }
}

#[cfg(test)]
mod countdown_tests {
    use super::{COUNTDOWN_MIN_ALPHA, countdown_pop};

    #[test]
    fn each_countdown_second_pops_in_and_fades() {
        let (start, start_alpha) = countdown_pop(3.0);
        assert!(start > 1.3 && (start_alpha - 1.0).abs() < 1e-4, "{start}");
        let (mid, mid_alpha) = countdown_pop(2.5);
        assert!((mid - 1.0).abs() < 1e-4 && (mid_alpha - 1.0).abs() < 1e-4);
        let (_, late) = countdown_pop(2.01);
        assert!((COUNTDOWN_MIN_ALPHA..1.0).contains(&late), "{late}");
        // The next second pops again.
        assert!(countdown_pop(1.999).0 > 1.3);
    }
}

#[cfg(test)]
mod offscreen_tests {
    use super::{arena_arrow_points, edge_point, scenario_arrow_points};
    use aim_core::math::{Vec3, ViewAngles};
    use aim_core::scenario::{FrameInput, Session, Shape, builtin};
    use aim_world::arena::BotView;
    use bevy_egui::egui::{Pos2, Rect};
    use std::f32::consts::{FRAC_PI_2, PI};

    /// A scenario a moment after it starts, with its first targets out.
    fn started(id: &str) -> Session {
        let def = builtin::all().into_iter().find(|d| d.id == id).unwrap();
        let mut s = Session::new(def, 7);
        s.update(FrameInput {
            dt: 0.05,
            view: ViewAngles::new(180.0, 0.0),
            ..Default::default()
        });
        s
    }

    #[test]
    fn scenario_arrows_point_at_live_targets_as_before() {
        let mut seen_humanoid = false;
        for def in builtin::all() {
            let s = started(&def.id);
            let humanoid = matches!(s.def().targets.shape, Shape::Humanoid { .. });
            seen_humanoid |= humanoid;
            let head = s.dims().head_center_height();
            let expected: Vec<Vec3> = s
                .targets()
                .filter(|t| t.health > 0.0)
                .map(|t| {
                    let mut p = t.position;
                    if humanoid {
                        p.y += head;
                    }
                    p
                })
                .collect();
            assert_eq!(scenario_arrow_points(&s), expected, "{}", def.id);
        }
        assert!(seen_humanoid, "the check covers humanoid targets too");
    }

    #[test]
    fn arena_arrows_point_at_the_heads_of_live_bots_in_sight() {
        let bot = |id, alive, visible_to_player| BotView {
            id,
            feet: Vec3::new(id as f32, 0.0, -5.0),
            facing_yaw_deg: 0.0,
            alive,
            visible_to_player,
        };
        let bots = [
            bot(1, true, true),
            bot(2, true, false),
            bot(3, false, false),
            bot(4, true, true),
        ];
        assert_eq!(
            arena_arrow_points(bots),
            vec![bots[0].head(), bots[3].head()]
        );
        assert!(arena_arrow_points([]).is_empty());
    }

    #[test]
    fn arrows_sit_on_the_inset_screen_edge_towards_the_bearing() {
        let r = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 100.0));
        let close = |a: Pos2, b: Pos2| (a - b).length() < 1e-3;
        assert!(close(edge_point(r, 0.0, 10.0), Pos2::new(190.0, 50.0)));
        assert!(close(edge_point(r, PI, 10.0), Pos2::new(10.0, 50.0)));
        // Screen y grows downwards, so "up" (π/2) is the top edge.
        assert!(close(
            edge_point(r, FRAC_PI_2, 10.0),
            Pos2::new(100.0, 10.0)
        ));
        assert!(close(
            edge_point(r, -FRAC_PI_2, 10.0),
            Pos2::new(100.0, 90.0)
        ));
        // A diagonal hits the nearer (top) edge first on a wide screen.
        let p = edge_point(r, PI / 4.0, 10.0);
        assert!((p.y - 10.0).abs() < 1e-3 && p.x > 100.0, "{p:?}");
    }
}
