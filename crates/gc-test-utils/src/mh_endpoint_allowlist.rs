//! The ONE home of the RegisterMH endpoint allowlist values GC's tests run
//! with (ADR-0039 §7, arm 0).
//!
//! The shipped base (`infra/services/gc-service/config.env`) carries the four
//! topology keys EMPTY (every deploy path renders them; GC refuses to start on
//! an empty one), so there are no shipped values for tests to reuse. These
//! are the test values, used by the server harness and every integration test
//! that drives `MhService::register_mh`. `GC_MH_GRPC_ALLOWED_SCHEMES` IS a
//! shipped policy value, so it is read from config.env, not retyped.
//!
//! Zones:
//! - client-facing: host `127.0.0.1` (exact) or any name under `.mh.test`,
//!   ports [`TEST_WT_PORTS`];
//! - internal: the shipped scheme, hosts in [`TEST_GRPC_CIDR`], port
//!   [`TEST_GRPC_PORT`].

use crate::reference_config::{parse_env_file, GC_CONFIG_ENV};
use gc_service::config::{mh_endpoint_allowlist_from_vars, MH_ENDPOINT_ALLOWLIST_KEYS};
use gc_service::mh_endpoint_policy::MhEndpointAllowlist;
use std::collections::HashMap;

/// Exact client-facing host entry.
pub const TEST_WT_EXACT_HOST: &str = "127.0.0.1";
/// Client-facing `.suffix` entry.
pub const TEST_WT_SUFFIX: &str = ".mh.test";
/// Client-facing ports.
pub const TEST_WT_PORTS: [u16; 2] = [4440, 4441];
/// Internal CIDR.
pub const TEST_GRPC_CIDR: &str = "10.244.0.0/16";
/// Internal port.
pub const TEST_GRPC_PORT: u16 = 50053;

/// The shipped `GC_MH_GRPC_ALLOWED_SCHEMES` (config.env).
///
/// # Panics
///
/// If config.env lacks the key or carries it empty.
#[must_use]
pub fn shipped_grpc_schemes() -> String {
    let all = parse_env_file(GC_CONFIG_ENV);
    let v = all
        .get("GC_MH_GRPC_ALLOWED_SCHEMES")
        .unwrap_or_else(|| {
            panic!("infra/services/gc-service/config.env is missing GC_MH_GRPC_ALLOWED_SCHEMES")
        })
        .clone();
    assert!(
        !v.trim().is_empty(),
        "config.env GC_MH_GRPC_ALLOWED_SCHEMES is empty"
    );
    v
}

/// The first shipped gRPC scheme (what a test endpoint uses).
#[must_use]
pub fn test_grpc_scheme() -> String {
    shipped_grpc_schemes()
        .split(',')
        .next()
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// The five allowlist keys with the test values.
#[must_use]
pub fn test_mh_endpoint_allowlist_vars() -> HashMap<String, String> {
    let ports = |p: &[u16]| p.iter().map(u16::to_string).collect::<Vec<_>>().join(",");
    let vars = HashMap::from([
        (
            "GC_MH_WT_ALLOWED_HOSTS".to_string(),
            format!("{TEST_WT_EXACT_HOST},{TEST_WT_SUFFIX}"),
        ),
        ("GC_MH_WT_ALLOWED_PORTS".to_string(), ports(&TEST_WT_PORTS)),
        (
            "GC_MH_GRPC_ALLOWED_SCHEMES".to_string(),
            shipped_grpc_schemes(),
        ),
        (
            "GC_MH_GRPC_ALLOWED_CIDRS".to_string(),
            TEST_GRPC_CIDR.to_string(),
        ),
        (
            "GC_MH_GRPC_ALLOWED_PORTS".to_string(),
            TEST_GRPC_PORT.to_string(),
        ),
    ]);
    debug_assert!(MH_ENDPOINT_ALLOWLIST_KEYS
        .iter()
        .all(|k| vars.contains_key(*k)));
    vars
}

/// Fill every REQUIRED GC config key a test has NOT set itself: the shipped
/// config.env value for each key, except the four topology allowlist keys
/// (empty in config.env, rendered per deploy), which take the test allowlist.
/// The one call for building a `Config::from_vars` map in tests: a test sets
/// what it is ABOUT, then calls this, so its own values are never
/// overwritten and a new required key flows in from config.env.
pub fn fill_required_config(vars: &mut HashMap<String, String>) {
    let mut shipped = crate::reference_config::reference_config_vars();
    shipped.extend(test_mh_endpoint_allowlist_vars());
    for (key, value) in shipped {
        vars.entry(key).or_insert(value);
    }
}

/// The test allowlist, parsed by GC's own parser (no second parse path).
///
/// # Panics
///
/// If the test values do not pass GC's validation.
#[must_use]
pub fn test_mh_endpoint_allowlist() -> MhEndpointAllowlist {
    mh_endpoint_allowlist_from_vars(&test_mh_endpoint_allowlist_vars())
        .unwrap_or_else(|e| panic!("test MH endpoint allowlist is invalid: {e}"))
}

/// An allowlisted client-facing endpoint: `https://<label>.mh.test:<port>`.
/// `label` must be a lowercase DNS label.
#[must_use]
pub fn test_wt_endpoint(label: &str) -> String {
    format!("https://{label}{TEST_WT_SUFFIX}:{}", TEST_WT_PORTS[0])
}

/// An allowlisted internal endpoint: `<scheme>://10.244.<a>.<b>:<port>`.
#[must_use]
pub fn test_grpc_endpoint(a: u8, b: u8) -> String {
    format!("{}://10.244.{a}.{b}:{TEST_GRPC_PORT}", test_grpc_scheme())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_values_validate_and_fixture_endpoints_pass() {
        let allowlist = test_mh_endpoint_allowlist();
        assert_eq!(
            allowlist.check(&test_wt_endpoint("mh-1"), &test_grpc_endpoint(1, 2)),
            Ok(())
        );
    }

    /// The shipped base is fail-closed: its four topology keys are EMPTY, so
    /// GC refuses to start until a deploy render supplies them.
    #[test]
    fn shipped_base_topology_keys_are_empty_and_rejected() {
        let all = parse_env_file(GC_CONFIG_ENV);
        for key in MH_ENDPOINT_ALLOWLIST_KEYS {
            let v = all
                .get(key)
                .unwrap_or_else(|| panic!("config.env is missing REQUIRED key {key}"));
            if key == "GC_MH_GRPC_ALLOWED_SCHEMES" {
                assert!(!v.is_empty(), "{key} is a shipped policy value");
            } else {
                assert!(
                    v.is_empty(),
                    "{key} must be EMPTY in the base (rendered per environment)"
                );
            }
        }
        let shipped: HashMap<String, String> = MH_ENDPOINT_ALLOWLIST_KEYS
            .iter()
            .map(|k| ((*k).to_string(), all[*k].clone()))
            .collect();
        assert!(mh_endpoint_allowlist_from_vars(&shipped).is_err());
    }
}
