//! The AI coach's app adapter: background requests, the context sent to the model, API
//! keys, the memory file and generated scenarios. Every call here that touches the network,
//! the OS keyring, SQLite or replay files blocks, so the coach page runs it in a [`Job`].

pub mod context;
pub mod request;

pub use request::{Job, Phase};

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use aim_ai::{
    ApiKey, CoachConfig, EnvStore, KeySource, KeyringStore, ProviderKind, SecretStore,
    SqliteMemory, lookup_key,
};
use aim_core::coach::{CoachContext, Message, ToolEnv, Turn, drills, run_turn};
use aim_core::scenario::ScenarioDef;
use aim_core::training::benchmark::BenchmarkDef;
use aim_core::training::routine::RoutineDef;

/// Model calls allowed per player message before the turn stops.
pub const MAX_STEPS: usize = 8;

/// The prompt "Test connection" sends.
pub const TEST_PROMPT: &str = "Reply with the single word OK.";

/// The keyring account of the key `config` uses: one per provider, and one per endpoint
/// for OpenAI-compatible servers (an OpenAI key and an OpenCode key are different keys).
pub fn key_account(config: &CoachConfig) -> String {
    match config.provider {
        ProviderKind::Anthropic => "anthropic".to_owned(),
        ProviderKind::OpenaiCompatible => format!(
            "openai-compatible {}",
            config.effective_base_url().unwrap_or_default()
        ),
        ProviderKind::CliDelegate => "cli-delegate".to_owned(),
    }
}

/// Where the key for the current provider comes from, as the coach page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyStatus {
    /// The provider takes no key (the CLI delegate signs in on its own).
    NotNeeded,
    /// Stored in the OS keyring.
    Keyring,
    /// Read from the named environment variable.
    Env(String),
    /// No key anywhere.
    Missing,
    /// The keyring failed and no environment variable had a key.
    Unavailable(String),
}

/// Look up where `config`'s key is. Blocking (the OS keyring): call it off the frame.
pub fn key_status(config: &CoachConfig) -> KeyStatus {
    if config.provider == ProviderKind::CliDelegate {
        return KeyStatus::NotNeeded;
    }
    match lookup_key(
        &KeyringStore::default(),
        &EnvStore,
        &key_account(config),
        config.api_key_env.as_deref(),
    ) {
        Ok(Some((_, KeySource::Keyring))) => KeyStatus::Keyring,
        Ok(Some((_, KeySource::Env(var)))) => KeyStatus::Env(var),
        Ok(None) => KeyStatus::Missing,
        Err(e) => KeyStatus::Unavailable(e.to_string()),
    }
}

/// Store `key` for `config`'s provider in the OS keyring, then report the new status.
/// An empty key is refused. Blocking.
pub fn store_key(config: &CoachConfig, key: ApiKey) -> Result<KeyStatus, String> {
    if key.is_empty() {
        return Err("the key is empty".to_owned());
    }
    KeyringStore::default()
        .set(&key_account(config), &key)
        .map_err(|e| e.to_string())?;
    Ok(key_status(config))
}

/// Remove `config`'s key from the OS keyring, then report the new status. Blocking.
pub fn forget_key(config: &CoachConfig) -> Result<KeyStatus, String> {
    KeyringStore::default()
        .delete(&key_account(config))
        .map_err(|e| e.to_string())?;
    Ok(key_status(config))
}

/// The provider `config` describes, with its key looked up. Blocking.
fn model(config: &CoachConfig) -> Result<Box<dyn aim_core::coach::ChatModel + Send>, String> {
    // Off or invalid settings fail before the keyring is touched.
    if !config.enabled {
        return Err("the coach is off; enable it on the Coach page".to_owned());
    }
    config.validate().map_err(|errors| errors.join("; "))?;
    let key = if config.provider == ProviderKind::CliDelegate {
        None
    } else {
        lookup_key(
            &KeyringStore::default(),
            &EnvStore,
            &key_account(config),
            config.api_key_env.as_deref(),
        )
        .map_err(|e| e.to_string())?
        .map(|(key, _)| key)
    };
    aim_ai::build_model(config, key).map_err(|errors| errors.join("; "))
}

/// Send [`TEST_PROMPT`] without tools and return the reply. Blocking.
pub fn test_connection(config: &CoachConfig) -> Result<String, String> {
    let reply = model(config)?
        .complete(
            &[Message::User {
                content: TEST_PROMPT.to_owned(),
            }],
            &[],
        )
        .map_err(|e| e.to_string())?;
    Ok(reply.text)
}

/// Everything one coach turn needs, owned so it can move to a worker thread.
#[derive(Debug, Clone)]
pub struct TurnRequest {
    /// The provider settings.
    pub config: CoachConfig,
    /// The coach memory file.
    pub memory_file: PathBuf,
    /// The player summary the tools read.
    pub context: CoachContext,
    /// Every playable scenario.
    pub scenarios: Vec<ScenarioDef>,
    /// Every playable routine.
    pub routines: Vec<RoutineDef>,
    /// The benchmark recommendations use.
    pub benchmark: Option<BenchmarkDef>,
    /// The conversation so far.
    pub history: Vec<Message>,
    /// What the player wrote.
    pub message: String,
    /// Unix seconds now.
    pub now: u64,
}

/// Run one coach turn: look up the key, build the provider, open the memory and run the
/// core's tool loop. Blocking: call it off the frame.
pub fn run_request(request: TurnRequest) -> Result<Turn, String> {
    let mut model = model(&request.config)?;
    let mut memory = open_memory(&request.memory_file)?;
    let env = ToolEnv {
        context: &request.context,
        scenarios: &request.scenarios,
        routines: &request.routines,
        benchmark: request.benchmark.as_ref(),
        now: request.now,
    };
    run_turn(
        &mut *model,
        &mut memory,
        &env,
        &request.history,
        &request.message,
        MAX_STEPS,
    )
    .map_err(|e| e.to_string())
}

fn open_memory(path: &Path) -> Result<SqliteMemory, String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("coach memory: {e}"))?;
    }
    SqliteMemory::open(path).map_err(|e| e.to_string())
}

/// Delete every note in the coach memory file. A missing file has nothing to delete.
/// Blocking.
pub fn wipe_memory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    SqliteMemory::open(path)
        .and_then(|mut m| m.wipe())
        .map_err(|e| e.to_string())
}

/// Check a generated scenario again: the definition rules and the playability gate (a
/// simulated run). Returns the simulated contender's score. Blocking.
pub fn check_scenario(def: &ScenarioDef) -> Result<f32, Vec<String>> {
    let errors: Vec<String> = def.validate().iter().map(|e| e.to_string()).collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    drills::check_playable(def).map_err(|e| vec![e])
}

/// The header of a saved generated scenario.
pub const GENERATED_HEADER: &str = "# Generated by the AI coach. Uncalibrated: its numbers are \
     estimates, not tuned from player data.\n";

/// Save `def` as `<dir>/<id>.toml` with the [`GENERATED_HEADER`]. Never replaces an
/// existing file. Blocking.
pub fn save_generated_scenario(dir: &Path, def: &ScenarioDef) -> Result<PathBuf, String> {
    // The core validated the id; checking again keeps the path inside the folder.
    let id_ok = !def.id.is_empty()
        && def
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !id_ok {
        return Err(format!("id {:?} cannot be used as a file name", def.id));
    }
    let body = toml::to_string(def).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.toml", def.id));
    fs::create_dir_all(dir)
        .and_then(|()| OpenOptions::new().write(true).create_new(true).open(&path))
        .and_then(|mut f| f.write_all(format!("{GENERATED_HEADER}{body}").as_bytes()))
        .map_err(|e| format!("could not save {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::coach::SubSkill;
    use aim_core::coach::drills::generate;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("aimtrainer-coach-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn drill() -> ScenarioDef {
        generate(SubSkill::MicroAdjustment, 0.0, 1).expect("a valid drill")
    }

    #[test]
    fn a_generated_scenario_is_saved_once_labelled_uncalibrated() {
        let scratch = Scratch::new("save");
        let dir = scratch.0.join("scenarios");
        let def = drill();
        let path = save_generated_scenario(&dir, &def).expect("saved");
        assert_eq!(path, dir.join(format!("{}.toml", def.id)));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with(GENERATED_HEADER), "{text}");
        assert!(text.to_lowercase().contains("uncalibrated"));
        assert_eq!(ScenarioDef::from_toml(&text).unwrap(), def, "it loads back");

        let again = save_generated_scenario(&dir, &def);
        assert!(again.is_err(), "an existing file is never replaced");
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn an_id_that_is_not_a_plain_file_name_is_refused() {
        let scratch = Scratch::new("bad-id");
        let def = ScenarioDef {
            id: "../escape".into(),
            ..drill()
        };
        assert!(save_generated_scenario(&scratch.0, &def).is_err());
        assert!(!scratch.0.parent().unwrap().join("escape.toml").exists());
    }

    #[test]
    fn keys_are_kept_per_provider_and_per_openai_compatible_endpoint() {
        let openai = CoachConfig {
            base_url: Some(aim_ai::config::OPENAI_BASE_URL.into()),
            ..CoachConfig::default()
        };
        let opencode = CoachConfig {
            base_url: Some(format!("{}/", aim_ai::config::OPENCODE_BASE_URL)),
            ..CoachConfig::default()
        };
        assert_eq!(
            key_account(&openai),
            "openai-compatible https://api.openai.com/v1"
        );
        assert_ne!(key_account(&openai), key_account(&opencode));
        let anthropic = CoachConfig {
            provider: ProviderKind::Anthropic,
            base_url: Some("https://example.test".into()),
            ..CoachConfig::default()
        };
        assert_eq!(key_account(&anthropic), "anthropic");
        let cli = CoachConfig {
            provider: ProviderKind::CliDelegate,
            ..CoachConfig::default()
        };
        assert_eq!(key_status(&cli), KeyStatus::NotNeeded, "no keyring access");
    }

    #[test]
    fn wiping_a_missing_memory_file_succeeds_and_a_kept_note_is_wiped() {
        use aim_core::coach::{CoachMemory, MemoryNote};
        let scratch = Scratch::new("wipe");
        let file = scratch.0.join("coach.db");
        assert_eq!(wipe_memory(&file), Ok(()));
        assert!(!file.exists(), "nothing is created");

        let mut memory = open_memory(&file).unwrap();
        memory
            .retain(MemoryNote {
                text: "Wants Immortal".into(),
                tags: Vec::new(),
                timestamp: 1,
            })
            .unwrap();
        drop(memory);
        assert_eq!(wipe_memory(&file), Ok(()));
        assert_eq!(SqliteMemory::open(&file).unwrap().len(), Ok(0));
    }

    #[test]
    fn a_disabled_coach_sends_nothing() {
        let off = CoachConfig::default();
        let err = test_connection(&off).unwrap_err();
        assert!(err.contains("off"), "{err}");
    }
}
