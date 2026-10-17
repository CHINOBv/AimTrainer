//! Replay viewer: pick a saved run and see where the crosshair went on each target.
//!
//! Presentational: the caller passes the replay list and a `load` function that turns an
//! entry into a core [`ReplayPath`]; [`ReplayViewerState`] remembers the selection and caches
//! the loaded path so a replay is only re-simulated when the selection changes. The plot is
//! painted with egui's painter (no plotting dependency), centered on the target's aim point.

use crate::persistence::replays::ReplayEntry;
use crate::ui::atoms::{BAD, GOOD, WARN};
use aim_core::analytics::{Engagement, ReplayPath};
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, StrokeKind, vec2};
use chrono::{Datelike, Local, TimeZone, Timelike};
use std::path::PathBuf;

const LIST_HEIGHT_PX: f32 = 120.0;
const PLOT_MAX_PX: f32 = 360.0;
const PLOT_MIN_PX: f32 = 160.0;
const PADDING_PX: f32 = 18.0;
/// Margin around the farthest point, as a fraction of the extent.
const EXTENT_MARGIN: f32 = 0.1;
/// The view always shows at least this many target radii around the center.
const MIN_EXTENT_RADII: f32 = 2.0;
/// "Zoom on target" shows this many target radii around the center.
const ZOOM_EXTENT_RADII: f32 = 4.0;
/// Smallest extent, so a perfectly still crosshair still gets a readable plot.
const MIN_EXTENT_DEG: f32 = 0.25;
/// Target grid line count per half axis.
const TICKS_PER_HALF_AXIS: f32 = 4.0;
/// Opacity of the oldest path point; the newest is opaque.
const OLDEST_ALPHA: f32 = 0.15;

/// Selection and the cached path of the selected replay.
#[derive(Debug, Default)]
pub struct ReplayViewerState {
    selected: Option<PathBuf>,
    /// `None` when loading failed.
    path: Option<ReplayPath>,
    /// Index into the path's engagements.
    engagement: usize,
    zoom: bool,
}

impl ReplayViewerState {
    fn select(&mut self, entry: &ReplayEntry, load: &impl Fn(&ReplayEntry) -> Option<ReplayPath>) {
        self.selected = Some(entry.path.clone());
        self.path = load(entry);
        self.engagement = 0;
    }
}

/// Half-width of the square plot, degrees: every path point and shot fits with a margin,
/// and the target is never shown smaller than [`MIN_EXTENT_RADII`] radii.
pub fn plot_extent_deg(e: &Engagement) -> f32 {
    let far = e
        .path_deg
        .iter()
        .chain(e.shots.iter().map(|s| &s.offset_deg))
        .fold(0.0_f32, |m, &(x, y)| m.max(x.abs()).max(y.abs()));
    (far * (1.0 + EXTENT_MARGIN))
        .max(e.radius_deg * MIN_EXTENT_RADII)
        .max(MIN_EXTENT_DEG)
}

/// A 1, 2 or 5 × 10ⁿ grid step giving about [`TICKS_PER_HALF_AXIS`] lines per half axis.
pub fn grid_step_deg(extent_deg: f32) -> f32 {
    let raw = (extent_deg / TICKS_PER_HALF_AXIS).max(1e-6);
    let magnitude = 10f32.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .find(|m| m * magnitude >= raw)
        .unwrap_or(10.0);
    step * magnitude
}

/// Screen position of an offset (degrees, +y up) in a plot centered at `center`.
fn to_screen(offset: (f32, f32), center: Pos2, px_per_deg: f32) -> Pos2 {
    Pos2::new(
        center.x + offset.0 * px_per_deg,
        center.y - offset.1 * px_per_deg,
    )
}

/// Local date and time of a replay, `YYYY-MM-DD HH:MM`.
fn timestamp_label(millis: u64) -> String {
    match Local.timestamp_millis_opt(millis as i64).single() {
        Some(t) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            t.year(),
            t.month(),
            t.day(),
            t.hour(),
            t.minute()
        ),
        None => "unknown date".to_owned(),
    }
}

fn summary(e: &Engagement) -> String {
    let outcome = if e.killed { "killed" } else { "not killed" };
    let mut parts = vec![
        format!(
            "{} shot(s), {} missed, {outcome}",
            e.shots.len(),
            e.misses()
        ),
        format!("{:.2} s", (e.end_secs - e.start_secs).max(0.0)),
        format!("target radius {:.2}°", e.radius_deg),
    ];
    if let Some(r) = e.reaction_secs {
        parts.push(format!("reaction {:.0} ms", r * 1000.0));
    }
    if !e.shots.is_empty() {
        parts.push(format!("{} correction(s)", e.corrections));
    }
    if e.overshot {
        parts.push("overshot".to_owned());
    }
    if e.undershot {
        parts.push("undershot".to_owned());
    }
    parts.join(" · ")
}

fn plot(ui: &mut egui::Ui, e: &Engagement, zoom: bool) {
    let side = ui.available_width().clamp(PLOT_MIN_PX, PLOT_MAX_PX);
    let (response, painter) = ui.allocate_painter(vec2(side, side), Sense::hover());
    let frame = response.rect;
    let visuals = ui.visuals();
    painter.rect_stroke(
        frame,
        2.0,
        visuals.widgets.noninteractive.bg_stroke,
        StrokeKind::Inside,
    );
    let area = frame.shrink(PADDING_PX);
    let painter = painter.with_clip_rect(frame.shrink(1.0));
    let extent = if zoom {
        (e.radius_deg * ZOOM_EXTENT_RADII).max(MIN_EXTENT_DEG)
    } else {
        plot_extent_deg(e)
    };
    let center = area.center();
    let px_per_deg = area.width() / 2.0 / extent;
    let weak = Color32::from_gray(150);
    let font = FontId::proportional(10.0);

    // Grid and axes, in degrees.
    let step = grid_step_deg(extent);
    let grid = Stroke::new(1.0, Color32::from_gray(60));
    let ticks = (extent / step).floor() as i32;
    let decimals = (-step.log10().floor()).max(0.0) as usize;
    for k in -ticks..=ticks {
        let d = k as f32 * step;
        let x = center.x + d * px_per_deg;
        let y = center.y - d * px_per_deg;
        let stroke = if k == 0 {
            Stroke::new(1.0, Color32::from_gray(100))
        } else {
            grid
        };
        painter.line_segment(
            [Pos2::new(x, area.top()), Pos2::new(x, area.bottom())],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(area.left(), y), Pos2::new(area.right(), y)],
            stroke,
        );
        if k != 0 {
            painter.text(
                Pos2::new(x, area.bottom() + 2.0),
                Align2::CENTER_TOP,
                format!("{d:+.decimals$}°"),
                font.clone(),
                weak,
            );
            painter.text(
                Pos2::new(area.left() - 2.0, y),
                Align2::RIGHT_CENTER,
                format!("{d:+.decimals$}°"),
                font.clone(),
                weak,
            );
        }
    }

    // Target aim zone.
    painter.circle_stroke(
        center,
        (e.radius_deg * px_per_deg).max(2.0),
        Stroke::new(1.5, WARN),
    );

    // Crosshair path, older points fading out.
    let line = visuals.widgets.active.fg_stroke.color;
    let points: Vec<Pos2> = e
        .path_deg
        .iter()
        .map(|&o| to_screen(o, center, px_per_deg))
        .collect();
    let last = points.len().saturating_sub(1).max(1) as f32;
    for (i, pair) in points.windows(2).enumerate() {
        let alpha = OLDEST_ALPHA + (1.0 - OLDEST_ALPHA) * (i + 1) as f32 / last;
        painter.line_segment(
            [pair[0], pair[1]],
            Stroke::new(1.5, line.gamma_multiply(alpha)),
        );
    }
    if let Some(start) = points.first() {
        painter.circle_stroke(*start, 3.0, Stroke::new(1.0, weak));
    }

    for s in &e.shots {
        let p = to_screen(s.offset_deg, center, px_per_deg);
        painter.circle_filled(p, 3.5, if s.hit { GOOD } else { BAD });
    }

    response.on_hover_text(
        "Crosshair offset from the target center, in degrees. Yellow: the target. \
         Faded to bright: older to newer. Hollow dot: start. Green: hit. Red: miss.",
    );
}

/// Draw the replay list, the selected replay's engagement plot and its summary.
pub fn replay_viewer(
    ui: &mut egui::Ui,
    state: &mut ReplayViewerState,
    entries: &[ReplayEntry],
    load: impl Fn(&ReplayEntry) -> Option<ReplayPath>,
) {
    if entries.is_empty() {
        ui.small("No replays yet. Every finished run is saved here.");
        return;
    }
    let current = state
        .selected
        .as_ref()
        .and_then(|p| entries.iter().find(|e| &e.path == p));
    if current.is_none() {
        state.select(&entries[0], &load);
    }

    egui::ScrollArea::vertical()
        .id_salt("replay_list")
        .max_height(LIST_HEIGHT_PX)
        .show(ui, |ui| {
            for entry in entries {
                let selected = state.selected.as_ref() == Some(&entry.path);
                let label = format!(
                    "{}  {}",
                    timestamp_label(entry.timestamp_millis),
                    entry.scenario_id
                );
                if ui.selectable_label(selected, label).clicked() && !selected {
                    state.select(entry, &load);
                }
            }
        });
    ui.separator();

    let Some(path) = &state.path else {
        ui.colored_label(BAD, "This replay could not be read.");
        return;
    };
    let count = path.engagements.len();
    if count == 0 {
        ui.small("No engagements in this run.");
        return;
    }
    state.engagement = state.engagement.min(count - 1);
    let mut shown = state.engagement + 1;
    ui.horizontal(|ui| {
        ui.add_enabled(
            count > 1,
            egui::Slider::new(&mut shown, 1..=count).text(format!("of {count} engagements")),
        );
        ui.checkbox(&mut state.zoom, "Zoom on target");
    });
    state.engagement = shown - 1;
    let e = &path.engagements[state.engagement];
    plot(ui, e, state.zoom);
    ui.label(summary(e));
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::analytics::ShotPoint;

    fn engagement(path: Vec<(f32, f32)>, shots: Vec<(f32, f32)>, radius: f32) -> Engagement {
        Engagement {
            target_id: 1,
            radius_deg: radius,
            start_secs: 0.0,
            end_secs: 0.5,
            path_deg: path,
            shots: shots
                .into_iter()
                .map(|o| ShotPoint {
                    offset_deg: o,
                    hit: true,
                })
                .collect(),
            killed: true,
            overshot: false,
            undershot: false,
            corrections: 0,
            reaction_secs: Some(0.2),
        }
    }

    #[test]
    fn the_extent_fits_the_farthest_point_with_a_margin() {
        let e = engagement(vec![(-10.0, 2.0), (0.1, 0.0)], vec![(0.0, 3.0)], 0.5);
        assert!((plot_extent_deg(&e) - 11.0).abs() < 1e-4);
    }

    #[test]
    fn a_small_path_still_shows_the_target_at_a_readable_size() {
        let e = engagement(vec![(0.1, 0.1)], vec![], 1.0);
        assert_eq!(plot_extent_deg(&e), 2.0);
        let e = engagement(vec![(0.0, 0.0)], vec![], 0.01);
        assert_eq!(plot_extent_deg(&e), MIN_EXTENT_DEG);
    }

    #[test]
    fn grid_steps_are_round_numbers() {
        assert_eq!(grid_step_deg(11.0), 5.0);
        assert_eq!(grid_step_deg(4.0), 1.0);
        assert!((grid_step_deg(0.6) - 0.2).abs() < 1e-6);
        assert_eq!(grid_step_deg(40.0), 10.0);
    }

    #[test]
    fn offsets_map_right_and_up_on_screen() {
        let p = to_screen((2.0, 1.0), Pos2::new(100.0, 100.0), 10.0);
        assert_eq!(p, Pos2::new(120.0, 90.0));
    }

    #[test]
    fn summary_names_the_outcome_and_mistakes() {
        let mut e = engagement(vec![(0.0, 0.0)], vec![(0.0, 0.0)], 0.5);
        e.overshot = true;
        e.corrections = 2;
        let s = summary(&e);
        assert!(s.contains("1 shot(s), 0 missed, killed"), "{s}");
        assert!(
            s.contains("2 correction(s)") && s.contains("overshot"),
            "{s}"
        );
        assert!(s.contains("reaction 200 ms"), "{s}");
    }

    #[test]
    fn timestamps_read_as_date_and_time() {
        let s = timestamp_label(1_700_000_000_000);
        assert_eq!(s.len(), "2023-11-14 22:13".len(), "{s}");
        assert!(s.starts_with("2023-11-1"), "{s}");
    }

    #[test]
    fn selecting_loads_once_and_resets_the_engagement() {
        let entry = ReplayEntry {
            path: PathBuf::from("/r/1-x.aimreplay"),
            timestamp_millis: 1,
            scenario_id: "x".into(),
        };
        let mut state = ReplayViewerState {
            engagement: 3,
            ..Default::default()
        };
        let loads = std::cell::Cell::new(0);
        state.select(&entry, &|_| {
            loads.set(loads.get() + 1);
            None
        });
        assert_eq!(loads.get(), 1);
        assert_eq!(state.selected, Some(entry.path));
        assert_eq!(state.engagement, 0);
        assert!(state.path.is_none());
    }
}
