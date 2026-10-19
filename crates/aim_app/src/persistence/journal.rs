//! The ranked session journal (`aim_core::training::journal`), stored as `journal.toml`
//! next to the run history.
//!
//! Writes run on a background thread through a temporary file and a rename, so the frame
//! never waits on disk and a crash leaves either the old or the new file. A file that fails
//! to parse is never overwritten: it is renamed to `journal.corrupt-<unix time>.toml`
//! before the first new entry is saved, and the page says so.

use crate::persistence::settings::Paths;
use aim_core::training::journal::{Journal, JournalEntry, JournalError};
use bevy::prelude::Resource;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

/// File name of the journal in the data folder.
pub const JOURNAL_FILE: &str = "journal.toml";

/// Where the journal lives: next to `history.csv`.
pub fn journal_file(paths: &Paths) -> PathBuf {
    paths.history_file.with_file_name(JOURNAL_FILE)
}

/// The journal in `path`. A missing file is an empty journal; an unreadable or invalid one
/// is an empty journal plus the reason, and the file is left alone.
pub fn load_from(path: &Path) -> (Journal, Option<String>) {
    match fs::read_to_string(path) {
        Ok(src) => match toml::from_str::<Journal>(&src) {
            Ok(j) => (j, None),
            Err(e) => (
                Journal::default(),
                Some(format!(
                    "{} could not be read: {}",
                    path.display(),
                    e.message()
                )),
            ),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Journal::default(), None),
        Err(e) => (
            Journal::default(),
            Some(format!("{} could not be read: {e}", path.display())),
        ),
    }
}

/// Write `text` to `path` through `<path>.tmp` and a rename. With `corrupt_backup`, an
/// existing file is first renamed to that backup path, so it is never lost.
pub fn write_atomic(path: &Path, text: &str, corrupt_backup: Option<&Path>) -> io::Result<()> {
    if let Some(backup) = corrupt_backup
        && path.exists()
    {
        fs::rename(path, backup)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// The journal in memory, and its file written in the background.
#[derive(Resource, Debug, Default)]
pub struct JournalStore {
    pub journal: Journal,
    file: Option<PathBuf>,
    /// Why the file could not be loaded; cleared once the broken file was moved aside.
    load_error: Option<String>,
    /// Where the unreadable file goes before the first save.
    corrupt_backup: Option<PathBuf>,
    job: Option<JoinHandle<io::Result<()>>>,
    /// The newest text waiting for the running write to finish.
    pending: Option<String>,
    /// The last write's failure, if any.
    write_error: Option<String>,
}

impl JournalStore {
    pub fn load(paths: &Paths) -> Self {
        Self::load_file(journal_file(paths), now_secs())
    }

    fn load_file(file: PathBuf, now: u64) -> Self {
        let (journal, load_error) = load_from(&file);
        if let Some(e) = &load_error {
            bevy::log::warn!("{e}");
        }
        let corrupt_backup = load_error
            .as_ref()
            .map(|_| file.with_file_name(format!("journal.corrupt-{now}.toml")));
        Self {
            journal,
            file: Some(file),
            load_error,
            corrupt_backup,
            ..Self::default()
        }
    }

    /// Log an entry and save the journal in the background.
    pub fn add(&mut self, entry: JournalEntry) -> Result<(), JournalError> {
        self.journal.add(entry)?;
        match toml::to_string_pretty(&self.journal) {
            Ok(text) => {
                self.pending = Some(text);
                self.poll();
            }
            Err(e) => self.write_error = Some(format!("could not save the journal: {e}")),
        }
        Ok(())
    }

    /// Collect a finished write and start the next one. Call once per frame.
    pub fn poll(&mut self) {
        if self.job.as_ref().is_some_and(|h| h.is_finished())
            && let Some(job) = self.job.take()
        {
            self.write_error = match job.join() {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(format!("could not save the journal: {e}")),
                Err(_) => Some("could not save the journal: the writer stopped".to_owned()),
            };
            if let Some(e) = &self.write_error {
                bevy::log::error!("{e}");
            }
        }
        if self.job.is_some() {
            return;
        }
        let (Some(text), Some(file)) = (self.pending.take(), self.file.clone()) else {
            return;
        };
        // The broken file is moved aside by the first write only.
        let backup = self.corrupt_backup.take();
        if backup.is_some() {
            self.load_error = None;
        }
        let spawned = std::thread::Builder::new()
            .name("journal-save".into())
            .spawn(move || write_atomic(&file, &text, backup.as_deref()));
        match spawned {
            Ok(h) => self.job = Some(h),
            Err(e) => self.write_error = Some(format!("could not save the journal: {e}")),
        }
    }

    /// What went wrong loading or saving, for the page.
    pub fn problem(&self) -> Option<String> {
        match (&self.load_error, &self.corrupt_backup) {
            (Some(e), Some(b)) => Some(format!(
                "{e}. It is kept and will be renamed to {} when you log a match.",
                b.display()
            )),
            _ => self.write_error.clone(),
        }
    }

    /// Wait for every write to finish (tests and exit).
    pub fn flush(&mut self) {
        while self.job.is_some() || self.pending.is_some() {
            if let Some(job) = &self.job {
                while !job.is_finished() {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            self.poll();
        }
    }
}

/// Collect finished journal writes and start the next one.
pub fn poll_journal(mut store: bevy::prelude::ResMut<JournalStore>) {
    store.poll();
}

/// Finish the journal writes before the app exits.
pub fn flush_journal_on_exit(
    mut exits: bevy::prelude::MessageReader<bevy::prelude::AppExit>,
    mut store: bevy::prelude::ResMut<JournalStore>,
) {
    if exits.read().next().is_some() {
        store.flush();
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::training::journal::{JournalTag, Outcome};
    use aim_core::training::progress::Warmth;

    fn dir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("aimtrainer-journal-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn entry(outcome: Outcome) -> JournalEntry {
        JournalEntry {
            timestamp: 1_800_000_000,
            outcome,
            rr_change: Some(-18),
            self_rating: 2,
            tag: Some(JournalTag::Tilted),
            readiness: Some(Warmth::Cold),
            fatigued: true,
        }
    }

    #[test]
    fn entries_survive_a_restart() {
        let d = dir("roundtrip");
        let file = d.join(JOURNAL_FILE);
        let mut store = JournalStore::load_file(file.clone(), 1);
        assert!(store.journal.entries.is_empty());
        assert_eq!(store.problem(), None, "a missing file is an empty journal");
        store.add(entry(Outcome::Loss)).unwrap();
        store.add(entry(Outcome::Win)).unwrap();
        store.flush();
        assert_eq!(store.problem(), None);
        let back = JournalStore::load_file(file.clone(), 2);
        assert_eq!(back.journal, store.journal);
        assert!(
            !file.with_extension("toml.tmp").exists(),
            "no temp file left"
        );
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_invalid_entry_is_refused_and_not_saved() {
        let d = dir("invalid");
        let file = d.join(JOURNAL_FILE);
        let mut store = JournalStore::load_file(file.clone(), 1);
        let bad = JournalEntry {
            self_rating: 9,
            ..entry(Outcome::Win)
        };
        assert!(store.add(bad).is_err());
        store.flush();
        assert!(!file.exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_corrupt_file_is_kept_and_moved_aside_before_the_next_save() {
        let d = dir("corrupt");
        let file = d.join(JOURNAL_FILE);
        let broken = "[[entries]]\noutcome = \"maybe\"\n";
        fs::write(&file, broken).unwrap();
        let mut store = JournalStore::load_file(file.clone(), 42);
        assert!(store.journal.entries.is_empty());
        let problem = store.problem().expect("the load error is shown");
        assert!(problem.contains("journal.corrupt-42.toml"), "{problem}");
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            broken,
            "loading never writes"
        );

        store.add(entry(Outcome::Draw)).unwrap();
        store.flush();
        let backup = d.join("journal.corrupt-42.toml");
        assert_eq!(
            fs::read_to_string(&backup).unwrap(),
            broken,
            "nothing is lost"
        );
        assert_eq!(load_from(&file).0.entries, vec![entry(Outcome::Draw)]);
        assert_eq!(store.problem(), None);

        // Later saves overwrite the new file only; the backup stays as it was.
        store.add(entry(Outcome::Win)).unwrap();
        store.flush();
        assert_eq!(fs::read_to_string(&backup).unwrap(), broken);
        assert_eq!(load_from(&file).0.entries.len(), 2);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn the_journal_lives_next_to_the_history() {
        let paths = Paths {
            settings_file: PathBuf::from("c/settings.toml"),
            scenarios_dir: PathBuf::from("c/scenarios"),
            routines_dir: PathBuf::from("c/routines"),
            benchmarks_dir: PathBuf::from("c/benchmarks"),
            history_file: PathBuf::from("data/history.csv"),
            coach_memory_file: PathBuf::from("data/coach.db"),
        };
        assert_eq!(journal_file(&paths), PathBuf::from("data/journal.toml"));
    }
}
