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
//! metric), §Key custody (no end-to-end or zero-trust boolean anywhere,
//! because the default deployment is neither), and §R4 (every media-path series
//! is identity-free and carries `key_custody="operator"`; its identity vocabulary
//! is a fenced block there, pinned against this module's consts by a unit test).
//! `observability` owns that file;
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

/// Which rule a violation breached. Separate variants because the rules have
/// different owners, different runbook entries and different fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// R1 — a meeting identifier, raw or hashed, on any metric.
    MeetingIdentifier,
    /// §Key custody — an end-to-end / zero-trust claim in a name, label key or
    /// label value.
    OverclaimToken,
    /// R4 — a meeting, participant, sender, key, slot or stream identity (or any
    /// other token of the identity vocabulary) in a label KEY of a media-path
    /// series.
    MediaPathIdentity,
    /// R4 — a media-path series without `key_custody="operator"`.
    MissingKeyCustody,
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

/// Metric-name prefixes that make a series a MEDIA-PATH series.
///
/// Defined ONCE, in the ```media-path-prefixes fenced block of
/// `docs/observability/label-taxonomy.md` §R4; this copy is pinned to it by
/// [`tests::media_path_prefixes_match_the_taxonomy_block`]. It is a prefix list rather than a name list
/// on purpose: a new `mh_media_*` series is in scope the day it lands, with no
/// edit here. `dt_client_time_to_first_media_frame_ms` is listed whole because it
/// is the one exported client media metric outside `dt_client_media_`.
pub const MEDIA_PATH_PREFIXES: &[&str] = &[
    "mh_media_",
    "mc_media_",
    "mc_meeting_kek_",
    "mc_meeting_sender_ids_",
    "dt_client_media_",
    "dt_client_time_to_first_media_frame_ms",
];

/// The single permitted `key_custody` value. Mirrors
/// `crates/common/src/observability/labels.rs::KEY_CUSTODY_OPERATOR`; this crate
/// does not depend on `common`, so the value is restated here and pinned by
/// [`tests::key_custody_value_matches_common`], which reads that file.
pub const KEY_CUSTODY_KEY: &str = "key_custody";
/// See [`KEY_CUSTODY_KEY`].
pub const KEY_CUSTODY_OPERATOR: &str = "operator";

/// R4 identity vocabulary, CONTAINMENT half: nouns long enough to match as a
/// substring of the lower-cased key (`senderIndex`, `x_stream_id`).
///
/// **One policy, two consumers.** The only home of this vocabulary is the
/// fenced ```identity-label-policy block in `docs/observability/label-taxonomy.md`
/// §R4; `dt-guard client-metrics-export` reads that block at runtime to police
/// the collector's `keep_keys` and GC's forwarded keys, and this kernel applies
/// the same vocabulary to STORED series.
/// [`tests::identity_vocabulary_matches_the_taxonomy_block`] parses the block and
/// fails if these consts differ from it. `meeting` overlaps
/// [`MEETING_ID_SUBSTRINGS`] by design: R1 covers every metric, R4 only the
/// media path.
pub const IDENTITY_CONTAINMENT: &[&str] = &[
    "meeting",
    "participant",
    "sender",
    "session",
    "slot",
    "stream",
];
/// R4 identity vocabulary, SEGMENT half: tokens short enough that containment
/// would false-fire (`id` in `valid`, `key` in `monkey`), so they match only a
/// whole segment after splitting on non-alphanumerics and camelCase boundaries.
pub const IDENTITY_SEGMENTS: &[&str] = &["user", "key", "kek", "id", "hash"];
/// R4 exemptions — EXACT keys, never segments or prefixes (so `slot_id` still
/// fires while `slot_state` passes). Reasons live with the taxonomy block.
pub const IDENTITY_EXEMPT: &[&str] = &["key_custody", "org_id", "slot_state"];

/// The client SDK's reserved metric-name namespace.
///
/// Enforced for the client by `R26_NAME_RE` in
/// `crates/dt-guard/src/ts_metric_naming.rs`; no Rust crate emits it. Series in
/// this namespace ARE scrape-reachable as of R-27 (they arrive under
/// `job="otel-collector"`), so this is now a SELECTOR for the client leg of the
/// hygiene rules — see [`scrape_reachable_client_series`] — and no longer the
/// subject of a non-reachability premise pin.
const CLIENT_METRIC_PREFIX: &str = "dt_client_";

/// Whether `name` is a media-path series under [`MEDIA_PATH_PREFIXES`].
#[must_use]
pub fn is_media_path_series(name: &str) -> bool {
    MEDIA_PATH_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Split a label key into lower-cased segments on every non-alphanumeric
/// character and on lower-to-upper camelCase boundaries (`meetingId` ->
/// `meeting`, `id`; `user-id` -> `user`, `id`).
///
/// MUST match `dt-guard`'s splitter (`ts_retained_credentials::segments`) and the
/// "Matcher semantics" paragraph of `docs/observability/label-taxonomy.md` §R4.
/// Both crates run the shared ```identity-label-cases table from that section,
/// so a divergence goes red in whichever crate drifted.
fn key_segments(key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in key.chars() {
        if !c.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if c.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        cur.push(c.to_ascii_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The R4 identity token a label key matches, if any. Exempt keys never match.
#[must_use]
pub fn identity_token_in_key(key: &str) -> Option<&'static str> {
    if IDENTITY_EXEMPT.contains(&key) {
        return None;
    }
    let lk = key.to_ascii_lowercase();
    if let Some(n) = IDENTITY_CONTAINMENT.iter().find(|n| lk.contains(**n)) {
        return Some(n);
    }
    let segs = key_segments(key);
    IDENTITY_SEGMENTS
        .iter()
        .find(|t| segs.iter().any(|s| s == **t))
        .copied()
}

/// Evaluate every rule over a series set.
///
/// **No allowlist, and the empty allowlist is still the point — but the reason
/// changed, and the old reason is now false.** ADR-0036 §11 grandfathers exactly
/// one set: the ADR-0028 join-flow metrics of the *client SDK*, which carry
/// `meeting_id_hash`.
///
/// This used to rest on those metrics being structurally unable to reach a
/// scrape — the collector declared a single `debug` exporter and no job scraped
/// it. **Both of those facts are false as of R-27**: the collector has a
/// `prometheus` exporter and an `otel-collector` scrape job, and client series
/// are now examined by this module.
///
/// The conclusion survives on a stronger premise. The collector's metric-NAME
/// allowlist (`infra/services/otel-collector/collector.yaml`) admits exactly the
/// media set and EXCLUDES all five grandfathered join-flow metrics, so no
/// `meeting_id_hash`-carrying series is exported at all. The exception therefore
/// still has **zero members**, now by curation rather than by accident. Had the
/// join-flow metrics been exported with the hash stripped instead, this empty
/// allowlist would have had to become a real carve-out — which is an independent
/// reason the name list is curated the way it is.
///
/// An allowlist here would still be dead on the day it was written and would
/// swallow the first real violation after that. If a carve-out ever seems
/// necessary, the diagnosis is that the collector's name allowlist was widened
/// to a metric carrying a meeting dimension — fix it there, not here.
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

        // R4 — media-path series only. `key_custody` must be PRESENT with the
        // single permitted value; absence and a wrong value are one violation
        // each, reported with the value seen.
        let media = is_media_path_series(&s.name);
        if media {
            match s.labels.get(KEY_CUSTODY_KEY).map(String::as_str) {
                Some(KEY_CUSTODY_OPERATOR) => {}
                other => out.push(Violation {
                    rule: Rule::MissingKeyCustody,
                    series: s.name.clone(),
                    detail: format!(
                        "media-path series must carry `{KEY_CUSTODY_KEY}=\"{KEY_CUSTODY_OPERATOR}\"`, \
                         saw {other:?}"
                    ),
                }),
            }
        }

        for (k, v) in &s.labels {
            let lk = k.to_ascii_lowercase();

            if media {
                if let Some(token) = identity_token_in_key(k) {
                    out.push(Violation {
                        rule: Rule::MediaPathIdentity,
                        series: s.name.clone(),
                        detail: format!("label KEY `{k}` matches identity token `{token}`"),
                    });
                }
            }

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
/// **A selector, not a premise pin.** It used to pin that client series were not
/// scrape-reachable; R-27 made them reachable, so the pin was replaced by the
/// client leg of the hygiene rules in `32_media_metric_hygiene.rs`. A non-empty
/// result is now the NORMAL case whenever a browser has emitted recently, and an
/// EMPTY result is not evidence of anything — on an idle cluster no browser has
/// emitted within the collector's `metric_expiration`. That is why callers must
/// carry a browser-independent positive control (`up{job="otel-collector"}`)
/// rather than gating on this being non-empty, which would red on every idle
/// run.
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

/// Turn a Prometheus instant-query response into the projection the rules read.
///
/// **Pure and total-or-loud, so it is FIRE-fixturable in the always-on Rust lane.**
/// This is the half of the old test-file-local `fetch_all_series` that had never
/// been exercised: it lived in a `tests/` file behind the `observability` feature
/// and the cluster gate, which `cargo test` cannot reach. That is the same root
/// cause this module's relabel scanner already documents — the escaped-JSON bug
/// that passed on every possible input, including a real violation, precisely
/// because nothing could run it.
///
/// # A series with no `__name__` PANICS rather than becoming a sentinel
///
/// The earlier version substituted `"<unnamed>"`. That is a **false negative in
/// two of the three consumers**: `32_media_metric_hygiene.rs`'s anchor compares
/// `s.name == MH_ANCHOR_METRIC`, and [`scrape_reachable_client_series`] compares a
/// prefix — a sentinel silently satisfies neither, so the anchor reports "absent"
/// and every assertion it guards passes vacuously. ([`check_series`] is unaffected;
/// it iterates labels regardless of name, so there a sentinel degrades triage
/// rather than opening a hole. Stating the real blast radius, not the alarming one.)
///
/// Refusing a key that cannot be formed is the same rule `results_to_instance_map`
/// applies to a result row with no `instance` label, and it is the right one here
/// for the sharper reason: a sentinel key is **indistinguishable from a real miss**
/// at exactly the assertion that depends on the key.
///
/// A Prometheus instant query cannot return a series without `__name__`, so this
/// panic is unreachable against a healthy Prometheus — which is the point. If it
/// ever fires, the response is not what this function was written against, and
/// continuing would mean evaluating rules over a projection nobody designed.
///
/// # What this function does NOT guarantee
///
/// It returns a `Vec`. It **cannot** make a caller use one fetch for two
/// assertions, so the "one fetch feeds both the anchor and the predicate" argument
/// is deliberately **not** stated here — it is a property of a *call site*, and it
/// lives at each one. See `32_media_metric_hygiene.rs` and
/// `30_observability.rs`, which each make that argument for their own assertion
/// pair. Writing it once here would be a false single-source-of-truth of
/// *reasoning*: it would read as a guarantee while a third caller fetching twice
/// silently made it hold nowhere.
#[must_use]
pub fn response_to_series(response: &crate::fixtures::metrics::QueryResponse) -> Vec<Series> {
    response
        .data
        .result
        .iter()
        .map(|r| {
            let name = r.metric.get("__name__").cloned().unwrap_or_else(|| {
                panic!(
                    "Prometheus returned a series with no `__name__` label: {:?}. \
                     This is refused rather than bucketed under a sentinel, because a \
                     sentinel name is indistinguishable from a real miss at the anchor \
                     assertions that key on the name. Do NOT 'fix' this by restoring a \
                     placeholder.",
                    r.metric
                )
            });
            let labels: BTreeMap<String, String> = r
                .metric
                .iter()
                .filter(|(k, _)| k.as_str() != "__name__")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Series { name, labels }
        })
        .collect()
}

/// Fetch every stored series matching `selector`, as [`Series`].
///
/// **A deliberately trivial wrapper over [`response_to_series`], and the module's
/// only I/O.** It lives here rather than in a `tests/` file because it has *two*
/// callers (`30_observability.rs` and `32_media_metric_hygiene.rs`) and a copy in
/// each is the duplication this hoist exists to remove. Contrast
/// [`service_jobs_with_metric_relabeling`], whose I/O legitimately stays in its one
/// test file: one caller, nothing duplicated.
///
/// The **parse** is what needed the always-on lane; the I/O is the thin part that
/// could not be fixtured either way. That split is why a module whose header
/// advertises purity now contains a network call — it is the seam
/// `fixtures/metrics.rs` already models with `instance_counter_map` over
/// `results_to_instance_map`, not an erosion of it.
///
/// Panics on a failed query, naming the PromQL: an unreachable cluster must not be
/// reported as an empty result set, which reads exactly like a clean one.
pub async fn fetch_all_series(
    client: &crate::fixtures::metrics::PrometheusClient,
    selector: &str,
    triage: &str,
) -> Vec<Series> {
    let response = client.query_promql(selector).await.unwrap_or_else(|e| {
        panic!(
            "{triage}: Prometheus instant query `{selector}` failed: {e}. \
             The cluster or the port-forward is the fault here, not the diff."
        )
    });
    response_to_series(&response)
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
        // R1 AND R4 both fire on a media-path series: R1 bars it on every
        // metric, R4's `meeting` noun on the media path. Count per rule.
        assert_eq!(count(&v, Rule::MeetingIdentifier), 1);
        assert_eq!(count(&v, Rule::MediaPathIdentity), 1);
        assert_eq!(v.len(), 2);
    }

    /// The case the Rust source guard documents as uncovered: `starts_with`
    /// cannot see a TRAILING compound, so `PII_PREFIX_DENYLIST` misses this and
    /// the taxonomy marks it `[reviewer-only]`. Containment catches it, which is
    /// this assertion's specific contribution over the source guard.
    #[test]
    fn fires_on_trailing_compound_meeting_identifier() {
        let v = check_series(&[s(
            "mh_media_forward_latency_seconds",
            &[("x_meeting_id", "m-1"), ("key_custody", "operator")],
        )]);
        assert_eq!(count(&v, Rule::MeetingIdentifier), 1);
    }

    #[test]
    fn fires_on_overclaim_label_key() {
        let v = check_series(&[s(
            "mh_media_frames_forwarded_total",
            &[("e2ee", "true"), ("key_custody", "operator")],
        )]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, Rule::OverclaimToken);
    }

    /// A boolean hiding under an innocuous key — the reason values are scanned
    /// and not only keys.
    #[test]
    fn fires_on_overclaim_label_value() {
        let v = check_series(&[s(
            "mc_media_policy_pushes_total",
            &[("mode", "zero_trust"), ("key_custody", "operator")],
        )]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, Rule::OverclaimToken);
    }

    #[test]
    fn fires_on_overclaim_in_metric_name() {
        let v = check_series(&[s(
            "mh_media_e2ee_frames_total",
            &[("key_custody", "operator")],
        )]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, Rule::OverclaimToken);
    }

    /// Case-insensitivity, since a camelCase label would otherwise slip through
    /// the same way a word-boundary matcher slips on compounds.
    #[test]
    fn fires_on_camel_case_overclaim_value() {
        let v = check_series(&[s(
            "mh_media_frames_forwarded_total",
            &[("mode", "ZeroTrust"), ("key_custody", "operator")],
        )]);
        assert_eq!(v.len(), 1);
    }

    fn count(v: &[Violation], rule: Rule) -> usize {
        v.iter().filter(|x| x.rule == rule).count()
    }

    // ------------------------------------------------------------------
    // R4 — media-path identity and key custody (story 2 task 16)
    // ------------------------------------------------------------------

    /// The clean control for R4, over REAL story-2 label shapes. Without it the
    /// FIRE fixtures below could pass because the predicate flags everything —
    /// and with `id` in the segment set, the scrape-added and bounded-vocabulary
    /// keys here are exactly where a false positive would appear.
    #[test]
    fn r4_real_story2_series_produce_nothing() {
        let ko = ("key_custody", "operator");
        let series = vec![
            s(
                "mc_meeting_kek_pushes_total",
                &[("outcome", "delivered"), ko],
            ),
            s(
                "mc_meeting_kek_generated_total",
                &[("trigger", "participant_left"), ko],
            ),
            s("mc_meeting_kek_rotation_pending_age_seconds", &[ko]),
            s(
                "mc_meeting_kek_rotation_coalesced_leaves_bucket",
                &[("le", "4"), ko],
            ),
            s("mc_meeting_sender_ids_issued_max", &[ko]),
            s(
                "mc_media_slot_states_total",
                &[("slot_state", "active"), ko],
            ),
            s(
                "mc_media_server_mute_requests_total",
                &[("outcome", "applied"), ko],
            ),
            s(
                "mh_media_egress_edges",
                &[ko, ("instance", "10.0.0.1:8081"), ("job", "mh-service")],
            ),
            s(
                "mh_media_stream_admission_total",
                &[("outcome", "admitted"), ko],
            ),
            s(
                "mh_media_frames_dropped_total",
                &[("reason", "server_muted"), ("direction", "ingress"), ko],
            ),
            s(
                "dt_client_media_capture_source",
                &[
                    ("mode", "test_tone"),
                    ("client_version", "0.1.0"),
                    ("org_id", "acme"),
                    ko,
                    ("otel_scope_name", "dt"),
                    ("otel_scope_version", "1"),
                    ("instance", "otel-collector:8889"),
                    ("job", "otel-collector"),
                ],
            ),
            s(
                "dt_client_media_kek_updates_total",
                &[("source", "kek_update"), ("action", "x"), ko],
            ),
            s(
                "dt_client_time_to_first_media_frame_ms_bucket",
                &[("le", "100"), ko],
            ),
        ];
        let v = check_series(&series);
        assert!(
            v.is_empty(),
            "unexpected R4 violations on real shapes: {v:?}"
        );
    }

    /// The fenced block `name` from label-taxonomy.md, as text (without fences).
    fn taxonomy_block(name: &str) -> String {
        let md = repo_file("docs/observability/label-taxonomy.md");
        let fence = format!("```{name}\n");
        let start = md
            .find(&fence)
            .unwrap_or_else(|| panic!("label-taxonomy.md must carry the ```{name} block (§R4)"))
            + fence.len();
        let end = md[start..]
            .find("```")
            .unwrap_or_else(|| panic!("unterminated ```{name} block"));
        md[start..start + end].to_string()
    }

    /// The SHARED matcher table (```identity-label-cases, §R4), which dt-guard's
    /// matcher also runs. Positive control: both halves parsed non-empty and the
    /// `-`-separated case is present, so an empty parse cannot pass.
    #[test]
    fn r4_matcher_runs_the_shared_case_table() {
        let block = taxonomy_block("identity-label-cases");
        let fire = policy_line(&block, "fire");
        let pass = policy_line(&block, "pass");
        assert!(
            !fire.is_empty() && !pass.is_empty() && fire.iter().any(|k| k == "user-id"),
            "positive control: the case table parsed empty or lost `user-id`"
        );
        for key in &fire {
            let v = check_series(&[s(
                "mh_media_frames_forwarded_total",
                &[(key.as_str(), "v"), ("key_custody", "operator")],
            )]);
            assert_eq!(
                count(&v, Rule::MediaPathIdentity),
                1,
                "`{key}` must fire R4: {v:?}"
            );
        }
        for key in &pass {
            assert_eq!(identity_token_in_key(key), None, "`{key}` must NOT fire R4");
        }
    }

    /// Exemptions are EXACT keys, not segment or prefix holes.
    #[test]
    fn r4_exemptions_are_exact() {
        assert_eq!(identity_token_in_key("slot_state_id"), Some("slot"));
        assert_eq!(identity_token_in_key("org_id_hash"), Some("id"));
    }

    /// DRIFT PIN for the media-path definition (```media-path-prefixes, §R4).
    #[test]
    fn media_path_prefixes_match_the_taxonomy_block() {
        let block = taxonomy_block("media-path-prefixes");
        let parsed: Vec<String> = block
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        assert!(
            parsed.iter().any(|p| p == "mh_media_"),
            "positive control: the prefix block parsed empty or lost `mh_media_`"
        );
        let want: Vec<String> = MEDIA_PATH_PREFIXES
            .iter()
            .map(|p| (*p).to_string())
            .collect();
        assert_eq!(
            parsed, want,
            "MEDIA_PATH_PREFIXES drifted from label-taxonomy.md §R4"
        );
    }

    /// Scope: R4 applies to media-path series only. The same key on a
    /// control-plane series is not R4's business (R1 still covers `meeting_id`).
    #[test]
    fn r4_is_scoped_to_media_path_series() {
        let v = check_series(&[s("mc_session_joins_total", &[("participant_id", "p")])]);
        assert_eq!(count(&v, Rule::MediaPathIdentity), 0);
        assert_eq!(count(&v, Rule::MissingKeyCustody), 0);
    }

    /// FIRE: a media-path series with key_custody absent, or with a wrong value.
    #[test]
    fn r4_fires_on_missing_or_wrong_key_custody() {
        let v = check_series(&[s("mh_media_egress_edges", &[])]);
        assert_eq!(count(&v, Rule::MissingKeyCustody), 1);
        let v = check_series(&[s(
            "dt_client_media_frames_sent_total",
            &[("key_custody", "participant")],
        )]);
        assert_eq!(count(&v, Rule::MissingKeyCustody), 1);
        // mc_meeting_sender_ids_ is IN the media-path set by decision.
        let v = check_series(&[s("mc_meeting_sender_ids_issued_max", &[])]);
        assert_eq!(count(&v, Rule::MissingKeyCustody), 1);
    }

    /// Parse one `name = a, b, c` line out of the taxonomy's fenced block.
    fn policy_line(block: &str, name: &str) -> Vec<String> {
        block
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix(name)
                    .and_then(|r| r.trim_start().strip_prefix('='))
            })
            .map(|r| {
                r.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn repo_file(rel: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::fs::read_to_string(root.join(rel))
            .unwrap_or_else(|e| panic!("read {rel}: {e} — the drift pin cannot run without it"))
    }

    /// DRIFT PIN: the consts above must equal the ONE home of the vocabulary,
    /// the fenced ```identity-label-policy block in label-taxonomy.md §R4. The
    /// positive control (non-empty, contains `participant`) keeps an empty parse
    /// from comparing equal to nothing.
    #[test]
    fn identity_vocabulary_matches_the_taxonomy_block() {
        let block = taxonomy_block("identity-label-policy");
        let block = block.as_str();
        let containment = policy_line(block, "containment");
        let segment = policy_line(block, "segment");
        let exempt = policy_line(block, "exempt");
        assert!(
            containment.iter().any(|t| t == "participant") && !segment.is_empty() && !exempt.is_empty(),
            "positive control: the policy block parsed empty or lost `participant` — extraction failure, not a clean result"
        );
        let as_vec = |c: &[&str]| c.iter().map(|t| (*t).to_string()).collect::<Vec<_>>();
        assert_eq!(
            containment,
            as_vec(IDENTITY_CONTAINMENT),
            "containment drifted from label-taxonomy.md §R4"
        );
        assert_eq!(
            segment,
            as_vec(IDENTITY_SEGMENTS),
            "segment drifted from label-taxonomy.md §R4"
        );
        assert_eq!(
            exempt,
            as_vec(IDENTITY_EXEMPT),
            "exempt drifted from label-taxonomy.md §R4"
        );
    }

    /// The restated `operator` value must equal common's constant.
    #[test]
    fn key_custody_value_matches_common() {
        let src = repo_file("crates/common/src/observability/labels.rs");
        let want = format!("KEY_CUSTODY_OPERATOR: &str = \"{KEY_CUSTODY_OPERATOR}\"");
        let want_key = format!("KEY_CUSTODY_LABEL: &str = \"{KEY_CUSTODY_KEY}\"");
        assert!(
            src.contains(&want),
            "common's KEY_CUSTODY_OPERATOR no longer equals {KEY_CUSTODY_OPERATOR:?}"
        );
        assert!(
            src.contains(&want_key),
            "common's KEY_CUSTODY_LABEL no longer equals {KEY_CUSTODY_KEY:?}"
        );
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

    fn response_with(
        metrics: Vec<std::collections::HashMap<String, String>>,
    ) -> crate::fixtures::metrics::QueryResponse {
        crate::fixtures::metrics::QueryResponse {
            status: "success".to_string(),
            data: crate::fixtures::metrics::QueryData {
                result_type: "vector".to_string(),
                result: metrics
                    .into_iter()
                    .map(|metric| crate::fixtures::metrics::QueryResult {
                        metric,
                        value: None,
                        values: None,
                    })
                    .collect(),
            },
        }
    }

    fn metric_map(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn response_to_series_lifts_name_and_keeps_remaining_labels() {
        let out = response_to_series(&response_with(vec![metric_map(&[
            ("__name__", "mh_media_frames_forwarded_total"),
            ("direction", "ingress"),
            ("key_custody", "operator"),
        ])]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "mh_media_frames_forwarded_total");
        assert_eq!(
            out[0].labels.get("direction").map(String::as_str),
            Some("ingress")
        );
        assert!(
            !out[0].labels.contains_key("__name__"),
            "`__name__` must be lifted out of the label map exactly once, not left in it"
        );
    }

    /// FIRE fixture for the decision recorded on [`response_to_series`]: a series
    /// with no `__name__` is refused, never bucketed under a sentinel. Without this
    /// the choice is a comment; with it, restoring a placeholder reds here.
    #[test]
    #[should_panic(expected = "no `__name__` label")]
    fn response_to_series_panics_rather_than_inventing_a_sentinel_name() {
        let _ = response_to_series(&response_with(vec![metric_map(&[(
            "direction",
            "ingress",
        )])]));
    }

    #[test]
    fn response_to_series_is_empty_for_an_empty_result() {
        assert!(response_to_series(&response_with(vec![])).is_empty());
    }
}
