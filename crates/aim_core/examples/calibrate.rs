//! Print simulated scores per aimer profile for every built-in scenario.
//! Used to generate provisional benchmark thresholds: `cargo run -p aim_core --release --example calibrate`.

use aim_core::scenario::builtin;
use aim_core::sim::{AimerProfile, simulate};

const SEEDS: [u64; 5] = [1, 2, 3, 4, 5];

fn main() {
    print!("{:<24}", "scenario");
    for p in AimerProfile::PRESETS {
        print!("{:>12}", p.name);
    }
    println!("{:>10}{:>8}", "acc(ctd)", "kills");
    for def in builtin::all() {
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
    }
}
