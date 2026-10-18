# AI coach

The coach's rules live in `aim_core::coach` (player context, knowledge base, drill generation and the tool registry). Adapters expose them; this page covers the MCP server. The plan and its status are in `odd/tasks/ai-coach.md`.

## MCP server (`aimtrainer-mcp`)

`aimtrainer-mcp` is a stdio [MCP](https://modelcontextprotocol.io) server built with the official Rust SDK (`rmcp`). A coding agent (Claude Code, Codex, OpenCode) can use it to read your stats, pick routines and generate scenarios. It never opens a window and never touches the game.

### Tools

These are the same tools as the in-app coach. Every call is forwarded to `aim_core::coach::tools::call`.

| Tool | What it does |
|---|---|
| `list_stats` | Your stats summary (recent runs, benchmark energy, fatigue) and the weakest sub-skills |
| `recommend_routine` | Built-in routines and scenarios for the weakest sub-skills, plus a benchmark playlist |
| `validate_scenario` | Checks scenario TOML and returns every error |
| `create_scenario` | Builds a validated, playable scenario from TOML or from a sub-skill and level |
| `remember` / `recall` | Notes about your goals and habits |

- **`create_scenario` is a dry run by default.** It only returns the TOML. Pass `"dry_run": false` to also save it as `<id>.toml` in your custom scenarios folder. An existing file is never replaced. Generated scenarios are uncalibrated.
- **Reads:** the server reads your custom content and `history.csv` from the same folders as the trainer (see `docs/scenarios.md`). Only history-based analyses are available; the skill profile, flick directions, miss compass and pace need replays, which the server does not read.
- **Memory:** notes are kept in memory and last only as long as the server process. The planned SQLite store will make them persistent.

### Resources

| URI | Content |
|---|---|
| `aimtrainer://scenarios/<id>` | Each built-in scenario's TOML |
| `aimtrainer://docs/scenarios.md` | The scenario and routine format |

### Install

```sh
cargo install --path crates/aim_mcp     # puts aimtrainer-mcp on your PATH (~/.cargo/bin)
```

Alternatively, use the full path to a built binary (`target/release/aimtrainer-mcp`, with `.exe` on Windows) in the commands below.

### Register it with an agent

**Claude Code:**

```sh
claude mcp add aimtrainer -- aimtrainer-mcp
```

To share it with a project instead, put this in `.mcp.json` at the project root:

```json
{ "mcpServers": { "aimtrainer": { "command": "aimtrainer-mcp", "args": [] } } }
```

**Codex:** run `codex mcp add aimtrainer -- aimtrainer-mcp`, or add this to `~/.codex/config.toml`:

```toml
[mcp_servers.aimtrainer]
command = "aimtrainer-mcp"
```

**OpenCode:** add this to `opencode.json` (in the project, or `~/.config/opencode/opencode.json`):

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "aimtrainer": { "type": "local", "command": ["aimtrainer-mcp"], "enabled": true }
  }
}
```

Then ask the agent, for example: "Look at my aim trainer stats and make me a micro-adjustment drill."
