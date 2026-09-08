//! Pure predicate for ADR-0036 §11 metric hygiene, plus its FIRE fixtures.
//!
//! # Why a pure function and not just an env-test
//!
//! A bad label cannot be planted on a live cluster, so an env-test alone can
//! only ever demonstrate the *absence* of a violation — which is exactly the
//! shape that passes when the control is dead. Keeping the predicate pure means
//! its **fire** half is demonstrable by unit fixtures that run in the always-on
//! Rust test lane, while the env-test supplies the **applies** half against real
//! scraped series. Both halves of ADR-0036 §11's "demonstrated, not asserted"
//! checklist, split along the only line that makes each of them provable.
//!
//! # What this evaluates, and why it is the right surface
//!
//! **Stored series, via `QueryResult`, not pod exposition text.**
//! `docs/observability/label-taxonomy.md` R1 bars a meeting identifier because
//! of cardinality and per-meeting aggregation *in the metrics backend*, so the
//! harm is realised at the stored series. The two surfaces can disagree in both
//! directions — exposition text would miss a label added by
//! `metric_relabel_configs` and would falsely flag one that relabeling drops
//! before storage. They agree today only as a configuration fact, which the
//! env-test asserts rather than assumes.
//!
//! # Policy source
//!
//! Derived from `docs/observability/label-taxonomy.md`, never restated here:
//! §Media-path identity R1 (no meeting identifier, raw or hashed, on any
//! metric) and §Key custody (no end-to-end or zero-trust boolean anywhere,
//! because the default deployment is neither). `observability` owns that file;
//! this module owns only the mechanics of checking it.

use std::collections::BTreeMap;

/// A scraped series reduced to what the rules are about.
///
/// **A PROJECTION of `QueryResult`, not a parallel declaration of it** — it drops
/// the sample value and lifts `__name__` out of the label map. That is why the
/// two coexist rather than one replacing the other, and it is load-bearing in
/// both directions: these rules treat metric name, label KEY and label VALUE as
/// three distinct surfaces, and `QueryResult` fuses `__name__` into the labels,
/// so consuming it raw would fork `__name__` special-casing across all three
/// checks. The `BTreeMap` also buys deterministic violation ordering that the
/// assertions rely on. Do not collapse them in either direction.
#[derive(Debug, Clone)]
pub struct Series {
    /// Metric name (`__name__`).
    pub name: String,
    /// Full label set, excluding `__name__`.
    pub labels: BTreeMap<String, String>,
}

/// Which rule a violation breached. Separate variants because the two have
/// different owners, different runbook entries and different fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// R1 — a meeting identifier, raw or hashed, on any metric.
    MeetingIdentifier,
    /// §Key custody — an end-to-end / zero-trust claim in a name, label key or
    /// label value.
    OverclaimToken,
}

/// One breach, carrying enough context for a runbook entry to be actionable.
#[derive(Debug, Clone)]
pub struct Violation {
    pub rule: Rule,
    pub series: String,
    pub detail: String,
}

/// Case-insensitive substring containment, **never word-boundary**.
///
/// This is the same lesson the credential-leak fixtures are about, applied here:
/// `\b` cannot see inside a compound, so a word-boundary matcher would miss
/// `is_e2ee`, `e2eeEnabled`, `endToEnd` and `zeroTrust`. Substring is also what
/// makes the meeting-identifier rule **stronger than the Rust source guard**:
/// `PII_PREFIX_DENYLIST` matches with `starts_with`, so a *trailing* compound
/// like `x_meeting_id` is not a prefix match and the taxonomy marks that case
/// `[reviewer-only]`. Containment catches it at the artifact.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_ascii_lowercase().contains(needle)
}

/// Meeting-identifier spellings, as substrings of a lowercased label key.
///
/// `meeting_id` covers `meeting_id`, `meeting_id_hash` and any leading or
/// trailing compound. The hashed form is deliberately NOT exempted: R1 bars a
/// meeting identifier "raw or hashed", and hashing a low-cardinality identifier
/// does not make a two-stream series less de-anonymising.
const MEETING_ID_SUBSTRINGS: &[&str] = &["meeting_id", "meetingid"];

/// Overclaim spellings. Matches the client-side test's token set in spirit
/// (`packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts`) but is
/// deliberately a SEPARATE list rather than a shared anchor: these are two sites
/// each making their own decision under one rule, so a shared list would be a
/// false single-source-of-truth hiding the fork. The rule they share lives in
/// `docs/observability/label-taxonomy.md` §Key custody.
///
/// `end_to_end` is included but **`e2e` alone is not**: end-to-end *latency* is
/// ordinary measurement vocabulary and the taxonomy's §Key custody scope note is
/// explicit that the prohibition covers end-to-end *encryption* and *zero-trust*
/// as deployment properties. A bare `e2e` would red on a legitimate latency
/// metric, and a control that false-fires gets muted.
const OVERCLAIM_SUBSTRINGS: &[&str] =
    &["e2ee", "end_to_end", "endtoend", "zero_trust", "zerotrust"];

/// The client SDK's reserved metric-name namespace.
///
/// Enforced for the client by `R26_NAME_RE` in
/// `crates/dt-guard/src/ts_metric_naming.rs`; no Rust crate emits it. Used as a
/// premise pin rather than a hygiene rule — see [`scrape_reachable_client_series`].
const CLIENT_METRIC_PREFIX: &str = "dt_client_";

/// Evaluate both rules over a series set.
///
/// **No allowlist, and the empty allowlist is the point.** ADR-0036 §11
/// grandfathers exactly one set — the ADR-0028 join-flow metrics of the *client
/// SDK*, which carry `meeting_id_hash`. Those are TypeScript and structurally
/// cannot reach a Prometheus scrape: the OTel collector declares a single
/// `debug` exporter and there is no otel-collector scrape job. So over Rust
/// service jobs the exception has **zero members**. An allowlist here would be
/// dead on the day it was written and would swallow the first real violation
/// after that. If a carve-out ever seems necessary, the diagnosis is that the
/// job selector was widened past the Rust services — narrow it back.
///
/// (The structural facts are cited deliberately, not the collector's
/// `verbosity: normal`: verbosity is a knob someone can turn, the missing
/// exporter and missing scrape job are not.)
#[must_use]
pub fn check_series(series: &[Series]) -> Vec<Violation> {
    let mut out = Vec::new();
    for s in series {
        let lname = s.name.to_ascii_lowercase();

        for needle in OVERCLAIM_SUBSTRINGS {
            if lname.contains(needle) {
                out.push(Violation {
                    rule: Rule::OverclaimToken,
                    series: s.name.clone(),
                    detail: format!("metric NAME contains `{needle}`"),
                });
            }
        }

        for (k, v) in &s.labels {
            let lk = k.to_ascii_lowercase();

            // KEYS ONLY, and deliberately — do NOT "fix" this into symmetry with
            // the overclaim loop below, which checks keys AND values.
            //
            // The two rules differ in what a VALUE means. An overclaim can hide
            // in a value under an innocuous key (`mode="zero_trust"`), so values
            // are scanned there. For a meeting identifier the opposite holds: a
            // value containing the literal `meeting_id` is metadata
            // (`id_kind="meeting_id"`), not a leak — while an actually-leaked
            // meeting id is a UUID that no substring check would catch. Scanning
            // values here would add false positives and exactly zero true
            // positives.
            //
            // The asymmetry is stated because the loops are adjacent and the next
            // reader will otherwise tidy them together (@observability).
            for needle in MEETING_ID_SUBSTRINGS {
                if lk.contains(needle) {
                    out.push(Violation {
                        rule: Rule::MeetingIdentifier,
                        series: s.name.clone(),
                        detail: format!("label KEY `{k}` contains `{needle}`"),
                    });
                }
            }
            for needle in OVERCLAIM_SUBSTRINGS {
                if lk.contains(needle) {
                    out.push(Violation {
                        rule: Rule::OverclaimToken,
                        series: s.name.clone(),
                        detail: format!("label KEY `{k}` contains `{needle}`"),
                    });
                }
                // Values too: a boolean can hide under an innocuous key.
                if contains_ci(v, needle) {
                    out.push(Violation {
                        rule: Rule::OverclaimToken,
                        series: s.name.clone(),
                        detail: format!("label VALUE of `{k}` contains `{needle}`"),
                    });
                }
            }
        }
    }
    out
}

/// Series whose metric name is in the client SDK's reserved namespace.
///
/// **A premise pin, not a hygiene rule.** Client media metrics are covered at
/// the node tier by task 19 and are not re-asserted here. What this pins is that
/// they are not scrape-reachable — trivially true today. If someone adds a
/// Prometheus exporter to the OTel collector, this becomes non-empty and forces
/// this suite's job list to be extended, rather than the client surface silently
/// remaining unexamined. A non-empty result means "extend the suite", **not**
/// "a leak occurred".
///
/// Keyed on the metric-name prefix rather than on a `client_version` label:
/// `client_version` is not intrinsically client-only (a server-side
/// `gc_join_attempts_total{client_version}` would be a reasonable metric), so
/// that predicate would have a false-positive mode whose failure message
/// misdiagnoses it.
#[must_use]
pub fn scrape_reachable_client_series(series: &[Series]) -> Vec<String> {
    series
        .iter()
        .filter(|s| s.name.starts_with(CLIENT_METRIC_PREFIX))
        .map(|s| s.name.clone())
        .collect()
}

/// The four Rust service jobs this suite reasons about.
///
/// The other four Prometheus jobs (`prometheus`, `kube-state-metrics`,
/// `node-exporter`, `kubelet`) are *exactly* where `metric_relabel_configs` is
/// standard practice — kubelet/cAdvisor and kube-state-metrics are the usual
/// high-cardinality offenders people drop. A whole-config check would therefore
/// fire first and most often on a relabel that cannot affect this suite at all.
pub const SERVICE_JOBS: &[&str] = &["ac-service", "gc-service", "mc-service", "mh-service"];

/// Why a relabel scan could not be performed. **Never an empty success** — an
/// unparseable or unrecognised config must fail loudly, not report "no offending
/// jobs", which is indistinguishable from a clean result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelabelScanError {
    /// The body was not the JSON envelope Prometheus returns.
    NotPrometheusConfigJson,
    /// Parsed, but not all four service jobs were found in the scrape config.
    /// The scan would be inspecting fewer jobs than it claims to.
    ServiceJobsMissing(Vec<String>),
}

/// Service jobs carrying `metric_relabel_configs`, from a `/api/v1/status/config`
/// body.
///
/// # This function exists because the obvious version was dead code
///
/// `/api/v1/status/config` returns **JSON with the config embedded as an escaped
/// string** — `{"status":"success","data":{"yaml":"global:\\n …"}}`. The `\n` are
/// two-character escapes in the HTTP body, not newlines. A previous version of
/// this check split the raw body on `- job_name:` and called `.lines().next()` on
/// each section; with zero real newlines in the body that returned the entire
/// remainder of the config, matched no job, and **passed on every input
/// including a real violation**. Verified against the live endpoint: the body
/// contains 0 newline characters.
///
/// That is item 1 of the vacuity taxonomy — an assertion over an empty set —
/// arriving inside the fix to a review finding. It is also why this lives in the
/// kernel with FIRE fixtures rather than inline in the cluster-gated test: the
/// parsing had no unit coverage precisely because it sat where `cargo test`
/// could not reach it.
///
/// # Errors
///
/// [`RelabelScanError::NotPrometheusConfigJson`] if the body is not the expected
/// envelope; [`RelabelScanError::ServiceJobsMissing`] if any of [`SERVICE_JOBS`]
/// is absent from the parsed config. The second is the extraction's own
/// "did this have input" guard: without it, a parse that silently found nothing
/// would report an empty offender list, which reads exactly like a clean result.
pub fn service_jobs_with_metric_relabeling(
    config_body: &str,
) -> Result<Vec<String>, RelabelScanError> {
    let parsed: serde_json::Value =
        serde_json::from_str(config_body).map_err(|_| RelabelScanError::NotPrometheusConfigJson)?;
    let yaml = parsed
        .get("data")
        .and_then(|d| d.get("yaml"))
        .and_then(serde_json::Value::as_str)
        .ok_or(RelabelScanError::NotPrometheusConfigJson)?;

    // `yaml` is now genuinely unescaped — serde_json turned `\n` into newlines.
    let mut found: Vec<String> = Vec::new();
    let mut offending: Vec<String> = Vec::new();

    for section in yaml.split("- job_name:").skip(1) {
        let Some(job) = section
            .lines()
            .next()
            .map(|l| l.trim().trim_matches(['\'', '"']).to_string())
        else {
            continue;
        };
        if !SERVICE_JOBS.contains(&job.as_str()) {
            continue;
        }
        found.push(job.clone());
        if section.contains("metric_relabel_configs") {
            offending.push(job);
        }
    }

    let missing: Vec<String> = SERVICE_JOBS
        .iter()
        .filter(|j| !found.iter().any(|f| f == *j))
        .map(|j| (*j).to_string())
        .collect();
    if !missing.is_empty() {
        return Err(RelabelScanError::ServiceJobsMissing(missing));
    }

    Ok(offending)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Prometheus-shaped config body: JSON envelope, escaped YAML.
    fn config_body(jobs: &[(&str, bool)]) -> String {
        let mut yaml = String::from("global:\n  scrape_interval: 15s\nscrape_configs:\n");
        for (job, relabel) in jobs {
            yaml.push_str(&format!("- job_name: {job}\n  metrics_path: /metrics\n"));
            if *relabel {
                yaml.push_str("  metric_relabel_configs:\n  - action: drop\n");
            }
        }
        serde_json::json!({"status": "success", "data": {"yaml": yaml}}).to_string()
    }

    fn all_four(extra: &[(&str, bool)]) -> Vec<(&'static str, bool)> {
        let mut v: Vec<(&'static str, bool)> = SERVICE_JOBS.iter().map(|j| (*j, false)).collect();
        for (j, r) in extra {
            if let Some(slot) = v.iter_mut().find(|(name, _)| name == j) {
                slot.1 = *r;
            }
        }
        v
    }

    /// FIRE: a relabel on a service job must be reported.
    #[test]
    fn fires_on_metric_relabeling_of_a_service_job() {
        let body = config_body(&all_four(&[("ac-service", true)]));
        assert_eq!(
            service_jobs_with_metric_relabeling(&body),
            Ok(vec!["ac-service".to_string()])
        );
    }

    /// Must NOT fire: relabeling `kubelet` is ordinary cardinality management and
    /// cannot affect a suite that only inspects the four service jobs. This is
    /// the false-positive mode the scoping exists to remove.
    #[test]
    fn silent_on_metric_relabeling_of_a_non_service_job() {
        let mut jobs = all_four(&[]);
        jobs.push(("kubelet", true));
        let body = config_body(&jobs);
        assert_eq!(service_jobs_with_metric_relabeling(&body), Ok(vec![]));
    }

    /// The extraction's own vacuity guard. A config missing service jobs must be
    /// an ERROR, never an empty success — an empty offender list is
    /// indistinguishable from a clean result, which is how the previous version
    /// of this check passed on every input.
    #[test]
    fn missing_service_jobs_is_an_error_not_an_empty_pass() {
        let body = config_body(&[("prometheus", false), ("kubelet", false)]);
        let err = service_jobs_with_metric_relabeling(&body).unwrap_err();
        match err {
            RelabelScanError::ServiceJobsMissing(missing) => assert_eq!(missing.len(), 4),
            other => panic!("expected ServiceJobsMissing, got {other:?}"),
        }
    }

    /// Proof the parser is actually parsing, not pattern-matching the raw body.
    /// The escaped-newline body is exactly what defeated the previous version.
    #[test]
    fn raw_escaped_body_is_parsed_not_scanned() {
        let body = config_body(&all_four(&[("mh-service", true)]));
        assert_eq!(
            body.matches('\n').count(),
            0,
            "fixture must reproduce the real endpoint's escaping — zero literal newlines"
        );
        assert_eq!(
            service_jobs_with_metric_relabeling(&body),
            Ok(vec!["mh-service".to_string()])
        );
    }

    #[test]
    fn non_prometheus_body_is_an_error() {
        assert_eq!(
            service_jobs_with_metric_relabeling("not json"),
            Err(RelabelScanError::NotPrometheusConfigJson)
        );
    }

    fn s(name: &str, labels: &[(&str, &str)]) -> Series {
        Series {
            name: name.to_string(),
            labels: labels
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }

    /// The clean control. Without this, every FIRE fixture below could be
    /// passing because the predicate flags everything.
    #[test]
    fn real_shaped_clean_series_produce_nothing() {
        let series = vec![
            s(
                "mh_media_frames_forwarded_total",
                &[("direction", "egress"), ("key_custody", "operator")],
            ),
            s(
                "mh_media_frames_dropped_total",
                &[
                    ("reason", "queue_full"),
                    ("direction", "ingress"),
                    ("key_custody", "operator"),
                ],
            ),
            s("mc_session_joins_total", &[("status", "ok")]),
            // End-to-end LATENCY vocabulary must not be flagged.
            s("gc_request_end_latency_seconds", &[("route", "join")]),
        ];
        assert!(check_series(&series).is_empty());
    }

    #[test]
    fn fires_on_hashed_meeting_identifier() {
        let v = check_series(&[s(
            "mh_media_frames_forwarded_total",
            &[("meeting_id_hash", "abc123"), ("key_custody", "operator")],
        )]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, Rule::MeetingIdentifier);
    }

    /// The case the Rust source guard documents as uncovered: `starts_with`
    /// cannot see a TRAILING compound, so `PII_PREFIX_DENYLIST` misses this and
    /// the taxonomy marks it `[reviewer-only]`. Containment catches it, which is
    /// this assertion's specific contribution over the source guard.
    #[test]
    fn fires_on_trailing_compound_meeting_identifier() {
        let v = check_series(&[s(
            "mh_media_forward_latency_seconds",
            &[("x_meeting_id", "m-1")],
        )]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, Rule::MeetingIdentifier);
    }

    #[test]
    fn fires_on_overclaim_label_key() {
        let v = check_series(&[s("mh_media_frames_forwarded_total", &[("e2ee", "true")])]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, Rule::OverclaimToken);
    }

    /// A boolean hiding under an innocuous key — the reason values are scanned
    /// and not only keys.
    #[test]
    fn fires_on_overclaim_label_value() {
        let v = check_series(&[s("mc_media_policy_pushes_total", &[("mode", "zero_trust")])]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, Rule::OverclaimToken);
    }

    #[test]
    fn fires_on_overclaim_in_metric_name() {
        let v = check_series(&[s("mh_media_e2ee_frames_total", &[])]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, Rule::OverclaimToken);
    }

    /// Case-insensitivity, since a camelCase label would otherwise slip through
    /// the same way a word-boundary matcher slips on compounds.
    #[test]
    fn fires_on_camel_case_overclaim_value() {
        let v = check_series(&[s(
            "mh_media_frames_forwarded_total",
            &[("mode", "ZeroTrust")],
        )]);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn client_namespace_pin_is_empty_for_rust_service_series() {
        let series = vec![s("mh_media_frames_forwarded_total", &[])];
        assert!(scrape_reachable_client_series(&series).is_empty());
    }

    /// Proof the pin can fire — otherwise its emptiness above is not evidence.
    #[test]
    fn client_namespace_pin_fires_when_client_series_are_scraped() {
        let series = vec![s("dt_client_media_frames_sent_total", &[])];
        assert_eq!(scrape_reachable_client_series(&series).len(), 1);
    }
}
