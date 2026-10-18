//! [`ChatModel`] that delegates to a CLI the player installed and signed in to
//! themselves: `codex exec`, `claude -p --output-format json` or
//! `opencode run --format json`.
//!
//! - The conversation is flattened into one prompt; the reply is the CLI's final text.
//! - Tool calls are not supported in this mode: the reply never contains any, so the
//!   coach answers from the conversation alone.
//! - This provider never manages logins and never reads any CLI's credential files. It
//!   runs the program with the player's environment, in an empty scratch directory, and
//!   kills it after the timeout.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use aim_core::coach::{ChatModel, Message, ModelError, Reply, ToolSpec};
use serde_json::Value;

use crate::config::PromptInput;

/// How often the child process is polled while waiting.
const POLL: Duration = Duration::from_millis(20);
/// Longest stderr excerpt kept in an error.
const MAX_STDERR_CHARS: usize = 300;

/// `codex exec`, reading the prompt from stdin (`-`). Codex refuses to run outside a git
/// repository unless told otherwise, and the scratch directory is not one.
pub fn codex_command() -> Vec<String> {
    to_strings(&["codex", "exec", "--skip-git-repo-check", "-"])
}

/// `claude -p --output-format json`, reading the prompt from stdin.
pub fn claude_command() -> Vec<String> {
    to_strings(&["claude", "-p", "--output-format", "json"])
}

/// `opencode run --format json`; use it with [`PromptInput::Argument`].
pub fn opencode_command() -> Vec<String> {
    to_strings(&["opencode", "run", "--format", "json"])
}

fn to_strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_owned()).collect()
}

/// Runs a CLI for each reply.
#[derive(Debug, Clone)]
pub struct CliDelegateProvider {
    command: Vec<String>,
    prompt_input: PromptInput,
    timeout: Duration,
    workdir: PathBuf,
}

impl CliDelegateProvider {
    /// A provider running `command` (program then arguments). The process starts in an
    /// empty directory under the system temp dir, never in the player's files.
    pub fn new(command: Vec<String>, prompt_input: PromptInput, timeout: Duration) -> Self {
        Self {
            command,
            prompt_input,
            timeout,
            workdir: std::env::temp_dir().join("aimtrainer-coach-cli"),
        }
    }

    fn run(&self, prompt: &str) -> Result<String, ModelError> {
        let (program, args) = self
            .command
            .split_first()
            .ok_or_else(|| ModelError::Request("no CLI command is configured".to_owned()))?;
        std::fs::create_dir_all(&self.workdir).map_err(|e| {
            ModelError::Request(format!("could not create the CLI scratch directory: {e}"))
        })?;
        let mut command = Command::new(resolve_program(program));
        command
            .args(args)
            .current_dir(&self.workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.prompt_input == PromptInput::Argument {
            command.arg(prompt);
        }
        let mut child = command
            .spawn()
            .map_err(|e| ModelError::Request(format!("could not start {program:?}: {e}")))?;

        let stdin = child.stdin.take();
        let input = match self.prompt_input {
            PromptInput::Stdin => prompt.to_owned(),
            PromptInput::Argument => String::new(),
        };
        let writer = thread::spawn(move || {
            if let Some(mut stdin) = stdin {
                // A CLI that exits without reading closes the pipe; that is not an error.
                let _ = stdin.write_all(input.as_bytes());
            }
        });
        let stdout = read_in_background(child.stdout.take());
        let stderr = read_in_background(child.stderr.take());

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ModelError::Request(format!(
                        "{program:?} did not answer within {} s and was stopped",
                        self.timeout.as_secs()
                    )));
                }
                Ok(None) => thread::sleep(POLL),
                Err(e) => {
                    return Err(ModelError::Request(format!(
                        "could not wait for {program:?}: {e}"
                    )));
                }
            }
        };
        let _ = writer.join();
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();
        if !status.success() {
            let excerpt: String = stderr.trim().chars().take(MAX_STDERR_CHARS).collect();
            return Err(ModelError::Request(format!(
                "{program:?} failed ({status}); is it installed and signed in? {excerpt}"
            )));
        }
        Ok(stdout)
    }
}

impl ChatModel for CliDelegateProvider {
    fn complete(&mut self, messages: &[Message], _tools: &[ToolSpec]) -> Result<Reply, ModelError> {
        let stdout = self.run(&flatten(messages))?;
        Ok(Reply {
            text: parse_output(&stdout)?,
            tool_calls: Vec::new(),
        })
    }
}

fn read_in_background<R: Read + Send + 'static>(pipe: Option<R>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// On Windows, npm installs CLIs as `.cmd` shims that `Command` does not find by bare
/// name; look for `.exe`, `.cmd` and `.bat` on `PATH`. Elsewhere the name is used as is.
fn resolve_program(program: &str) -> PathBuf {
    if cfg!(windows) && std::path::Path::new(program).extension().is_none() {
        let paths = std::env::var_os("PATH").unwrap_or_default();
        for dir in std::env::split_paths(&paths) {
            for ext in ["exe", "cmd", "bat"] {
                let candidate = dir.join(format!("{program}.{ext}"));
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    PathBuf::from(program)
}

/// The conversation as one prompt. Tool turns are summarized, since this mode cannot run
/// tools.
pub fn flatten(messages: &[Message]) -> String {
    let mut out = String::new();
    for message in messages {
        let (label, text) = match message {
            Message::System { content } => ("Instructions", content.as_str()),
            Message::User { content } => ("Player", content.as_str()),
            Message::Assistant { content, .. } => ("Coach", content.as_str()),
            Message::Tool { content, .. } => ("Tool result", content.as_str()),
        };
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(label);
        out.push_str(":\n");
        out.push_str(text);
    }
    out.push_str("\n\nAnswer as the coach, in plain text. No tools are available.");
    out
}

/// The reply text in a CLI's output:
/// - one JSON object with a `result` string (`claude -p --output-format json`); an
///   `is_error: true` object is an error;
/// - JSON lines with text events (`opencode run --format json`): the text is joined;
/// - anything else is plain text (`codex exec` prints its final message).
pub fn parse_output(stdout: &str) -> Result<String, ModelError> {
    let trimmed = stdout.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed)
        && let Some(result) = value.get("result").and_then(Value::as_str)
    {
        if value.get("is_error").and_then(Value::as_bool) == Some(true) {
            return Err(ModelError::Request(format!(
                "the CLI reported an error: {result}"
            )));
        }
        return Ok(result.trim().to_owned());
    }
    let events: Vec<Value> = trimmed
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .filter(Value::is_object)
        .collect();
    if !events.is_empty()
        && events.len() == trimmed.lines().filter(|l| !l.trim().is_empty()).count()
    {
        let text: String = events.iter().filter_map(event_text).collect();
        if !text.trim().is_empty() {
            return Ok(text.trim().to_owned());
        }
    }
    if trimmed.is_empty() {
        return Err(ModelError::InvalidReply(
            "the CLI printed nothing".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

/// Text from one JSON event: `{"type":"text","part":{"text":..}}` or
/// `{"type":"text","text":..}`.
fn event_text(event: &Value) -> Option<&str> {
    if event.get("type").and_then(Value::as_str) != Some("text") {
        return None;
    }
    event
        .pointer("/part/text")
        .or_else(|| event.get("text"))
        .and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_conversation_becomes_one_labelled_prompt() {
        let prompt = flatten(&[
            Message::System {
                content: "Be brief.".into(),
            },
            Message::User {
                content: "How am I doing?".into(),
            },
            Message::Assistant {
                content: String::new(),
                tool_calls: Vec::new(),
            },
        ]);
        assert!(
            prompt.starts_with("Instructions:\nBe brief.\n\nPlayer:\nHow am I doing?\n\n"),
            "{prompt}"
        );
        assert!(prompt.ends_with("No tools are available."));
        assert!(!prompt.contains("Coach:"), "empty turns are skipped");
    }

    #[test]
    fn claude_json_opencode_events_and_plain_text_are_read() {
        let claude = r#"{"type":"result","subtype":"success","is_error":false,"result":"Train flicks.","session_id":"s"}"#;
        assert_eq!(parse_output(claude).unwrap(), "Train flicks.");
        let failed = r#"{"type":"result","is_error":true,"result":"not logged in"}"#;
        assert!(
            matches!(parse_output(failed), Err(ModelError::Request(t)) if t.contains("not logged in"))
        );
        let opencode = concat!(
            "{\"type\":\"step_start\",\"part\":{}}\n",
            "{\"type\":\"text\",\"part\":{\"text\":\"Track \"}}\n",
            "{\"type\":\"text\",\"text\":\"strafes.\"}\n",
            "{\"type\":\"step_finish\",\"part\":{}}\n"
        );
        assert_eq!(parse_output(opencode).unwrap(), "Track strafes.");
        assert_eq!(parse_output("  Warm up.\n").unwrap(), "Warm up.");
        assert_eq!(
            parse_output("{\"a\":1}\nthen plain text").unwrap(),
            "{\"a\":1}\nthen plain text",
            "mixed output is plain text"
        );
        assert!(matches!(
            parse_output(" \n"),
            Err(ModelError::InvalidReply(_))
        ));
    }

    #[cfg(windows)]
    fn fake(script: &str) -> Vec<String> {
        to_strings(&[
            "powershell",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
    }

    #[cfg(unix)]
    fn fake(script: &str) -> Vec<String> {
        to_strings(&["sh", "-c", script])
    }

    #[cfg(windows)]
    const ECHO_STDIN_AS_CLAUDE_JSON: &str = "$p = [Console]::In.ReadToEnd(); \
        $o = @{ type = 'result'; is_error = $false; result = ('got ' + $p.Length) }; \
        [Console]::Out.Write(($o | ConvertTo-Json -Compress))";
    #[cfg(unix)]
    const ECHO_STDIN_AS_CLAUDE_JSON: &str = "n=$(wc -c | tr -d ' '); \
        printf '{\"type\":\"result\",\"is_error\":false,\"result\":\"got %s\"}' \"$n\"";

    #[cfg(windows)]
    const SLEEP: &str = "Start-Sleep -Seconds 20";
    #[cfg(unix)]
    const SLEEP: &str = "sleep 20";

    #[cfg(windows)]
    const FAIL: &str = "[Console]::Error.Write('please log in'); exit 3";
    #[cfg(unix)]
    const FAIL: &str = "printf 'please log in' >&2; exit 3";

    fn messages() -> Vec<Message> {
        vec![Message::User {
            content: "hello".into(),
        }]
    }

    #[test]
    fn a_fake_cli_receives_the_prompt_on_stdin_and_its_json_is_parsed() {
        let mut p = CliDelegateProvider::new(
            fake(ECHO_STDIN_AS_CLAUDE_JSON),
            PromptInput::Stdin,
            Duration::from_secs(60),
        );
        let reply = p
            .complete(&messages(), &aim_core::coach::tools::specs())
            .unwrap();
        let expected = flatten(&messages()).len();
        assert_eq!(reply.text, format!("got {expected}"));
        assert!(reply.tool_calls.is_empty(), "this mode never calls tools");
    }

    #[test]
    fn a_slow_cli_is_stopped_at_the_timeout() {
        let mut p =
            CliDelegateProvider::new(fake(SLEEP), PromptInput::Stdin, Duration::from_secs(2));
        let started = Instant::now();
        let err = p.complete(&messages(), &[]).unwrap_err();
        assert!(
            matches!(&err, ModelError::Request(t) if t.contains("did not answer")),
            "{err:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_failing_or_missing_cli_is_an_error_with_its_stderr() {
        let mut p =
            CliDelegateProvider::new(fake(FAIL), PromptInput::Stdin, Duration::from_secs(60));
        let err = p.complete(&messages(), &[]).unwrap_err();
        assert!(
            matches!(&err, ModelError::Request(t) if t.contains("please log in")),
            "{err:?}"
        );
        let mut missing = CliDelegateProvider::new(
            to_strings(&["aimtrainer-no-such-cli-xyz"]),
            PromptInput::Stdin,
            Duration::from_secs(5),
        );
        let err = missing.complete(&messages(), &[]).unwrap_err();
        assert!(
            matches!(&err, ModelError::Request(t) if t.contains("could not start")),
            "{err:?}"
        );
    }

    #[test]
    fn presets_match_the_documented_commands() {
        assert_eq!(
            claude_command(),
            ["claude", "-p", "--output-format", "json"]
        );
        assert_eq!(opencode_command(), ["opencode", "run", "--format", "json"]);
        assert_eq!(codex_command()[..2], ["codex", "exec"]);
    }
}
