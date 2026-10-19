//! Run history stored as CSV so it is easy to inspect or import into a spreadsheet.
//!
//! Columns, in order (README "Run history"):
//! - the result: `timestamp, scenario, score, accuracy, kills, kills_per_min,
//!   avg_time_to_kill, headshot_rate, moving_shots, duration_secs`;
//! - the settings snapshot: `sens_game, sens_value, dpi, cm_per_360, vfov_deg,
//!   crosshair_code, avg_fps, low_1pct_fps, pause_count, pause_secs, content_hash`.
//!
//! Older files have only the first nine or ten columns. Their rows load as before, with an
//! empty snapshot; the next run written upgrades the header line and leaves the rows alone.

use crate::persistence::settings::Paths;
use aim_core::games::Game;
use aim_core::scenario::RunResult;
use aim_core::sensitivity::Sensitivity;
use aim_core::training::progress::RunRecord;
use aim_core::training::store::{RunSnapshot, RunStore, record_of};
use bevy::prelude::Resource;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const HEADER: &str = "timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots,duration_secs,sens_game,sens_value,dpi,cm_per_360,vfov_deg,crosshair_code,avg_fps,low_1pct_fps,pause_count,pause_secs,content_hash";
/// Every history header starts this way, old or new.
const HEADER_PREFIX: &str = "timestamp,scenario,score";
/// Rows written before `duration_secs` existed were full-length runs.
const LEGACY_DURATION_SECS: f32 = 60.0;
/// Index of the first snapshot column.
const SNAPSHOT_AT: usize = 10;

/// Run history kept in `history.csv`: the app's adapter for the core's [`RunStore`] port.
/// The file keeps every column of the result; the store keeps what the core queries.
/// Built only by [`History::load`], so every run reaches the file (no `Default`).
#[derive(Resource, Debug)]
pub struct History {
    records: Vec<RunRecord>,
    snapshots: Vec<RunSnapshot>,
    file: Option<PathBuf>,
}

impl History {
    pub fn load(paths: &Paths) -> Self {
        let file = Some(paths.history_file.clone());
        let Ok(src) = fs::read_to_string(&paths.history_file) else {
            return Self {
                records: Vec::new(),
                snapshots: Vec::new(),
                file,
            };
        };
        let (records, snapshots) = src.lines().skip(1).filter_map(parse_row).unzip();
        Self {
            records,
            snapshots,
            file,
        }
    }

    /// Keep a run that finished now, without a settings snapshot (arena runs).
    pub fn record(&mut self, r: &RunResult) {
        self.record_with(r, &RunSnapshot::default());
    }

    /// Keep a run that finished now with the settings it was played with.
    pub fn record_with(&mut self, r: &RunResult, snapshot: &RunSnapshot) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.append(r, snapshot, timestamp);
    }
}

impl RunStore for History {
    fn runs(&self) -> &[RunRecord] {
        &self.records
    }

    fn snapshots(&self) -> &[RunSnapshot] {
        &self.snapshots
    }

    fn append(&mut self, r: &RunResult, snapshot: &RunSnapshot, timestamp: u64) {
        if let Some(path) = &self.file {
            let line = format!(
                "{},{}",
                result_fields(r, timestamp),
                snapshot_fields(snapshot)
            );
            if let Err(e) = append_line(path, &line) {
                bevy::log::error!("could not write history: {e}");
            }
        }
        self.records.push(record_of(r, timestamp));
        self.snapshots.push(snapshot.clone());
    }
}

/// Append `line` to the history file, writing the header first for a new file and
/// upgrading an older header in place. A failed upgrade only leaves the old header: the run
/// is still appended.
fn append_line(path: &Path, line: &str) -> io::Result<()> {
    let new_file = !path.exists();
    if !new_file && let Err(e) = upgrade_header(path) {
        bevy::log::warn!("could not upgrade the history header: {e}");
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    if new_file {
        writeln!(f, "{HEADER}")?;
    }
    writeln!(f, "{line}")
}

/// Replace an older history header with [`HEADER`], keeping every row byte for byte. The
/// file is rewritten through a temporary file and a rename, so a crash leaves either the
/// old or the new file. A file that does not start with a history header is left alone.
fn upgrade_header(path: &Path) -> io::Result<()> {
    // Only line 1 is read on every run end; the whole file only on the one-time upgrade.
    let mut first = String::new();
    io::BufRead::read_line(&mut io::BufReader::new(fs::File::open(path)?), &mut first)?;
    let first = first.trim_end_matches(['\r', '\n']);
    if first == HEADER || !first.starts_with(HEADER_PREFIX) {
        return Ok(());
    }
    let src = fs::read_to_string(path)?;
    let rest = src.split_once('\n').map_or("", |(_, rest)| rest);
    let tmp = path.with_extension("csv.tmp");
    fs::write(&tmp, format!("{HEADER}\n{rest}"))?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// The result columns of a row, formatted as they always were.
fn result_fields(r: &RunResult, timestamp: u64) -> String {
    format!(
        "{timestamp},{},{:.1},{:.4},{},{:.2},{},{:.4},{},{:.1}",
        r.scenario_id,
        r.score,
        r.accuracy,
        r.kills,
        r.kills_per_min,
        r.avg_time_to_kill
            .map_or(String::new(), |t| format!("{t:.4}")),
        r.headshot_rate,
        r.moving_shots,
        r.duration_secs,
    )
}

/// The snapshot columns of a row; missing values are empty. Values round-trip exactly,
/// except `cm_per_360`, which is derived from the sensitivity and written for reading.
fn snapshot_fields(s: &RunSnapshot) -> String {
    fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
        v.map_or(String::new(), |v| v.to_string())
    }
    let sens = s.sensitivity;
    [
        sens.map_or(String::new(), |s| game_name(s.game)),
        opt(sens.map(|s| s.value)),
        opt(sens.map(|s| s.dpi)),
        sens.map_or(String::new(), |s| format!("{:.2}", s.cm_per_360())),
        opt(s.vertical_fov_deg),
        s.crosshair_code.as_deref().map_or(String::new(), quote),
        opt(s.avg_fps),
        opt(s.low_fps),
        opt(s.pause_count),
        opt(s.pause_secs),
        s.content_hash
            .map_or(String::new(), |h| format!("{h:016x}")),
    ]
    .join(",")
}

/// One history row as a record and its snapshot; `None` when the row is unusable.
fn parse_row(line: &str) -> Option<(RunRecord, RunSnapshot)> {
    let f = split_fields(line);
    let get = |i: usize| f.get(i).map(String::as_str).filter(|v| !v.is_empty());
    let record = RunRecord {
        timestamp: get(0)?.parse().ok()?,
        scenario: f.get(1)?.clone(),
        score: get(2)?.parse().ok()?,
        duration_secs: get(9)
            .and_then(|d| d.parse().ok())
            .unwrap_or(LEGACY_DURATION_SECS),
    };
    let at = |k: usize| get(SNAPSHOT_AT + k);
    let num = |k: usize| at(k).and_then(|v| v.parse::<f32>().ok());
    let sensitivity = match (
        at(0).and_then(parse_game),
        at(1).and_then(|v| v.parse().ok()),
        at(2).and_then(|v| v.parse().ok()),
    ) {
        (Some(game), Some(value), Some(dpi)) => Some(Sensitivity { game, value, dpi }),
        _ => None,
    };
    let snapshot = RunSnapshot {
        sensitivity,
        vertical_fov_deg: num(4),
        crosshair_code: at(5).map(str::to_owned),
        avg_fps: num(6),
        low_fps: num(7),
        pause_count: at(8).and_then(|v| v.parse().ok()),
        pause_secs: num(9),
        content_hash: at(10).and_then(|v| u64::from_str_radix(v, 16).ok()),
    };
    Some((record, snapshot))
}

/// The history file `src` as a CSV export: [`HEADER`], then one row per run the history
/// loads, padded with empty fields to every column (older rows lack the snapshot columns).
/// Returns the CSV and the number of runs.
pub fn export_csv(src: &str) -> (String, usize) {
    let width = HEADER.split(',').count();
    let mut out = format!("{HEADER}\n");
    let mut rows = 0;
    for line in src.lines().skip(1).filter(|l| parse_row(l).is_some()) {
        let mut fields = split_fields(line);
        fields.resize(width, String::new());
        let quoted: Vec<String> = fields.iter().map(|f| quote(f)).collect();
        out.push_str(&quoted.join(","));
        out.push('\n');
        rows += 1;
    }
    (out, rows)
}

/// Write the export of `history_file` to a new file `out`; a missing history exports no
/// runs. An existing file (the history itself included) is never replaced.
pub fn export_history(history_file: &Path, out: &Path) -> io::Result<usize> {
    let src = match fs::read_to_string(history_file) {
        Ok(src) => src,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let (csv, rows) = export_csv(&src);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)?;
    // A half-written export would block a retry with the same name, so it is removed.
    io::Write::write_all(&mut file, csv.as_bytes()).inspect_err(|_| {
        let _ = fs::remove_file(out);
    })?;
    Ok(rows)
}

/// Where an export goes when the player names no file: next to `history.csv`.
pub fn default_export_path(paths: &Paths, timestamp: u64) -> PathBuf {
    paths
        .history_file
        .with_file_name(format!("history-export-{timestamp}.csv"))
}

/// A history export running on a background thread, so the frame never waits on disk.
#[derive(Debug, Default)]
pub struct HistoryExport {
    job: Option<std::thread::JoinHandle<io::Result<usize>>>,
    out: PathBuf,
    status: Option<String>,
}

impl HistoryExport {
    /// Start exporting `history_file` to `out`, unless an export is already running.
    pub fn start(&mut self, history_file: PathBuf, out: PathBuf) {
        if self.is_running() {
            return;
        }
        self.out = out.clone();
        let spawned = std::thread::Builder::new()
            .name("history-export".into())
            .spawn(move || export_history(&history_file, &out));
        match spawned {
            Ok(handle) => {
                self.job = Some(handle);
                self.status = None;
            }
            Err(e) => self.status = Some(format!("Export failed: {e}")),
        }
    }

    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// Collect a finished export. Call once per frame.
    pub fn poll(&mut self) {
        if self.job.as_ref().is_some_and(|h| h.is_finished()) {
            self.finish();
        }
    }

    /// The outcome of the last export, once it finished.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    fn finish(&mut self) {
        let Some(handle) = self.job.take() else {
            return;
        };
        let status = match handle.join() {
            Ok(Ok(rows)) => format!("Exported {rows} runs to {}", self.out.display()),
            Ok(Err(e)) => format!("Export failed: {e}"),
            Err(_) => "Export failed: the export thread stopped".to_owned(),
        };
        if status.starts_with("Export failed") {
            bevy::log::error!("{status}");
        }
        self.status = Some(status);
    }

    /// Block until the running export ends and return its status (tests).
    #[cfg(test)]
    fn wait(&mut self) -> String {
        self.finish();
        self.status.clone().unwrap_or_default()
    }
}

/// The stored (settings-file) name of a game, e.g. `valorant`.
fn game_name(game: Game) -> String {
    match toml::Value::try_from(game) {
        Ok(toml::Value::String(s)) => s,
        _ => String::new(),
    }
}

fn parse_game(name: &str) -> Option<Game> {
    toml::Value::String(name.to_owned()).try_into().ok()
}

/// A CSV field, quoted when it holds a comma, a quote or a line break.
fn quote(v: &str) -> String {
    if v.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_owned()
    }
}

/// The fields of one CSV line, honouring quoted fields.
fn split_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.trim_end_matches('\r').chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    out.push(field);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_in(dir: &std::path::Path) -> Paths {
        Paths {
            settings_file: dir.join("settings.toml"),
            scenarios_dir: dir.join("scenarios"),
            routines_dir: dir.join("routines"),
            benchmarks_dir: dir.join("benchmarks"),
            history_file: dir.join("history.csv"),
            coach_memory_file: dir.join("coach.db"),
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aimtrainer-history-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn result(key: &str, score: f32, secs: f32) -> RunResult {
        RunResult {
            scenario_id: key.into(),
            score,
            accuracy: 0.5,
            kills: 1,
            kills_per_min: 3.0,
            avg_time_to_kill: None,
            headshot_rate: 0.0,
            moving_shots: 0,
            expired: 0,
            duration_secs: secs,
        }
    }

    fn snapshot() -> RunSnapshot {
        RunSnapshot {
            sensitivity: Some(Sensitivity::valorant(0.314, 1600)),
            vertical_fov_deg: Some(70.528_77),
            crosshair_code: Some("0;P;c;5;h;0;f;0;0l;4;0o;2;0a;1".into()),
            avg_fps: Some(243.71),
            low_fps: Some(161.2),
            pause_count: Some(2),
            pause_secs: Some(7.25),
            content_hash: Some(0x00ab_cdef_0123_4567),
        }
    }

    #[test]
    fn legacy_rows_load_with_a_default_duration_and_new_rows_keep_theirs() {
        let dir = scratch("legacy");
        let paths = paths_in(&dir);
        fs::write(
            &paths.history_file,
            "timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots\n1,micro-adjust,100.0,0.5,1,1.0,,0.0,0\n",
        )
        .unwrap();
        let mut h = History::load(&paths);
        assert_eq!(h.runs()[0].duration_secs, LEGACY_DURATION_SECS);
        h.record(&result("micro-adjust~20s", 50.0, 20.0));
        let reloaded = History::load(&paths);
        assert_eq!(reloaded.runs().len(), 2);
        assert_eq!(reloaded.runs()[1].duration_secs, 20.0);
        assert_eq!(reloaded.best("micro-adjust~20s"), Some(50.0));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn old_rows_load_with_an_empty_snapshot() {
        let dir = scratch("old");
        let paths = paths_in(&dir);
        fs::write(
            &paths.history_file,
            "timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots,duration_secs\r\n5,flick-wide,812.0,0.6100,40,40.00,0.5120,0.1000,0,60.0\r\n6,micro-adjust,90.0,0.5,1,1.0,,0.0,0\r\n",
        )
        .unwrap();
        let h = History::load(&paths);
        assert_eq!(h.runs().len(), 2);
        assert_eq!(h.runs()[0].score, 812.0);
        assert_eq!(h.snapshots().len(), 2);
        assert!(h.snapshots().iter().all(RunSnapshot::is_empty));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_snapshot_round_trips_through_the_file() {
        let dir = scratch("snapshot");
        let paths = paths_in(&dir);
        let mut h = History::load(&paths);
        h.record_with(&result("flick-wide", 700.0, 60.0), &snapshot());
        h.record(&result("arena", 5.0, 90.0));
        assert_eq!(h.snapshots()[0], snapshot());
        let reloaded = History::load(&paths);
        assert_eq!(reloaded.runs(), h.runs());
        assert_eq!(reloaded.snapshots()[0], snapshot());
        assert!(reloaded.snapshots()[1].is_empty());
        let src = fs::read_to_string(&paths.history_file).unwrap();
        let mut lines = src.lines();
        assert_eq!(lines.next(), Some(HEADER));
        let row = lines.next().unwrap();
        let cm = Sensitivity::valorant(0.314, 1600).cm_per_360();
        assert!(
            row.ends_with(&format!(
                ",valorant,0.314,1600,{cm:.2},70.52877,0;P;c;5;h;0;f;0;0l;4;0o;2;0a;1,243.71,161.2,2,7.25,00abcdef01234567"
            )),
            "{row}"
        );
        assert_eq!(split_fields(row).len(), HEADER.split(',').count());
        let empty = lines.next().unwrap();
        assert!(
            empty.ends_with(&format!(",90.0{}", ",".repeat(11))),
            "an empty snapshot writes empty fields: {empty}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_next_write_upgrades_an_old_header_and_keeps_its_rows() {
        let dir = scratch("upgrade");
        let paths = paths_in(&dir);
        let old_rows = "5,flick-wide,812.0,0.6100,40,40.00,0.5120,0.1000,0,60.0\n6,micro-adjust,90.0,0.5,1,1.0,,0.0,0\n";
        fs::write(
            &paths.history_file,
            format!("timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots,duration_secs\n{old_rows}"),
        )
        .unwrap();
        let mut h = History::load(&paths);
        h.record_with(&result("flick-wide", 900.0, 60.0), &snapshot());
        let src = fs::read_to_string(&paths.history_file).unwrap();
        assert!(src.starts_with(&format!("{HEADER}\n{old_rows}")), "{src}");
        let reloaded = History::load(&paths);
        assert_eq!(reloaded.runs().len(), 3);
        assert_eq!(reloaded.snapshots()[2], snapshot());
        // A file that is not a history file is never rewritten.
        fs::write(&paths.history_file, "something else\n").unwrap();
        upgrade_header(&paths.history_file).unwrap();
        assert_eq!(
            fs::read_to_string(&paths.history_file).unwrap(),
            "something else\n"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn quoted_fields_split_and_join_back() {
        assert_eq!(quote("a,b"), "\"a,b\"");
        assert_eq!(quote("plain"), "plain");
        assert_eq!(split_fields("1,\"a,\"\"b\"\"\",c\r"), ["1", "a,\"b\"", "c"]);
    }

    #[test]
    fn the_export_has_one_full_width_row_per_run() {
        let src = "timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots\n\
            1,micro-adjust,100.0,0.5,1,1.0,,0.0,0\n\
            \n\
            not-a-number,x,1\n\
            2,flick-wide,700.0,0.6,40,40.0,0.5,0.1,0,60.0,valorant,0.314,1600,26.00,70.5,\"a,b\",240,180,1,2.5,00000000000000ff\n";
        let (csv, rows) = export_csv(src);
        assert_eq!(rows, 2, "blank and unreadable rows are skipped");
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], HEADER);
        let width = HEADER.split(',').count();
        assert_eq!(lines.len(), 3);
        for l in &lines[1..] {
            assert_eq!(split_fields(l).len(), width, "{l}");
        }
        assert_eq!(
            lines[1],
            format!(
                "1,micro-adjust,100.0,0.5,1,1.0,,0.0,0{}",
                ",".repeat(width - 9)
            )
        );
        assert!(lines[2].contains(",\"a,b\","), "quoted fields stay quoted");
    }

    #[test]
    fn exporting_writes_the_file_off_the_frame() {
        let dir = scratch("export");
        let paths = paths_in(&dir);
        let mut h = History::load(&paths);
        h.record_with(&result("flick-wide", 700.0, 60.0), &snapshot());
        h.record(&result("arena", 5.0, 90.0));
        let out = default_export_path(&paths, 1_700_000_000);
        assert_eq!(out, dir.join("history-export-1700000000.csv"));
        let mut job = HistoryExport::default();
        assert!(!job.is_running());
        job.start(paths.history_file.clone(), out.clone());
        assert!(job.is_running() || job.status().is_none());
        let status = job.wait();
        assert_eq!(status, format!("Exported 2 runs to {}", out.display()));
        assert_eq!(job.status(), Some(status.as_str()));
        let written = fs::read_to_string(&out).unwrap();
        assert_eq!(written.lines().count(), 3);
        assert_eq!(written.lines().next(), Some(HEADER));
        let mut failing = HistoryExport::default();
        failing.start(
            dir.join("missing.csv"),
            dir.join("no-such-dir").join("x.csv"),
        );
        assert!(failing.wait().starts_with("Export failed: "));

        // An existing file, the history itself included, is never replaced.
        let history_before = fs::read(&paths.history_file).unwrap();
        for existing in [out.clone(), paths.history_file.clone()] {
            let before = fs::read(&existing).unwrap();
            let mut again = HistoryExport::default();
            again.start(paths.history_file.clone(), existing.clone());
            assert!(again.wait().starts_with("Export failed: "));
            assert_eq!(fs::read(&existing).unwrap(), before);
        }
        assert_eq!(fs::read(&paths.history_file).unwrap(), history_before);
        fs::remove_dir_all(&dir).unwrap();
    }
}
