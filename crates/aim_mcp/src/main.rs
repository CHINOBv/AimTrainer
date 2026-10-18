//! `aimtrainer-mcp`: the trainer's MCP server over stdio. Agents spawn it; it never opens a
//! window or touches the game.

use aim_mcp::{CoachServer, Paths};
use rmcp::ServiceExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    // stdout carries the protocol, so every diagnostic goes to stderr.
    let server = CoachServer::new(Paths::resolve());
    let running = match server.serve(rmcp::transport::stdio()).await {
        Ok(running) => running,
        Err(e) => {
            eprintln!("aimtrainer-mcp: could not start: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match running.waiting().await {
        Ok(_) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("aimtrainer-mcp: stopped with an error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
