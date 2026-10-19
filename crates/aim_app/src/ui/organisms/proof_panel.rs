//! Verifiable score proofs (`aim_core::analytics::proof`): the share code of a finished run,
//! and a "Verify a proof" field that re-simulates a pasted code on a background thread
//! against the player's own library.

use crate::i18n::{tr, tr_fmt, tr_named};
use crate::ui::atoms::{BAD, GOOD, big};
use aim_core::analytics::proof::{ScoreProof, verify};
use aim_core::scenario::{Recording, RunKey, RunResult, ScenarioDef};
use bevy_egui::egui;
use std::thread::JoinHandle;

/// The share code proving that `recording` scores `result`.
pub fn proof_code(recording: &Recording, result: &RunResult) -> Result<String, String> {
    ScoreProof::new(recording.clone(), result)
        .and_then(|p| p.encode())
        .map_err(|e| tr_fmt("Could not build a proof code: {}", &[&e]))
}

/// Re-simulate a pasted code against `library`: the verified line, or why it failed.
pub fn verify_code(code: &str, library: &[ScenarioDef]) -> Result<String, String> {
    let proof = ScoreProof::decode(code).map_err(|e| not_verified(&e))?;
    let v = verify(&proof, library).map_err(|e| not_verified(&e))?;
    let base = RunKey::parse(&v.scenario_id).base.to_owned();
    let name = library
        .iter()
        .find(|d| d.id == base)
        .map_or(base.clone(), |d| tr(&d.name).to_owned());
    let scenario = if v.scenario_id == base {
        name
    } else {
        format!("{name} ({})", v.scenario_id)
    };
    Ok(tr_named(
        "Verified: {scenario} score {score}, accuracy {accuracy}%",
        &[
            ("scenario", &scenario),
            ("score", &format!("{:.0}", v.result.score)),
            ("accuracy", &format!("{:.1}", v.result.accuracy * 100.0)),
        ],
    ))
}

/// The failed-verification line for `reason`.
fn not_verified(reason: &dyn std::fmt::Display) -> String {
    tr_fmt("Not verified: {}", &[reason])
}

/// The pasted code and the verification running or finished.
#[derive(Debug, Default)]
pub struct ProofVerifier {
    pub code: String,
    job: Option<JoinHandle<Result<String, String>>>,
    outcome: Option<Result<String, String>>,
}

impl ProofVerifier {
    /// Verify the pasted code against `library` on a background thread, unless one runs.
    pub fn start(&mut self, library: Vec<ScenarioDef>) {
        if self.job.is_some() {
            return;
        }
        let code = self.code.clone();
        let spawned = std::thread::Builder::new()
            .name("proof-verify".into())
            .spawn(move || verify_code(&code, &library));
        match spawned {
            Ok(h) => {
                self.job = Some(h);
                self.outcome = None;
            }
            Err(e) => self.outcome = Some(Err(not_verified(&e))),
        }
    }

    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// Collect a finished verification. Call once per frame.
    pub fn poll(&mut self) {
        if self.job.as_ref().is_some_and(|h| h.is_finished())
            && let Some(h) = self.job.take()
        {
            self.outcome = Some(
                h.join()
                    .unwrap_or_else(|_| Err(tr("Not verified: the verifier stopped").to_owned())),
            );
        }
    }

    pub fn outcome(&self) -> Option<&Result<String, String>> {
        self.outcome.as_ref()
    }

    /// Block until the running verification ends (tests).
    #[cfg(test)]
    fn wait(&mut self) {
        if let Some(h) = &self.job {
            while !h.is_finished() {
                std::thread::yield_now();
            }
        }
        self.poll();
    }
}

/// "Verify a proof": returns true when the player asks to verify the pasted code.
pub fn verify_section(ui: &mut egui::Ui, v: &mut ProofVerifier) -> bool {
    ui.add_space(10.0);
    ui.label(big(tr("Verify a proof"), 20.0));
    ui.small(tr("Paste a friend's proof code: the run is replayed against your own scenarios, so an edited scenario or a changed score is caught. It shows the inputs give that score, not who played them."));
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut v.code)
                .hint_text(tr("proof code"))
                .desired_width(360.0),
        );
        let ready = !v.is_running() && !v.code.trim().is_empty();
        clicked = ui
            .add_enabled(ready, egui::Button::new(tr("Verify")))
            .clicked();
        if v.is_running() {
            ui.spinner();
        }
    });
    match v.outcome() {
        Some(Ok(line)) => {
            ui.colored_label(GOOD, line);
        }
        Some(Err(e)) => {
            ui.colored_label(BAD, e);
        }
        None => {}
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::scenario::builtin;

    /// A real run of `def` by the core's simulated aimer, recorded frame by frame.
    fn run(def: ScenarioDef) -> (Recording, RunResult) {
        use aim_core::scenario::Session;
        use aim_core::sim::{AimerProfile, SimAimer};
        let mut session = Session::new(def.clone(), 7);
        let mut aimer = SimAimer::new(AimerProfile::PRESETS[2], 7);
        let mut inputs = Vec::new();
        while !session.is_finished() {
            let input = aimer.input(&session, 1.0 / 120.0);
            session.update(input);
            inputs.push(input);
        }
        let result = session.result();
        assert!(result.score > 0.0, "the run scores");
        let rec = Recording {
            def,
            seed: 7,
            inputs,
        };
        (rec, result)
    }

    fn micro_adjust() -> ScenarioDef {
        builtin::all()
            .into_iter()
            .find(|d| d.id == "micro-adjust")
            .unwrap()
    }

    #[test]
    fn a_runs_proof_code_verifies_against_the_library() {
        let lib = builtin::all();
        let (rec, result) = run(micro_adjust());
        let code = proof_code(&rec, &result).unwrap();
        let line = verify_code(&code, &lib).unwrap();
        assert_eq!(
            line,
            format!(
                "Verified: {} score {:.0}, accuracy {:.1}%",
                rec.def.name,
                result.score,
                result.accuracy * 100.0
            )
        );
    }

    #[test]
    fn a_variant_names_its_run_key() {
        let lib = builtin::all();
        let def = micro_adjust().at_difficulty(0.5).with_duration(20.0);
        let key = def.id.clone();
        let (rec, result) = run(def);
        let line = verify_code(&proof_code(&rec, &result).unwrap(), &lib).unwrap();
        assert!(line.contains(&format!("({key})")), "{line}");
    }

    #[test]
    fn bad_codes_and_changed_scores_give_the_precise_error() {
        let lib = builtin::all();
        assert_eq!(
            verify_code("not a code!", &lib),
            Err("Not verified: not a score proof code".to_owned())
        );
        let (rec, mut result) = run(micro_adjust());
        result.score += 1.0;
        let err = verify_code(&proof_code(&rec, &result).unwrap(), &lib).unwrap_err();
        assert!(err.starts_with("Not verified: claimed"), "{err}");
        let (rec, result) = run(micro_adjust());
        let err = verify_code(&proof_code(&rec, &result).unwrap(), &[]).unwrap_err();
        assert_eq!(err, "Not verified: unknown scenario micro-adjust");
    }

    #[test]
    fn verification_runs_in_the_background_and_reports_back() {
        let (rec, result) = run(micro_adjust());
        let mut v = ProofVerifier {
            code: proof_code(&rec, &result).unwrap(),
            ..ProofVerifier::default()
        };
        v.start(builtin::all());
        assert!(v.is_running() || v.outcome().is_some());
        v.wait();
        assert!(!v.is_running());
        assert!(matches!(v.outcome(), Some(Ok(line)) if line.starts_with("Verified: ")));
    }

    #[test]
    fn proof_lines_read_in_spanish() {
        use aim_core::i18n::{Lang, tr_fmt_in, tr_in};
        assert_eq!(
            tr_fmt_in(Lang::Es, "Not verified: {}", &[&"x"]),
            "No verificado: x"
        );
        assert_eq!(tr_in(Lang::Es, "Verify a proof"), "Verificar una prueba");
    }
}
