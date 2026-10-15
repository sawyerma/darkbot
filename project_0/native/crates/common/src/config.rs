//! Typisierte Env-Konfiguration ohne Dependencies (S0).
//! Fehlende Pflichtwerte sind Fehler, keine stillen Defaults.

use std::env;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    Missing(String),
    Invalid { key: String, value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(k) => write!(f, "missing config: {}", k),
            ConfigError::Invalid { key, value } => write!(f, "invalid config {}={:?}", key, value),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Pflichtwert aus der Umgebung.
pub fn require(key: &str) -> Result<String, ConfigError> {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => Ok(v),
        _ => Err(ConfigError::Missing(key.to_string())),
    }
}

/// Pflichtwert, geparst.
pub fn require_parsed<T: FromStr>(key: &str) -> Result<T, ConfigError> {
    let v = require(key)?;
    v.parse::<T>().map_err(|_| ConfigError::Invalid { key: key.to_string(), value: v })
}

/// Optionaler Wert mit **explizitem** Default (Default steht beim Aufrufer, sichtbar).
pub fn parsed_or<T: FromStr>(key: &str, default: T) -> Result<T, ConfigError> {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => {
            v.parse::<T>().map_err(|_| ConfigError::Invalid { key: key.to_string(), value: v })
        }
        _ => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_is_error_not_default() {
        env::remove_var("DARKBOT_TEST_MISSING_X");
        assert_eq!(
            require("DARKBOT_TEST_MISSING_X"),
            Err(ConfigError::Missing("DARKBOT_TEST_MISSING_X".into()))
        );
    }

    #[test]
    fn parsed_or_uses_explicit_default_and_rejects_garbage() {
        env::remove_var("DARKBOT_TEST_CAP");
        assert_eq!(parsed_or::<usize>("DARKBOT_TEST_CAP", 64), Ok(64));
        env::set_var("DARKBOT_TEST_CAP", "abc");
        assert!(matches!(parsed_or::<usize>("DARKBOT_TEST_CAP", 64), Err(ConfigError::Invalid { .. })));
        env::set_var("DARKBOT_TEST_CAP", "128");
        assert_eq!(parsed_or::<usize>("DARKBOT_TEST_CAP", 64), Ok(128));
        env::remove_var("DARKBOT_TEST_CAP");
    }
}
