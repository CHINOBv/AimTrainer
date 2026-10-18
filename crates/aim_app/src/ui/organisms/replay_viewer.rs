//! Replay viewer: pick a saved run and see where the crosshair went on each target.
//!
//! Presentational: the caller passes the replay list and a `load` function that turns an
//! entry into a core [`ReplayPath`]; [`ReplayViewerState`] remembers the selection and caches
//! the loaded path so a replay is only re-simulated when the selection changes. The plot is
//! painted with egui's painter (no plotting dependency), centered on the target's aim point.
//!
//! Two views: the crosshair path of one engagement, coloured by crosshair speed, and a
//! heatmap of the whole run's shots around a normalized target. Either view can be shown
//! side by side with an earlier run of the same scenario ([`super::replay_compare`]).

use super::replay_compare::{self, ReplayCompareState};
use crate::persistence::replays::{self, ReplayEntry};
use crate::ui::atoms::speed_ramp::ramp_color;
use crate::ui::atoms::{BAD, GOOD, WARN};
use aim_core::analytics::path::{
    FAST_SPEED_DEG_PER_SEC, HEATMAP_BINS, HEATMAP_EXTENT_RADII, SLOW_SPEED_DEG_PER_SEC,
    shot_heatmap, speed_scale,
};
use aim_core::analytics::{Engagement, ReplayPath, ShotHeatmap};
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, vec2};
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
/// Half-size of the "×" marking a missed shot, pixels.
const MISS_MARK_PX: f32 = 3.5;

/// What the plot shows.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// The crosshair path of one engagement.
    #[default]
    Path,
    /// Every shot of the run around a normalized target.
    Heatmap,
}

/// Which shots the heatmap counts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ShotFilter {
    #[default]
    All,
    Hits,
    Misses,
}

impl ShotFilter {
    /// Shots of cell `i` this filter counts.
    fn cell_count(self, h: &ShotHeatmap, i: usize) -> u32 {
        match self {
            Self::All => h.hits[i] + h.misses[i],
            Self::Hits => h.hits[i],
            Self::Misses => h.misses[i],
        }
    }

    /// Largest counted cell.
    fn max_count(self, h: &ShotHeatmap) -> u32 {
        (0..h.hits.len())
            .map(|i| self.cell_count(h, i))
            .max()
            .unwrap_or(0)
    }

    /// Counted shots beyond the grid.
    fn outside(self, h: &ShotHeatmap) -> u32 {
        match self {
            Self::All => h.outside_hits + h.outside_misses,
            Self::Hits => h.outside_hits,
            Self::Misses => h.outside_misses,
        }
    }
}

/// Selection, view options and the cached path of the selected replay.
#[derive(Debug, Default)]
pub struct ReplayViewerState {
    selected: Option<PathBuf>,
    /// `None` when loading failed.
    path: Option<ReplayPath>,
    /// Index into the path's engagements.
    engagement: usize,
    zoom: bool,
    mode: ViewMode,
    filter: ShotFilter,
    compare: ReplayCompareState,
}

impl ReplayViewerState {
    fn select(&mut self, entry: &ReplayEntry, load: &impl Fn(&ReplayEntry) -> Option<ReplayPath>) {
        self.selected = Some(entry.path.clone());
        self.path = load(entry);
        self.engagement = 0;
        self.compare.clear();
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
pub(crate) fn timestamp_label(millis: u64) -> String {
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

/// A framed square plot sized to the available width. Returns the hover response, a
/// painter clipped to the frame, and the inner drawing area.
fn plot_frame(ui: &mut egui::Ui) -> (egui::Response, egui::Painter, Rect) {
    let side = ui.available_width().clamp(PLOT_MIN_PX, PLOT_MAX_PX);
    let (response, painter) = ui.allocate_painter(vec2(side, side), Sense::hover());
    let frame = response.rect;
    painter.rect_stroke(
        frame,
        2.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        StrokeKind::Inside,
    );
    let painter = painter.with_clip_rect(frame.shrink(1.0));
    (response, painter, frame.shrink(PADDING_PX))
}

/// Grid lines and axis labels over `area`, spanning `±extent` around its center, each
/// label suffixed with `unit`. Returns pixels per unit.
fn paint_grid(painter: &egui::Painter, area: Rect, extent: f32, unit: &str) -> f32 {
    let center = area.center();
    let px_per_unit = area.width() / 2.0 / extent;
    let weak = Color32::from_gray(150);
    let font = FontId::proportional(10.0);
    let step = grid_step_deg(extent);
    let grid = Stroke::new(1.0, Color32::from_gray(60));
    let ticks = (extent / step).floor() as i32;
    let decimals = (-step.log10().floor()).max(0.0) as usize;
    for k in -ticks..=ticks {
        let d = k as f32 * step;
        let x = center.x + d * px_per_unit;
        let y = center.y - d * px_per_unit;
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
                format!("{d:+.decimals$}{unit}"),
                font.clone(),
                weak,
            );
            painter.text(
                Pos2::new(area.left() - 2.0, y),
                Align2::RIGHT_CENTER,
                format!("{d:+.decimals$}{unit}"),
                font.clone(),
                weak,
            );
        }
    }
    px_per_unit
}

/// A shot marker: a filled, outlined dot for a hit and a cross for a miss, so the two
/// differ by shape as well as colour.
fn paint_shot(painter: &egui::Painter, p: Pos2, hit: bool) {
    if hit {
        painter.circle(p, 3.5, GOOD, Stroke::new(1.0, Color32::BLACK));
    } else {
        let stroke = Stroke::new(2.0, BAD);
        let d = MISS_MARK_PX;
        painter.line_segment([p + vec2(-d, -d), p + vec2(d, d)], stroke);
        painter.line_segment([p + vec2(-d, d), p + vec2(d, -d)], stroke);
    }
}

fn plot(ui: &mut egui::Ui, e: &Engagement, zoom: bool) {
    let (response, painter, area) = plot_frame(ui);
    let extent = if zoom {
        (e.radius_deg * ZOOM_EXTENT_RADII).max(MIN_EXTENT_DEG)
    } else {
        plot_extent_deg(e)
    };
    let center = area.center();
    let px_per_deg = paint_grid(&painter, area, extent, "°");

    // Target aim zone.
    painter.circle_stroke(
        center,
        (e.radius_deg * px_per_deg).max(2.0),
        Stroke::new(1.5, WARN),
    );

    // Crosshair path; each segment takes the speed of the frame it ends on.
    let points: Vec<Pos2> = e
        .path_deg
        .iter()
        .map(|&o| to_screen(o, center, px_per_deg))
        .collect();
    for (i, pair) in points.windows(2).enumerate() {
        let speed = e.speed_deg_per_sec.get(i + 1).copied().unwrap_or(0.0);
        painter.line_segment(
            [pair[0], pair[1]],
            Stroke::new(1.5, ramp_color(speed_scale(speed))),
        );
    }
    if let Some(start) = points.first() {
        painter.circle_stroke(*start, 3.0, Stroke::new(1.0, Color32::from_gray(150)));
    }

    for s in &e.shots {
        paint_shot(&painter, to_screen(s.offset_deg, center, px_per_deg), s.hit);
    }

    response.on_hover_text(
        "Crosshair offset from the target center, in degrees. Yellow circle: the target. \
         Trail colour: crosshair speed (see the legend). Hollow dot: start. \
         Green dot: hit. Red cross: miss.",
    );
}

/// Legend of the speed-coloured trail: one swatch per speed band.
fn speed_legend(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.small("Trail speed:");
        let bands = [
            (
                1.0 / 6.0,
                format!("slow (< {SLOW_SPEED_DEG_PER_SEC:.0}°/s)"),
            ),
            (0.5, "medium".to_owned()),
            (
                5.0 / 6.0,
                format!("fast (≥ {FAST_SPEED_DEG_PER_SEC:.0}°/s)"),
            ),
        ];
        for (t, label) in bands {
            ui.colored_label(ramp_color(t), "■");
            ui.small(label);
        }
    });
}

/// The run's shots on a grid of target radii, cells coloured by how many shots `filter`
/// counts, relative to `max` (shared between side-by-side heatmaps).
fn heatmap_plot(ui: &mut egui::Ui, h: &ShotHeatmap, filter: ShotFilter, max: u32) {
    let (response, painter, area) = plot_frame(ui);
    let cell_px = area.width() / h.bins as f32;
    if max > 0 {
        for row in 0..h.bins {
            for column in 0..h.bins {
                let n = filter.cell_count(h, h.index(column, row));
                if n == 0 {
                    continue;
                }
                let min = Pos2::new(
                    area.left() + column as f32 * cell_px,
                    area.top() + row as f32 * cell_px,
                );
                painter.rect_filled(
                    Rect::from_min_size(min, vec2(cell_px, cell_px)),
                    0.0,
                    ramp_color(n as f32 / max as f32),
                );
            }
        }
    }
    let px_per_radius = paint_grid(&painter, area, h.extent_radii, "r");
    painter.circle_stroke(area.center(), px_per_radius, Stroke::new(1.5, WARN));
    response.on_hover_text(
        "Where your shots landed, in target radii (1r is the target's edge), so targets of \
         every size share one grid. Yellow circle: the target. Brighter cells: more shots.",
    );
}

/// Shot totals under a heatmap.
fn heatmap_counts(ui: &mut egui::Ui, h: &ShotHeatmap, filter: ShotFilter) {
    let (hits, misses) = h.total();
    if hits + misses == 0 {
        ui.small("No shots in this run (hold scenarios record none).");
        return;
    }
    ui.small(format!(
        "{hits} hit(s), {misses} miss(es); {} outside ±{:.0} radii, not drawn",
        filter.outside(h),
        h.extent_radii
    ));
}

/// Legend of the heatmap colours.
fn heatmap_legend(ui: &mut egui::Ui, max: u32) {
    if max == 0 {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.small("Shots per cell:");
        ui.colored_label(ramp_color(1.0 / max as f32), "■");
        ui.small("1");
        ui.colored_label(ramp_color(1.0), "■");
        ui.small(max.to_string());
    });
}

/// The path view: one engagement, side by side with the same engagement of `earlier`.
fn path_view(
    ui: &mut egui::Ui,
    path: &ReplayPath,
    earlier: Option<&ReplayPath>,
    engagement: &mut usize,
    zoom: &mut bool,
) {
    let count = path.engagements.len();
    if count == 0 {
        ui.small("No engagements in this run.");
        return;
    }
    *engagement = (*engagement).min(count - 1);
    let mut shown = *engagement + 1;
    ui.horizontal(|ui| {
        ui.add_enabled(
            count > 1,
            egui::Slider::new(&mut shown, 1..=count).text(format!("of {count} engagements")),
        );
        ui.checkbox(zoom, "Zoom on target");
    });
    *engagement = shown - 1;
    let e = &path.engagements[*engagement];
    let Some(other) = earlier else {
        plot(ui, e, *zoom);
        speed_legend(ui);
        ui.label(summary(e));
        return;
    };
    ui.columns(2, |cols| {
        cols[0].strong("This run");
        plot(&mut cols[0], e, *zoom);
        cols[0].label(summary(e));
        cols[1].strong(format!("Earlier run, engagement {shown}"));
        match other.engagements.get(*engagement) {
            Some(o) => {
                plot(&mut cols[1], o, *zoom);
                cols[1].label(summary(o));
            }
            None => {
                cols[1].small(format!(
                    "The earlier run has only {} engagement(s).",
                    other.engagements.len()
                ));
            }
        }
    });
    speed_legend(ui);
}

/// The heatmap view: every shot of the run, side by side with `earlier`'s on one colour
/// scale.
fn heatmap_view(
    ui: &mut egui::Ui,
    path: &ReplayPath,
    earlier: Option<&ReplayPath>,
    filter: &mut ShotFilter,
) {
    ui.horizontal(|ui| {
        ui.label("Shots:");
        ui.radio_value(filter, ShotFilter::All, "All");
        ui.radio_value(filter, ShotFilter::Hits, "Hits");
        ui.radio_value(filter, ShotFilter::Misses, "Misses");
    });
    let f = *filter;
    let heatmap = |p: &ReplayPath| shot_heatmap(&p.engagements, HEATMAP_BINS, HEATMAP_EXTENT_RADII);
    let h = heatmap(path);
    let Some(other) = earlier else {
        let max = f.max_count(&h);
        heatmap_plot(ui, &h, f, max);
        heatmap_counts(ui, &h, f);
        heatmap_legend(ui, max);
        return;
    };
    let ho = heatmap(other);
    let max = f.max_count(&h).max(f.max_count(&ho));
    ui.columns(2, |cols| {
        cols[0].strong("This run");
        heatmap_plot(&mut cols[0], &h, f, max);
        heatmap_counts(&mut cols[0], &h, f);
        cols[1].strong("Earlier run");
        heatmap_plot(&mut cols[1], &ho, f, max);
        heatmap_counts(&mut cols[1], &ho, f);
    });
    heatmap_legend(ui, max);
}

/// Draw the replay list, the selected replay's path or heatmap view, and the optional
/// comparison with an earlier run of the same scenario.
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

    let Some(current) = state
        .selected
        .as_ref()
        .and_then(|p| entries.iter().find(|e| &e.path == p))
    else {
        return;
    };
    let ReplayViewerState {
        path,
        engagement,
        zoom,
        mode,
        filter,
        compare,
        ..
    } = state;
    let Some(path) = path.as_ref() else {
        ui.colored_label(BAD, "This replay could not be read.");
        return;
    };
    ui.horizontal(|ui| {
        ui.label("View:");
        ui.selectable_value(mode, ViewMode::Path, "Crosshair path");
        ui.selectable_value(mode, ViewMode::Heatmap, "Shot heatmap");
    });
    let candidates = replays::earlier_runs_of_same_scenario(entries, current);
    replay_compare::compare_picker(ui, compare, &candidates, &load);
    let earlier = compare.path();
    match mode {
        ViewMode::Path => path_view(ui, path, earlier, engagement, zoom),
        ViewMode::Heatmap => heatmap_view(ui, path, earlier, filter),
    }
    if let Some(other) = earlier {
        ui.separator();
        replay_compare::differences_table(ui, &path.summary(), &other.summary());
    }
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
            speed_deg_per_sec: vec![0.0; path.len()],
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

    #[test]
    fn selecting_another_replay_drops_the_comparison_but_keeps_the_view_mode() {
        let path = ReplayPath {
            scenario_id: "x".into(),
            engagements: vec![],
        };
        let earlier = ReplayEntry {
            path: PathBuf::from("/r/1-x.aimreplay"),
            timestamp_millis: 1,
            scenario_id: "x".into(),
        };
        let mut state = ReplayViewerState {
            mode: ViewMode::Heatmap,
            filter: ShotFilter::Misses,
            ..Default::default()
        };
        assert_eq!(ReplayViewerState::default().mode, ViewMode::Path);
        state.compare.select(&earlier, &|_| Some(path.clone()));
        assert_eq!(state.compare.path(), Some(&path));
        let next = ReplayEntry {
            path: PathBuf::from("/r/2-x.aimreplay"),
            timestamp_millis: 2,
            scenario_id: "x".into(),
        };
        state.select(&next, &|_| Some(path.clone()));
        assert!(state.compare.path().is_none());
        assert_eq!(
            (state.mode, state.filter),
            (ViewMode::Heatmap, ShotFilter::Misses)
        );
    }

    #[test]
    fn the_heatmap_filter_counts_hits_misses_or_both() {
        let h = ShotHeatmap {
            bins: 2,
            extent_radii: 1.0,
            hits: vec![3, 0, 1, 0],
            misses: vec![1, 2, 0, 0],
            outside_hits: 4,
            outside_misses: 5,
        };
        let counts = |f: ShotFilter| -> Vec<u32> { (0..4).map(|i| f.cell_count(&h, i)).collect() };
        assert_eq!(counts(ShotFilter::All), vec![4, 2, 1, 0]);
        assert_eq!(counts(ShotFilter::Hits), vec![3, 0, 1, 0]);
        assert_eq!(counts(ShotFilter::Misses), vec![1, 2, 0, 0]);
        assert_eq!(ShotFilter::All.max_count(&h), 4);
        assert_eq!(ShotFilter::Hits.max_count(&h), 3);
        assert_eq!(ShotFilter::Misses.max_count(&h), 2);
        assert_eq!(ShotFilter::All.outside(&h), 9);
        assert_eq!(ShotFilter::Hits.outside(&h), 4);
        assert_eq!(ShotFilter::Misses.outside(&h), 5);
    }
}
