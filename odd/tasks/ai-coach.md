# AI coach
Objective: a model-agnostic AI coach that recommends routines, analyses the player's scenarios and stats, remembers the player, and can generate validated scenarios, exposed to external agents through MCP.
Branch: `feat/ai-coach` (worktree `../AimTrainer-worktrees/ai-coach`) · Delivery: local merge to `main`; push at the end of the batch (authorized) · Test runner: `cargo test --workspace`

## Specs

- **S1 Core coach (pure).** `aim_core::coach`, with no I/O:
  - `CoachContext`: a compact, serializable summary of the skill profile, flick directions, miss compass, pace, potential score, benchmark energy, recent runs and missions.
  - A tool registry: `list_stats`, `recommend_routine`, `validate_scenario`, `create_scenario` (parse, validate and return errors so the model can retry), `remember` and `recall`.
  - Ports: `ChatModel` (messages + tool specs in, text + tool calls out) and `CoachMemory` (retain, recall).
  - A tool loop tested against a fake model.
  - The coaching knowledge base (sub-skill diagnosis → method → drill parameters) comes from the research and is marked as estimates.
- **S2 Providers** (a new adapter crate `aim_ai`):
  - an OpenAI-compatible Chat Completions client (OpenAI API key, OpenCode pay-as-you-go key, Ollama, LM Studio);
  - an Anthropic Messages client (API key, strict tool schemas, `tool_choice` auto);
  - a CLI-delegate provider (the user's own installed `codex exec`, `claude -p --output-format json`, `opencode run --format json`).
  - Keys live in the OS keyring with a visible fallback.
  - Each player configures the provider, model and base URL.
- **S3 ChatGPT plan provider** (later, gated): only through OpenAI's official "Sign in with ChatGPT" for open-source local apps (PKCE, 127.0.0.1 loopback), once eligibility is confirmed. Never reuse the Codex CLI's `auth.json`.
- **S4 Memory.** A local SQLite store (FTS5 keyword search plus recency) behind `CoachMemory`, with a wipe button. Hindsight was evaluated and rejected as the default (it needs a Postgres + pgvector + LLM sidecar); it could be an optional adapter later.
- **S5 App coach page.**
  - Off by default and opt-in per provider, with a "preview what is sent" panel (aggregates only: no raw traces, paths or names).
  - Requests run off the frame (latency rules).
  - Generated scenarios are validated, simulated (playability gate), shown for approval, and saved to the custom content folder, labelled "uncalibrated".
- **S6 MCP server.** `aimtrainer-mcp`, a stdio binary built with rmcp, exposing the same tool registry plus read-only resources (built-in scenarios, the schema docs) for Claude Code, Codex and OpenCode.

Rules:
- `aim_core` stays pure; network, keyring, SQLite and MCP live in adapter crates.
- New dependencies (reqwest with rustls, tokio, keyring, rusqlite, rmcp) are delivery changes: risk items 2 and 5. They get the verifier and a review.
- The coach never touches the game, and never invents VALORANT constants (it cites `docs/valorant-mechanics.md`).

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | plan | inline | done | this commit |
| T2 | S1 | delegated (worktree `coach-core`) | in progress | |
| T3 | S2, S4 | delegated, after T2 | pending | |
| T4 | S6 | delegated, after T2 (parallel with T3) | pending | |
| T5 | S5 | delegated, after T3 | pending | |
| T6 | S3 | later, gated on OpenAI eligibility | pending | |
| T7 | seam: merge, verifier, review, merge to `main` | inline | pending | |

## Log

- L1 (2026-10-18) User (verbatim excerpt): "evalúa cómo podemos implementar el AI. Pero que sea custom, o sea, que acepte diferentes modelos. Ahorita principalmente pues OpenAI mediante suscripción. Mm, Claude, pues con API. La suscripción de OpenCode [...] Y que esto sea configurable por cada [...] persona [...] la idea es crear como un tipo coach de que vaya recomendando rutinas según los puntos que tenemos, haga análisis de todos los [...] escenarios y estadísticas [...] https://hindsight.vectorize.io/ Igual evalúalo [...] Chance de exponer un MCP o algo así, que el coach pueda ir generando los escenarios necesarios"
- L2 (2026-10-18) Research verdicts:
  - OpenAI subscription only via the official Sign-in-with-ChatGPT preview for open-source local apps.
  - Anthropic API key only; claude.ai login in third-party apps is not allowed without approval.
  - OpenCode: the pay-as-you-go Console key is fine; the Go subscription is for coding-agent traffic (avoid).
  - Hindsight is MIT but heavy, so a local SQLite store is the default.
  - rmcp 3.x is the official Rust MCP SDK.
