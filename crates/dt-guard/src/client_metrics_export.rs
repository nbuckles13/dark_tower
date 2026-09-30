//! `client-metrics-export` subcommand — drift control for the browser metrics
//! export pipeline (R-27).
//!
//! # The failure this exists to make loud
//!
//! Every control on the collector's metrics path fails by making something
//! ABSENT: an un-allowlisted metric name is dropped, an un-allowlisted label key
//! is dropped, a non-delta datapoint is dropped. On a running cluster each of
//! those is INDISTINGUISHABLE from "no browser is connected" — and because
//! ADR-0036 §11 bars a per-browser identity label, there is no per-browser `up`
//! to disambiguate with. That is exactly how `dt_client_*` stayed dead for a
//! whole story without anyone noticing.
//!
//! So the relationships between the encodings are asserted HERE, at build time,
//! where divergence is loud.
//!
//! # Rules
//!
//! * **G1** `export_set_mismatch` — `client.md` `Exported: yes` set == the
//!   collector's metric-name include list, compared in BOTH directions.
//! * **G2 / rule 4** `missing_export_marker` — every `dt_client_*` string
//!   literal in ANY non-test sdk-core source file carries an explicit
//!   `Exported:` marker in the catalog. Absent is not "no".
//! * **G3** `exported_without_emitter` — every `Exported: yes` name occurs as a
//!   literal somewhere in non-test sdk-core source.
//! * **G4** `keep_key_not_forwarded` — collector `keep_keys` ⊆ GC's
//!   `ALLOWLIST ∪ MEDIA_DATAPOINT_EXTRA`.
//! * **G5** `expiration_relation` — `metric_expiration` strictly exceeds the
//!   longest `dt_client_*` range window in a loaded rule expr, and `max_stale >=
//!   metric_expiration + that window`. Both keys must be PRESENT.
//! * **G6** `sentinel_collision` — the collector's rewrite sentinel collides
//!   with no token in the TypeScript media vocabularies or MH's
//!   `MediaDropReason`.
//! * **Rule 1** `label_key_not_forwarded` / `unresolved_label_bag` — every
//!   resolved label key at every emission of an `Exported: yes` name is in GC's
//!   forwarded set AND in `keep_keys`; label bags the extractor cannot resolve
//!   fail closed. See [`check_label_bags`] for the scope and why.
//! * **Rule 2** `dead_alert_reference` / `client_alert_groups_by_instance` —
//!   every `dt_client_*` name in a loaded rule expr is exported, and every
//!   label it matches or groups on survives the pipeline.
//! * **Rule 3** `forwarded_key_not_kept` — every `MEDIA_DATAPOINT_EXTRA` key is
//!   in `keep_keys`.
//! * **Identity** `identity_key_forwarded` — no kept or media-forwarded key
//!   matches the identity vocabulary in `label-taxonomy.md` §R4.
//! * **Tripwire** `tripwire_rate_wrapped` / `tripwire_list_stale` — the
//!   lazily created client counters that carry a presence-form tripwire alert
//!   are never wrapped in `increase()`/`rate()` in a loaded rule.
//! * **G7** `reject_token_unclassified` / `reject_token_misclassified` — the
//!   frame-dropping reject tokens partition exactly between
//!   `MCMediaMissingKeyMaterial`'s reason alternation and [`NOT_KEY_DELIVERY`].
//! * **Suffix premise** `exporter_suffixing_enabled` — the collector's
//!   prometheus exporter sets `add_metric_suffixes: false`; any other present
//!   value renames every stored series out from under every name comparison
//!   here. Absent or empty is [`EMPTY_INPUT_RULE_ID`].
//! * **Rules 5/6** (input integrity) — every input is required; a missing file
//!   or an under-parsed extraction is [`EMPTY_INPUT_RULE_ID`].
//!
//! # Anti-vacuity contract, uniform across every rule
//!
//! Every extractor has a positive control and fails CLOSED on an empty or short
//! parse, with [`EMPTY_INPUT_RULE_ID`] — a reason token deliberately DISTINCT
//! from every content-failure reason. If an extraction break (a moved file, a
//! renamed export, a regex that stops matching after a refactor) reported the
//! same reason as a content failure, it would be mis-triaged as a content
//! result and "fixed" by loosening the extractor, leaving a guard that is green
//! forever while checking nothing. A MISSING input is the same class: there is
//! no "no artifacts, nothing to check" green path.
//!
//! When an extractor fails, the checks that depend on it are skipped (their
//! input is `None` in [`Inputs`]): reporting content findings over a broken
//! extraction would bury the real cause.

use crate::common::alert_rule_files::{load_rule_exprs_from, loadable_rules_files, LoadedRuleExpr};
use crate::common::duration::parse_prometheus_duration;
use crate::common::metric_catalog::CATALOG_HEAD_RE;
use crate::common::status::emit_ok;
use crate::common::test_code_filter::{is_test_path, strip_comments};
use crate::common::ts_lex::{lex, Lexed, StrLit};
use crate::ts_retained_credentials::segments;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

const CATALOG: &str = "docs/observability/metrics/client.md";
const COLLECTOR_CONFIG: &str = "infra/services/otel-collector/collector.yaml";
const GC_FILTER: &str = "crates/gc-service/src/services/telemetry_filter.rs";
const LABELS_RS: &str = "crates/common/src/observability/labels.rs";
const MEDIA_METRICS_TS: &str = "packages/sdk-core/src/media/setup/mediaMetrics.ts";
const REJECT_REASON_TS: &str = "packages/sdk-core/src/media/frame/rejectReason.ts";
const RECEIVE_PATH_TS: &str = "packages/sdk-core/src/media/frame/receivePath.ts";
const MEETING_SESSION_TS: &str = "packages/sdk-core/src/session/MeetingSession.ts";
const MH_METRICS_RS: &str = "crates/mh-service/src/observability/metrics.rs";
const SDK_SRC: &str = "packages/sdk-core/src";
const ALERTS_SUBDIR: &str = "infra/docker/prometheus/rules";
const VECTORS_JSON: &str = "proto/test-vectors/frame-v2.vectors.json";
const LABEL_TAXONOMY: &str = "docs/observability/label-taxonomy.md";

/// The value the collector rewrites a shape-failing label value to.
///
/// Mirrors `transform/client_metric_labels` in the collector config. If that
/// string changes, change it here — G6 is the check that the two agree about
/// what is reserved, not a derivation of it.
const REWRITE_SENTINEL: &str = "invalid";

/// Positive-control token for the TypeScript vocabulary extractor.
///
/// `no_roster_entry` is one of the reasons `MCMediaMissingKeyMaterial` selects
/// on, so if the extractor ever stops seeing it, the guard is blind to
/// precisely the vocabulary the alert depends on.
const TS_VOCAB_CANARY: &str = "no_roster_entry";

/// The media-path base label builder whose returned object literal defines the
/// keys behind `this.#base` in [`MEDIA_METRICS_TS`].
const MEDIA_LABELS_FN: &str = "export function mediaMetricLabels";

/// Positive control for the `this.#base` resolution: the custody label every
/// media-path series carries (label-taxonomy.md §R4).
const MEDIA_BASE_CANARY: &str = "key_custody";

/// The alert whose reason alternation G7 partitions against.
const MISSING_KEY_MATERIAL_ALERT: &str = "MCMediaMissingKeyMaterial";

/// Positive control for the G7 alternation parse.
const ALTERNATION_CANARY: &str = "no_kek_for_generation";

/// Positive control for the identity-policy block parse.
const IDENTITY_CANARY: &str = "participant";

/// The trace-only key that must never be kept on the metrics path (ADR-0036
/// §11). Checked by name, with its own message, on top of the vocabulary match.
const MEETING_ID_HASH: &str = "meeting_id_hash";

pub const EMPTY_INPUT_RULE_ID: &str = "extractor_empty_input";
pub const EXPORT_SET_MISMATCH_RULE_ID: &str = "export_set_mismatch";
pub const MISSING_EXPORT_MARKER_RULE_ID: &str = "missing_export_marker";
pub const EXPORTED_WITHOUT_EMITTER_RULE_ID: &str = "exported_without_emitter";
pub const KEEP_KEY_NOT_FORWARDED_RULE_ID: &str = "keep_key_not_forwarded";
pub const EXPIRATION_RELATION_RULE_ID: &str = "expiration_relation";
pub const SENTINEL_COLLISION_RULE_ID: &str = "sentinel_collision";
pub const LABEL_KEY_NOT_FORWARDED_RULE_ID: &str = "label_key_not_forwarded";
pub const UNRESOLVED_LABEL_BAG_RULE_ID: &str = "unresolved_label_bag";
pub const DEAD_ALERT_REFERENCE_RULE_ID: &str = "dead_alert_reference";
pub const CLIENT_ALERT_GROUPS_BY_INSTANCE_RULE_ID: &str = "client_alert_groups_by_instance";
pub const FORWARDED_KEY_NOT_KEPT_RULE_ID: &str = "forwarded_key_not_kept";
pub const IDENTITY_KEY_FORWARDED_RULE_ID: &str = "identity_key_forwarded";
pub const TRIPWIRE_RATE_WRAPPED_RULE_ID: &str = "tripwire_rate_wrapped";
pub const TRIPWIRE_LIST_STALE_RULE_ID: &str = "tripwire_list_stale";
pub const REJECT_TOKEN_UNCLASSIFIED_RULE_ID: &str = "reject_token_unclassified";
pub const REJECT_TOKEN_MISCLASSIFIED_RULE_ID: &str = "reject_token_misclassified";
pub const EXPORTER_SUFFIXING_ENABLED_RULE_ID: &str = "exporter_suffixing_enabled";

/// Labels present on a stored client series that the collector's `keep_keys`
/// never sees, because they are added AFTER it: `job`/`instance` are the
/// scrape target labels of the collector (`honor_labels: false`, so on a
/// client series `instance` names the collector pod — label-taxonomy.md §R4),
/// `le` is the histogram bucket bound the Prometheus exporter synthesises,
/// `otel_scope_*` are the exporter's instrumentation-scope labels, and
/// `__name__` is the metric name itself. No existing dt-guard or env-tests
/// enumeration of this set exists to reuse (searched 2026-09-30).
///
/// A rule-2 finding is NEVER fixed by widening `keep_keys` to make a label
/// "survive" — it is fixed by the alert selecting a label that exists.
pub const EXPORTER_SYNTHESISED_LABELS: &[&str] = &[
    "job",
    "instance",
    "le",
    "otel_scope_name",
    "otel_scope_version",
    "__name__",
];

/// Client counters that CARRY a presence-form tripwire alert.
///
/// Every arm of each is created lazily: the series is absent until its first
/// increment, and the SDK exports DELTA temporality only when something was
/// recorded (OTel sdk-metrics `DeltaMetricProcessor`). So the first sample
/// Prometheus ever sees is already `>= 1`, and `increase()`/`rate()` — which
/// need a prior sample to difference against — never see the first event. No
/// loaded rule may wrap any of them in a differencing function; their alerts
/// use the presence form `sum(X{...}) > 0`.
///
/// Deliberately NAME-level: this says nothing about how often the counters
/// increment, only how any rule over them must be shaped. It is a SHAPE guard
/// over the loaded rule exprs, not a behavioural test — there is no
/// promtool/PromQL execution lane, and the appear-at-1 claim is unverified by
/// execution here.
pub const TRIPWIRE_CLIENT_COUNTERS: &[&str] = &[
    "dt_client_media_kek_retention_violations_total",
    "dt_client_media_kek_install_refusals_total",
    "dt_client_media_roster_key_rebinds_total",
];

/// PromQL functions that difference a counter and therefore miss the first
/// sample of a lazily created series.
const COUNTER_DIFF_FUNCS: &[&str] = &["increase", "rate", "irate", "delta", "idelta"];

/// Frame-dropping reject tokens that are deliberately NOT key-delivery
/// failures, each with the reason it is excluded from
/// `MCMediaMissingKeyMaterial`'s numerator.
///
/// G7 asserts that every `drops_frame: true` token in [`VECTORS_JSON`] is in
/// EXACTLY ONE of this list or the alert's parsed reason alternation. Deriving
/// membership from the vectors' `layer` field was considered and REJECTED:
/// `layer` names the processing stage, not the remedy owner — `unwrap_failed`
/// is `layer=crypto` yet is key delivery. The partition instead makes every
/// new token force a decision.
pub const NOT_KEY_DELIVERY: &[(&str, &str)] = &[
    ("unknown_version", CODEC_SHAPE),
    ("reserved_flag_bit_set", CODEC_SHAPE),
    ("payload_length_exceeds_max", CODEC_SHAPE),
    ("payload_length_exceeds_available", CODEC_SHAPE),
    ("truncated", CODEC_SHAPE),
    ("extensions_too_large", CODEC_SHAPE),
    ("extensions_malformed", CODEC_SHAPE),
    ("trailing_bytes", CODEC_SHAPE),
    (
        "no_transmit_key",
        "codec: neither a cached key nor a usable wrap on the frame — a protocol violation \
         (audio frames are always key-bearing, packages/sdk-core/src/media/pipeline/egress.ts), \
         not a delivery gap",
    ),
    (
        "signature_invalid",
        "integrity/attribution failure: transit corruption or forgery lands here (the signature \
         covers the wrap block); a security signal with its own triage, not a delivery gap",
    ),
    (
        "decrypt_failed",
        "SFrame payload AEAD failure AFTER a successful unwrap: a sender key-schedule defect, \
         not delivery",
    ),
    (
        "replay_detected",
        "replay-window rejection: duplicate or replayed frame, not delivery",
    ),
    (
        "sender_not_assigned",
        "misrouting: a verified frame from a sender outside the client's MC slot assignment; \
         remedy is MC placement / MH forwarding (docs/observability/metrics/client.md)",
    ),
];

const CODEC_SHAPE: &str =
    "codec: malformed or non-conforming frame shape; a parse failure, not a key-delivery gap";

/// Files that IMPLEMENT the `MetricsSink` interface. Inside them a
/// `.counter(`/`.gauge(`/`.histogram(` call with a non-literal name is the sink
/// forwarding its caller's name, not an emission — the only exemption from the
/// non-literal-name rule in [`check_label_bags`].
pub const SINK_IMPLEMENTATION_FILES: &[&str] = &[
    "packages/sdk-core/src/telemetry/MetricsSink.ts",
    "packages/sdk-core/src/telemetry/OtelMetricsSink.ts",
    "packages/sdk-core/src/telemetry/ConsoleMetricsSink.ts",
    "packages/sdk-core/src/telemetry/NoopMetricsSink.ts",
];

/// A private method that wraps a sink call and injects a label bag of its own.
#[derive(Debug, Clone, Copy)]
pub struct DeclaredWrapper {
    /// Repo-relative file the method lives in.
    pub file: &'static str,
    /// Method name as written, e.g. `#counter`.
    pub method: &'static str,
    /// The expression the wrapper spreads into every call's label bag. Its keys
    /// are resolved from the `<injects> = { … }` object-literal assignments in
    /// the same file (union, ignoring the self-spread).
    pub injects: &'static str,
}

/// Every wrapper through which a `dt_client_*` name may reach a sink with a
/// non-literal name argument. A call through one of these
/// (`this.#counter('dt_client_x', {...})`) is checked against the caller's bag
/// ∪ the wrapper's injected bag; any other non-literal-name sink call is
/// `unresolved_label_bag`.
pub const DECLARED_WRAPPERS: &[DeclaredWrapper] = &[
    DeclaredWrapper {
        file: MEETING_SESSION_TS,
        method: "#counter",
        injects: "this.#metricLabels",
    },
    DeclaredWrapper {
        file: MEETING_SESSION_TS,
        method: "#histogram",
        injects: "this.#metricLabels",
    },
];

/// The media-path base label bag in [`MEDIA_METRICS_TS`].
const MEDIA_BASE_EXPR: &str = "this.#base";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static EXPORTED_MARKER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*(?:[-*]\s*)?(?:\*\*)?Exported(?:\*\*)?\s*:\s*(yes|no)\b")
        .expect("static pattern compiles")
});

/// A whole string-literal BODY that is a client metric name. Applied to the
/// bodies of `'…'`, `"…"` and interpolation-free `` `…` `` literals from
/// [`crate::common::ts_lex`], so all three quote styles are read.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static DT_CLIENT_LITERAL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^dt_client_[a-z0-9_]+$").expect("static pattern compiles"));

/// A whole string-literal BODY that is a vocabulary token (any quote style).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static TS_TOKEN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z][a-z0-9_]*$").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static QUOTED_STR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""([^"]+)""#).expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MATCH_ARM_LITERAL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"Self::[A-Za-z0-9_]+\s*=>\s*"([^"]+)""#).expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ALL_LEN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"ALL:\s*\[Self;\s*(\d+)\s*\]").expect("static pattern compiles"));

/// `pub const NAME: &str = "value";` in a Rust source file.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static RUST_STR_CONST_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?m)^\s*pub\s+const\s+([A-Z][A-Z0-9_]*)\s*:\s*&(?:'static\s+)?str\s*=\s*"([^"]*)"\s*;"#,
    )
    .expect("static pattern compiles")
});

/// A direct `MetricsSink` method call in TS source.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SINK_CALL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\.(?:counter|gauge|histogram)\s*\(").expect("static pattern compiles")
});

/// A `dt_client_*` name in a PromQL expr.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PROMQL_DT_CLIENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\bdt_client_[a-z0-9_]+\b").expect("static pattern compiles"));

/// A `dt_client_*` selector with a matcher block (group 2 = matcher body).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PROMQL_DT_CLIENT_SELECTOR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(dt_client_[a-z0-9_]+)\s*\{([^}]*)\}").expect("static pattern compiles")
});

/// A range window directly on a `dt_client_*` selector (group 1 = duration).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PROMQL_DT_CLIENT_RANGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bdt_client_[a-z0-9_]+\s*(?:\{[^}]*\})?\s*\[\s*((?:\d+[smhdwy])+)\s*\]")
        .expect("static pattern compiles")
});

/// A label-list modifier: `by (a, b)` etc. (group 1 = modifier, 2 = labels).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PROMQL_GROUPING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(by|without|on|ignoring|group_left|group_right)\s*\(([^)]*)\)")
        .expect("static pattern compiles")
});

/// The key of one label matcher.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PROMQL_MATCHER_KEY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?:=~|!~|!=|=)").expect("static pattern compiles")
});

/// The `reason=~"a|b|c"` alternation in an expr (group 1 = alternation).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static REASON_ALTERNATION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\breason\s*=~\s*"([^"]*)""#).expect("static pattern compiles"));

// -----------------------------------------------------------------------------
// Findings.
// -----------------------------------------------------------------------------

/// One guard finding. `rule_id` is one of the `*_RULE_ID` consts.
#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub rule_id: &'static str,
    pub message: String,
}

impl Finding {
    fn new(file: &str, rule_id: &'static str, message: String) -> Self {
        Self {
            file: file.to_string(),
            rule_id,
            message,
        }
    }

    fn print(&self, explain: bool) {
        if explain {
            let policy = format!("client-metrics-export::{}", self.rule_id);
            crate::common::explain::print_finding(&crate::common::explain::Finding {
                file: &self.file,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &self.message,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!("VIOLATION {}: {}", self.file, self.message);
        }
    }
}

/// An empty-parse failure. Kept separate from content findings by construction:
/// callers build it through this helper so the reason token cannot drift.
fn empty_input(file: &str, what: &str) -> Finding {
    Finding {
        file: file.to_string(),
        rule_id: EMPTY_INPUT_RULE_ID,
        message: format!(
            "extractor found no {what} in {file} — this is an EXTRACTION failure, \
             NOT a clean result. Do not loosen the extractor to make it pass; find \
             out what moved."
        ),
    }
}

/// A required input that does not exist at all.
fn missing_input(path: &str) -> Finding {
    empty_input(
        path,
        "file at all (required input is missing — there is no 'nothing to check' result)",
    )
}

/// Read a required repo-relative file. `NotFound` is a [`missing_input`]
/// finding; every other I/O error propagates with the path attached.
fn read_required(root: &Path, rel: &str, findings: &mut Vec<Finding>) -> Result<Option<String>> {
    match std::fs::read_to_string(root.join(rel)) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            findings.push(missing_input(rel));
            Ok(None)
        }
        Err(e) => Err(e).with_context(|| format!("read {rel}")),
    }
}

// -----------------------------------------------------------------------------
// Rust array / enum-arm extraction.
//
// SHARED MECHANISM, PER-CALLER POLICY. This helper PARSES ONLY. Every caller
// keeps its OWN count assertion and its OWN empty-parse reason, deliberately:
// the counts are DIFFERENT FACTS that merely sit behind one parser (GC's two
// allowlists' declared `[&str; N]` on one side, `MediaDropReason`'s
// compile-checked `ALL` length on the other), so collapsing them would be a
// false SSoT over unrelated values. `crates/dt-guard/src/common/metric_catalog.rs`
// documents the same hazard for a shared regex: two callers reading one parser
// in opposite directions means a degraded parse "fails LOUD in one and SILENT
// in the other". If you are here to hoist the duplicated count checks — that is
// the thing this comment exists to stop.
// -----------------------------------------------------------------------------

/// Drop `//` and `/* */` comments from Rust source, line by line, through the
/// shared Rust lexer (`common::test_code_filter::strip_comments`), carrying the
/// block-comment state across lines.
fn strip_rust_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_block = false;
    for line in src.lines() {
        let (stripped, still_in_block) = strip_comments(line, in_block);
        in_block = still_in_block;
        out.push_str(&stripped);
        out.push('\n');
    }
    out
}

/// Collect the entries of a `const NAME: [&str; N] = [ ... ];` block, plus the
/// declared `N`. `N` is compile-checked on the source side, which makes it the
/// structural positive control for the parse.
fn rust_str_array(src: &str, const_name: &str) -> (Vec<String>, Option<usize>) {
    let src = strip_rust_comments(src);
    let Some(start) = src.find(&format!("{const_name}: [&str;")) else {
        return (Vec::new(), None);
    };
    let ty_start = start + const_name.len() + ": [&str;".len();
    let declared = src
        .get(ty_start..)
        .and_then(|t| t.find(']').and_then(|e| t.get(..e)))
        .and_then(|n| n.trim().parse::<usize>().ok());
    let Some(eq) = src[start..].find('=').map(|i| start + i) else {
        return (Vec::new(), declared);
    };
    let Some(body_start) = src[eq..].find('[').map(|i| eq + i) else {
        return (Vec::new(), declared);
    };
    let Some(body_end) = src[body_start..].find("];").map(|i| body_start + i) else {
        return (Vec::new(), declared);
    };
    let body = &src[body_start..body_end];
    // COMMA-SPLIT, NOT LINE-SPLIT. `cargo fmt` reflows a short array onto one
    // line, so anything keyed on "one entry per line" silently under-counts
    // after a formatting pass. Comments are stripped FIRST, so a comma inside a
    // comment cannot split an entry.
    let mut out = Vec::new();
    for raw in body.trim_start_matches('[').split(',') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(m) = QUOTED_STR_RE.captures(token).and_then(|c| c.get(1)) {
            out.push(m.as_str().to_string());
            continue;
        }
        // A bare identifier entry (a `const` imported from another module) is a
        // real element this parser cannot read in place. Record it as a
        // placeholder rather than skipping it, so the count check cannot be
        // fooled, and so the caller must RESOLVE it or fail.
        if token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            out.push(format!("<const:{token}>"));
        }
    }
    (out, declared)
}

/// `pub const NAME: &str = "value";` pairs in a Rust source file.
fn rust_str_consts(src: &str) -> HashMap<String, String> {
    RUST_STR_CONST_RE
        .captures_iter(src)
        .filter_map(|c| {
            Some((
                c.get(1)?.as_str().to_string(),
                c.get(2)?.as_str().to_string(),
            ))
        })
        .collect()
}

/// Collect the string literals on the right of `Self::Variant => "literal",`
/// match arms.
fn rust_match_arm_literals(src: &str, enum_hint: &str) -> Vec<String> {
    let Some(start) = src.find(enum_hint) else {
        return Vec::new();
    };
    MATCH_ARM_LITERAL_RE
        .captures_iter(&src[start..])
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// The declared length of the first `const ALL: [Self; N]` array AFTER
/// `enum_hint` — a compile-checked number, which is what makes it a structural
/// positive control rather than a hand-picked token. Scoped to the enum: a
/// file with several enums would otherwise compare against the wrong `ALL`.
fn declared_all_len(src: &str, enum_hint: &str) -> Option<usize> {
    let start = src.find(enum_hint)?;
    ALL_LEN_RE
        .captures(&src[start..])?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

// -----------------------------------------------------------------------------
// Collector-config extraction.
// -----------------------------------------------------------------------------

/// The metric names in the `filter/client_metric_names` strict include list.
fn collector_metric_names(cfg: &str) -> Vec<String> {
    let Some(start) = cfg.find("filter/client_metric_names:") else {
        return Vec::new();
    };
    let tail = &cfg[start..];
    // Line-anchored, NOT a substring search: the section header
    // `filter/client_metric_names:` itself ENDS with `metric_names:`, so a bare
    // `find` matches the header and the list is read as empty — which would look
    // exactly like an intentionally empty allowlist.
    let Some(list_start) = tail.lines().position(|l| l.trim() == "metric_names:") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in tail.lines().skip(list_start + 1) {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("- ") {
            let name = name.trim();
            if name.starts_with("dt_client_") {
                out.push(name.to_string());
                continue;
            }
        }
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        break;
    }
    out
}

/// The label keys in the metrics-path `keep_keys(...)` call.
fn collector_keep_keys(cfg: &str) -> Vec<String> {
    let Some(start) = cfg.find("keep_keys(attributes,") else {
        return Vec::new();
    };
    let tail = &cfg[start..];
    let Some(end) = tail.find(')') else {
        return Vec::new();
    };
    QUOTED_STR_RE
        .captures_iter(&tail[..end])
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// A `key: value` scalar in the collector config (first occurrence), with any
/// trailing `# comment` removed.
fn collector_scalar(cfg: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    cfg.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(&prefix)?;
        Some(rest.split('#').next().unwrap_or("").trim().to_string())
    })
}

/// A `key: <duration>` scalar under the collector config, as seconds.
fn collector_duration(cfg: &str, key: &str) -> Option<u64> {
    let prefix = format!("{key}:");
    cfg.lines()
        .find_map(|line| line.trim().strip_prefix(&prefix).map(str::to_owned))
        .and_then(|rest| parse_prometheus_duration(&rest))
}

// -----------------------------------------------------------------------------
// Catalog extraction.
// -----------------------------------------------------------------------------

/// `(name -> Some(exported))` for every `### \`dt_client_*\`` heading; `None`
/// when the metric has no explicit `Exported:` marker.
fn catalog_export_markers(src: &str) -> Vec<(String, Option<bool>)> {
    let mut out: Vec<(String, Option<bool>)> = Vec::new();
    let mut current: Option<usize> = None;
    for line in src.lines() {
        if let Some(caps) = CATALOG_HEAD_RE.captures(line) {
            if let Some(name) = caps.get(1) {
                out.push((name.as_str().to_string(), None));
                current = Some(out.len() - 1);
            }
            continue;
        }
        if let (Some(idx), Some(caps)) = (current, EXPORTED_MARKER_RE.captures(line)) {
            if let (Some(entry), Some(v)) = (out.get_mut(idx), caps.get(1)) {
                entry.1 = Some(v.as_str().eq_ignore_ascii_case("yes"));
            }
        }
    }
    out
}

// -----------------------------------------------------------------------------
// Identity-label policy (label-taxonomy.md §R4 fenced block).
// -----------------------------------------------------------------------------

/// The identity vocabulary, parsed from the ```` ```identity-label-policy ````
/// block in `docs/observability/label-taxonomy.md` — that block is the SSoT and
/// this guard reads it at runtime (no const copy here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityPolicy {
    /// Nouns matched as substrings of the lower-cased key.
    pub containment: Vec<String>,
    /// Short tokens matched only as a whole `_`/`.`/camelCase segment.
    pub segment: Vec<String>,
    /// Exact keys that never match.
    pub exempt: Vec<String>,
}

impl IdentityPolicy {
    /// Parse the fenced block out of the taxonomy markdown. `None` when the
    /// block is absent, a line is not `name = a, b`, a name is unknown, a
    /// required list is missing/empty, or the positive control (`participant`
    /// in `containment`) fails — every one of those is an extraction failure.
    #[must_use]
    pub fn parse_doc(md: &str) -> Option<Self> {
        let mut lines = md
            .lines()
            .skip_while(|l| l.trim() != "```identity-label-policy");
        lines.next()?;
        let mut containment = None;
        let mut segment = None;
        let mut exempt = None;
        let mut closed = false;
        for line in lines {
            let t = line.trim();
            if t == "```" {
                closed = true;
                break;
            }
            if t.is_empty() {
                continue;
            }
            let (name, list) = t.split_once('=')?;
            let items: Vec<String> = list
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let slot = match name.trim() {
                "containment" => &mut containment,
                "segment" => &mut segment,
                "exempt" => &mut exempt,
                _ => return None,
            };
            *slot = Some(items);
        }
        let policy = Self {
            containment: containment?,
            segment: segment?,
            exempt: exempt?,
        };
        let ok = closed
            && !policy.segment.is_empty()
            && policy.containment.iter().any(|c| c == IDENTITY_CANARY);
        ok.then_some(policy)
    }

    /// The vocabulary term `key` matches, or `None` if it is identity-free.
    ///
    /// `exempt` is an exact-key match; `containment` nouns match anywhere in
    /// the lower-cased key; `segment` tokens match a whole segment after
    /// splitting on every non-alphanumeric character and camelCase boundaries
    /// (`crate::ts_retained_credentials::segments`, which splits BEFORE
    /// lowering, or the camelCase boundary is gone).
    ///
    /// These semantics MUST match the "Matcher semantics" paragraph of
    /// `docs/observability/label-taxonomy.md` §R4 and the env-tests kernel
    /// (`crates/env-tests/src/fixtures/metric_hygiene.rs`); the shared
    /// ```` ```identity-label-cases ```` table in that section is run against
    /// both.
    #[must_use]
    pub fn matched_term(&self, key: &str) -> Option<String> {
        if self.exempt.iter().any(|e| e == key) {
            return None;
        }
        let lower = key.to_lowercase();
        if let Some(n) = self.containment.iter().find(|n| lower.contains(n.as_str())) {
            return Some(n.clone());
        }
        let segs = segments(key);
        self.segment
            .iter()
            .find(|t| segs.iter().any(|s| s == *t))
            .cloned()
    }
}

// -----------------------------------------------------------------------------
// TypeScript call / label-bag extraction.
// -----------------------------------------------------------------------------

/// Offset of the bracket closing the one at `open`, skipping string spans.
fn matching_close(lexed: &Lexed, open: usize) -> Option<usize> {
    let bytes = lexed.text.as_bytes();
    let mut depth = 0i64;
    let mut i = open;
    while let Some(&b) = bytes.get(i) {
        if let Some(s) = lexed.string_starting_at(i) {
            i = s.end;
            continue;
        }
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Split `[start, end)` at depth-0 occurrences of `sep`, skipping strings.
fn split_top_level(lexed: &Lexed, start: usize, end: usize, sep: u8) -> Vec<(usize, usize)> {
    let bytes = lexed.text.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i64;
    let mut seg = start;
    let mut i = start;
    while i < end {
        if let Some(s) = lexed.string_starting_at(i) {
            i = s.end;
            continue;
        }
        match bytes.get(i) {
            Some(b'(' | b'[' | b'{') => depth += 1,
            Some(b')' | b']' | b'}') => depth -= 1,
            Some(b) if *b == sep && depth == 0 => {
                out.push((seg, i));
                seg = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push((seg, end.min(bytes.len())));
    out
}

/// End of the call argument starting at `from`: the offset of the depth-0 `,`
/// or of the `)` closing the call.
fn arg_end(lexed: &Lexed, from: usize) -> Option<usize> {
    let bytes = lexed.text.as_bytes();
    let mut depth = 0i64;
    let mut i = from;
    while let Some(&b) = bytes.get(i) {
        if let Some(s) = lexed.string_starting_at(i) {
            i = s.end;
            continue;
        }
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    return (b == b')').then_some(i);
                }
                depth -= 1;
            }
            b',' if depth == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_ws(text: &str, mut i: usize) -> usize {
    let bytes = text.as_bytes();
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

fn trimmed_range(text: &str, start: usize, end: usize) -> (usize, usize) {
    let s = text.get(start..end).unwrap_or("");
    let lead = s.len() - s.trim_start().len();
    let trail = s.len() - s.trim_end().len();
    (start + lead, (end - trail).max(start + lead))
}

const NON_CALL_KEYWORDS: &[&str] = &[
    "if", "for", "while", "switch", "catch", "return", "typeof", "function", "in", "of",
];

/// A call whose FIRST argument is a string literal.
#[derive(Debug, Clone)]
struct CallSite {
    /// Callee text, e.g. `this.#sink?.counter` or `this.#counter`.
    callee: String,
    line: usize,
    /// The second argument's (trimmed) range, if any.
    label_arg: Option<(usize, usize)>,
}

/// If `lit` is the first argument of a call, describe that call.
fn call_for_first_arg(lexed: &Lexed, lit: &StrLit) -> Option<CallSite> {
    let text = &lexed.text;
    let before = text.get(..lit.start)?.trim_end();
    let paren_prefix = before.strip_suffix('(')?.trim_end();
    let paren_prefix = paren_prefix.strip_suffix("?.").unwrap_or(paren_prefix);
    let callee_len = paren_prefix
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '#' | '.' | '?'))
        .map(char::len_utf8)
        .sum::<usize>();
    let callee = paren_prefix.get(paren_prefix.len() - callee_len..)?;
    if callee.is_empty() || NON_CALL_KEYWORDS.contains(&callee) {
        return None;
    }
    let after = skip_ws(text, lit.end);
    let label_arg = match text.as_bytes().get(after) {
        Some(b')') => None,
        Some(b',') => {
            let a_start = after + 1;
            let a_end = arg_end(lexed, a_start)?;
            let (s, e) = trimmed_range(text, a_start, a_end);
            (s < e).then_some((s, e))
        }
        _ => return None,
    };
    Some(CallSite {
        callee: callee.to_string(),
        line: lexed.line_of(lit.start),
        label_arg,
    })
}

/// A label bag as written, before spread resolution.
#[derive(Debug, Default)]
struct RawBag {
    keys: BTreeSet<String>,
    /// Spread / bare-identifier expressions, e.g. `this.#base`.
    spreads: Vec<String>,
    /// Members the parser cannot read (computed keys, methods, …).
    problems: Vec<String>,
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn is_member_path(s: &str) -> bool {
    !s.is_empty()
        && s.split('.')
            .all(|p| is_ident(p.trim_start_matches('#').trim_end_matches('?')))
}

/// Parse the label-bag expression at `[start, end)`: an object literal
/// (`key: v`, `'k': v`, `"k": v`, shorthand `k`, spreads `...x`) or a bare
/// member path (`this.#base`).
fn parse_raw_bag(lexed: &Lexed, start: usize, end: usize) -> RawBag {
    let text = &lexed.text;
    let mut bag = RawBag::default();
    let whole = text.get(start..end).unwrap_or("").trim();
    if !(whole.starts_with('{') && whole.ends_with('}')) {
        if is_member_path(whole) {
            bag.spreads.push(whole.to_string());
        } else {
            bag.problems.push(format!(
                "label bag `{whole}` is not an object literal or a member path"
            ));
        }
        return bag;
    }
    let (inner_s, inner_e) = (start + 1, end.saturating_sub(1));
    for (ms, me) in split_top_level(lexed, inner_s, inner_e, b',') {
        let (ms, me) = trimmed_range(text, ms, me);
        let member = text.get(ms..me).unwrap_or("");
        if member.is_empty() {
            continue;
        }
        if let Some(rest) = member.strip_prefix("...") {
            let rest = rest.trim();
            if is_member_path(rest) {
                bag.spreads.push(rest.to_string());
            } else {
                bag.problems.push(format!("spread `...{rest}`"));
            }
            continue;
        }
        let colon = split_top_level(lexed, ms, me, b':');
        if colon.len() >= 2 {
            let Some(&(ks, ke)) = colon.first() else {
                continue;
            };
            let (ks, ke) = trimmed_range(text, ks, ke);
            let key_src = text.get(ks..ke).unwrap_or("");
            if let Some(lit) = lexed
                .string_starting_at(ks)
                .filter(|l| l.end == ke && l.is_static())
            {
                bag.keys.insert(lit.body(text).to_string());
            } else if is_ident(key_src) {
                bag.keys.insert(key_src.to_string());
            } else {
                bag.problems
                    .push(format!("computed or non-literal key `{key_src}`"));
            }
        } else if is_ident(member) {
            // Shorthand property: `{ reason }` is `{ reason: reason }`.
            bag.keys.insert(member.to_string());
        } else {
            bag.problems.push(format!("unreadable member `{member}`"));
        }
    }
    bag
}

/// Resolve a raw bag's spreads against the file-scoped resolutions. Spreads
/// naming one of `caller_params` (a wrapper's own label parameter) are the
/// caller's bag and contribute nothing here.
fn resolve_bag(
    raw: &RawBag,
    file: &str,
    resolutions: &HashMap<(String, String), BTreeSet<String>>,
    caller_params: &[String],
    ignore_spread: Option<&str>,
) -> std::result::Result<BTreeSet<String>, String> {
    if let Some(p) = raw.problems.first() {
        return Err(p.clone());
    }
    let mut keys = raw.keys.clone();
    for s in &raw.spreads {
        if caller_params.iter().any(|p| p == s) || ignore_spread == Some(s.as_str()) {
            continue;
        }
        match resolutions.get(&(file.to_string(), s.clone())) {
            Some(k) => keys.extend(k.iter().cloned()),
            None => return Err(format!("spread/bag `{s}` cannot be resolved in {file}")),
        }
    }
    Ok(keys)
}

/// The keys of the object literal returned by `export function
/// mediaMetricLabels` — what `this.#base` holds in [`MEDIA_METRICS_TS`].
fn media_base_keys(lexed: &Lexed) -> Option<BTreeSet<String>> {
    let text = &lexed.text;
    let fn_at = text.find(MEDIA_LABELS_FN)?;
    let body_open = fn_at + text.get(fn_at..)?.find('{')?;
    // The parameter type annotation may itself contain braces; the function
    // body is the first `{` after the parameter list's closing `)`.
    let params_open = fn_at + text.get(fn_at..)?.find('(')?;
    let params_close = matching_close(lexed, params_open)?;
    let body_open = if body_open < params_close {
        params_close + text.get(params_close..)?.find('{')?
    } else {
        body_open
    };
    let body_close = matching_close(lexed, body_open)?;
    let ret = body_open + text.get(body_open..body_close)?.find("return")?;
    let obj_open = ret + text.get(ret..body_close)?.find('{')?;
    let obj_close = matching_close(lexed, obj_open)?;
    let raw = parse_raw_bag(lexed, obj_open, obj_close + 1);
    if !raw.problems.is_empty() || !raw.spreads.is_empty() {
        return None;
    }
    Some(raw.keys)
}

/// Assignment operators, longest first. Plain `=` is handled by the caller
/// (it must not be `==`, `===` or `=>`).
const COMPOUND_ASSIGN_OPS: &[&str] = &[
    ">>>=", "**=", "<<=", ">>=", "&&=", "||=", "??=", "+=", "-=", "*=", "/=", "%=", "&=", "|=",
    "^=",
];

/// The assignment operator at `i`, if any: `Some("=")` for a plain assignment,
/// `Some(op)` for a compound one, `None` for anything else (including the
/// comparisons `==`/`===` and the arrow `=>`).
fn assign_op_at(text: &str, i: usize) -> Option<&'static str> {
    let rest = text.get(i..)?;
    if let Some(op) = COMPOUND_ASSIGN_OPS.iter().find(|op| rest.starts_with(**op)) {
        return Some(op);
    }
    let mut chars = rest.chars();
    (chars.next() == Some('=') && !matches!(chars.next(), Some('=' | '>'))).then_some("=")
}

/// One place `expr` is written to.
#[derive(Debug)]
enum WriteKind {
    /// `expr = <rhs>`; the offset is the first non-space byte of the RHS.
    Assign(usize),
    /// A write the resolvers cannot follow (member write, compound assignment,
    /// `Object.assign(expr, …)`), with a description.
    Opaque(String),
}

/// Every WRITE to `expr` in the file: plain or compound assignment to `expr`
/// itself, assignment (plain or compound) to a member of it (`expr.x = …`,
/// `expr[k] = …`), and `expr` as the first argument of `Object.assign(…)`.
/// Reads (`...expr`, `expr` passed as an argument, `expr === x`) are not
/// writes. Returns `(line, kind)` pairs.
fn target_writes(lexed: &Lexed, expr: &str) -> Vec<(usize, WriteKind)> {
    let text = &lexed.text;
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_' || c == '$' || c == '#';
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = text.get(from..).and_then(|t| t.find(expr)) {
        let at = from + rel;
        from = at + expr.len();
        if lexed.string_at(at).is_some()
            || text
                .get(from..)
                .and_then(|t| t.chars().next())
                .is_some_and(is_ident_char)
            || text
                .get(..at)
                .and_then(|t| t.chars().last())
                .is_some_and(|c| is_ident_char(c) || c == '.')
        {
            continue;
        }
        let line = lexed.line_of(at);
        if text.get(..at).is_some_and(|t| {
            t.trim_end()
                .strip_suffix('(')
                .is_some_and(|t| t.trim_end().ends_with("Object.assign"))
        }) {
            out.push((
                line,
                WriteKind::Opaque(format!("`Object.assign({expr}, …)`")),
            ));
            continue;
        }
        // Walk a member chain: `.x`, `?.x`, `[k]`.
        let mut i = from;
        let mut member = false;
        loop {
            let j = skip_ws(text, i);
            let rest = text.get(j..).unwrap_or("");
            let dot = if rest.starts_with("?.") {
                2
            } else if rest.starts_with('.') {
                1
            } else {
                0
            };
            if dot > 0 {
                let k = skip_ws(text, j + dot);
                let len: usize = text
                    .get(k..)
                    .unwrap_or("")
                    .chars()
                    .take_while(|c| is_ident_char(*c))
                    .map(char::len_utf8)
                    .sum();
                if len == 0 {
                    break;
                }
                i = k + len;
                member = true;
            } else if rest.starts_with('[') {
                let Some(close) = matching_close(lexed, j) else {
                    break;
                };
                i = close + 1;
                member = true;
            } else {
                break;
            }
        }
        let op_at = skip_ws(text, i);
        match assign_op_at(text, op_at) {
            None => {}
            Some(op) if member => out.push((
                line,
                WriteKind::Opaque(format!(
                    "member write `{expr}….{op}` (`{expr}.x {op} …` / `{expr}[k] {op} …`)"
                )),
            )),
            Some("=") => out.push((line, WriteKind::Assign(skip_ws(text, op_at + 1)))),
            Some(op) => out.push((
                line,
                WriteKind::Opaque(format!("compound assignment `{expr} {op} …`")),
            )),
        }
    }
    out
}

/// The source text of a statement starting at `start`, up to `;` or a
/// newline, for messages.
fn statement_text(text: &str, start: usize) -> String {
    let rest = text.get(start..).unwrap_or("");
    let end = rest.find([';', '\n']).unwrap_or(rest.len());
    let s = rest.get(..end).unwrap_or(rest).trim();
    if s.chars().count() > 80 {
        format!("{}…", s.chars().take(80).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Soundness of the `this.#base` resolution: every write to `this.#base` in [`MEDIA_METRICS_TS`]
/// must be exactly `this.#base = mediaMetricLabels(<args>)` — only then is resolving it to that
/// function's returned keys sound. Returns the count of conforming assignments and a description
/// (with line) of every other write.
fn media_base_writes(lexed: &Lexed) -> (usize, Vec<(usize, String)>) {
    let text = &lexed.text;
    let fn_name = MEDIA_LABELS_FN
        .rsplit(' ')
        .next()
        .unwrap_or(MEDIA_LABELS_FN);
    let mut ok = 0usize;
    let mut bad = Vec::new();
    for (line, kind) in target_writes(lexed, MEDIA_BASE_EXPR) {
        match kind {
            WriteKind::Opaque(d) => bad.push((line, d)),
            WriteKind::Assign(rhs) => {
                let conforming = text
                    .get(rhs..)
                    .and_then(|r| r.strip_prefix(fn_name))
                    .and_then(|_| {
                        let open = skip_ws(text, rhs + fn_name.len());
                        (text.as_bytes().get(open) == Some(&b'(')).then_some(open)
                    })
                    .and_then(|open| matching_close(lexed, open))
                    .is_some_and(|close| {
                        let tail = text.get(close + 1..).unwrap_or("");
                        let tail = tail.trim_start_matches([' ', '\t']);
                        tail.is_empty() || tail.starts_with([';', '\n', '\r', '}'])
                    });
                if conforming {
                    ok += 1;
                } else {
                    bad.push((
                        line,
                        format!(
                            "assignment `{MEDIA_BASE_EXPR} = {}` (must be exactly \
                             `{MEDIA_BASE_EXPR} = {fn_name}(…)`)",
                            statement_text(text, rhs)
                        ),
                    ));
                }
            }
        }
    }
    (ok, bad)
}

/// Every write to an injected label bag must be readable. Returns the union of the literal keys of
/// every `<expr> = { … }` assignment in the file (ignoring the self-spread `...<expr>`), plus a
/// description (with line) of every write that is NOT such an assignment — a non-literal RHS, a
/// member write, a compound assignment, `Object.assign(<expr>, …)`, or an object literal the parser
/// cannot fully read. `None` keys = no `= { … }` assignment at all.
fn assigned_object_keys(
    lexed: &Lexed,
    expr: &str,
) -> (Option<BTreeSet<String>>, Vec<(usize, String)>) {
    let text = &lexed.text;
    let mut keys: Option<BTreeSet<String>> = None;
    let mut bad = Vec::new();
    for (line, kind) in target_writes(lexed, expr) {
        let rhs = match kind {
            WriteKind::Opaque(d) => {
                bad.push((line, d));
                continue;
            }
            WriteKind::Assign(rhs) => rhs,
        };
        let close = (text.as_bytes().get(rhs) == Some(&b'{'))
            .then(|| matching_close(lexed, rhs))
            .flatten();
        let Some(close) = close else {
            bad.push((
                line,
                format!(
                    "non-literal assignment `{expr} = {}`",
                    statement_text(text, rhs)
                ),
            ));
            continue;
        };
        let raw = parse_raw_bag(lexed, rhs, close + 1);
        match resolve_bag(&raw, "", &HashMap::new(), &[], Some(expr)) {
            Ok(k) => keys.get_or_insert_with(BTreeSet::new).extend(k),
            Err(e) => bad.push((line, format!("assignment to `{expr}`: {e}"))),
        }
    }
    (keys, bad)
}

/// A declared wrapper located and resolved in source.
#[derive(Debug, Clone)]
struct ResolvedWrapper {
    decl: DeclaredWrapper,
    /// Body range `[open, close]` in the file's lexed text.
    body: (usize, usize),
    /// Keys the wrapper itself contributes to every call through it.
    injected: std::result::Result<BTreeSet<String>, String>,
}

/// Locate a declared wrapper's method body and resolve its injected bag from
/// the sink call(s) inside it. `None` = the declaration no longer matches the
/// source (an extraction failure).
fn resolve_wrapper(
    lexed: &Lexed,
    decl: DeclaredWrapper,
    resolutions: &HashMap<(String, String), BTreeSet<String>>,
) -> Option<ResolvedWrapper> {
    let text = &lexed.text;
    let mut from = 0usize;
    // Method DEFINITION: `#counter(` not preceded by `.` (that is a call).
    let def = loop {
        let at = from + text.get(from..)?.find(decl.method)?;
        from = at + decl.method.len();
        let prev = text.get(..at)?.chars().last();
        let next = skip_ws(text, from);
        if prev.is_some_and(|c| c == '.' || c.is_alphanumeric() || c == '_')
            || text.as_bytes().get(next) != Some(&b'(')
            || lexed.string_at(at).is_some()
        {
            continue;
        }
        break next;
    };
    let params_close = matching_close(lexed, def)?;
    let params: Vec<String> = split_top_level(lexed, def + 1, params_close, b',')
        .into_iter()
        .filter_map(|(s, e)| {
            let p = text.get(s..e)?.trim();
            let name = p.split([':', '=', '?']).next()?.trim();
            is_ident(name).then(|| name.to_string())
        })
        .collect();
    let open = params_close + text.get(params_close..)?.find('{')?;
    let close = matching_close(lexed, open)?;

    let mut injected: Option<std::result::Result<BTreeSet<String>, String>> = None;
    for m in SINK_CALL_RE.find_iter(text.get(open..close)?) {
        let paren = open + m.end() - 1;
        let first_end = arg_end(lexed, paren + 1)?;
        if text.as_bytes().get(first_end) != Some(&b',') {
            continue;
        }
        let bag_end = arg_end(lexed, first_end + 1)?;
        let (bs, be) = trimmed_range(text, first_end + 1, bag_end);
        let raw = parse_raw_bag(lexed, bs, be);
        if !raw.spreads.iter().any(|s| s == decl.injects) {
            // The declaration says this wrapper injects `decl.injects`; a body
            // that does not is a stale declaration.
            continue;
        }
        let this = resolve_bag(&raw, decl.file, resolutions, &params, None);
        injected = Some(match (injected.take(), this) {
            (None, r) => r,
            (Some(Ok(mut a)), Ok(b)) => {
                a.extend(b);
                Ok(a)
            }
            (Some(Err(e)), _) | (_, Err(e)) => Err(e),
        });
    }
    Some(ResolvedWrapper {
        decl,
        body: (open, close),
        injected: injected?,
    })
}

// -----------------------------------------------------------------------------
// PromQL helpers.
// -----------------------------------------------------------------------------

/// A rule expr prepared for structural scanning.
#[derive(Debug, Clone)]
struct PreparedExpr {
    file: String,
    alert: Option<String>,
    /// `#` comments removed; strings intact.
    code: String,
    /// As `code`, with quoted-string bodies blanked to spaces.
    masked: String,
}

fn prepare_expr(e: &LoadedRuleExpr) -> PreparedExpr {
    let mut code = String::new();
    let mut masked = String::new();
    let mut quote: Option<char> = None;
    let mut chars = e.rule.expr.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                code.push(c);
                if c == '\\' {
                    masked.push(' ');
                    if let Some(n) = chars.next() {
                        code.push(n);
                        masked.push(' ');
                    }
                    continue;
                }
                if c == q {
                    quote = None;
                    masked.push(c);
                } else {
                    masked.push(if c == '\n' { '\n' } else { ' ' });
                }
            }
            None => {
                if c == '#' {
                    while let Some(&n) = chars.peek() {
                        if n == '\n' {
                            break;
                        }
                        chars.next();
                    }
                    continue;
                }
                if matches!(c, '"' | '\'' | '`') {
                    quote = Some(c);
                }
                code.push(c);
                masked.push(c);
            }
        }
    }
    PreparedExpr {
        file: e.file.clone(),
        alert: e.rule.alert.clone(),
        code,
        masked,
    }
}

fn rule_label(e: &PreparedExpr) -> String {
    format!(
        "{ALERTS_SUBDIR}/{} ({})",
        e.file,
        e.alert.as_deref().unwrap_or("recording rule")
    )
}

/// A name referenced by a rule, as the exported base name (histogram series
/// suffixes stripped when the bare name is not itself exported).
fn exported_base<'a>(name: &'a str, exported: &BTreeSet<String>) -> Option<&'a str> {
    if exported.contains(name) {
        return Some(name);
    }
    ["_bucket", "_count", "_sum"]
        .iter()
        .find_map(|sfx| name.strip_suffix(sfx))
        .filter(|b| exported.contains(*b))
}

/// Whole-word occurrences of `name` in `s`.
fn word_occurrences(s: &str, name: &str) -> Vec<usize> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = s.get(from..).and_then(|t| t.find(name)) {
        let at = from + rel;
        from = at + name.len();
        let before_ok = s
            .get(..at)
            .and_then(|p| p.chars().last())
            .is_none_or(|c| !is_word(c));
        let after_ok = s
            .get(from..)
            .and_then(|p| p.chars().next())
            .is_none_or(|c| !is_word(c));
        if before_ok && after_ok {
            out.push(at);
        }
    }
    out
}

/// Names of every function whose argument list encloses offset `pos`.
fn enclosing_functions(s: &str, pos: usize) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i64;
    let mut i = pos;
    while i > 0 {
        i -= 1;
        match bytes.get(i) {
            Some(b')') => depth += 1,
            Some(b'(') => {
                if depth == 0 {
                    let pre = s.get(..i).unwrap_or("").trim_end();
                    let name: String = pre
                        .chars()
                        .rev()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    if !name.is_empty() {
                        out.push(name);
                    }
                } else {
                    depth -= 1;
                }
            }
            _ => {}
        }
    }
    out
}

// -----------------------------------------------------------------------------
// Vectors (G7).
// -----------------------------------------------------------------------------

/// The typed slice of `frame-v2.vectors.json` G7 reads. Unknown fields are
/// allowed; the three named fields are REQUIRED, so a rename fails the parse
/// (and the run) instead of defaulting.
#[derive(Debug, Deserialize)]
struct VectorsFile {
    reject_reasons: Vec<RejectReasonRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct RejectReasonRow {
    token: String,
    layer: String,
    drops_frame: bool,
}

// -----------------------------------------------------------------------------
// Inputs.
// -----------------------------------------------------------------------------

/// GC's forwarded key sets, with `<const:…>` entries resolved.
#[derive(Debug, Clone)]
struct GcSets {
    allowlist: Vec<String>,
    extra: Vec<String>,
}

/// Label-bag expressions resolved from source, keyed by `(file, expr)`.
///
/// Resolution is FILE-SCOPED on purpose: `this.#metricLabels` in
/// `MediaTransport.ts` is a different field from the one in
/// `MeetingSession.ts`, and must stay unresolved there. An entry exists only
/// if EVERY write to the expression in that file is one the resolver followed
/// (see [`media_base_writes`] and [`assigned_object_keys`]).
type Resolutions = HashMap<(String, String), BTreeSet<String>>;

/// Every input, parsed once. A `None` field means its extractor failed and a
/// finding (usually [`EMPTY_INPUT_RULE_ID`]) has already been recorded; checks
/// that need it are skipped.
#[derive(Debug, Default)]
pub struct Inputs {
    markers: Option<Vec<(String, Option<bool>)>>,
    config_names: Option<Vec<String>>,
    keep_keys: Option<Vec<String>>,
    /// `(metric_expiration, max_stale)` in seconds; `None` if either is absent.
    durations: Option<(u64, u64)>,
    gc: Option<GcSets>,
    /// Non-test sdk-core `.ts` sources, comment-stripped, keyed by repo-relative
    /// path.
    sdk: Option<BTreeMap<String, Lexed>>,
    resolutions: Option<Resolutions>,
    wrappers: Option<Vec<ResolvedWrapper>>,
    ts_vocab: Option<HashSet<String>>,
    mh_arms: Option<Vec<String>>,
    rules: Option<Vec<PreparedExpr>>,
    vectors: Option<Vec<RejectReasonRow>>,
    identity: Option<IdentityPolicy>,
}

/// The sdk-core walker: every `*.ts` under [`SDK_SRC`] that is not a test path
/// (`common::test_code_filter::is_test_path`), comment-stripped.
fn walk_sdk(root: &Path) -> Result<BTreeMap<String, Lexed>> {
    let mut out = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root.join(SDK_SRC)).sort_by_file_name() {
        let entry = entry.with_context(|| format!("walk {SDK_SRC}"))?;
        let path = entry.path();
        if !entry.file_type().is_file() || path.extension().is_none_or(|e| e != "ts") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .with_context(|| format!("relativise {}", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        if is_test_path(Path::new(&rel)) {
            continue;
        }
        let src = std::fs::read_to_string(path).with_context(|| format!("read {rel}"))?;
        out.insert(rel, lex(&src));
    }
    Ok(out)
}

/// Static string-literal bodies in a lexed file.
fn static_literals(lexed: &Lexed) -> impl Iterator<Item = (&StrLit, &str)> {
    lexed
        .strings
        .iter()
        .filter(|s| s.is_static())
        .map(move |s| (s, s.body(&lexed.text)))
}

fn load_inputs(root: &Path) -> Result<(Inputs, Vec<Finding>)> {
    let mut f: Vec<Finding> = Vec::new();
    let mut inp = Inputs::default();

    if let Some(src) = read_required(root, CATALOG, &mut f)? {
        let markers = catalog_export_markers(&src);
        if markers.is_empty() {
            f.push(empty_input(CATALOG, "`### `dt_client_*`` metric headings"));
        } else {
            inp.markers = Some(markers);
        }
    }
    if let Some(cfg) = read_required(root, COLLECTOR_CONFIG, &mut f)? {
        load_collector(&cfg, &mut inp, &mut f);
    }
    let gc_src = read_required(root, GC_FILTER, &mut f)?;
    let labels_src = read_required(root, LABELS_RS, &mut f)?;
    if let (Some(gc_src), Some(labels_src)) = (gc_src, labels_src) {
        inp.gc = parse_gc_sets(&gc_src, &labels_src, &mut f);
    }
    if root.join(SDK_SRC).is_dir() {
        let sdk = walk_sdk(root)?;
        if sdk.is_empty() {
            f.push(empty_input(SDK_SRC, "non-test `*.ts` sources"));
        } else {
            inp.ts_vocab = load_ts_vocab(&sdk, &mut f);
            if let Some((res, wrappers)) = load_label_resolutions(&sdk, &mut f) {
                inp.resolutions = Some(res);
                inp.wrappers = Some(wrappers);
            }
            inp.sdk = Some(sdk);
        }
    } else {
        f.push(missing_input(SDK_SRC));
    }
    if let Some(src) = read_required(root, MH_METRICS_RS, &mut f)? {
        let hint = "enum MediaDropReason";
        let arms = rust_match_arm_literals(&src, hint);
        // Per-caller count assertion, structural rather than hand-picked: the
        // declared `ALL: [Self; N]` length is compile-checked on MH's side.
        // A DIFFERENT fact from GC's allowlist lengths — see the helper comment.
        match declared_all_len(&src, hint) {
            Some(declared) if declared > 0 && arms.len() >= declared => inp.mh_arms = Some(arms),
            _ => f.push(empty_input(
                MH_METRICS_RS,
                "MediaDropReason arms matching its declared ALL length",
            )),
        }
    }
    inp.rules = load_rules(root, &mut f)?;
    if let Some(src) = read_required(root, VECTORS_JSON, &mut f)? {
        let parsed: VectorsFile = serde_json::from_str(&src).with_context(|| {
            format!("parse {VECTORS_JSON} (reject_reasons[].token/layer/drops_frame are required)")
        })?;
        if parsed.reject_reasons.iter().any(|r| r.drops_frame) {
            inp.vectors = Some(parsed.reject_reasons);
        } else {
            f.push(empty_input(
                VECTORS_JSON,
                "`drops_frame: true` reject_reasons",
            ));
        }
    }
    if let Some(md) = read_required(root, LABEL_TAXONOMY, &mut f)? {
        match IdentityPolicy::parse_doc(&md) {
            Some(p) => inp.identity = Some(p),
            None => f.push(empty_input(
                LABEL_TAXONOMY,
                &format!(
                    "well-formed ```identity-label-policy block (containment/segment/exempt, \
                     `{IDENTITY_CANARY}` in containment)"
                ),
            )),
        }
    }
    Ok((inp, f))
}

/// Collector config: include list, keep_keys, the two durations, and the
/// `add_metric_suffixes: false` premise.
fn load_collector(cfg: &str, inp: &mut Inputs, f: &mut Vec<Finding>) {
    let names = collector_metric_names(cfg);
    if names.is_empty() {
        f.push(empty_input(
            COLLECTOR_CONFIG,
            "metric names in the filter/client_metric_names include list",
        ));
    } else {
        inp.config_names = Some(names);
    }
    let keep = collector_keep_keys(cfg);
    if keep.is_empty() {
        f.push(empty_input(COLLECTOR_CONFIG, "keys in the keep_keys call"));
    } else {
        inp.keep_keys = Some(keep);
    }
    match (
        collector_duration(cfg, "metric_expiration"),
        collector_duration(cfg, "max_stale"),
    ) {
        (Some(e), Some(s)) => inp.durations = Some((e, s)),
        _ => f.push(Finding::new(
            COLLECTOR_CONFIG,
            EMPTY_INPUT_RULE_ID,
            format!(
                "could not read both `metric_expiration` and `max_stale` from \
                 {COLLECTOR_CONFIG}. These key names have MOVED between collector versions; \
                 a rename must fail here rather than silently skip the relation check."
            ),
        )),
    }
    // Premise of G1, G5 and rule 2: every one of them compares metric NAMES
    // as written. With exporter suffixing on, the stored names gain `_total`
    // / unit suffixes and every dt_client selector silently matches nothing.
    // Absent or empty is an extraction failure (the key may have moved); a
    // present value other than `false` is a content failure with its own id.
    match collector_scalar(cfg, "add_metric_suffixes").filter(|v| !v.is_empty()) {
        Some(v) if v == "false" => {}
        Some(v) => f.push(Finding::new(
            COLLECTOR_CONFIG,
            EXPORTER_SUFFIXING_ENABLED_RULE_ID,
            format!(
                "{COLLECTOR_CONFIG} sets `add_metric_suffixes: {v}` on the prometheus exporter. \
                 Exporter suffixing renames every stored series, which silently kills every \
                 dt_client selector in the alert rules and every name comparison this guard \
                 makes. Set it to `false`."
            ),
        )),
        None => f.push(empty_input(
            COLLECTOR_CONFIG,
            "`add_metric_suffixes:` value on the prometheus exporter (it must be present and \
             `false`; suffixing silently kills every dt_client selector)",
        )),
    }
}

/// G6 TS leg: tokens from the three vocabulary files, with the canary as the
/// positive control.
fn load_ts_vocab(sdk: &BTreeMap<String, Lexed>, f: &mut Vec<Finding>) -> Option<HashSet<String>> {
    let mut tokens = HashSet::new();
    let mut all_present = true;
    for rel in [MEDIA_METRICS_TS, REJECT_REASON_TS, RECEIVE_PATH_TS] {
        match sdk.get(rel) {
            Some(lexed) => tokens.extend(
                static_literals(lexed)
                    .filter(|(_, b)| TS_TOKEN_RE.is_match(b))
                    .map(|(_, b)| b.to_string()),
            ),
            None => {
                all_present = false;
                f.push(missing_input(rel));
            }
        }
    }
    if !all_present {
        return None;
    }
    if tokens.contains(TS_VOCAB_CANARY) {
        return Some(tokens);
    }
    // Positive control: if the extractor cannot see the token the alert
    // itself selects on, it is not seeing the vocabulary.
    f.push(empty_input(
        MEDIA_METRICS_TS,
        &format!("the media vocabularies (canary {TS_VOCAB_CANARY} absent)"),
    ));
    None
}

fn unresolved_write(file: &str, expr: &str, line: usize, what: &str) -> Finding {
    Finding::new(
        file,
        UNRESOLVED_LABEL_BAG_RULE_ID,
        format!(
            "{file}:{line}: {what} — `{expr}` is resolved from source for rule 1, and a \
             write the resolver cannot follow means the keys it resolves to are not the \
             keys emitted. Write it only as the recognised assignment form."
        ),
    )
}

/// Rule 1's resolutions and declared wrappers.
///
/// Soundness of every resolution: a resolved bag expression is only sound if EVERY write to it is
/// one the resolver followed. Any other write is `unresolved_label_bag` here (regardless of export
/// status) and rule 1 is skipped — every resolution downstream of it is untrustworthy, and the
/// finding already fails the run and names the cause.
fn load_label_resolutions(
    sdk: &BTreeMap<String, Lexed>,
    f: &mut Vec<Finding>,
) -> Option<(Resolutions, Vec<ResolvedWrapper>)> {
    let mut res: Resolutions = HashMap::new();
    let mut ok = true;
    if let Some(lexed) = sdk.get(MEDIA_METRICS_TS) {
        let (conforming, bad) = media_base_writes(lexed);
        for (line, what) in &bad {
            f.push(unresolved_write(
                MEDIA_METRICS_TS,
                MEDIA_BASE_EXPR,
                *line,
                what,
            ));
        }
        if !bad.is_empty() {
            ok = false;
        } else if conforming == 0 {
            ok = false;
            f.push(empty_input(
                MEDIA_METRICS_TS,
                &format!("`{MEDIA_BASE_EXPR} = mediaMetricLabels(…)` assignment"),
            ));
        }
        match media_base_keys(lexed) {
            Some(keys) if keys.contains(MEDIA_BASE_CANARY) => {
                res.insert((MEDIA_METRICS_TS.into(), MEDIA_BASE_EXPR.into()), keys);
            }
            _ => {
                ok = false;
                f.push(empty_input(
                    MEDIA_METRICS_TS,
                    &format!(
                        "`{MEDIA_LABELS_FN}` returning a literal object with \
                         `{MEDIA_BASE_CANARY}` (what `{MEDIA_BASE_EXPR}` resolves to)"
                    ),
                ));
            }
        }
    } else {
        ok = false; // reported missing by the G6 leg
    }
    let injects: BTreeSet<(&str, &str)> = DECLARED_WRAPPERS
        .iter()
        .map(|w| (w.file, w.injects))
        .collect();
    for (file, expr) in injects {
        let Some(lexed) = sdk.get(file) else {
            ok = false;
            f.push(missing_input(file));
            continue;
        };
        let (keys, bad) = assigned_object_keys(lexed, expr);
        for (line, what) in &bad {
            f.push(unresolved_write(file, expr, *line, what));
        }
        ok &= bad.is_empty();
        match keys {
            Some(keys) if !keys.is_empty() => {
                res.insert((file.into(), expr.into()), keys);
            }
            _ => {
                ok = false;
                f.push(empty_input(
                    file,
                    &format!("`{expr} = {{ … }}` object-literal assignments"),
                ));
            }
        }
    }
    if !ok {
        return None;
    }
    let mut wrappers = Vec::new();
    for w in DECLARED_WRAPPERS {
        match sdk.get(w.file).and_then(|l| resolve_wrapper(l, *w, &res)) {
            Some(rw) => wrappers.push(rw),
            None => {
                ok = false;
                f.push(empty_input(
                    w.file,
                    &format!(
                        "declared wrapper `{}` with a sink call spreading `...{}`",
                        w.method, w.injects
                    ),
                ));
            }
        }
    }
    ok.then_some((res, wrappers))
}

/// Loaded rule exprs (G5, rule 2, tripwire, G7), with the positive control
/// that at least one references a `dt_client_*` name.
fn load_rules(root: &Path, f: &mut Vec<Finding>) -> Result<Option<Vec<PreparedExpr>>> {
    let alerts_dir = root.join(ALERTS_SUBDIR);
    if !alerts_dir.is_dir() {
        f.push(missing_input(ALERTS_SUBDIR));
        return Ok(None);
    }
    let files = loadable_rules_files(&alerts_dir)?;
    if files.is_empty() {
        f.push(empty_input(
            ALERTS_SUBDIR,
            "loadable `*-alerts.yaml` rule files",
        ));
        return Ok(None);
    }
    let exprs: Vec<PreparedExpr> = load_rule_exprs_from(&alerts_dir, &files)?
        .iter()
        .map(prepare_expr)
        .collect();
    if exprs.iter().any(|e| PROMQL_DT_CLIENT_RE.is_match(&e.code)) {
        return Ok(Some(exprs));
    }
    f.push(empty_input(
        ALERTS_SUBDIR,
        "`dt_client_*` reference in any loaded rule expr",
    ));
    Ok(None)
}

/// Rule 6: parse GC's arrays, check parsed length == declared `N`, resolve
/// `<const:…>` entries from `labels.rs`, and apply named-key positive controls.
fn parse_gc_sets(gc_src: &str, labels_src: &str, f: &mut Vec<Finding>) -> Option<GcSets> {
    let consts = rust_str_consts(labels_src);
    let mut parse = |name: &str, canary: &str| -> Option<Vec<String>> {
        let (entries, declared) = rust_str_array(gc_src, name);
        if declared != Some(entries.len()) || entries.is_empty() {
            f.push(empty_input(
                GC_FILTER,
                &format!(
                    "all {name} entries (parsed {}, declared {})",
                    entries.len(),
                    declared.map_or_else(|| "none".to_string(), |d| d.to_string())
                ),
            ));
            return None;
        }
        let mut out = Vec::new();
        for e in entries {
            let Some(c) = e.strip_prefix("<const:").and_then(|c| c.strip_suffix('>')) else {
                out.push(e);
                continue;
            };
            let Some(v) = consts.get(c) else {
                let what = format!("`pub const {c}: &str = \"…\"`");
                let why = format!("{what} (entry of {name} in {GC_FILTER})");
                f.push(empty_input(LABELS_RS, &why));
                return None;
            };
            out.push(v.clone());
        }
        if !out.iter().any(|k| k == canary) {
            f.push(empty_input(
                GC_FILTER,
                &format!("{name} containing its positive-control key `{canary}`"),
            ));
            return None;
        }
        Some(out)
    };
    let allowlist = parse("ALLOWLIST", "org_id");
    let extra = parse("MEDIA_DATAPOINT_EXTRA", "reason");
    Some(GcSets {
        allowlist: allowlist?,
        extra: extra?,
    })
}

// -----------------------------------------------------------------------------
// Checks.
// -----------------------------------------------------------------------------

fn exported_set(inp: &Inputs) -> Option<BTreeSet<String>> {
    Some(
        inp.markers
            .as_ref()?
            .iter()
            .filter(|(_, e)| *e == Some(true))
            .map(|(n, _)| n.clone())
            .collect(),
    )
}

/// All static `dt_client_*` literal bodies across the walked sources.
fn sdk_name_literals(sdk: &BTreeMap<String, Lexed>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (file, lexed) in sdk {
        for (_, body) in static_literals(lexed) {
            if DT_CLIENT_LITERAL_RE.is_match(body) {
                out.entry(body.to_string()).or_insert_with(|| file.clone());
            }
        }
    }
    out
}

/// G1: set equality, both directions, naming which side diverged.
fn check_export_set(inp: &Inputs) -> Vec<Finding> {
    let (Some(exported), Some(names)) = (exported_set(inp), inp.config_names.as_ref()) else {
        return Vec::new();
    };
    let config_set: BTreeSet<String> = names.iter().cloned().collect();
    let mut out = Vec::new();
    for name in config_set.difference(&exported) {
        out.push(Finding::new(
            COLLECTOR_CONFIG,
            EXPORT_SET_MISMATCH_RULE_ID,
            format!(
                "{name} is exported by the collector but is not marked `Exported: yes` in \
                 {CATALOG} — an undocumented exported series. Add the catalog entry, or \
                 remove the name from the include list."
            ),
        ));
    }
    for name in exported.difference(&config_set) {
        out.push(Finding::new(
            CATALOG,
            EXPORT_SET_MISMATCH_RULE_ID,
            format!(
                "{name} is marked `Exported: yes` but is missing from the \
                 filter/client_metric_names list in {COLLECTOR_CONFIG}. It will be dropped \
                 silently and look identical to 'no browser is running' — the exact failure \
                 this guard exists to prevent."
            ),
        ));
    }
    out
}

/// G2 / rule 4: every `dt_client_*` literal in any walked sdk-core file has an
/// explicit marker.
fn check_export_markers(inp: &Inputs) -> Vec<Finding> {
    let (Some(markers), Some(sdk)) = (inp.markers.as_ref(), inp.sdk.as_ref()) else {
        return Vec::new();
    };
    let declared: HashSet<&str> = markers
        .iter()
        .filter(|(_, e)| e.is_some())
        .map(|(n, _)| n.as_str())
        .collect();
    let literals = sdk_name_literals(sdk);
    if literals.is_empty() {
        return vec![empty_input(SDK_SRC, "`dt_client_*` metric-name literals")];
    }
    literals
        .iter()
        .filter(|(name, _)| !declared.contains(name.as_str()))
        .map(|(name, file)| {
            Finding::new(
                CATALOG,
                MISSING_EXPORT_MARKER_RULE_ID,
                format!(
                    "{name} is named in {file} but has no explicit `Exported: yes|no` marker in \
                     {CATALOG}. Absent is NOT 'no': a new client metric must force an export \
                     decision, not default to invisible."
                ),
            )
        })
        .collect()
}

/// G3: every exported name occurs as a literal in non-test sdk-core.
fn check_emitters(inp: &Inputs) -> Vec<Finding> {
    let (Some(exported), Some(sdk)) = (exported_set(inp), inp.sdk.as_ref()) else {
        return Vec::new();
    };
    let literals = sdk_name_literals(sdk);
    if literals.is_empty() {
        // Reported once, by `check_export_markers`.
        return Vec::new();
    }
    exported
        .iter()
        .filter(|n| !literals.contains_key(*n))
        .map(|name| {
            Finding::new(
                CATALOG,
                EXPORTED_WITHOUT_EMITTER_RULE_ID,
                format!(
                    "{name} is marked `Exported: yes` but no non-test source under {SDK_SRC} \
                     emits it — it would be a permanently empty panel that reads as a quiet \
                     system."
                ),
            )
        })
        .collect()
}

/// G4: `keep_keys` ⊆ GC's forwarded set.
fn check_keep_keys_forwarded(inp: &Inputs) -> Vec<Finding> {
    let (Some(keep), Some(gc)) = (inp.keep_keys.as_ref(), inp.gc.as_ref()) else {
        return Vec::new();
    };
    let forwarded: HashSet<&String> = gc.allowlist.iter().chain(&gc.extra).collect();
    keep.iter()
        .filter(|k| !forwarded.contains(k))
        .map(|key| {
            Finding::new(
                COLLECTOR_CONFIG,
                KEEP_KEY_NOT_FORWARDED_RULE_ID,
                format!(
                    "keep_keys retains {key:?}, but GC's filter never forwards it ({GC_FILTER}) \
                     — the label can never arrive, so keeping it is a no-op that reads as \
                     coverage."
                ),
            )
        })
        .collect()
}

/// Rule 3: every `MEDIA_DATAPOINT_EXTRA` key is kept. The base `ALLOWLIST` is
/// deliberately NOT required: it carries trace-only keys (`meeting_id_hash`)
/// that `keep_keys` must never have.
fn check_extra_kept(inp: &Inputs) -> Vec<Finding> {
    let (Some(keep), Some(gc)) = (inp.keep_keys.as_ref(), inp.gc.as_ref()) else {
        return Vec::new();
    };
    gc.extra
        .iter()
        .filter(|k| !keep.contains(k))
        .map(|key| {
            Finding::new(
                COLLECTOR_CONFIG,
                FORWARDED_KEY_NOT_KEPT_RULE_ID,
                format!(
                    "GC forwards media datapoint key {key:?} (MEDIA_DATAPOINT_EXTRA in \
                     {GC_FILTER}) but keep_keys strips it — every client media series loses \
                     that label at the collector."
                ),
            )
        })
        .collect()
}

/// Identity counter-control over kept and media-forwarded keys.
fn check_identity_keys(inp: &Inputs) -> Vec<Finding> {
    let Some(policy) = inp.identity.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(keep) = inp.keep_keys.as_ref() {
        if keep.iter().any(|k| k == MEETING_ID_HASH) {
            seen.insert(MEETING_ID_HASH.to_string());
            out.push(Finding::new(
                COLLECTOR_CONFIG,
                IDENTITY_KEY_FORWARDED_RULE_ID,
                format!(
                    "keep_keys retains {MEETING_ID_HASH:?}: a per-meeting identity on stored \
                     client series is the ADR-0036 §11 breach. It is trace-only (GC's base \
                     ALLOWLIST keeps it for traces); never keep it on the metrics path."
                ),
            ));
        }
        for k in keep {
            if seen.insert(k.clone()) {
                if let Some(term) = policy.matched_term(k) {
                    out.push(identity_finding(COLLECTOR_CONFIG, k, &term, "keep_keys"));
                }
            }
        }
    }
    if let Some(gc) = inp.gc.as_ref() {
        for k in &gc.extra {
            if seen.insert(k.clone()) {
                if let Some(term) = policy.matched_term(k) {
                    out.push(identity_finding(
                        GC_FILTER,
                        k,
                        &term,
                        "MEDIA_DATAPOINT_EXTRA",
                    ));
                }
            }
        }
    }
    out
}

fn identity_finding(file: &str, key: &str, term: &str, list: &str) -> Finding {
    Finding::new(
        file,
        IDENTITY_KEY_FORWARDED_RULE_ID,
        format!(
            "{list} carries {key:?}, which matches identity term {term:?} from the \
             identity-label-policy block in {LABEL_TAXONOMY} §R4. No media-path series may \
             carry a meeting/participant/sender/key/slot/stream identity."
        ),
    )
}

fn unresolved_bag(file: &str, line: usize, msg: &str) -> Finding {
    Finding::new(
        file,
        UNRESOLVED_LABEL_BAG_RULE_ID,
        format!(
            "{file}:{line}: {msg}. The guard cannot prove which label keys reach the \
             collector, so it fails closed. Emit the name as a literal first argument of a \
             sink method with an object-literal bag, or declare the wrapper in \
             DECLARED_WRAPPERS."
        ),
    )
}

/// Rule 1, part (a), regardless of export status: a `.counter(` / `.gauge(` /
/// `.histogram(` call whose name argument is not a static literal, outside a
/// declared wrapper's body and outside [`SINK_IMPLEMENTATION_FILES`], is
/// `unresolved_label_bag`.
///
/// Residual, recorded not fixed: [`SINK_CALL_RE`] sees only the
/// `.counter/.gauge/.histogram(` spelling, so a sink method reached by alias
/// (`const c = sink.counter; c(n, …)`) or bracket access
/// (`sink['counter'](n, …)`) evades this check. That takes deliberate
/// obfuscation. (An EXPORTED name passed to such an alias is still caught by
/// the callee check in [`check_label_bags`].)
fn check_nonliteral_sink_calls(inp: &Inputs) -> Vec<Finding> {
    let (Some(sdk), Some(wrappers)) = (inp.sdk.as_ref(), inp.wrappers.as_ref()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (file, lexed) in sdk {
        if SINK_IMPLEMENTATION_FILES.contains(&file.as_str()) {
            continue;
        }
        let text = &lexed.text;
        for m in SINK_CALL_RE.find_iter(text) {
            if lexed.string_at(m.start()).is_some() {
                continue;
            }
            let first = skip_ws(text, m.end());
            match lexed.string_starting_at(first) {
                Some(s) if s.is_static() => continue,
                // Reported by `check_label_bags` as an interpolated metric name.
                Some(s) if s.body(text).starts_with("dt_client_") => continue,
                _ => {}
            }
            if wrappers
                .iter()
                .any(|w| w.decl.file == file && w.body.0 < m.start() && m.start() < w.body.1)
            {
                continue;
            }
            let arg = arg_end(lexed, first)
                .and_then(|e| text.get(first..e))
                .unwrap_or("?")
                .trim();
            let call = m.as_str().trim_end_matches('(').trim();
            out.push(unresolved_bag(
                file,
                lexed.line_of(m.start()),
                &format!(
                    "sink call `{call}` with non-literal metric name `{arg}` outside a declared \
                     wrapper"
                ),
            ));
        }
    }
    out
}

/// Whether `callee` is a `MetricsSink` method call (`x.counter`, `x?.gauge`, …).
fn is_sink_method_callee(callee: &str) -> bool {
    ["counter", "gauge", "histogram"].iter().any(|m| {
        callee
            .strip_suffix(m)
            .is_some_and(|pre| pre.ends_with('.') && pre.len() > 1)
    })
}

/// The resolved label keys at one emission call site: the caller's bag plus,
/// for a call through a declared wrapper, the wrapper's injected bag. `Err`
/// explains why the bag cannot be resolved (including a callee that is neither
/// a sink method nor a declared wrapper).
fn emission_keys(
    lexed: &Lexed,
    file: &str,
    call: &CallSite,
    res: &Resolutions,
    wrappers: &[ResolvedWrapper],
) -> std::result::Result<BTreeSet<String>, String> {
    let wrapper = wrappers
        .iter()
        .find(|w| w.decl.file == file && call.callee == format!("this.{}", w.decl.method));
    if wrapper.is_none() && !is_sink_method_callee(&call.callee) {
        return Err(format!(
            "exported name passed to non-sink callee `{}` (neither `.counter/.gauge/.histogram` \
             nor a declared wrapper), so the keys it adds are unknown",
            call.callee
        ));
    }
    let mut keys = match call.label_arg {
        Some((s, e)) => resolve_bag(&parse_raw_bag(lexed, s, e), file, res, &[], None)?,
        None => BTreeSet::new(),
    };
    if let Some(w) = wrapper {
        match &w.injected {
            Ok(inj) => keys.extend(inj.iter().cloned()),
            Err(e) => {
                return Err(format!(
                    "wrapper {} injects an unresolved bag: {e}",
                    w.decl.method
                ))
            }
        }
    }
    Ok(keys)
}

/// Rule 1: label keys at emissions of exported names survive the pipeline.
///
/// **Emission** = a call whose FIRST argument is a static `dt_client_*` string
/// literal; the second argument is the label bag, read as an object literal
/// (`key: v`, quoted keys, shorthand, spreads) or a bare member path.
/// `this.#base` in `mediaMetrics.ts` resolves to the keys of
/// `mediaMetricLabels`' returned literal; a call through a [`DECLARED_WRAPPERS`]
/// entry adds the wrapper's injected keys. Every other spread is unresolved.
///
/// **Scope is `Exported: yes` names only**, and that is deliberate: a
/// non-exported name is dropped WHOLE by the collector's metric-name filter, so
/// its labels never reach storage. Applying `keep_keys` to them would demand
/// `meeting_id_hash` in `keep_keys` (the join-flow metrics carry it) — which is
/// exactly the ADR-0036 §11 breach. Do not "fix" a finding here by widening
/// `keep_keys`. Known and harmless while unexported: the join-flow metrics emit
/// bare `status` / `failure_stage` / `close_reason` / `mh_index_bucket`, while
/// GC's `ALLOWLIST` only has the `dt.`-dotted spellings; this rule fires the day
/// one of them is exported.
///
/// **Fail-closed paths (`unresolved_label_bag`):** an interpolated backtick
/// literal starting `dt_client_` in first-argument position (any name). For
/// exported names: any occurrence of the name literal that is not a call's
/// first argument; a call whose callee is neither a sink method
/// (`.counter/.gauge/.histogram`, optionally `?.`) nor a declared wrapper in
/// that file; and any bag with an unresolvable spread, computed key, or
/// non-literal shape. Non-literal-name sink calls are
/// [`check_nonliteral_sink_calls`] (see its residual note on aliased or
/// bracket-accessed sink methods).
///
/// Positive control: every exported name found in source has ≥1 call site
/// with a resolved bag, else [`EMPTY_INPUT_RULE_ID`].
fn check_label_bags(inp: &Inputs) -> Vec<Finding> {
    let (Some(exported), Some(sdk), Some(res), Some(wrappers), Some(keep), Some(gc)) = (
        exported_set(inp),
        inp.sdk.as_ref(),
        inp.resolutions.as_ref(),
        inp.wrappers.as_ref(),
        inp.keep_keys.as_ref(),
        inp.gc.as_ref(),
    ) else {
        return Vec::new();
    };
    let forwarded: HashSet<&String> = gc.allowlist.iter().chain(&gc.extra).collect();
    let mut out = Vec::new();
    let mut resolved_sites: BTreeSet<String> = BTreeSet::new();
    for (file, lexed) in sdk {
        let text = &lexed.text;
        for lit in &lexed.strings {
            let body = lit.body(text);
            let line = lexed.line_of(lit.start);
            if lit.interpolated {
                if body.starts_with("dt_client_") && call_for_first_arg(lexed, lit).is_some() {
                    let msg = format!("interpolated template `{body}` in metric-name position");
                    out.push(unresolved_bag(file, line, &msg));
                }
                continue;
            }
            if !exported.contains(body) {
                continue;
            }
            let Some(call) = call_for_first_arg(lexed, lit) else {
                let msg = format!(
                    "exported name {body} occurs other than as a call's first argument (e.g. \
                     bound to a variable and passed on) — its emission site cannot be tied to a \
                     label bag"
                );
                out.push(unresolved_bag(file, line, &msg));
                continue;
            };
            let keys = match emission_keys(lexed, file, &call, res, wrappers) {
                Ok(k) => k,
                Err(e) => {
                    let msg = format!("emission of exported {body} via `{}`: {e}", call.callee);
                    out.push(unresolved_bag(file, line, &msg));
                    continue;
                }
            };
            resolved_sites.insert(body.to_string());
            for key in &keys {
                let (fwd, kept) = (forwarded.contains(key), keep.contains(key));
                let why = match (fwd, kept) {
                    (true, true) => continue,
                    (false, false) => "neither forwarded by GC nor kept by the collector".into(),
                    (false, true) => format!("never forwarded by GC ({GC_FILTER})"),
                    (true, false) => {
                        format!("stripped by the collector's keep_keys ({COLLECTOR_CONFIG})")
                    }
                };
                out.push(Finding::new(
                    file,
                    LABEL_KEY_NOT_FORWARDED_RULE_ID,
                    format!(
                        "{file}:{} emits exported {body} with label key {key:?}, which is {why}. \
                         The series arrives without it — a selector or grouping on it matches \
                         nothing.",
                        call.line
                    ),
                ));
            }
        }
    }
    // Structural positive control.
    for name in sdk_name_literals(sdk).keys() {
        if exported.contains(name) && !resolved_sites.contains(name) {
            out.push(empty_input(
                SDK_SRC,
                &format!("call site with a resolved label bag for exported {name}"),
            ));
        }
    }
    out
}

/// G5: expiration / max_stale relations over the loaded rules.
///
/// Positive control: the loaded rules reference `dt_client_*` (a precondition
/// of `inp.rules` being `Some`), so ZERO parsed `dt_client_*` range windows
/// means the window extractor stopped matching — [`EMPTY_INPUT_RULE_ID`], not
/// a silently skipped relation.
fn check_expiration(inp: &Inputs) -> Vec<Finding> {
    let Some(rules) = inp.rules.as_ref() else {
        return Vec::new();
    };
    let window = rules
        .iter()
        .flat_map(|e| PROMQL_DT_CLIENT_RANGE_RE.captures_iter(&e.masked))
        .filter_map(|c| parse_prometheus_duration(c.get(1)?.as_str()))
        .max()
        .unwrap_or(0);
    if window == 0 {
        return vec![empty_input(
            ALERTS_SUBDIR,
            "`dt_client_*` range-vector window in any loaded rule expr (the G5 relation needs one)",
        )];
    }
    let Some((exp, stale)) = inp.durations else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if exp <= window {
        out.push(Finding::new(
            COLLECTOR_CONFIG,
            EXPIRATION_RELATION_RULE_ID,
            format!(
                "metric_expiration ({exp}s) must strictly EXCEED the longest dt_client_* range \
                 window in a loaded rule ({window}s), or a series expires mid-window while a \
                 client is merely slow and the alert stops evaluating instead of firing."
            ),
        ));
    }
    if stale < exp + window {
        out.push(Finding::new(
            COLLECTOR_CONFIG,
            EXPIRATION_RELATION_RULE_ID,
            format!(
                "max_stale ({stale}s) must be >= metric_expiration + longest window ({exp}s + \
                 {window}s). Otherwise the accumulator can forget while samples of the exposed \
                 series are still inside a rate() window, and the re-entering low value \
                 under-counts with no visible reset."
            ),
        ));
    }
    out
}

/// G6: the rewrite sentinel collides with no real token.
fn check_sentinel(inp: &Inputs) -> Vec<Finding> {
    let mut out = Vec::new();
    if inp
        .ts_vocab
        .as_ref()
        .is_some_and(|t| t.contains(REWRITE_SENTINEL))
    {
        out.push(Finding::new(
            MEDIA_METRICS_TS,
            SENTINEL_COLLISION_RULE_ID,
            format!(
                "{REWRITE_SENTINEL:?} is the collector's rewrite sentinel ({COLLECTOR_CONFIG}) \
                 and must never be a real token in a media vocabulary — a rewritten junk value \
                 from a patched client would be indistinguishable from a legitimate one."
            ),
        ));
    }
    if inp
        .mh_arms
        .as_ref()
        .is_some_and(|a| a.iter().any(|x| x == REWRITE_SENTINEL))
    {
        out.push(Finding::new(
            MH_METRICS_RS,
            SENTINEL_COLLISION_RULE_ID,
            format!(
                "{REWRITE_SENTINEL:?} is the collector's rewrite sentinel ({COLLECTOR_CONFIG}) \
                 and must never be a MediaDropReason value: four of these spellings are \
                 mirrored by the client, so a collision poisons `sum by(reason)` across the hop."
            ),
        ));
    }
    out
}

/// Rule 2: loaded `dt_client_*` alert references name exported series and
/// surviving labels.
fn check_alert_references(inp: &Inputs) -> Vec<Finding> {
    let (Some(exported), Some(keep), Some(rules)) = (
        exported_set(inp),
        inp.keep_keys.as_ref(),
        inp.rules.as_ref(),
    ) else {
        return Vec::new();
    };
    let allowed = |k: &str| keep.iter().any(|x| x == k) || EXPORTER_SYNTHESISED_LABELS.contains(&k);
    let mut out = Vec::new();
    for e in rules {
        let names: BTreeSet<&str> = PROMQL_DT_CLIENT_RE
            .find_iter(&e.code)
            .map(|m| m.as_str())
            .collect();
        if names.is_empty() {
            continue;
        }
        let where_ = rule_label(e);
        for name in &names {
            if exported_base(name, &exported).is_none() {
                out.push(Finding::new(
                    ALERTS_SUBDIR,
                    DEAD_ALERT_REFERENCE_RULE_ID,
                    format!(
                        "{where_} selects {name}, which is not `Exported: yes` in {CATALOG} — \
                         the collector drops it, so the selector matches nothing forever and \
                         the rule can never fire."
                    ),
                ));
            }
        }
        for c in PROMQL_DT_CLIENT_SELECTOR_RE.captures_iter(&e.masked) {
            let (Some(name), Some(body)) = (c.get(1), c.get(2)) else {
                continue;
            };
            for matcher in body.as_str().split(',') {
                let Some(key) = PROMQL_MATCHER_KEY_RE
                    .captures(matcher)
                    .and_then(|m| m.get(1))
                    .map(|m| m.as_str())
                else {
                    continue;
                };
                if !allowed(key) {
                    out.push(Finding::new(
                        ALERTS_SUBDIR,
                        DEAD_ALERT_REFERENCE_RULE_ID,
                        format!(
                            "{where_} matches label {key:?} on {}, but the collector's \
                             keep_keys strips it and the exporter does not synthesise it — the \
                             matcher can never be satisfied.",
                            name.as_str()
                        ),
                    ));
                }
            }
        }
        for c in PROMQL_GROUPING_RE.captures_iter(&e.masked) {
            let (Some(modifier), Some(labels)) = (c.get(1), c.get(2)) else {
                continue;
            };
            for label in labels
                .as_str()
                .split(',')
                .map(str::trim)
                .filter(|l| !l.is_empty())
            {
                // `without(instance)` / `ignoring(instance)` aggregate the
                // collector pod AWAY, which is correct; only the modifiers that
                // KEEP `instance` as a dimension are the defect.
                let keeps_dimension = matches!(
                    modifier.as_str(),
                    "by" | "on" | "group_left" | "group_right"
                );
                if label == "instance" && keeps_dimension {
                    out.push(Finding::new(
                        ALERTS_SUBDIR,
                        CLIENT_ALERT_GROUPS_BY_INSTANCE_RULE_ID,
                        format!(
                            "{where_} groups a dt_client_* expr {}(instance): on a client \
                             series `instance` is the collector pod, not a browser \
                             ({LABEL_TAXONOMY} §R4). The grouping is by collector.",
                            modifier.as_str()
                        ),
                    ));
                } else if !allowed(label) {
                    out.push(Finding::new(
                        ALERTS_SUBDIR,
                        DEAD_ALERT_REFERENCE_RULE_ID,
                        format!(
                            "{where_} uses {}({label}) on a dt_client_* expr, but {label:?} \
                             never survives to storage (not in keep_keys, not \
                             exporter-synthesised).",
                            modifier.as_str()
                        ),
                    ));
                }
            }
        }
    }
    out
}

/// Tripwire shape: tripwire counters alerted on in presence form only, and
/// the enumerated list is not stale.
fn check_tripwires(inp: &Inputs) -> Vec<Finding> {
    let Some(rules) = inp.rules.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in TRIPWIRE_CLIENT_COUNTERS {
        let mut seen = false;
        for e in rules {
            for at in word_occurrences(&e.masked, name) {
                seen = true;
                if let Some(func) = enclosing_functions(&e.masked, at)
                    .into_iter()
                    .find(|f| COUNTER_DIFF_FUNCS.contains(&f.as_str()))
                {
                    out.push(Finding::new(
                        ALERTS_SUBDIR,
                        TRIPWIRE_RATE_WRAPPED_RULE_ID,
                        format!(
                            "{} wraps tripwire counter {name} in {func}(). Every arm of it is \
                             created lazily at its first increment, so the first sample is \
                             already >= 1 and {func}() never sees the first event. Use the \
                             presence form `sum({name}{{...}}) > 0`.",
                            rule_label(e)
                        ),
                    ));
                }
            }
        }
        if !seen {
            out.push(Finding::new(
                ALERTS_SUBDIR,
                TRIPWIRE_LIST_STALE_RULE_ID,
                format!(
                    "{name} is listed in TRIPWIRE_CLIENT_COUNTERS but no loaded rule expr \
                     references it — either its tripwire alert is gone or the list is stale."
                ),
            ));
        }
    }
    out
}

/// G7: `drops_frame` reject tokens partition exactly between the
/// `MCMediaMissingKeyMaterial` alternation and [`NOT_KEY_DELIVERY`].
fn check_reject_partition(inp: &Inputs) -> Vec<Finding> {
    let (Some(rules), Some(vectors)) = (inp.rules.as_ref(), inp.vectors.as_ref()) else {
        return Vec::new();
    };
    let Some(rule) = rules
        .iter()
        .find(|e| e.alert.as_deref() == Some(MISSING_KEY_MATERIAL_ALERT))
    else {
        return vec![empty_input(
            ALERTS_SUBDIR,
            &format!("loaded rule `alert: {MISSING_KEY_MATERIAL_ALERT}`"),
        )];
    };
    let alternation: BTreeSet<String> = REASON_ALTERNATION_RE
        .captures(&rule.code)
        .and_then(|c| c.get(1))
        .map(|m| {
            m.as_str()
                .split('|')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if !alternation.contains(ALTERNATION_CANARY) {
        return vec![empty_input(
            ALERTS_SUBDIR,
            &format!(
                "`reason=~\"…\"` alternation containing `{ALTERNATION_CANARY}` in \
                 {MISSING_KEY_MATERIAL_ALERT}"
            ),
        )];
    }
    let not_kd: BTreeMap<&str, &str> = NOT_KEY_DELIVERY.iter().copied().collect();
    let by_token: BTreeMap<&str, &RejectReasonRow> =
        vectors.iter().map(|r| (r.token.as_str(), r)).collect();
    let where_ = rule_label(rule);
    let mut out = Vec::new();
    for r in vectors.iter().filter(|r| r.drops_frame) {
        let in_alt = alternation.contains(&r.token);
        let in_nkd = not_kd.contains_key(r.token.as_str());
        match (in_alt, in_nkd) {
            (false, false) => out.push(Finding::new(
                VECTORS_JSON,
                REJECT_TOKEN_UNCLASSIFIED_RULE_ID,
                format!(
                    "frame-dropping reject token {:?} (layer {}) is in neither {where_}'s reason \
                     alternation nor NOT_KEY_DELIVERY. Decide: is it a key-delivery failure (add \
                     it to the alert) or not (add it to NOT_KEY_DELIVERY with a reason)?",
                    r.token, r.layer
                ),
            )),
            (true, true) => out.push(Finding::new(
                VECTORS_JSON,
                REJECT_TOKEN_MISCLASSIFIED_RULE_ID,
                format!(
                    "reject token {:?} is BOTH in {where_}'s reason alternation and in \
                     NOT_KEY_DELIVERY ({}) — the partition must be exact.",
                    r.token,
                    not_kd.get(r.token.as_str()).unwrap_or(&"")
                ),
            )),
            _ => {}
        }
    }
    for t in &alternation {
        match by_token.get(t.as_str()) {
            Some(r) if r.drops_frame => {}
            Some(_) => out.push(Finding::new(
                ALERTS_SUBDIR,
                REJECT_TOKEN_MISCLASSIFIED_RULE_ID,
                format!(
                    "{where_}'s reason alternation includes {t:?}, which does not drop the \
                     frame (drops_frame: false in {VECTORS_JSON}) — it never reaches the \
                     frames_dropped numerator."
                ),
            )),
            None => out.push(Finding::new(
                ALERTS_SUBDIR,
                REJECT_TOKEN_MISCLASSIFIED_RULE_ID,
                format!(
                    "{where_}'s reason alternation includes {t:?}, which is not a reject token \
                     in {VECTORS_JSON} — a selector on a value that is never emitted."
                ),
            )),
        }
    }
    for (t, _) in NOT_KEY_DELIVERY {
        if !by_token.get(t).is_some_and(|r| r.drops_frame) {
            out.push(Finding::new(
                VECTORS_JSON,
                REJECT_TOKEN_MISCLASSIFIED_RULE_ID,
                format!(
                    "NOT_KEY_DELIVERY lists {t:?}, which is not a `drops_frame: true` reject \
                     token in {VECTORS_JSON} — a stale classification entry."
                ),
            ));
        }
    }
    out
}

// -----------------------------------------------------------------------------
// Run.
// -----------------------------------------------------------------------------

/// Parse every input once and run every rule. I/O and parse errors propagate
/// (with the path); a missing input or an under-parsed extraction is an
/// [`EMPTY_INPUT_RULE_ID`] finding.
pub fn collect_findings(repo_root: &Path) -> Result<Vec<Finding>> {
    let (inp, mut findings) = load_inputs(repo_root)?;
    let checks: [fn(&Inputs) -> Vec<Finding>; 13] = [
        check_export_set,
        check_export_markers,
        check_emitters,
        check_keep_keys_forwarded,
        check_extra_kept,
        check_identity_keys,
        check_nonliteral_sink_calls,
        check_label_bags,
        check_expiration,
        check_sentinel,
        check_alert_references,
        check_tripwires,
        check_reject_partition,
    ];
    for check in checks {
        findings.extend(check(&inp));
    }
    Ok(findings)
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let findings = collect_findings(repo_root)?;
    report(&findings, explain)
}

fn report(findings: &[Finding], explain: bool) -> Result<()> {
    if findings.is_empty() {
        emit_ok("client-metrics-export-clean");
        return Ok(());
    }
    for f in findings {
        f.print(explain);
    }
    anyhow::bail!("client-metrics-export: {} violation(s)", findings.len());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::alert_rule_files::load_rule_exprs;

    const CFG: &str = r#"
    processors:
      filter/client_metric_names:
        metrics:
          include:
            match_type: strict
            metric_names:
              - dt_client_media_frames_sent_total
              - dt_client_media_frames_received_total
      transform/client_metric_labels:
        metric_statements:
          - context: datapoint
            statements:
              - keep_keys(attributes, ["client_version", "org_id", "key_custody", "reason"])
      delta_to_cumulative:
        max_stale: 15m
        max_streams: 500
    exporters:
      prometheus:
        metric_expiration: 10m
"#;

    #[test]
    fn extracts_metric_names_from_the_include_list() {
        let names = collector_metric_names(CFG);
        assert_eq!(
            names,
            vec![
                "dt_client_media_frames_sent_total",
                "dt_client_media_frames_received_total"
            ]
        );
    }

    #[test]
    fn extracts_keep_keys() {
        assert_eq!(
            collector_keep_keys(CFG),
            vec!["client_version", "org_id", "key_custody", "reason"]
        );
    }

    #[test]
    fn extracts_durations_in_seconds() {
        assert_eq!(collector_duration(CFG, "metric_expiration"), Some(600));
        assert_eq!(collector_duration(CFG, "max_stale"), Some(900));
        assert_eq!(collector_duration(CFG, "no_such_key"), None);
    }

    #[test]
    fn a_renamed_duration_key_is_not_silently_skipped() {
        // The whole point of requiring PRESENCE: a future collector version that
        // renames the key must go red, not pass having checked nothing.
        let renamed = CFG.replace("max_stale:", "maxStaleness:");
        assert_eq!(collector_duration(&renamed, "max_stale"), None);
    }

    #[test]
    fn parses_export_markers_and_distinguishes_absent_from_no() {
        let md = "\
### `dt_client_media_frames_sent_total`
Exported: yes

### `dt_client_join_attempts_total`
Exported: no

### `dt_client_media_decoder_errors_total`
some prose but no marker
";
        let got = catalog_export_markers(md);
        assert_eq!(
            got[0],
            ("dt_client_media_frames_sent_total".into(), Some(true))
        );
        assert_eq!(
            got[1],
            ("dt_client_join_attempts_total".into(), Some(false))
        );
        assert_eq!(
            got[2],
            ("dt_client_media_decoder_errors_total".into(), None),
            "absent must stay distinguishable from an explicit `no`"
        );
    }

    #[test]
    fn extracts_rust_string_arrays_regardless_of_line_wrapping() {
        // BOTH forms must parse: `cargo fmt` reflows a short array onto one
        // line, and a line-keyed parser would under-count after a formatting
        // pass — reporting an extraction failure on a healthy file.
        let one_line = r#"
pub const MEDIA_DATAPOINT_EXTRA: [&str; 5] =
    ["reason", "outcome", "action", "source", KEY_CUSTODY_LABEL];
"#;
        let (got, declared) = rust_str_array(one_line, "MEDIA_DATAPOINT_EXTRA");
        assert_eq!(declared, Some(5));
        assert_eq!(
            got.len(),
            5,
            "reflowed array must still yield 5 entries: {got:?}"
        );
        assert!(got.contains(&"reason".to_string()));
        assert!(got.iter().any(|e| e.starts_with("<const:")));
    }

    #[test]
    fn extracts_rust_string_arrays_and_flags_unresolved_consts() {
        let src = r#"
pub const ALLOWLIST: [&str; 3] = [
    "client_version",
    // a comment, with a comma and a "quoted" word
    "org_id",
    "error.code",
];
pub const MEDIA_DATAPOINT_EXTRA: [&str; 2] = [
    "reason",
    KEY_CUSTODY_LABEL,
];
"#;
        assert_eq!(
            rust_str_array(src, "ALLOWLIST"),
            (
                vec![
                    "client_version".to_string(),
                    "org_id".to_string(),
                    "error.code".to_string()
                ],
                Some(3)
            )
        );
        let (extra, declared) = rust_str_array(src, "MEDIA_DATAPOINT_EXTRA");
        assert_eq!(declared, Some(2));
        assert!(extra.contains(&"reason".to_string()));
        assert!(
            extra.iter().any(|e| e.starts_with("<const:")),
            "an unresolved const entry must still be counted, so the caller's \
             length check cannot be fooled by a const it can't read"
        );
    }

    #[test]
    fn resolves_rust_str_consts() {
        let src = "pub const KEY_CUSTODY_LABEL: &str = \"key_custody\";\n";
        assert_eq!(
            rust_str_consts(src)
                .get("KEY_CUSTODY_LABEL")
                .map(String::as_str),
            Some("key_custody")
        );
    }

    #[test]
    fn extracts_match_arm_literals_and_declared_all_len() {
        let src = r#"
pub enum Other { X }
impl Other { pub const ALL: [Self; 1] = [Self::X]; }
pub enum MediaDropReason { A, B }
impl MediaDropReason {
    pub const ALL: [Self; 2] = [Self::A, Self::B];
    fn label(self) -> &'static str {
        match self {
            Self::A => "egress_queue_overflow",
            Self::B => "connection_closed",
        }
    }
}
"#;
        assert_eq!(
            rust_match_arm_literals(src, "enum MediaDropReason"),
            vec!["egress_queue_overflow", "connection_closed"]
        );
        assert_eq!(
            declared_all_len(src, "enum MediaDropReason"),
            Some(2),
            "must read the ALL of the named enum, not the first ALL in the file"
        );
    }

    #[test]
    fn a_degraded_arm_parse_is_detectable_against_the_declared_length() {
        // The structural positive control: `ALL` says 3, the arms yield 2.
        let src = r#"
pub enum MediaDropReason { A, B, C }
impl MediaDropReason {
    pub const ALL: [Self; 3] = [Self::A, Self::B, Self::C];
    fn label(self) -> &'static str {
        match self {
            Self::A => "a_reason",
            Self::B => "b_reason",
        }
    }
}
"#;
        let arms = rust_match_arm_literals(src, "enum MediaDropReason");
        assert!(arms.len() < declared_all_len(src, "enum MediaDropReason").unwrap());
    }

    #[test]
    fn empty_input_findings_use_a_distinct_reason_token() {
        // The two red paths must never be confusable: an extraction break gets
        // "fixed" by loosening the extractor if it looks like a content result.
        let f = empty_input(CATALOG, "headings");
        assert_eq!(f.rule_id, EMPTY_INPUT_RULE_ID);
        assert_ne!(f.rule_id, EXPORT_SET_MISMATCH_RULE_ID);
        assert_ne!(f.rule_id, SENTINEL_COLLISION_RULE_ID);
        assert!(
            f.message.contains("EXTRACTION failure"),
            "the message must say which kind of red this is"
        );
    }

    #[test]
    fn an_empty_parse_yields_no_names_rather_than_a_clean_set() {
        assert!(collector_metric_names("nothing: here").is_empty());
        assert!(collector_keep_keys("nothing: here").is_empty());
        assert!(catalog_export_markers("# not a metric catalog").is_empty());
        assert!(rust_str_array("fn main() {}", "ALLOWLIST").0.is_empty());
        assert!(rust_match_arm_literals("fn main() {}", "enum MediaDropReason").is_empty());
    }

    #[test]
    fn window_extraction_reads_the_longest_dt_client_range() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("mc-alerts.yaml"),
            "groups:\n  - name: g\n    rules:\n      - alert: A\n        expr: \
             sum(rate(dt_client_media_frames_dropped_total{reason=\"x\"}[5m])) / \
             sum(rate(dt_client_media_frames_received_total[5m])) + rate(mc_something_total[30m])\n",
        )
        .unwrap();
        // A template file must NOT be read: it is never loaded by Prometheus.
        std::fs::write(
            dir.path().join("_template-service-alerts.yaml"),
            "groups:\n  - name: g\n    rules:\n      - alert: T\n        expr: rate(dt_client_foo[99h])\n",
        )
        .unwrap();
        let rules: Vec<PreparedExpr> = load_rule_exprs(dir.path())
            .unwrap()
            .iter()
            .map(prepare_expr)
            .collect();
        let inp = Inputs {
            durations: Some((301, 601)),
            rules: Some(rules),
            ..Inputs::default()
        };
        assert!(
            check_expiration(&inp).is_empty(),
            "30m is not on a dt_client_ selector; 99h is a template"
        );
        let inp = Inputs {
            durations: Some((300, 900)),
            ..inp
        };
        assert_eq!(check_expiration(&inp).len(), 1, "300s does not exceed 5m");
    }

    #[test]
    fn promql_comments_are_ignored_and_strings_masked() {
        let e = LoadedRuleExpr {
            file: "x-alerts.yaml".into(),
            rule: crate::common::alert_rule_files::RuleExpr {
                alert: None,
                expr: "sum(x{reason=\"by(a)\"}) # dt_client_ghost\n".into(),
            },
        };
        let p = prepare_expr(&e);
        assert!(!p.code.contains("dt_client_ghost"));
        assert!(p.code.contains("by(a)"));
        assert!(!p.masked.contains("by(a)"));
    }

    #[test]
    fn enclosing_functions_walks_outward() {
        let s = "sum by(outcome)(increase(X[15m])) > 0";
        let at = s.find('X').unwrap();
        assert_eq!(enclosing_functions(s, at), vec!["increase".to_string()]);
        let s = "sum(X{a=\"b\"}) > 0";
        assert_eq!(enclosing_functions(s, 4), vec!["sum".to_string()]);
    }

    /// Neither artifact present is a broken tree, not "no artifacts": the old
    /// `(false, false)` OK arm is gone.
    #[test]
    fn missing_catalog_and_config_fail_instead_of_reporting_no_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let findings = collect_findings(dir.path()).unwrap();
        for path in [CATALOG, COLLECTOR_CONFIG] {
            assert!(
                findings
                    .iter()
                    .any(|f| f.rule_id == EMPTY_INPUT_RULE_ID && f.file == path),
                "{path} missing must be an empty-input finding: {findings:?}"
            );
        }
    }
}
