//! Dev OTel collector fixtures: one selector, one log fetch, one leak scan.
//!
//! The collector's `debug` exporter at `verbosity: normal` prints every span's
//! name, trace_id/span_id and attributes, plus every resource's attributes, to
//! its pod log (see `exporters.debug` in
//! `infra/services/otel-collector/collector.yaml`). That makes the log both the
//! place cross-service trace continuity is observable and a secondary sink for
//! anything a service puts on a span — hence the shared leak scan.

use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;

use crate::fixtures::gc_client::jwt_pattern;
use crate::NAMESPACE;

/// Selector for the dev OTel collector pod (the label `deploy_otel_collector()`
/// in `infra/kind/scripts/deploy.sh` applies).
pub const OTEL_COLLECTOR_SELECTOR: &str = "app=otel-collector";

/// The collector pod log for the last `since` (a `kubectl --since` duration,
/// e.g. `"5m"`).
///
/// `--tail=-1` is explicit and load-bearing: with a label selector `kubectl
/// logs` defaults to `--tail=10` per pod, and `--since` does not lift that, so
/// without it a scan would see only the last ten lines. `--since` is the bound.
///
/// # Panics
///
/// When `kubectl` cannot run or `kubectl logs` fails — env-tests require
/// kubectl, and a missing log must fail loudly rather than scan as clean.
#[must_use]
pub fn collector_log(since: &str) -> String {
    let output = Command::new("kubectl")
        .args([
            "logs",
            "-n",
            NAMESPACE,
            "-l",
            OTEL_COLLECTOR_SELECTOR,
            "--tail=-1",
            &format!("--since={since}"),
        ])
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "kubectl not available - env-tests require kubectl to read the collector log: {e}"
            )
        });
    assert!(
        output.status.success(),
        "kubectl logs for the OTel collector failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Secret-material keys carrying a value: any key CONTAINING `client_secret`,
/// `password`, `secret` or `api_key` (prefix- and suffix-permissive, so
/// `client_secret_hash`, `password_hash`, `join_token_secret`, `db_password`
/// count — leaked secret-derived material fails), optionally quoted, then `=`
/// or `:`, then a non-empty value.
///
/// The required separator + value is what keeps bare mentions out: AC's
/// `#[instrument(skip_all)]` span NAMES `hash_client_secret` /
/// `verify_client_secret` appear in the collector log as `<name> <trace_id>
/// <span_id> …` and in JSON logs as values, never as `name=value` keys.
#[must_use]
pub fn secret_key_value_pattern() -> &'static Regex {
    &SECRET_KEY_VALUE
}

/// `token` / `authorization` as EXACT whole keys (no word character before or
/// after), optionally quoted, then `=` or `:`, then a non-empty value. Exact,
/// so `token_type=Bearer`, `join_token` and `access_token_ttl` stay out.
#[must_use]
pub fn token_key_value_pattern() -> &'static Regex {
    &TOKEN_KEY_VALUE
}

#[expect(
    clippy::disallowed_methods,
    reason = "test fixture LazyLock<Regex>; static pattern compiles or load-time panic. Per ADR-0034 §6 + ADR-0002 §expect-over-allow — same canonical-home discipline as crates/dt-guard/."
)]
static SECRET_KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)[A-Za-z0-9_]*(?:client_secret|password|secret|api_key)[A-Za-z0-9_]*["']?\s*[:=]\s*["']?[^\s"',}]+"#,
    )
    .unwrap()
});

#[expect(
    clippy::disallowed_methods,
    reason = "test fixture LazyLock<Regex>; static pattern compiles or load-time panic. Per ADR-0034 §6 + ADR-0002 §expect-over-allow — same canonical-home discipline as crates/dt-guard/."
)]
static TOKEN_KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:^|[^A-Za-z0-9_])(?:token|authorization)["']?\s*[:=]\s*["']?[^\s"',}]+"#)
        .unwrap()
});

/// The ONE secret-field + secret-literal scan, shared by the AC service-log
/// scan and the collector-log scan so the two cannot drift.
///
/// `known_secrets` are secret VALUES this run used (the dev client secret, plus
/// any rotated secret); none may appear anywhere in `log`, under any key. An
/// empty list fails: the literal check is the strongest control.
///
/// # Panics
///
/// On an empty `known_secrets`, a secret literal in `log`, or any secret-field
/// match.
pub fn assert_log_has_no_secret_fields(source: &str, log: &str, known_secrets: &[&str]) {
    assert!(
        !known_secrets.is_empty() && known_secrets.iter().all(|s| !s.is_empty()),
        "positive control: the {source} scan needs the secret values this run used"
    );
    for secret in known_secrets {
        assert!(
            !log.contains(secret),
            "{source} log contains a secret value used by this run"
        );
    }
    for (kind, pattern) in [
        ("secret-material", secret_key_value_pattern()),
        ("token/authorization", token_key_value_pattern()),
    ] {
        let hits: Vec<&str> = pattern.find_iter(log).map(|m| m.as_str()).collect();
        let keys: Vec<&str> = hits
            .iter()
            .map(|h| h.split(['=', ':']).next().unwrap_or_default().trim())
            .collect();
        assert!(
            hits.is_empty(),
            "{source} log carries {} {kind} field(s) with a value; keys: {keys:?}",
            hits.len()
        );
    }
}

/// Leak-scan a collector log captured AFTER authenticated traffic.
///
/// `known_secrets` are secret VALUES used by this run (e.g. the dev client
/// secret a caller authenticated with); none may appear anywhere in the log.
///
/// Positive controls, so the scan cannot pass vacuously:
/// - `trace_id` (a fresh id the caller sent as `traceparent` on an
///   authenticated request) must be in `log`, proving that request's spans are
///   inside the scanned window;
/// - the JWT-shape regex must match `real_token` (an AC-minted token from this
///   run), proving the detector recognises the tokens we actually issue.
///
/// # Panics
///
/// On a failed positive control or any leak pattern found.
pub fn assert_collector_log_has_no_secrets(
    log: &str,
    trace_id: &str,
    real_token: &str,
    known_secrets: &[&str],
) {
    assert!(
        log.contains(trace_id),
        "positive control: trace {trace_id} (sent on an authenticated request) is not in the \
         scanned collector log, so the leak scan below would not cover token-bearing traffic"
    );
    assert!(
        jwt_pattern().is_match(real_token),
        "positive control: the JWT-shape regex does not match a real AC-minted token, so it \
         cannot detect our tokens in the collector log"
    );

    assert_log_has_no_secret_fields("OTel collector", log, known_secrets);
    let bearer_count = log.matches("Bearer ey").count();
    assert!(
        bearer_count == 0,
        "OTel collector log should not contain raw Bearer tokens (found {bearer_count} instances)"
    );
    let jwt_count = jwt_pattern().find_iter(log).count();
    assert!(
        jwt_count == 0,
        "OTel collector log should not contain raw JWTs (found {jwt_count} JWT-shaped values) - \
         a span attribute is carrying a token"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::auth_client::DEV_TEST_CLIENT_SECRET;

    fn flagged(line: &str) -> bool {
        secret_key_value_pattern().is_match(line) || token_key_value_pattern().is_match(line)
    }

    /// Lines that must NOT trip the scan: the real AC span names
    /// (`ac-service/src/crypto/mod.rs`) as the debug exporter and the JSON
    /// logs render them, `token_type`, bare/valueless keys.
    #[test]
    fn benign_lines_do_not_match() {
        for line in [
            "hash_client_secret 4bf92f3577b34da6a3ce929d0e0e4736 00f067aa0ba902b7",
            "verify_client_secret 4bf92f3577b34da6a3ce929d0e0e4736 00f067aa0ba902b7 code.file.path=crates/ac-service/src/crypto/mod.rs",
            "    -> Name: hash_client_secret",
            r#"{"span":{"name":"verify_client_secret"},"target":"ac_service::crypto"}"#,
            "token_type=Bearer",
            r#""token_type":"Bearer""#,
            "MIN_PASSWORD_LENGTH",
            "client_secret=",
            "password: ",
        ] {
            assert!(!flagged(line), "benign line must not match: {line}");
        }
    }

    /// Value-bearing secret fields, in collector `key=value` and JSON
    /// `"key":"value"` shapes, including the real dev secret.
    #[test]
    fn secret_fields_with_values_match() {
        let real = format!("client_secret={DEV_TEST_CLIENT_SECRET}");
        let real_json = format!(r#""client_secret":"{DEV_TEST_CLIENT_SECRET}""#);
        for line in [
            real.as_str(),
            real_json.as_str(),
            "client_secret=abc",
            r#""client_secret": "abc""#,
            "client_secret_hash=x",
            "password_hash=$2b$12$abc",
            "user_password=x",
            r#""password":"x""#,
            "join_token_secret=abc",
            "api_key=k",
            "token=abc",
            r#""token":"x""#,
            "authorization=Bearer x",
        ] {
            assert!(flagged(line), "value-bearing field must match: {line}");
        }
    }

    const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
    /// A real-shaped JWT (header.payload.signature) for the regex control.
    const REAL_SHAPED_JWT: &str =
        "eyJ0eXAiOiJKV1QiLCJhbGciOiJFZERTQSJ9.eyJzdWIiOiJ0ZXN0In0.c2lnbmF0dXJl";

    fn clean_log() -> String {
        format!(
            "verify_client_secret {TRACE_ID} 00f067aa0ba902b7 code.file.path=crates/ac-service/src/crypto/mod.rs\n\
             request {TRACE_ID} 1111111111111111 method=GET endpoint=/api/v1/me\n"
        )
    }

    #[test]
    fn clean_log_with_trace_and_span_names_passes() {
        assert_collector_log_has_no_secrets(
            &clean_log(),
            TRACE_ID,
            REAL_SHAPED_JWT,
            &[DEV_TEST_CLIENT_SECRET],
        );
    }

    #[test]
    #[should_panic(expected = "positive control: trace")]
    fn missing_trace_id_fails() {
        assert_collector_log_has_no_secrets(
            "verify_client_secret 0000 1111\n",
            TRACE_ID,
            REAL_SHAPED_JWT,
            &[DEV_TEST_CLIENT_SECRET],
        );
    }

    #[test]
    #[should_panic(expected = "contains a secret value used by this run")]
    fn known_secret_value_in_log_fails() {
        let log = format!("{}note {DEV_TEST_CLIENT_SECRET}\n", clean_log());
        assert_collector_log_has_no_secrets(
            &log,
            TRACE_ID,
            REAL_SHAPED_JWT,
            &[DEV_TEST_CLIENT_SECRET],
        );
    }

    #[test]
    #[should_panic(expected = "needs the secret values this run used")]
    fn empty_known_secrets_fails() {
        assert_collector_log_has_no_secrets(&clean_log(), TRACE_ID, REAL_SHAPED_JWT, &[]);
    }

    #[test]
    #[should_panic(expected = "needs the secret values this run used")]
    fn secret_field_scan_rejects_empty_secret_list() {
        assert_log_has_no_secret_fields("test", "clean line\n", &[]);
    }

    /// The literal layer stands on its own: a secret value under a key neither
    /// regex knows (`x=`) still fails the scan.
    #[test]
    #[should_panic(expected = "contains a secret value used by this run")]
    fn secret_literal_under_unknown_key_fails() {
        let log = format!("x={DEV_TEST_CLIENT_SECRET}\n");
        assert_log_has_no_secret_fields("test", &log, &[DEV_TEST_CLIENT_SECRET]);
    }

    #[test]
    #[should_panic(expected = "raw JWTs")]
    fn jwt_shaped_value_in_collector_log_fails() {
        let log = format!("{}leak {REAL_SHAPED_JWT}\n", clean_log());
        assert_collector_log_has_no_secrets(
            &log,
            TRACE_ID,
            REAL_SHAPED_JWT,
            &[DEV_TEST_CLIENT_SECRET],
        );
    }
}
