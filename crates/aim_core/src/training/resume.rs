//! Resuming split sessions: where the player is in each routine played today. The rules
//! live here; the app stores the positions through [`PositionStore`].

use serde::{Deserialize, Serialize};

/// A routine in progress. Matching `date` and `routine` means "resume"; anything else
/// means a new session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutinePosition {
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    /// Routine id being played.
    pub routine: String,
    /// Base difficulty level the routine is played at.
    pub level: f32,
    /// Shuffle seed, so the resumed plan has the same order.
    pub seed: u64,
    /// Index of the next run to play.
    pub next_index: usize,
    /// Total number of runs in the routine plan.
    pub total_runs: usize,
}

impl RoutinePosition {
    /// Whether every run of the routine has been played.
    pub fn is_done(&self) -> bool {
        self.next_index >= self.total_runs
    }

    /// Whether this is the position of `routine` on `date` (finished or not; see
    /// [`Self::is_done`]).
    pub fn resumes(&self, date: &str, routine: &str) -> bool {
        self.date == date && self.routine == routine
    }
}

/// Port: where positions are kept between launches.
pub trait PositionStore {
    /// All stored positions.
    fn positions(&self) -> &[RoutinePosition];
    /// Replace the stored positions.
    fn save_positions(&mut self, positions: Vec<RoutinePosition>);

    /// Today's unfinished position in `routine`.
    fn resumable(&self, date: &str, routine: &str) -> Option<&RoutinePosition> {
        self.positions()
            .iter()
            .find(|p| p.resumes(date, routine) && !p.is_done())
    }
}

/// Run `played` of `routine` (planned at `level` with `seed`, `total_runs` long) just
/// finished on `date`: move its position forward, start it if it is new, and drop
/// positions from other days.
pub fn record_played(
    store: &mut impl PositionStore,
    date: &str,
    routine: &str,
    level: f32,
    seed: u64,
    played: usize,
    total_runs: usize,
) {
    let mut positions: Vec<RoutinePosition> = store
        .positions()
        .iter()
        .filter(|p| p.date == date)
        .cloned()
        .collect();
    match positions.iter_mut().find(|p| p.routine == routine) {
        Some(p) if p.seed == seed => p.next_index = p.next_index.max(played + 1),
        Some(p) => {
            // The routine was restarted (new shuffle): its position starts over.
            *p = RoutinePosition {
                date: date.to_owned(),
                routine: routine.to_owned(),
                level,
                seed,
                next_index: played + 1,
                total_runs,
            };
        }
        None => positions.push(RoutinePosition {
            date: date.to_owned(),
            routine: routine.to_owned(),
            level,
            seed,
            next_index: played + 1,
            total_runs,
        }),
    }
    store.save_positions(positions);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Memory(Vec<RoutinePosition>, usize);

    impl PositionStore for Memory {
        fn positions(&self) -> &[RoutinePosition] {
            &self.0
        }
        fn save_positions(&mut self, positions: Vec<RoutinePosition>) {
            self.0 = positions;
            self.1 += 1;
        }
    }

    #[test]
    fn played_runs_move_the_position_and_resume_the_same_day_only() {
        let mut s = Memory::default();
        record_played(&mut s, "2026-10-17", "r", 0.25, 7, 0, 4);
        record_played(&mut s, "2026-10-17", "r", 0.25, 7, 1, 4);
        let p = s.resumable("2026-10-17", "r").unwrap();
        assert_eq!(
            (p.next_index, p.total_runs, p.seed, p.level),
            (2, 4, 7, 0.25)
        );
        assert!(s.resumable("2026-10-18", "r").is_none());
        assert!(s.resumable("2026-10-17", "other").is_none());
        assert_eq!(s.1, 2, "every played run is saved");
    }

    #[test]
    fn a_new_day_drops_old_positions_and_a_restart_starts_over() {
        let mut s = Memory::default();
        record_played(&mut s, "2026-10-17", "r", 0.0, 7, 2, 4);
        record_played(&mut s, "2026-10-18", "q", 0.0, 1, 0, 3);
        assert_eq!(s.positions().len(), 1);
        record_played(&mut s, "2026-10-18", "q", 0.0, 2, 0, 3);
        assert_eq!(s.resumable("2026-10-18", "q").unwrap().seed, 2);
        assert_eq!(s.resumable("2026-10-18", "q").unwrap().next_index, 1);
    }

    #[test]
    fn finished_routines_are_not_resumable() {
        let mut s = Memory::default();
        record_played(&mut s, "d", "r", 0.0, 1, 2, 3);
        assert!(s.positions()[0].is_done());
        assert!(s.resumable("d", "r").is_none());
    }
}
