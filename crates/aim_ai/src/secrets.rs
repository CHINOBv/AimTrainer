//! API key storage. Keys live in the OS keyring ([`KeyringStore`]); an environment
//! variable the player names is the visible fallback ([`EnvStore`]). Keys are never
//! written to settings files, and [`ApiKey`] never prints its value.

use std::fmt;

/// Keyring service name for every coach secret.
pub const KEYRING_SERVICE: &str = "aimtrainer-coach";

/// An API key. `Debug` is redacted and there is no `Display`, so a key cannot end up in a
/// log or an error message by accident; [`ApiKey::expose`] is the only way to read it.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// Wrap a key; surrounding whitespace (a pasted newline) is removed.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into().trim().to_owned())
    }

    /// The key itself, for the request header only.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the key is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

/// Where a key was found, for the settings page to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySource {
    /// The OS keyring.
    Keyring,
    /// The named environment variable.
    Env(String),
}

/// Why a secret could not be read or written. Never contains the secret.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    /// The OS keyring is unavailable or refused the operation.
    #[error("the OS keyring failed: {0}")]
    Keyring(String),
    /// The store cannot be written (environment variables are read-only here).
    #[error("this key store is read-only")]
    ReadOnly,
}

/// Somewhere API keys are kept, by account name (e.g. `"anthropic"`).
pub trait SecretStore {
    /// The key for `account`, if one is stored.
    fn get(&self, account: &str) -> Result<Option<ApiKey>, SecretError>;
    /// Store `key` for `account`, replacing any previous one.
    fn set(&self, account: &str, key: &ApiKey) -> Result<(), SecretError>;
    /// Forget the key for `account`; forgetting a missing key succeeds.
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

/// The OS keyring: Windows Credential Manager, macOS Keychain, or the Secret Service on
/// Linux.
#[derive(Debug, Clone)]
pub struct KeyringStore {
    service: String,
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self {
            service: KEYRING_SERVICE.to_owned(),
        }
    }
}

impl KeyringStore {
    /// A store under a custom service name (tests use a unique one).
    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, account).map_err(keyring_error)
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, account: &str) -> Result<Option<ApiKey>, SecretError> {
        match self.entry(account)?.get_password() {
            Ok(key) => Ok(Some(ApiKey::new(key)).filter(|k| !k.is_empty())),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }

    fn set(&self, account: &str, key: &ApiKey) -> Result<(), SecretError> {
        self.entry(account)?
            .set_password(key.expose())
            .map_err(keyring_error)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(e)),
        }
    }
}

/// Keyring errors describe the platform failure; they do not carry the secret.
fn keyring_error(e: keyring::Error) -> SecretError {
    SecretError::Keyring(e.to_string())
}

/// Reads keys from environment variables; the account is the variable name.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnvStore;

impl SecretStore for EnvStore {
    fn get(&self, account: &str) -> Result<Option<ApiKey>, SecretError> {
        Ok(std::env::var(account)
            .ok()
            .map(ApiKey::new)
            .filter(|k| !k.is_empty()))
    }

    fn set(&self, _account: &str, _key: &ApiKey) -> Result<(), SecretError> {
        Err(SecretError::ReadOnly)
    }

    fn delete(&self, _account: &str) -> Result<(), SecretError> {
        Err(SecretError::ReadOnly)
    }
}

/// The key for `account`: the keyring first, then the environment variable `env_var`
/// when one is configured. A keyring failure falls through to the environment and is
/// returned only when no key was found anywhere.
pub fn lookup_key(
    keyring: &dyn SecretStore,
    env: &dyn SecretStore,
    account: &str,
    env_var: Option<&str>,
) -> Result<Option<(ApiKey, KeySource)>, SecretError> {
    let from_keyring = keyring.get(account);
    if let Ok(Some(key)) = &from_keyring {
        return Ok(Some((key.clone(), KeySource::Keyring)));
    }
    if let Some(var) = env_var
        && let Some(key) = env.get(var)?
    {
        return Ok(Some((key, KeySource::Env(var.to_owned()))));
    }
    from_keyring.map(|_| None)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// An in-memory store; `fail` makes every read fail like a locked keyring.
    #[derive(Default)]
    pub struct MemoryStore {
        pub keys: RefCell<HashMap<String, ApiKey>>,
        pub fail: bool,
    }

    impl SecretStore for MemoryStore {
        fn get(&self, account: &str) -> Result<Option<ApiKey>, SecretError> {
            if self.fail {
                return Err(SecretError::Keyring("locked".into()));
            }
            Ok(self.keys.borrow().get(account).cloned())
        }
        fn set(&self, account: &str, key: &ApiKey) -> Result<(), SecretError> {
            self.keys.borrow_mut().insert(account.into(), key.clone());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<(), SecretError> {
            self.keys.borrow_mut().remove(account);
            Ok(())
        }
    }

    #[test]
    fn a_key_never_prints_its_value() {
        let key = ApiKey::new("  sk-very-secret\n");
        assert_eq!(
            key.expose(),
            "sk-very-secret",
            "pasted whitespace is trimmed"
        );
        let shown = format!("{key:?} {:?}", Some(&key));
        assert!(!shown.contains("sk-very-secret"), "{shown}");
        assert!(shown.contains("redacted"));
    }

    #[test]
    fn the_keyring_wins_and_the_environment_is_the_visible_fallback() {
        let keyring = MemoryStore::default();
        let env = MemoryStore::default();
        env.set("COACH_KEY", &ApiKey::new("from-env")).unwrap();

        let found = lookup_key(&keyring, &env, "anthropic", Some("COACH_KEY")).unwrap();
        assert_eq!(
            found,
            Some((ApiKey::new("from-env"), KeySource::Env("COACH_KEY".into())))
        );

        keyring
            .set("anthropic", &ApiKey::new("from-keyring"))
            .unwrap();
        let found = lookup_key(&keyring, &env, "anthropic", Some("COACH_KEY")).unwrap();
        assert_eq!(
            found,
            Some((ApiKey::new("from-keyring"), KeySource::Keyring))
        );

        keyring.delete("anthropic").unwrap();
        assert_eq!(lookup_key(&keyring, &env, "anthropic", None), Ok(None));
    }

    #[test]
    fn a_failing_keyring_falls_back_and_reports_only_when_nothing_is_found() {
        let keyring = MemoryStore {
            fail: true,
            ..MemoryStore::default()
        };
        let env = MemoryStore::default();
        assert_eq!(
            lookup_key(&keyring, &env, "openai", None),
            Err(SecretError::Keyring("locked".into()))
        );
        env.set("OPENAI_KEY", &ApiKey::new("k")).unwrap();
        assert_eq!(
            lookup_key(&keyring, &env, "openai", Some("OPENAI_KEY"))
                .unwrap()
                .map(|(_, source)| source),
            Some(KeySource::Env("OPENAI_KEY".into()))
        );
    }

    #[test]
    fn the_environment_store_is_read_only() {
        assert_eq!(
            EnvStore.set("X", &ApiKey::new("k")),
            Err(SecretError::ReadOnly)
        );
        assert_eq!(EnvStore.delete("X"), Err(SecretError::ReadOnly));
        assert_eq!(
            EnvStore.get("AIMTRAINER_SURELY_UNSET_VARIABLE_1234"),
            Ok(None)
        );
    }

    /// Touches the real OS keyring; run by hand with `--ignored`.
    #[test]
    #[ignore = "uses the real OS keyring"]
    fn the_os_keyring_round_trips() {
        let store = KeyringStore::with_service("aimtrainer-coach-test");
        store.set("probe", &ApiKey::new("value")).unwrap();
        assert_eq!(store.get("probe").unwrap(), Some(ApiKey::new("value")));
        store.delete("probe").unwrap();
        assert_eq!(store.get("probe").unwrap(), None);
    }
}
