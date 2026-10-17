//! Replay files: each finished run's recording, stored in `<data dir>/replays/` in the
//! `aim_core::analytics::codec` format. Only the newest [`KEEP_REPLAYS`] are kept.
//!
//! File name: `<unix millis, 20 digits>-<scenario id>.aimreplay`, so sorting the names
//! sorts the replays by time. Reading them back is `codec::decode` on the file bytes; a
//! listing/loading API is added with the first feature that opens replays (YAGNI).

use crate::persistence::settings::Paths;
use aim_core::analytics::codec;
use aim_core::scenario::Recording;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const KEEP_REPLAYS: usize = 50;
const EXTENSION: &str = "aimreplay";

#[derive(Debug, Clone, PartialEq)]
struct ReplayFile {
    path: PathBuf,
    timestamp_millis: u64,
}

/// `<data dir>/replays`, next to the history file.
pub fn replays_dir(paths: &Paths) -> PathBuf {
    replays_dir_beside(&paths.history_file)
}

fn replays_dir_beside(history_file: &Path) -> PathBuf {
    history_file
        .parent()
        .map_or_else(|| PathBuf::from("replays"), |p| p.join("replays"))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Write `recording` as a new replay and drop the oldest beyond [`KEEP_REPLAYS`].
pub fn save(paths: &Paths, recording: &Recording) -> io::Result<PathBuf> {
    save_in(&replays_dir(paths), recording, now_millis(), KEEP_REPLAYS)
}

fn save_in(
    dir: &Path,
    recording: &Recording,
    timestamp_millis: u64,
    keep: usize,
) -> io::Result<PathBuf> {
    let bytes = codec::encode(recording).map_err(io::Error::other)?;
    fs::create_dir_all(dir)?;
    let path = dir.join(format!(
        "{timestamp_millis:020}-{}.{EXTENSION}",
        recording.def.id
    ));
    fs::write(&path, bytes)?;
    prune(dir, keep)?;
    Ok(path)
}

fn parse_name(path: &Path) -> Option<ReplayFile> {
    if path.extension()? != EXTENSION {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let (ts, id) = stem.split_once('-')?;
    if id.is_empty() {
        return None;
    }
    Some(ReplayFile {
        path: path.to_path_buf(),
        timestamp_millis: ts.parse().ok()?,
    })
}

fn list_in(dir: &Path) -> Vec<ReplayFile> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<ReplayFile> = entries
        .filter_map(|e| parse_name(&e.ok()?.path()))
        .collect();
    v.sort_by_key(|f| f.timestamp_millis);
    v
}

fn prune(dir: &Path, keep: usize) -> io::Result<()> {
    let all = list_in(dir);
    let excess = all.len().saturating_sub(keep);
    for old in &all[..excess] {
        fs::remove_file(&old.path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::scenario::{FrameInput, builtin};

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn scratch(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "aimtrainer-replays-{name}-{}-{}",
            std::process::id(),
            now_millis()
        ));
        Scratch(dir)
    }

    fn recording() -> Recording {
        let mut r = Recording::new(builtin::all().remove(0), 5);
        r.inputs.push(FrameInput {
            dt: 0.01,
            fire_pressed: true,
            ..Default::default()
        });
        r
    }

    #[test]
    fn saved_replays_load_back_and_only_the_newest_are_kept() {
        let s = scratch("keep");
        let rec = recording();
        for t in 1..=4 {
            save_in(&s.0, &rec, t, 3).unwrap();
        }
        let list = list_in(&s.0);
        let times: Vec<u64> = list.iter().map(|e| e.timestamp_millis).collect();
        assert_eq!(times, vec![2, 3, 4]);
        assert!(
            list[0]
                .path
                .to_string_lossy()
                .ends_with(&format!("-{}.aimreplay", rec.def.id))
        );
        assert_eq!(
            codec::decode(&fs::read(&list[2].path).unwrap()).unwrap(),
            rec
        );
    }

    #[test]
    fn missing_dir_lists_nothing_and_foreign_files_are_ignored() {
        let s = scratch("foreign");
        assert!(list_in(&s.0).is_empty());
        fs::create_dir_all(&s.0).unwrap();
        fs::write(s.0.join("notes.txt"), "x").unwrap();
        fs::write(s.0.join("abc-x.aimreplay"), "x").unwrap();
        assert!(list_in(&s.0).is_empty());
    }

    #[test]
    fn replays_live_next_to_the_history_file() {
        assert_eq!(
            replays_dir_beside(Path::new("/d/history.csv")),
            PathBuf::from("/d/replays")
        );
    }
}
