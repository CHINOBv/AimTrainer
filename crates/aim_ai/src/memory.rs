//! [`CoachMemory`] in a local SQLite file, with an FTS5 table for keyword recall.
//!
//! - Recall ranks notes by FTS5 relevance (bm25), then by recency. Each query word also
//!   matches as a prefix, so "track" finds "tracking". A query without words returns the
//!   newest notes.
//! - [`SqliteMemory::wipe`] deletes every note. `secure_delete` is on, so wiped text is
//!   overwritten in the file rather than left in free pages.
//! - The schema version is SQLite's `user_version`. A file from a newer version is
//!   refused instead of being modified.
//! - The app chooses the file path; this module never guesses one.

use std::path::Path;

use aim_core::coach::{CoachMemory, MemoryError, MemoryNote};
use rusqlite::{Connection, params};

/// The schema this code writes.
pub const SCHEMA_VERSION: i64 = 1;

/// The coach's long-term memory, in one SQLite file.
pub struct SqliteMemory {
    conn: Connection,
}

impl std::fmt::Debug for SqliteMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteMemory").finish_non_exhaustive()
    }
}

impl SqliteMemory {
    /// Open (or create) the store at `path`. The parent directory must exist.
    pub fn open(path: &Path) -> Result<Self, MemoryError> {
        Self::init(Connection::open(path).map_err(storage)?)
    }

    /// A store that lives only in memory (tests, or a player who opts out of keeping
    /// notes).
    pub fn open_in_memory() -> Result<Self, MemoryError> {
        Self::init(Connection::open_in_memory().map_err(storage)?)
    }

    fn init(conn: Connection) -> Result<Self, MemoryError> {
        conn.pragma_update(None, "secure_delete", true)
            .map_err(storage)?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(storage)?;
        match version {
            0 => {
                conn.execute_batch(
                    "BEGIN;
                     CREATE VIRTUAL TABLE notes USING fts5(
                         text, tags, timestamp UNINDEXED, tokenize = 'unicode61'
                     );
                     PRAGMA user_version = 1;
                     COMMIT;",
                )
                .map_err(storage)?;
            }
            SCHEMA_VERSION => {}
            newer => {
                return Err(MemoryError::Storage(format!(
                    "the coach memory uses schema {newer}, newer than this version \
                     ({SCHEMA_VERSION}); update the app"
                )));
            }
        }
        Ok(Self { conn })
    }

    /// Delete every note.
    pub fn wipe(&mut self) -> Result<(), MemoryError> {
        self.conn
            .execute("DELETE FROM notes", [])
            .map(|_| ())
            .map_err(storage)
    }

    /// How many notes are kept.
    pub fn len(&self) -> Result<usize, MemoryError> {
        self.conn
            .query_row("SELECT count(*) FROM notes", [], |row| row.get::<_, i64>(0))
            .map(|n| usize::try_from(n).unwrap_or(0))
            .map_err(storage)
    }

    /// Whether no note is kept.
    pub fn is_empty(&self) -> Result<bool, MemoryError> {
        self.len().map(|n| n == 0)
    }

    /// Like [`CoachMemory::recall`], but reports storage errors.
    pub fn try_recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryNote>, MemoryError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let words = keywords(query);
        let (sql, fts_query) = if words.is_empty() {
            (
                "SELECT text, tags, timestamp FROM notes
                 ORDER BY timestamp DESC, rowid DESC LIMIT ?1",
                String::new(),
            )
        } else {
            (
                "SELECT text, tags, timestamp FROM notes WHERE notes MATCH ?1
                 ORDER BY rank, timestamp DESC, rowid DESC LIMIT ?2",
                // Words are alphanumeric, so quoting them leaves no FTS5 syntax to inject.
                words
                    .iter()
                    .map(|w| format!("\"{w}\"*"))
                    .collect::<Vec<_>>()
                    .join(" OR "),
            )
        };
        let mut statement = self.conn.prepare(sql).map_err(storage)?;
        let rows = if words.is_empty() {
            statement.query_map(params![limit], note_from_row)
        } else {
            statement.query_map(params![fts_query, limit], note_from_row)
        }
        .map_err(storage)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage)
    }
}

impl CoachMemory for SqliteMemory {
    fn retain(&mut self, note: MemoryNote) -> Result<(), MemoryError> {
        let tags =
            serde_json::to_string(&note.tags).map_err(|e| MemoryError::Storage(e.to_string()))?;
        let timestamp = i64::try_from(note.timestamp).unwrap_or(i64::MAX);
        self.conn
            .execute(
                "INSERT INTO notes (text, tags, timestamp) VALUES (?1, ?2, ?3)",
                params![note.text, tags, timestamp],
            )
            .map(|_| ())
            .map_err(storage)
    }

    /// Storage errors return no notes; use [`SqliteMemory::try_recall`] to see them.
    fn recall(&self, query: &str, limit: usize) -> Vec<MemoryNote> {
        self.try_recall(query, limit).unwrap_or_default()
    }
}

fn note_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryNote> {
    let tags: String = row.get(1)?;
    let timestamp: i64 = row.get(2)?;
    Ok(MemoryNote {
        text: row.get(0)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        timestamp: u64::try_from(timestamp).unwrap_or(0),
    })
}

/// Lowercase alphanumeric words of two or more characters, as the core's in-memory store
/// splits them.
fn keywords(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 1)
        .map(str::to_owned)
        .collect()
}

fn storage(e: rusqlite::Error) -> MemoryError {
    MemoryError::Storage(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(text: &str, tags: &[&str], timestamp: u64) -> MemoryNote {
        MemoryNote {
            text: text.into(),
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            timestamp,
        }
    }

    fn seeded(memory: &mut SqliteMemory) {
        for n in [
            note("Wants to reach Immortal this act", &["goal"], 1),
            note("Struggles with tracking strafes", &[], 2),
            note(
                "Plays Jett; tracking warm-up before ranked",
                &["routine"],
                3,
            ),
            note("Prefers short sessions", &[], 4),
        ] {
            memory.retain(n).unwrap();
        }
    }

    fn timestamps(notes: &[MemoryNote]) -> Vec<u64> {
        notes.iter().map(|n| n.timestamp).collect()
    }

    #[test]
    fn notes_are_retained_recalled_and_wiped_in_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coach.sqlite");
        {
            let mut memory = SqliteMemory::open(&path).unwrap();
            seeded(&mut memory);
            assert_eq!(memory.len().unwrap(), 4);
        }
        let mut memory = SqliteMemory::open(&path).unwrap();
        assert_eq!(memory.len().unwrap(), 4, "notes survive reopening");
        let hits = memory.recall("tracking", 5);
        assert_eq!(
            timestamps(&hits),
            [2, 3],
            "bm25: the shorter, denser note first"
        );
        assert_eq!(hits[1].tags, ["routine"]);

        memory.wipe().unwrap();
        assert!(memory.is_empty().unwrap());
        assert!(memory.recall("", 5).is_empty());
        drop(memory);
        assert!(SqliteMemory::open(&path).unwrap().is_empty().unwrap());
    }

    #[test]
    fn relevance_ranks_first_then_recency_and_the_limit_holds() {
        let mut memory = SqliteMemory::open_in_memory().unwrap();
        seeded(&mut memory);
        assert_eq!(timestamps(&memory.recall("tracking warm-up", 5))[0], 3);
        assert_eq!(memory.recall("tracking", 1).len(), 1);
        assert_eq!(
            timestamps(&memory.recall("", 2)),
            [4, 3],
            "empty query: newest"
        );
        assert_eq!(
            timestamps(&memory.recall("goal", 5)),
            [1],
            "tags are searched"
        );
        assert_eq!(
            timestamps(&memory.recall("track", 5)),
            [2, 3],
            "prefix match"
        );
        assert!(memory.recall("operator", 5).is_empty());
    }

    #[test]
    fn equally_relevant_notes_come_newest_first() {
        let mut memory = SqliteMemory::open_in_memory().unwrap();
        for (text, ts) in [
            ("aim drill alpha", 10),
            ("aim drill gamma", 12),
            ("aim drill delta", 11),
        ] {
            memory.retain(note(text, &[], ts)).unwrap();
        }
        assert_eq!(timestamps(&memory.recall("drill", 5)), [12, 11, 10]);
    }
    #[test]
    fn query_syntax_cannot_break_the_search() {
        let mut memory = SqliteMemory::open_in_memory().unwrap();
        seeded(&mut memory);
        for query in [
            "\"",
            "NEAR(",
            "a OR",
            "*",
            "jett\" OR \"x",
            "-- ; DROP TABLE notes",
        ] {
            assert!(memory.try_recall(query, 5).is_ok(), "{query}");
        }
        assert_eq!(timestamps(&memory.try_recall("jett\" OR", 5).unwrap()), [3]);
        assert_eq!(memory.len().unwrap(), 4);
    }

    #[test]
    fn a_newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coach.sqlite");
        drop(SqliteMemory::open(&path).unwrap());
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 2)
            .unwrap();
        let err = SqliteMemory::open(&path).unwrap_err();
        assert!(err.to_string().contains("schema 2"), "{err}");
    }
}
