//! GC's shipped reference configuration (`infra/services/gc-service/config.env`)
//! for tests that are NOT about the values themselves.
//!
//! This is the single GC home of both the file text and its parse: tests that
//! build a `Config` take the shipped `GC_MH_*` values from here instead of
//! retyping them, so a change in config.env flows into every such test. Tests
//! ABOUT freshness or config validation set their own values.

use gc_service::config::MhRegistryConfig;
pub use gc_service::config::MH_REGISTRY_KEYS;
use std::collections::HashMap;

/// The shipped GC ConfigMap source, compiled in.
pub(crate) const GC_CONFIG_ENV: &str =
    include_str!("../../../infra/services/gc-service/config.env");

/// Strictly parse a KEY=VALUE env file.
///
/// Blank lines and `#` comment lines are skipped. Everything else must be
/// `KEY=VALUE` with a non-empty key; the value is everything after the first
/// `=` and may be empty (e.g. `CORS_ALLOWED_ORIGINS=`). A malformed line or a
/// duplicate key panics naming the line: a lenient parser would turn a broken
/// config.env into a confusing "missing key" or a stale value.
///
/// # Panics
///
/// On a malformed line, an empty key, or a duplicate key.
#[must_use]
pub fn parse_env_file(text: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line_no = index + 1;
        let Some((key, value)) = line.split_once('=') else {
            panic!("env file line {line_no} is not KEY=VALUE: {raw:?}");
        };
        let key = key.trim();
        assert!(
            !key.is_empty(),
            "env file line {line_no} has an empty key: {raw:?}"
        );
        assert!(
            vars.insert(key.to_string(), value.to_string()).is_none(),
            "env file line {line_no} duplicates key {key}"
        );
    }
    vars
}

/// The shipped values of the four `GC_MH_*` keys.
///
/// # Panics
///
/// If config.env lacks any of the four keys (never returns a partial map), or
/// the file is malformed.
#[must_use]
pub fn reference_mh_registry_vars() -> HashMap<String, String> {
    let all = parse_env_file(GC_CONFIG_ENV);
    MH_REGISTRY_KEYS
        .iter()
        .map(|key| {
            let value = all.get(*key).unwrap_or_else(|| {
                panic!("infra/services/gc-service/config.env is missing REQUIRED key {key}")
            });
            ((*key).to_string(), value.clone())
        })
        .collect()
}

/// The shipped MH registry config, parsed and validated by GC's own
/// `MhRegistryConfig::from_vars` (no second parse path).
///
/// # Panics
///
/// If the shipped values do not pass GC's validation.
#[must_use]
pub fn reference_mh_registry() -> MhRegistryConfig {
    MhRegistryConfig::from_vars(&reference_mh_registry_vars())
        .unwrap_or_else(|e| panic!("shipped MH registry config is invalid: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comments_blanks_empty_values_and_embedded_equals() {
        let vars = parse_env_file("# c\n\n  # indented\nA=1\nEMPTY=\nURL=http://x/?a=b\n");
        assert_eq!(vars.len(), 3);
        assert_eq!(vars["A"], "1");
        assert_eq!(vars["EMPTY"], "");
        assert_eq!(vars["URL"], "http://x/?a=b");
    }

    #[test]
    #[should_panic(expected = "line 2 is not KEY=VALUE")]
    fn malformed_line_panics_naming_line() {
        let _ = parse_env_file("A=1\nnot a pair\n");
    }

    #[test]
    #[should_panic(expected = "empty key")]
    fn empty_key_panics() {
        let _ = parse_env_file("=1\n");
    }

    #[test]
    #[should_panic(expected = "duplicates key A")]
    fn duplicate_key_panics() {
        let _ = parse_env_file("A=1\nA=2\n");
    }

    #[test]
    fn shipped_config_env_carries_every_mh_key_and_validates() {
        let vars = reference_mh_registry_vars();
        assert_eq!(vars.len(), MH_REGISTRY_KEYS.len());
        let reg = reference_mh_registry();
        assert!(reg.retention() > reg.staleness_bound());
    }
}
