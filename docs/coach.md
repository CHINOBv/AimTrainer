# AI coach

The coach recommends routines, reads the player's stats and can generate scenarios. Its rules live in `aim_core::coach` (pure, no I/O). The adapters live in `crates/aim_ai`. This page covers the adapters: providers, keys, privacy and memory.

## Quick facts

| Topic | Rule |
|---|---|
| Default | Off. `CoachConfig.enabled = false` until the player opts in. |
| What is sent | Only the `CoachContext` aggregates and the chat. No raw traces, file paths or names. |
| Keys | In the OS keyring, or in an environment variable the player names. Never in settings files, never logged. |
| Threading | Every call blocks. The app runs it off the frame (latency rules). |
| Model ids | Plain strings the player sets. The code ships no default model. |

## Providers

Pick one with `provider` in `CoachConfig`. `aim_ai::build_model(&config, key)` validates the settings and returns the provider as a `ChatModel`.

| `provider` | Talks to | Key | Tool calls |
|---|---|---|---|
| `openai-compatible` | OpenAI, OpenCode pay-as-you-go, Ollama, LM Studio | Bearer key; none for local servers | Yes |
| `anthropic` | Anthropic Messages API | `x-api-key` | Yes |
| `cli-delegate` | The player's own installed CLI | The CLI's own login | No (text only) |

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

- **No tool calls.** In this mode the coach cannot read stats or create scenarios; it answers from the conversation only.
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
- The S5 settings page will show a preview of exactly what is sent.
- Local servers (Ollama, LM Studio) keep everything on the machine.
- Plain `http://` is accepted only for `localhost`, `127.0.0.1` and `[::1]`. Any other host must use `https://`.

## Why there is no subscription login

- **ChatGPT plan.** The only sanctioned path for a third-party app is OpenAI's official "Sign in with ChatGPT" for open-source local apps (PKCE with a `127.0.0.1` loopback). It is planned for later (S3), once eligibility is confirmed. Reusing the Codex CLI's `auth.json` is not allowed, so the coach never reads it.
- **claude.ai plan.** Anthropic does not allow third-party apps to sign in with a claude.ai account without its approval. Use an Anthropic API key, or the `cli-delegate` provider with the player's own `claude` CLI.
