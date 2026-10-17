//! A finished (or in-progress) run captured as its definition, seed and input log.
//! Because `Session` is deterministic, replaying the inputs reproduces every frame, which
//! is what analytics and replays build on.

use super::def::ScenarioDef;
use super::session::{FrameInput, Session};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recording {
    pub def: ScenarioDef,
    pub seed: u64,
    /// One entry per `Session::update` call, in order.
    pub inputs: Vec<FrameInput>,
}

impl Recording {
    pub fn new(def: ScenarioDef, seed: u64) -> Self {
        Self {
            def,
            seed,
            inputs: Vec::new(),
        }
    }

    /// Fresh session in the recording's initial state.
    pub fn start(&self) -> Session {
        Session::new(self.def.clone(), self.seed)
    }

    /// Re-simulate the whole run, calling `observe` after every frame.
    pub fn replay(&self, mut observe: impl FnMut(&Session, &FrameInput)) -> Session {
        let mut s = self.start();
        for input in &self.inputs {
            s.update(*input);
            observe(&s, input);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;
    use crate::sim::{AimerProfile, SimAimer};

    #[test]
    fn replaying_the_input_log_reproduces_the_result_exactly() {
        let def = builtin::all()
            .into_iter()
            .find(|d| d.id == "switch-speed")
            .unwrap();
        let mut rec = Recording::new(def, 77);
        let mut live = rec.start();
        let mut aimer = SimAimer::new(AimerProfile::PRESETS[2], 77);
        while !live.is_finished() {
            let input = aimer.input(&live, 1.0 / 144.0);
            rec.inputs.push(input);
            live.update(input);
        }
        let mut frames = 0;
        let replayed = rec.replay(|_, _| frames += 1);
        assert_eq!(frames, rec.inputs.len());
        assert_eq!(replayed.result(), live.result());
        assert_eq!(replayed.stats(), live.stats());
    }
}
