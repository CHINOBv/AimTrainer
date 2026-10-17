//! Command-line entry points that run without opening the trainer window.

use crate::platform::ipc;
use bevy::app::AppExit;

pub const USAGE: &str = "\
usage:
  aimtrainer                                   start the trainer
  aimtrainer toggle|show|hide                  control a running trainer (Linux)
  aimtrainer analyze-match <match.json> --player <Name#TAG|puuid>
                                               post-match analysis of an official match file";

#[derive(Debug, Clone, PartialEq)]
pub enum CliCommand {
    Overlay(ipc::Command),
    AnalyzeMatch { file: String, player: String },
    Help,
}

/// Parse arguments after the program name. `Ok(None)` means "start the trainer".
pub fn parse(args: &[String]) -> Result<Option<CliCommand>, String> {
    let Some(first) = args.first() else {
        return Ok(None);
    };
    if let Some(cmd) = ipc::Command::parse(first) {
        return Ok(Some(CliCommand::Overlay(cmd)));
    }
    match first.as_str() {
        "-h" | "--help" | "help" => Ok(Some(CliCommand::Help)),
        "analyze-match" => {
            let file = args
                .get(1)
                .filter(|a| !a.starts_with("--"))
                .ok_or("analyze-match needs a match JSON file")?;
            let player = args
                .iter()
                .position(|a| a == "--player")
                .and_then(|i| args.get(i + 1))
                .ok_or("analyze-match needs --player <Name#TAG|puuid>")?;
            Ok(Some(CliCommand::AnalyzeMatch {
                file: file.clone(),
                player: player.clone(),
            }))
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

/// Run a CLI command; returns `None` when the trainer should start instead.
pub fn run(args: &[String]) -> Option<AppExit> {
    let command = match parse(args) {
        Ok(Some(c)) => c,
        Ok(None) => return None,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return Some(AppExit::error());
        }
    };
    Some(match command {
        CliCommand::Help => {
            println!("{USAGE}");
            AppExit::Success
        }
        CliCommand::Overlay(cmd) => match ipc::send(cmd) {
            Ok(()) => AppExit::Success,
            Err(e) => {
                eprintln!("{e}");
                AppExit::error()
            }
        },
        CliCommand::AnalyzeMatch { file, player } => analyze_match(&file, &player),
    })
}

#[cfg(feature = "match-analysis")]
fn analyze_match(file: &str, player: &str) -> AppExit {
    let json = match std::fs::read_to_string(file) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("cannot read {file}: {e}");
            return AppExit::error();
        }
    };
    match aim_integrations::analyze_json(&json, player) {
        Ok(report) => {
            println!("{}", report.to_text());
            AppExit::Success
        }
        Err(e) => {
            eprintln!("{e}");
            AppExit::error()
        }
    }
}

#[cfg(not(feature = "match-analysis"))]
fn analyze_match(_file: &str, _player: &str) -> AppExit {
    eprintln!("this build was compiled without the `match-analysis` feature");
    AppExit::error()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_start_the_trainer() {
        assert_eq!(parse(&[]), Ok(None));
    }

    #[test]
    fn overlay_and_help_commands_parse() {
        assert_eq!(
            parse(&args(&["toggle"])),
            Ok(Some(CliCommand::Overlay(ipc::Command::Toggle)))
        );
        assert_eq!(parse(&args(&["--help"])), Ok(Some(CliCommand::Help)));
        assert!(
            parse(&args(&["explode"]))
                .unwrap_err()
                .contains("unknown command")
        );
    }

    #[test]
    fn analyze_match_needs_a_file_and_a_player() {
        assert_eq!(
            parse(&args(&["analyze-match", "m.json", "--player", "Alpha#NA1"])),
            Ok(Some(CliCommand::AnalyzeMatch {
                file: "m.json".into(),
                player: "Alpha#NA1".into()
            }))
        );
        assert!(parse(&args(&["analyze-match", "--player", "x"])).is_err());
        assert!(parse(&args(&["analyze-match", "m.json"])).is_err());
    }
}
