# AI coach

The coach's rules live in `aim_core::coach` (player context, knowledge base, drill generation and the tool registry). Adapters expose them: the trainer's Coach tab and the MCP server. The plan and its status are in `odd/tasks/ai-coach.md`.

## In the trainer (Coach tab)

The Coach tab (key 9) is off until you tick **Enable the AI coach**. While it is off, nothing is sent anywhere.

1. **Pick a provider** under *Provider*:
   - **OpenAI-compatible:** click a base URL preset (OpenAI, OpenCode pay-as-you-go, Ollama, LM Studio) or type one, then type a model id.
   - **Anthropic:** type a model id; the base URL defaults to `https://api.anthropic.com`.
   - **CLI delegate:** click Codex, Claude Code or OpenCode to use your own installed, signed-in CLI. Codex can call the coach's tools through the trainer's MCP server (`aimtrainer-mcp`, found next to the trainer or on `PATH`), so it sees your stats summary. Claude Code and OpenCode are text only: they cannot call the tools and never see your stats summary. The one exception is the results screen: when you press **Ask the coach** or **Ask the coach why**, the run or mistake digest goes inline in the prompt to every provider (see *Coach's take and Ask why*).
2. **Add a key** (not needed for Ollama, LM Studio or a CLI). Paste it and click **Store in keyring**. The field is cleared at once and the key is never shown again; the page only says "Key stored in OS keyring" or, when you name an environment variable in *Key env var*, "Key from env VAR". **Forget key** removes it from the keyring. Each provider has its own key, and each OpenAI-compatible base URL too.
3. **Test connection** sends a one-line prompt without tools and shows the reply or the error.
4. **Preview what is sent** shows the exact `CoachContext::to_prompt_json()` text the model receives when it calls `list_stats`. It holds aggregates only (skill findings, flick directions, miss compass, pace, potential score, benchmark energy, recent runs, missions, readiness, fatigue), built from your history and your newest 20 replays. **Refresh** rebuilds it; it also rebuilds after each new run.
5. **Chat.** Type a message, or click **Analyse my stats** or **Recommend a routine**. Each answer lists the tools the coach used and any that failed. **Cancel** stops waiting (a request already sent still runs to its timeout in the background). **New chat** clears the conversation; it is never saved to disk.
6. **Generated scenarios** appear as cards. **Validate** re-checks the definition and the playability gate (a simulated player must score). **Play** starts it at once. **Save** writes `<id>.toml` to your custom scenarios folder with an "Uncalibrated" header comment; an existing file is never replaced, and the scenario appears under Scenarios after a restart.
7. **Memory.** *Forget everything* (asks for confirmation) deletes every note in `coach.db`.

Every request, keyring access, replay analysis and file write runs on a background thread; the page shows a spinner and never blocks the frame. The settings are the `[coach]` table of `settings.toml` (see below). A `[coach]` table that fails to load (for example one with an `api_key` line) resets only the coach, switched off, and the next save removes the offending line.

## Coach's take and Ask why (results screen)

With the coach enabled, the results screen gets two on-demand requests. Neither runs on its own: nothing is sent until the player clicks, and while the coach is off neither button is shown.

- **Coach's take.** A card under the main fix. **Ask the coach** sends the run digest once and shows one to three points, most important first, and optionally one drill from the library with a **Play** button. A spinner and **Cancel** show while it waits.
- **Ask the coach why.** In the mistake replays, under the template tip of each mistake (the tip always stays). It sends that mistake's digest once and shows why it happened and one cue.
- **Cached per result.** Each answer is kept for the result it belongs to (per mistake for Ask why), so reopening the card, the replay window or a mistake sends nothing again. A new result starts empty.
- **Preview what is sent.** A collapsed section next to each button shows the exact digest JSON and its size.

Both use the same provider, key and background job as the Coach tab, with one model call, no tools and no retries. The rules are in `aim_core::coach::debrief`.

### What is sent

| Request | Content |
|---|---|
| Coach's take | The run digest (`RunDigest`, at most 2 KiB): the scenario id and category, and named numbers with stable metric keys, such as `accuracy_pct`, `flick.overshoot_pct`, `tracking.on_target_pct`, `moving.counter_strafe_pct`, `potential.points_gain` and `history.score_vs_previous` (the score against up to five earlier runs of the same scenario, computed by the core). Also up to 40 scenario ids the coach may suggest, the run's category first. |
| Ask why | The mistake digest (`MistakeDigest`): the scenario id and category, the mistake kind, whether the shot hit, and its numbers (`distance_deg`, `target_radius_deg`, `direction_right_deg`, `direction_up_deg`, `endpoint_error_radii`, `edge_error_deg`, `delay_ms`, `corrections`). |

Never sent: crosshair paths (the player's and the corrected example), trace sample indices, target ids, seeds, the template tip, file paths and names. Both requests also carry the coach's instructions (one external-focus cue, use only the given numbers, never touch the game process).

### How a reply is checked

The model must answer JSON (a fenced `json` block is accepted): `{"bullets":[{"text","cites"}],"drill":{"scenario_id","reason"}}` for the take, `{"why","cue","cites"}` for Ask why. The core rejects the reply, and the card shows the reason, when:

- it is not that JSON, or the model asks for tools;
- the take has no bullets or more than three, a bullet is longer than 160 characters or a drill reason longer than 120; a why is longer than 200 characters or a cue longer than 80;
- a point cites no metric key, or a key that is not in the digest;
- a number in the text is not the value of a cited key, within the rounding of the number as written ("62%" matches 62.4; "71%" does not). Digits glued to a letter, such as "M2", are names, not numbers;
- the drill is not one of the offered scenario ids.

A rejected or failed reply is never retried automatically. The rule-based main fix and tips stay on screen, and the player can click **Ask again**.

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
- **Memory:** `remember` and `recall` use the trainer's `coach.db` (in the data folder next to `history.csv`), so the agent and the Coach tab share notes. The file is created on the first `remember` or `recall`; the other tools never touch it.

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

The coach recommends routines, reads the player's stats and can generate scenarios. Its rules live in `aim_core::coach` (pure, no I/O). The adapters live in `crates/aim_ai`. This page covers the adapters: providers, keys, privacy and memory.

## Quick facts

| Topic | Rule |
|---|---|
| Default | Off. `CoachConfig.enabled = false` until the player opts in. |
| What is sent | Only the `CoachContext` aggregates and the chat, and, when the player asks on the results screen, the run or mistake digest. No raw traces, file paths or names. |
| Keys | In the OS keyring, or in an environment variable the player names. Never in settings files, never logged. |
| Threading | Every call blocks. The app runs it off the frame (latency rules). |
| Model ids | Plain strings the player sets. The code ships no default model. |

## Providers

Pick one with `provider` in `CoachConfig`. `aim_ai::build_model(&config, key)` validates the settings and returns the provider as a `ChatModel`.

| `provider` | Talks to | Key | Tool calls |
|---|---|---|---|
| `openai-compatible` | OpenAI, OpenCode pay-as-you-go, Ollama, LM Studio | Bearer key; none for local servers | Yes |
| `anthropic` | Anthropic Messages API | `x-api-key` | Yes |
| `cli-delegate` | The player's own installed CLI | The CLI's own login | Codex: through `aimtrainer-mcp`. Others: no (text only) |

### OpenAI-compatible (`OpenAiCompatProvider`)

Calls `POST {base_url}/chat/completions`. The coach tools go out as function tools with `tool_choice: "auto"`. They are sent without `strict`, because local servers differ in what they accept. The core validates every argument anyway.

| Service | `base_url` | Key |
|---|---|---|
| OpenAI | `https://api.openai.com/v1` | OpenAI API key |
| OpenCode (pay-as-you-go Console key) | `https://opencode.ai/inference/openai/v1` | OpenCode Console key |
| Ollama | `http://localhost:11434/v1` | none |
| LM Studio | `http://localhost:1234/v1` | none |

The constants are in `aim_ai::config` (`OPENAI_BASE_URL`, `OPENCODE_BASE_URL`, `OLLAMA_BASE_URL`, `LM_STUDIO_BASE_URL`).

Use the OpenCode Console (pay-as-you-go) key. The OpenCode Go subscription is meant for coding-agent traffic, so do not use it here.

Example settings:

```toml
[coach]
enabled = true
provider = "openai-compatible"
base_url = "http://localhost:11434/v1"
model = "llama3.1"
```

### Anthropic (`AnthropicProvider`)

Calls `POST https://api.anthropic.com/v1/messages` with the headers `x-api-key` and `anthropic-version: 2023-06-01`.

- **Model:** any Messages API model id, for example `claude-sonnet-5-5` or `claude-haiku-5-5`.
- **Tools:** sent with `input_schema` and `strict: true`. Strict mode rejects numeric, length and array-size keywords (`minimum`, `maxLength`, `maxItems`, ...), so those are removed from the schema that is sent. The core still enforces them.
- **`tool_choice`:** always `auto`. Current Opus and Sonnet models return 400 for forced `any` or `tool`.
- **Messages:** `tool_use` and `tool_result` blocks map to and from the core `Message`. Consecutive tool results go back in one user message.
- **Thinking:** replies can contain thinking blocks, which the core `Message` cannot hold. The provider remembers the raw content of its last 32 replies and sends it back unchanged when the same turn reappears, because a tool-use continuation without its thinking is rejected.
- **Refusals:** `stop_reason: "refusal"` becomes an error the app shows.

```toml
[coach]
enabled = true
provider = "anthropic"
model = "claude-sonnet-5-5"
api_key_env = "ANTHROPIC_API_KEY"   # optional fallback when the keyring has no key
```

### CLI delegate (`CliDelegateProvider`)

Runs a CLI the player installed and signed in to themselves. The conversation is flattened into one labelled prompt, and the reply is the CLI's final text.

| CLI | `cli_command` (presets in `aim_ai::cli`) | `cli_prompt` | Output read |
|---|---|---|---|
| Codex | `["codex", "exec", "--skip-git-repo-check", "-"]` | `stdin` | Plain text (the final message) |
| Claude Code | `["claude", "-p", "--output-format", "json"]` | `stdin` | The `result` field; `is_error: true` becomes an error |
| OpenCode | `["opencode", "run", "--format", "json"]` | `argument` | Text events (`{"type":"text",...}`), joined |

- **Tools through MCP (Codex only).** When the command is `codex exec` and `aimtrainer-mcp` is found next to the trainer or on `PATH`, the app adds `-c mcp_servers.aimtrainer.command=<path>` and `-c mcp_servers.aimtrainer.default_tools_approval_mode="approve"`, and the prompt tells Codex to use those tools. Codex then calls them itself: the app's own tool loop is not used, and the "tools used" list stays empty. The approval is needed because `codex exec` cannot ask and otherwise fails the call with "MCP tool call requires approval, but approval policy is never". Without the server, Codex is text only.
- **No tool calls for other CLIs.** Claude Code and OpenCode cannot read stats or create scenarios; they answer from the conversation only.
- **No logins.** The app never manages a CLI's login and never reads any CLI's credential files. If the CLI is not signed in, its error is shown.
- **Sandboxing.** The process starts in an empty scratch directory under the system temp dir, not in the player's files, with the player's own environment.
- **Timeout.** The process is killed after `request_timeout_secs`. On Windows, killing an npm `.cmd` shim may leave its child running until that child finishes.
- **Windows.** A bare program name is resolved on `PATH` as `.exe`, `.cmd` or `.bat`, because npm installs CLIs as `.cmd` shims. Prefer `stdin` there: Windows refuses to pass some arguments (such as multi-line text) to `.cmd` files safely.

```toml
[coach]
enabled = true
provider = "cli-delegate"
cli_command = ["claude", "-p", "--output-format", "json"]
cli_prompt = "stdin"
request_timeout_secs = 180
```

### Errors

| Situation | What the player sees |
|---|---|
| 401 | The key was rejected. No part of the provider's message is kept, since it can echo the key. |
| 403 | Access refused, with the provider's message. |
| 429 | Rate limit reached; wait and retry. |
| 5xx, 529 | The provider is unavailable; try later. |
| Timeout | The model did not answer in time; raise the timeout or pick a faster model. |
| Other 4xx | The provider's message, with the key redacted and cut to 300 characters. |

Requests are not retried automatically: the player sees the error and asks again. Redirects are not followed, so a key is never re-sent to another host.

## Keys

`SecretStore` has two implementations:

- **`KeyringStore`** keeps keys in the OS keyring, under the service `aimtrainer-coach`. That is the Windows Credential Manager, the macOS Keychain, or the Secret Service on Linux (GNOME Keyring or KWallet).
- **`EnvStore`** reads the environment variable named in `api_key_env`. It is read-only.

`lookup_key` checks the keyring first, then the environment variable, and returns where the key came from (`KeySource`) so the settings page can show it.

Keys never go into settings files: `CoachConfig` has no key field, and a settings file with `api_key` fails to load. `ApiKey` prints as `ApiKey(<redacted>)` and has no `Display`. Request bodies never contain the key; it travels only in a header.

## Privacy

- The coach is off until the player enables it and picks a provider.
- Only what the core puts in `CoachContext` is sent: aggregates such as skill scores, flick directions, the miss compass, pace, recent run summaries and missions. Raw traces, file paths, account names and machine names are never sent.
- The Coach tab's *Preview what is sent* shows exactly what is sent; on the results screen, each coach button has its own preview of the digest.
- **Results screen.** Coach's take and Ask why send the run or mistake digest only when the player clicks, once per click, with no automatic retries. Unlike the chat, the digest goes inline in the prompt, so every provider receives it, including the CLI delegates: Claude Code and OpenCode (and Codex) get the run digest through the player's own CLI and account when the player presses the button.
- Local servers (Ollama, LM Studio) keep everything on the machine.
- With the Codex CLI delegate, the stats summary that `list_stats` returns goes to OpenAI through Codex. Tool results from an earlier provider in the same chat are never put into a CLI prompt.
- Plain `http://` is accepted only for `localhost`, `127.0.0.1` and `[::1]`. Any other host must use `https://`.

## Why there is no subscription login

- **ChatGPT plan.** The only sanctioned path for a third-party app is OpenAI's official "Sign in with ChatGPT" for open-source local apps (PKCE with a `127.0.0.1` loopback). It is planned for later (S3), once eligibility is confirmed. Reusing the Codex CLI's `auth.json` is not allowed, so the coach never reads it.
- **claude.ai plan.** Anthropic does not allow third-party apps to sign in with a claude.ai account without its approval. Use an Anthropic API key, or the `cli-delegate` provider with the player's own `claude` CLI.

## Memory (`SqliteMemory`)

The coach's `remember` and `recall` tools use `SqliteMemory`, a `CoachMemory` in one local SQLite file. SQLite is bundled, so nothing needs to be installed.

| Topic | Behavior |
|---|---|
| File | `coach.db` in the trainer's data folder, next to `history.csv` (`~/.local/share/aimtrainer/` on Linux). The trainer and `aimtrainer-mcp` pass that path; this crate hard-codes none. `open_in_memory()` keeps nothing on disk. |
| Search | An FTS5 table over the note text and tags. Each query word also matches as a prefix ("track" finds "tracking"). |
| Order | FTS5 relevance (bm25, which favors short, focused notes) first, then newest first on ties. A query without words returns the newest notes. |
| Wipe | `wipe()` deletes every note, for the settings page's wipe button. `secure_delete` is on, so wiped text is overwritten in the file. |
| Schema | Version 1, stored in SQLite's `user_version`. A file from a newer version is refused, never modified. |
| Errors | `recall` returns no notes on a storage error (the port has no error channel); `try_recall` reports it. |

Query words are reduced to letters and digits before they reach FTS5, so a player's or model's text cannot inject FTS5 query syntax.

Hindsight was evaluated and rejected as the default: it needs a Postgres, pgvector and LLM sidecar. It could become an optional adapter later.
