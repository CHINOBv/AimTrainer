//! Spawn the real binary and speak raw JSON-RPC over its stdio, as an agent would.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

#[test]
fn initialize_then_tools_list_returns_the_six_coach_tools() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aimtrainer-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the server binary starts");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    let (tx, rx) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(msg) = serde_json::from_str(&line)
                && tx.send(msg).is_err()
            {
                break;
            }
        }
    });
    // The closure owns stdin, so dropping it closes the server's input.
    let mut send = move |msg: Value| {
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
    };
    let reply = |id: u64| loop {
        let msg = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("the server answers in time");
        if msg["id"] == id {
            break msg;
        }
    };

    send(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "stdio-test", "version": "0" }
        }
    }));
    let init = reply(1);
    assert_eq!(init["result"]["serverInfo"]["name"], "aimtrainer", "{init}");
    assert!(
        init["result"]["capabilities"]["tools"].is_object(),
        "{init}"
    );
    assert!(
        init["result"]["capabilities"]["resources"].is_object(),
        "{init}"
    );

    send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let list = reply(2);
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list returns tools: {list}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "list_stats",
            "recommend_routine",
            "validate_scenario",
            "create_scenario",
            "remember",
            "recall"
        ]
    );

    drop(send);
    let status = child.wait().expect("the server exits when stdin closes");
    assert!(status.success(), "{status}");
}
