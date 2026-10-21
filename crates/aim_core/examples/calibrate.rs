//! Print simulated scores per aimer profile for every built-in scenario.
//! Used to generate provisional benchmark thresholds: `cargo run -p aim_core --release --example calibrate`.
//!
//! Scenarios with phases, fail conditions or a non-time end get one more line: how far the
//! contender got and how often its runs failed.
//!
//! Options: `--level <x>` plays every scenario at difficulty level `x` (the benchmark tiers use
//! -0.5 and 0.5), and any other argument keeps only the scenarios with that id, e.g.
//! `cargo run -p aim_core --release --example calibrate -- --level -0.5 crossing-lanes`.

use aim_core::scenario::{RunOutcome, ScenarioDef, Session, builtin};
use aim_core::sim::{AimerProfile, SimAimer, simulate};

const SEEDS: [u64; 5] = [1, 2, 3, 4, 5];

fn main() {
    let mut level = 0.0f32;
    let mut ids = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--level" {
            level = args
                .next()
                .and_then(|v| v.parse().ok())
                .expect("--level needs a number");
        } else {
            ids.push(arg);
        }
    }
    print!("{:<24}", "scenario");
    for p in AimerProfile::PRESETS {
        print!("{:>12}", p.name);
    }
    println!("{:>10}{:>8}", "acc(ctd)", "kills");
    let defs = builtin::all()
        .into_iter()
        .filter(|d| ids.is_empty() || ids.contains(&d.id))
        .map(|d| {
            if level == 0.0 {
                d
            } else {
                d.at_difficulty(level)
            }
        });
    for def in defs {
        print!("{:<24}", def.id);
        let mut mid = None;
        for p in AimerProfile::PRESETS {
            let runs: Vec<_> = SEEDS.iter().map(|s| simulate(&def, p, *s)).collect();
            let score = runs.iter().map(|r| r.score).sum::<f32>() / runs.len() as f32;
            print!("{score:>12.0}");
            if p.name == "contender" {
                mid = Some(runs[0].clone());
            }
        }
        let m = mid.unwrap();
        println!("{:>10.2}{:>8}", m.accuracy, m.kills);
        if def.phases.is_some() || !def.fail.is_empty() || !def.end.is_time() {
            print_run_rules(&def);
        }
    }
}

/// The contender's phase reached (1-based) and outcome over every seed.
fn print_run_rules(def: &ScenarioDef) {
    const DT: f32 = 1.0 / 240.0;
    let contender = AimerProfile::PRESETS[1];
    let (mut phases, mut failed, mut secs) = (Vec::new(), 0, 0.0);
    for seed in SEEDS {
        let mut session = Session::new(def.clone(), seed);
        let mut aimer = SimAimer::new(contender, seed);
        while !session.is_finished() {
            let input = aimer.input(&session, DT);
            session.update(input);
        }
        phases.push(session.phase_index().map_or(0, |i| i + 1));
        failed += usize::from(matches!(session.outcome(), Some(RunOutcome::Failed(_))));
        secs += session.time() / SEEDS.len() as f32;
    }
    println!(
        "  contender: phase reached {phases:?} of {}, failed {failed}/{}, mean {secs:.0} s",
        def.phases.as_ref().map_or(0, Vec::len),
        SEEDS.len()
    );
}
