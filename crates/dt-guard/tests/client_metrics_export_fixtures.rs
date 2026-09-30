// Crate-level test override, per the note in `clippy.toml`: `allow-expect-in-tests` /
// `allow-unwrap-in-tests` only apply inside `#[test]` fns, so this file's non-`#[test]`
// helpers (`Tree::edit`, `Tree::findings`) still trip `expect_used` / `panic`. A fixture
// edit whose anchor text is absent MUST abort loudly: a silently no-op mutation would
// leave the tree clean and turn a FIRE assertion into a vacuous pass.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration-test helpers outside #[test] fns; a missing fixture anchor MUST abort loudly (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

//! Fixture tests for `dt-guard client-metrics-export`, driven through
//! `collect_findings` over a synthetic tempdir tree.
//!
//! [`Tree::valid`] is a minimal, real-shaped tree that is CLEAN (asserted by
//! `the_valid_tree_is_clean`). Every other test mutates exactly one thing and
//! asserts a specific `rule_id` — never `is_err()`. [`Tree::edit`] panics if its
//! anchor is absent, so a mutation cannot silently no-op.

use dt_guard::client_metrics_export::{
    collect_findings, Finding, IdentityPolicy, CLIENT_ALERT_GROUPS_BY_INSTANCE_RULE_ID,
    DEAD_ALERT_REFERENCE_RULE_ID, EMPTY_INPUT_RULE_ID, EXPIRATION_RELATION_RULE_ID,
    EXPORTED_WITHOUT_EMITTER_RULE_ID, EXPORTER_SUFFIXING_ENABLED_RULE_ID,
    EXPORT_SET_MISMATCH_RULE_ID, FORWARDED_KEY_NOT_KEPT_RULE_ID, IDENTITY_KEY_FORWARDED_RULE_ID,
    KEEP_KEY_NOT_FORWARDED_RULE_ID, LABEL_KEY_NOT_FORWARDED_RULE_ID, MISSING_EXPORT_MARKER_RULE_ID,
    REJECT_TOKEN_MISCLASSIFIED_RULE_ID, REJECT_TOKEN_UNCLASSIFIED_RULE_ID,
    SENTINEL_COLLISION_RULE_ID, TRIPWIRE_LIST_STALE_RULE_ID, TRIPWIRE_RATE_WRAPPED_RULE_ID,
    UNRESOLVED_LABEL_BAG_RULE_ID,
};
use std::collections::BTreeMap;
use std::path::Path;

const CATALOG: &str = "docs/observability/metrics/client.md";
const COLLECTOR: &str = "infra/services/otel-collector/collector.yaml";
const GC_FILTER: &str = "crates/gc-service/src/services/telemetry_filter.rs";
const LABELS_RS: &str = "crates/common/src/observability/labels.rs";
const MEDIA_METRICS: &str = "packages/sdk-core/src/media/setup/mediaMetrics.ts";
const REJECT_REASON: &str = "packages/sdk-core/src/media/frame/rejectReason.ts";
const RECEIVE_PATH: &str = "packages/sdk-core/src/media/frame/receivePath.ts";
const MEETING_SESSION: &str = "packages/sdk-core/src/session/MeetingSession.ts";
const MEDIA_TRANSPORT: &str = "packages/sdk-core/src/media/MediaTransport.ts";
const OTEL_SINK: &str = "packages/sdk-core/src/telemetry/OtelMetricsSink.ts";
const SDK_SRC: &str = "packages/sdk-core/src";
const MH_METRICS: &str = "crates/mh-service/src/observability/metrics.rs";
const ALERTS_DIR: &str = "infra/docker/prometheus/rules";
const MC_ALERTS: &str = "infra/docker/prometheus/rules/mc-alerts.yaml";
const CLIENT_ALERTS: &str = "infra/docker/prometheus/rules/client-alerts.yaml";
const VECTORS: &str = "proto/test-vectors/frame-v2.vectors.json";
const TAXONOMY: &str = "docs/observability/label-taxonomy.md";

const EXPORTED: &[&str] = &[
    "dt_client_media_frames_sent_total",
    "dt_client_media_frames_received_total",
    "dt_client_media_frames_dropped_total",
    "dt_client_media_kek_retention_violations_total",
    "dt_client_media_kek_install_refusals_total",
    "dt_client_media_roster_key_rebinds_total",
];
const NOT_EXPORTED: &[&str] = &[
    "dt_client_join_attempts_total",
    "dt_client_time_to_signaling_ready_ms",
    "dt_client_mh_connection_total",
];

/// `(token, layer, drops_frame)` — the real vocabulary's shape.
const REJECT_TOKENS: &[(&str, &str, bool)] = &[
    ("unknown_version", "codec", true),
    ("reserved_flag_bit_set", "codec", true),
    ("payload_length_exceeds_max", "codec", true),
    ("payload_length_exceeds_available", "codec", true),
    ("truncated", "codec", true),
    ("extensions_too_large", "codec", true),
    ("extensions_malformed", "codec", true),
    ("trailing_bytes", "codec", true),
    ("no_transmit_key", "codec", true),
    ("signature_invalid", "crypto", true),
    ("decrypt_failed", "crypto", true),
    ("unwrap_failed", "crypto", true),
    ("replay_detected", "crypto", true),
    ("wrap_key_id_mismatch", "crypto", false),
    ("no_kek_for_generation", "key", true),
    ("no_roster_entry", "key", true),
    ("kek_generation_stale", "key", true),
    ("sender_not_assigned", "assignment", true),
];

const POLICY_BLOCK: &str = "\
```identity-label-policy
containment = meeting, participant, sender, session, slot, stream
segment = user, key, kek, id, hash
exempt = key_custody, org_id, slot_state
```
";

struct Tree {
    files: BTreeMap<String, String>,
}

impl Tree {
    #[expect(clippy::too_many_lines, reason = "one synthetic file per guard input")]
    fn valid() -> Self {
        let mut files = BTreeMap::new();
        let mut put = |p: &str, s: String| {
            files.insert(p.to_string(), s);
        };

        let mut catalog = String::from("# Client metrics\n\n");
        for n in EXPORTED {
            catalog.push_str(&format!("### `{n}`\n\n- **Exported**: yes\n\n"));
        }
        for n in NOT_EXPORTED {
            catalog.push_str(&format!("### `{n}`\n\n- **Exported**: no\n\n"));
        }
        put(CATALOG, catalog);

        let mut cfg = String::from(
            "processors:\n  filter/client_metric_names:\n    metrics:\n      include:\n        \
             match_type: strict\n        metric_names:\n",
        );
        for n in EXPORTED {
            cfg.push_str(&format!("          - {n}\n"));
        }
        cfg.push_str(
            "  transform/client_metric_labels:\n    metric_statements:\n      - context: datapoint\n        \
             statements:\n          - keep_keys(attributes, [\"client_version\", \"org_id\", \
             \"key_custody\", \"reason\", \"outcome\", \"action\", \"source\", \"mode\"])\n  \
             deltatocumulative:\n    max_stale: 20m\nexporters:\n  prometheus:\n    \
             metric_expiration: 10m\n    add_metric_suffixes: false\n",
        );
        put(COLLECTOR, cfg);

        put(
            GC_FILTER,
            r#"use common::observability::labels::KEY_CUSTODY_LABEL;

pub const ALLOWLIST: [&str; 6] = [
    "client_version",
    // Trace-only, never kept on the metrics path, "quoted", with commas.
    "meeting_id_hash",
    "org_id",
    "dt.failure_stage",
    "dt.close_reason",
    "error.code",
];

pub const MEDIA_DATAPOINT_EXTRA: [&str; 6] = [
    "reason",
    "outcome",
    "action",
    "source",
    "mode",
    KEY_CUSTODY_LABEL,
];
"#
            .into(),
        );
        put(
            LABELS_RS,
            "pub const KEY_CUSTODY_LABEL: &str = \"key_custody\";\n\
             pub const KEY_CUSTODY_OPERATOR: &str = \"operator\";\n"
                .into(),
        );

        put(
            MEDIA_METRICS,
            r#"// THE ONLY PLACE IN `media/**` THAT MAY NAME A `dt_client_*` METRIC.
// A comment naming 'dt_client_media_commented_total' is not a literal.
import type { MetricsSink, MetricLabels } from '../../telemetry/MetricsSink';

export type MediaFrameDropReason = 'no_roster_entry' | 'no_kek_for_generation' | "unwrap_failed";
export const KEY_CUSTODY = 'operator';
const DOCS = 'https://example.invalid/docs'; // a URL literal: not a comment

export function mediaMetricLabels(identity: { clientVersion: string; orgId: string }): MetricLabels {
  return {
    client_version: identity.clientVersion,
    org_id: identity.orgId,
    key_custody: KEY_CUSTODY,
  };
}

export class MediaMetrics {
  readonly #sink: MetricsSink | undefined;
  /** Resolved once. */
  readonly #base: MetricLabels;

  constructor(identity: { clientVersion: string; orgId: string }, sink: MetricsSink | undefined) {
    this.#base = mediaMetricLabels(identity);
    this.#sink = sink;
  }

  frameSent(): void {
    this.#sink?.counter('dt_client_media_frames_sent_total', this.#base);
  }

  frameReceived(): void {
    this.#sink?.counter("dt_client_media_frames_received_total", this.#base);
  }

  frameDropped(reason: MediaFrameDropReason): void {
    this.#sink?.counter(`dt_client_media_frames_dropped_total`, { ...this.#base, reason });
  }

  retentionViolation(): void {
    this.#sink?.counter('dt_client_media_kek_retention_violations_total', this.#base);
  }

  installRefused(outcome: string): void {
    this.#sink?.counter('dt_client_media_kek_install_refusals_total', {
      ...this.#base,
      'outcome': outcome,
    });
  }

  rosterRebind(outcome: string): void {
    this.#sink?.counter('dt_client_media_roster_key_rebinds_total', { ...this.#base, outcome: outcome });
  }
}
"#
            .into(),
        );
        put(
            REJECT_REASON,
            "export const ALL_REJECT_REASONS = ['no_roster_entry', 'truncated'] as const;\n".into(),
        );
        put(RECEIVE_PATH, "export const ACCEPTED = 'accepted';\n".into());
        put(
            MEETING_SESSION,
            r#"export class MeetingSession {
  readonly #metricsSink: MetricsSink | undefined;
  #metricLabels: MetricLabels = {};

  async join(options: JoinOptions): Promise<void> {
    this.#metricLabels = {
      client_version: 'v',
      org_id: options.orgSubdomain,
      meeting_id_hash: 'none',
    };
    this.#metricLabels = {
      ...this.#metricLabels,
      meeting_id_hash: await meetingIdHash('m'),
    };
    this.#histogram('dt_client_time_to_signaling_ready_ms');
    this.#counter('dt_client_join_attempts_total', { status, failure_stage: stage });
  }

  #counter(name: string, labels: MetricLabels): void {
    this.#metricsSink?.counter(name, { ...this.#metricLabels, ...labels });
  }

  #histogram(name: string): void {
    this.#metricsSink?.histogram(
      name,
      { ...this.#metricLabels },
      1,
    );
  }
}
"#
            .into(),
        );
        put(
            MEDIA_TRANSPORT,
            r#"export class MediaTransport {
  #emitMetric(status: 'success' | 'failure'): void {
    this.#metricsSink?.counter('dt_client_mh_connection_total', {
      ...this.#metricLabels,
      status,
    });
  }
}
"#
            .into(),
        );
        put(
            OTEL_SINK,
            r#"export class OtelMetricsSink implements MetricsSink {
  counter(name: string, labels: MetricLabels, value = 1): void {
    this.#delegate.counter(name, labels, value);
  }
}
"#
            .into(),
        );

        put(
            MH_METRICS,
            r#"pub enum MediaDropReason { A, B }
impl MediaDropReason {
    pub const ALL: [Self; 2] = [Self::A, Self::B];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "egress_queue_overflow",
            Self::B => "no_subscriber",
        }
    }
}
"#
            .into(),
        );

        put(
            MC_ALERTS,
            r#"groups:
  - name: mc
    rules:
      # dt_client_commented_out_total in a YAML comment must not count.
      - alert: MCMediaMissingKeyMaterial
        expr: |
          (
            sum(rate(dt_client_media_frames_dropped_total{reason=~"no_kek_for_generation|kek_generation_stale|no_roster_entry|unwrap_failed"}[5m]))
            /
            sum(rate(dt_client_media_frames_received_total[5m]))
          ) > 0.05
        for: 15m
      - alert: MCClientKekConflictingKey
        expr: sum(dt_client_media_kek_install_refusals_total{outcome="conflicting_key"}) > 0
        for: 1m
      - alert: MCClientRosterKeyRebind
        expr: sum(dt_client_media_roster_key_rebinds_total{outcome="rebind"}) > 0
        for: 1m
"#
            .into(),
        );
        put(
            CLIENT_ALERTS,
            r#"groups:
  - name: client
    rules:
      - alert: ClientKekRetentionViolation
        expr: sum(dt_client_media_kek_retention_violations_total) > 0
        for: 1m
"#
            .into(),
        );

        put(VECTORS, vectors_json(REJECT_TOKENS));
        put(
            TAXONOMY,
            format!("# Label taxonomy\n\n### R4\n\n{POLICY_BLOCK}\nMore prose.\n"),
        );

        Self { files }
    }

    /// Replace the FIRST occurrence of `from` in `path`. Panics if `from` is
    /// absent, so a mutation can never silently no-op.
    fn edit(&mut self, path: &str, from: &str, to: &str) -> &mut Self {
        let s = self
            .files
            .get_mut(path)
            .unwrap_or_else(|| panic!("fixture has no {path}"));
        assert!(s.contains(from), "anchor {from:?} not found in {path}");
        *s = s.replacen(from, to, 1);
        self
    }

    fn set(&mut self, path: &str, content: &str) -> &mut Self {
        self.files.insert(path.to_string(), content.to_string());
        self
    }

    /// Remove `path` and everything under it. Panics if nothing matched.
    fn remove(&mut self, path: &str) -> &mut Self {
        let before = self.files.len();
        self.files
            .retain(|k, _| k != path && !k.starts_with(&format!("{path}/")));
        assert!(self.files.len() < before, "nothing to remove at {path}");
        self
    }

    fn findings(&self) -> Vec<Finding> {
        let dir = tempfile::tempdir().expect("tempdir");
        for (rel, content) in &self.files {
            let p = dir.path().join(rel);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            std::fs::write(&p, content).expect("write fixture");
        }
        collect_findings(dir.path()).expect("collect_findings")
    }
}

fn vectors_json(tokens: &[(&str, &str, bool)]) -> String {
    let rows: Vec<String> = tokens
        .iter()
        .map(|(t, l, d)| {
            format!(r#"{{"token": "{t}", "layer": "{l}", "drops_frame": {d}, "has_vector": true}}"#)
        })
        .collect();
    format!(
        "{{\"schema_version\": 2, \"reject_reasons\": [\n{}\n]}}\n",
        rows.join(",\n")
    )
}

fn has(findings: &[Finding], rule_id: &str) -> bool {
    findings.iter().any(|f| f.rule_id == rule_id)
}

#[track_caller]
fn assert_fires(findings: &[Finding], rule_id: &str) {
    assert!(
        has(findings, rule_id),
        "expected a {rule_id} finding, got: {findings:#?}"
    );
}

#[track_caller]
fn assert_fires_with(findings: &[Finding], rule_id: &str, needle: &str) {
    assert!(
        findings
            .iter()
            .any(|f| f.rule_id == rule_id && f.message.contains(needle)),
        "expected a {rule_id} finding mentioning {needle:?}, got: {findings:#?}"
    );
}

#[track_caller]
fn assert_clean(findings: &[Finding]) {
    assert!(findings.is_empty(), "expected clean, got: {findings:#?}");
}

#[track_caller]
fn assert_empty_input_for(findings: &[Finding], path: &str) {
    assert!(
        findings
            .iter()
            .any(|f| f.rule_id == EMPTY_INPUT_RULE_ID && f.file == path),
        "expected an {EMPTY_INPUT_RULE_ID} finding naming {path}, got: {findings:#?}"
    );
}

// -----------------------------------------------------------------------------
// Baseline.
// -----------------------------------------------------------------------------

#[test]
fn the_valid_tree_is_clean() {
    assert_clean(&Tree::valid().findings());
}

// -----------------------------------------------------------------------------
// Rule 5: every missing input is an empty-input finding naming its path.
// -----------------------------------------------------------------------------

macro_rules! missing_input_test {
    ($name:ident, $path:expr) => {
        #[test]
        fn $name() {
            let f = Tree::valid().remove($path).findings();
            assert_empty_input_for(&f, $path);
        }
    };
}

missing_input_test!(missing_catalog, CATALOG);
missing_input_test!(missing_collector_config, COLLECTOR);
missing_input_test!(missing_gc_filter, GC_FILTER);
missing_input_test!(missing_labels_rs, LABELS_RS);
missing_input_test!(missing_sdk_core_dir, SDK_SRC);
missing_input_test!(missing_media_metrics_ts, MEDIA_METRICS);
missing_input_test!(missing_reject_reason_ts, REJECT_REASON);
missing_input_test!(missing_receive_path_ts, RECEIVE_PATH);
missing_input_test!(missing_meeting_session_ts, MEETING_SESSION);
missing_input_test!(missing_mh_metrics_rs, MH_METRICS);
missing_input_test!(missing_alerts_dir, ALERTS_DIR);
missing_input_test!(missing_vectors_file, VECTORS);
missing_input_test!(missing_label_taxonomy, TAXONOMY);

/// The old `(false, false)` arm reported "no artifacts" OK. Neither present is
/// now a broken tree.
#[test]
fn neither_catalog_nor_config_is_a_failure_not_no_artifacts() {
    let f = Tree::valid().remove(CATALOG).remove(COLLECTOR).findings();
    assert_empty_input_for(&f, CATALOG);
    assert_empty_input_for(&f, COLLECTOR);
}

#[test]
fn an_empty_repo_reports_every_input_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let f = collect_findings(dir.path()).expect("collect_findings");
    for path in [
        CATALOG, COLLECTOR, GC_FILTER, LABELS_RS, SDK_SRC, MH_METRICS, ALERTS_DIR, VECTORS,
        TAXONOMY,
    ] {
        assert_empty_input_for(&f, path);
    }
    assert!(f.iter().all(|x| x.rule_id == EMPTY_INPUT_RULE_ID), "{f:#?}");
}

#[test]
fn no_loadable_rule_files_is_empty_input() {
    let f = Tree::valid()
        .remove(MC_ALERTS)
        .remove(CLIENT_ALERTS)
        .set(
            "infra/docker/prometheus/rules/_template-service-alerts.yaml",
            "groups: []\n",
        )
        .findings();
    assert_empty_input_for(&f, ALERTS_DIR);
}

// -----------------------------------------------------------------------------
// G1 / G3 / G4 / G5 / G6.
// -----------------------------------------------------------------------------

#[test]
fn g1_config_name_not_in_catalog_fires() {
    let f = Tree::valid()
        .edit(
            COLLECTOR,
            "          - dt_client_media_frames_sent_total\n",
            "          - dt_client_media_frames_sent_total\n          - dt_client_media_extra_total\n",
        )
        .findings();
    assert_fires_with(
        &f,
        EXPORT_SET_MISMATCH_RULE_ID,
        "dt_client_media_extra_total",
    );
}

#[test]
fn g3_exported_without_emitter_fires() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "this.#sink?.counter('dt_client_media_kek_retention_violations_total', this.#base);",
            "",
        )
        .findings();
    assert_fires_with(
        &f,
        EXPORTED_WITHOUT_EMITTER_RULE_ID,
        "dt_client_media_kek_retention_violations_total",
    );
}

#[test]
fn g4_kept_key_gc_never_forwards_fires() {
    let f = Tree::valid()
        .edit(COLLECTOR, "\"mode\"]", "\"mode\", \"bucket\"]")
        .findings();
    assert_fires_with(&f, KEEP_KEY_NOT_FORWARDED_RULE_ID, "bucket");
}

#[test]
fn g5_no_dt_client_range_window_is_empty_input() {
    let mut t = Tree::valid();
    let s = t
        .files
        .get(MC_ALERTS)
        .expect("mc")
        .replace("sum(rate(", "sum((")
        .replace("[5m]", "");
    t.set(MC_ALERTS, &s);
    let f = t.findings();
    assert_empty_input_for(&f, ALERTS_DIR);
    assert!(!has(&f, EXPIRATION_RELATION_RULE_ID), "{f:#?}");
}

#[test]
fn collector_suffix_key_absent_is_empty_input() {
    let f = Tree::valid()
        .edit(COLLECTOR, "    add_metric_suffixes: false\n", "")
        .findings();
    assert_fires_with(&f, EMPTY_INPUT_RULE_ID, "add_metric_suffixes:");
    assert!(!has(&f, EXPORTER_SUFFIXING_ENABLED_RULE_ID), "{f:#?}");
}

#[test]
fn collector_suffixing_enabled_fires_its_own_rule() {
    let f = Tree::valid()
        .edit(
            COLLECTOR,
            "add_metric_suffixes: false",
            "add_metric_suffixes: true",
        )
        .findings();
    assert_fires_with(
        &f,
        EXPORTER_SUFFIXING_ENABLED_RULE_ID,
        "silently kills every dt_client selector",
    );
    assert!(!has(&f, EMPTY_INPUT_RULE_ID), "{f:#?}");
}

#[test]
fn g5_window_not_exceeded_by_expiration_fires() {
    let f = Tree::valid()
        .edit(
            MC_ALERTS,
            "sum(rate(dt_client_media_frames_received_total[5m]))",
            "sum(rate(dt_client_media_frames_received_total[15m]))",
        )
        .findings();
    assert_fires(&f, EXPIRATION_RELATION_RULE_ID);
}

#[test]
fn g6_sentinel_in_a_double_quoted_ts_vocabulary_fires() {
    let f = Tree::valid()
        .edit(REJECT_REASON, "'truncated']", "'truncated', \"invalid\"]")
        .findings();
    assert_fires(&f, SENTINEL_COLLISION_RULE_ID);
}

#[test]
fn g6_vocabulary_canary_absent_is_empty_input() {
    let f = Tree::valid()
        .edit(MEDIA_METRICS, "'no_roster_entry' | ", "")
        .edit(REJECT_REASON, "'no_roster_entry', ", "")
        .findings();
    assert_empty_input_for(&f, MEDIA_METRICS);
}

// -----------------------------------------------------------------------------
// Rule 4 (G2 widened): any walked file, tests excluded.
// -----------------------------------------------------------------------------

#[test]
fn rule4_unmarked_literal_outside_media_metrics_fires() {
    let f = Tree::valid()
        .set(
            "packages/sdk-core/src/session/other.ts",
            "export const n = () => sink.counter(\"dt_client_session_unmarked_total\", {});\n",
        )
        .findings();
    assert_fires_with(
        &f,
        MISSING_EXPORT_MARKER_RULE_ID,
        "dt_client_session_unmarked_total",
    );
}

#[test]
fn rule4_same_literal_in_tests_is_not_flagged() {
    let f = Tree::valid()
        .set(
            "packages/sdk-core/src/session/__tests__/other.ts",
            "sink.counter('dt_client_session_unmarked_total', {});\n",
        )
        .set(
            "packages/sdk-core/src/session/other.test.ts",
            "sink.counter('dt_client_session_unmarked_total', {});\n",
        )
        .findings();
    assert_clean(&f);
}

#[test]
fn rule4_spec_file_is_not_flagged() {
    let f = Tree::valid()
        .set(
            "packages/sdk-core/src/session/other.spec.ts",
            "sink.counter('dt_client_session_unmarked_total', {});\n",
        )
        .findings();
    assert_clean(&f);
}

#[test]
fn rule4_comment_mention_is_not_a_literal() {
    let f = Tree::valid()
        .set(
            "packages/sdk-core/src/session/other.ts",
            "// see 'dt_client_session_unmarked_total'\n/* \"dt_client_x_total\" */\nexport {};\n",
        )
        .findings();
    assert_clean(&f);
}

#[test]
fn rule4_no_dt_client_literals_at_all_is_empty_input() {
    let mut t = Tree::valid();
    for p in [MEDIA_METRICS, MEETING_SESSION, MEDIA_TRANSPORT] {
        let s = t.files.get(p).expect("file").replace("dt_client_", "dtc_");
        t.set(p, &s);
    }
    let f = t.findings();
    assert_empty_input_for(&f, SDK_SRC);
}

// -----------------------------------------------------------------------------
// Rule 6: GC parse integrity.
// -----------------------------------------------------------------------------

#[test]
fn rule6_parsed_shorter_than_declared_is_empty_input() {
    let f = Tree::valid()
        .edit(
            GC_FILTER,
            "MEDIA_DATAPOINT_EXTRA: [&str; 6]",
            "MEDIA_DATAPOINT_EXTRA: [&str; 7]",
        )
        .findings();
    assert_empty_input_for(&f, GC_FILTER);
}

#[test]
fn rule6_block_comment_inside_an_array_is_not_an_entry() {
    let f = Tree::valid()
        .edit(
            GC_FILTER,
            "    \"org_id\",\n",
            "    \"org_id\", /* \"x\", */\n",
        )
        .findings();
    assert_clean(&f);
}

#[test]
fn rule6_unresolvable_const_is_empty_input() {
    let f = Tree::valid()
        .edit(LABELS_RS, "KEY_CUSTODY_LABEL", "KEY_CUSTODY_KEY")
        .findings();
    assert_empty_input_for(&f, LABELS_RS);
}

#[test]
fn rule6_allowlist_positive_control_missing_is_empty_input() {
    let f = Tree::valid()
        .edit(GC_FILTER, "\"org_id\",", "\"tenant\",")
        .findings();
    assert_empty_input_for(&f, GC_FILTER);
}

#[test]
fn rule6_resolved_const_is_used_not_a_hand_insert() {
    // Resolve KEY_CUSTODY_LABEL to a different spelling: now keep_keys'
    // "key_custody" is not forwarded (G4) and the resolved spelling is not kept
    // (rule 3) — proving the value came from labels.rs.
    let f = Tree::valid()
        .edit(LABELS_RS, "= \"key_custody\"", "= \"custody\"")
        .findings();
    assert_fires_with(&f, KEEP_KEY_NOT_FORWARDED_RULE_ID, "key_custody");
    assert_fires_with(&f, FORWARDED_KEY_NOT_KEPT_RULE_ID, "custody");
}

// -----------------------------------------------------------------------------
// Rule 3: MEDIA_DATAPOINT_EXTRA ⊆ keep_keys.
// -----------------------------------------------------------------------------

#[test]
fn rule3_forwarded_extra_key_not_kept_fires() {
    let f = Tree::valid().edit(COLLECTOR, ", \"mode\"]", "]").findings();
    assert_fires_with(&f, FORWARDED_KEY_NOT_KEPT_RULE_ID, "mode");
}

#[test]
fn rule3_base_allowlist_trace_key_need_not_be_kept() {
    // `meeting_id_hash` is in the fixture's ALLOWLIST and absent from keep_keys:
    // the baseline being clean is the assertion; restate it explicitly.
    let t = Tree::valid();
    assert!(t
        .files
        .get(GC_FILTER)
        .expect("gc")
        .contains("\"meeting_id_hash\""));
    assert!(!has(&t.findings(), FORWARDED_KEY_NOT_KEPT_RULE_ID));
}

#[test]
fn rule3_extra_positive_control_missing_is_empty_input() {
    let f = Tree::valid()
        .edit(GC_FILTER, "[&str; 6] = [\n    \"reason\",", "[&str; 5] = [")
        .findings();
    assert_empty_input_for(&f, GC_FILTER);
}

// -----------------------------------------------------------------------------
// Identity counter-control.
// -----------------------------------------------------------------------------

fn real_policy() -> IdentityPolicy {
    let md = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/observability/label-taxonomy.md"),
    )
    .expect("read real label-taxonomy.md");
    IdentityPolicy::parse_doc(&md).expect("real identity-label-policy block parses")
}

/// `(fire, pass)` from the shared ```` ```identity-label-cases ```` block in
/// label-taxonomy.md §R4 — the SAME table the env-tests kernel runs.
fn real_identity_cases() -> (Vec<String>, Vec<String>) {
    let md = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/observability/label-taxonomy.md"),
    )
    .expect("read real label-taxonomy.md");
    let mut lines = md
        .lines()
        .skip_while(|l| l.trim() != "```identity-label-cases");
    assert!(
        lines.next().is_some(),
        "```identity-label-cases block missing"
    );
    let (mut fire, mut pass) = (Vec::new(), Vec::new());
    for line in lines.take_while(|l| l.trim() != "```") {
        let Some((name, list)) = line.split_once('=') else {
            continue;
        };
        let items = list
            .split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty());
        match name.trim() {
            "fire" => fire.extend(items),
            "pass" => pass.extend(items),
            other => panic!("unknown identity-label-cases line {other:?}"),
        }
    }
    (fire, pass)
}

#[test]
fn identity_matcher_runs_the_shared_case_table() {
    let p = real_policy();
    let (fire, pass) = real_identity_cases();
    // Positive control: a truncated or renamed block must not pass vacuously.
    assert!(!fire.is_empty() && !pass.is_empty(), "case lists empty");
    assert!(fire.iter().any(|k| k == "user-id"), "fire lacks user-id");
    for key in &fire {
        assert!(p.matched_term(key).is_some(), "{key} must fire");
    }
    for key in &pass {
        assert_eq!(p.matched_term(key), None, "{key} must NOT fire");
    }
}

#[test]
fn identity_meeting_id_hash_in_keep_keys_fires_with_its_own_message() {
    let f = Tree::valid()
        .edit(COLLECTOR, "\"mode\"]", "\"mode\", \"meeting_id_hash\"]")
        .findings();
    assert_fires_with(&f, IDENTITY_KEY_FORWARDED_RULE_ID, "ADR-0036 §11");
}

#[test]
fn identity_kept_participant_key_fires() {
    let f = Tree::valid()
        .edit(COLLECTOR, "\"mode\"]", "\"mode\", \"participantId\"]")
        .findings();
    assert_fires_with(&f, IDENTITY_KEY_FORWARDED_RULE_ID, "participantId");
}

#[test]
fn identity_forwarded_extra_key_fires() {
    let f = Tree::valid()
        .edit(
            GC_FILTER,
            "[&str; 6] = [\n    \"reason\",",
            "[&str; 7] = [\n    \"reason\",\n    \"stream_id\",",
        )
        .findings();
    assert_fires_with(&f, IDENTITY_KEY_FORWARDED_RULE_ID, "stream_id");
}

#[test]
fn identity_policy_block_missing_is_empty_input() {
    let f = Tree::valid()
        .edit(TAXONOMY, "```identity-label-policy", "```text")
        .findings();
    assert_empty_input_for(&f, TAXONOMY);
}

#[test]
fn identity_policy_block_without_canary_is_empty_input() {
    let f = Tree::valid().edit(TAXONOMY, "participant, ", "").findings();
    assert_empty_input_for(&f, TAXONOMY);
}

#[test]
fn identity_policy_block_with_unknown_line_is_empty_input() {
    let f = Tree::valid()
        .edit(TAXONOMY, "exempt = ", "prefix = x\nexempt = ")
        .findings();
    assert_empty_input_for(&f, TAXONOMY);
}

// -----------------------------------------------------------------------------
// Rule 1: label bags at exported emissions.
// -----------------------------------------------------------------------------

#[test]
fn rule1_shorthand_key_not_forwarded_fires() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "{ ...this.#base, reason }",
            "{ ...this.#base, reason, bucket }",
        )
        .findings();
    assert_fires_with(&f, LABEL_KEY_NOT_FORWARDED_RULE_ID, "\"bucket\"");
}

#[test]
fn rule1_brace_inside_a_label_value_does_not_desync_the_scan() {
    // Clean: '{' / '(' inside string values are not structure.
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "{ ...this.#base, reason }",
            "{ ...this.#base, reason: '{(', action: `}` }",
        )
        .findings();
    assert_clean(&f);
    // And a key AFTER such a value is still read.
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "{ ...this.#base, reason }",
            "{ ...this.#base, reason: '{', bucket: 1 }",
        )
        .findings();
    assert_fires_with(&f, LABEL_KEY_NOT_FORWARDED_RULE_ID, "\"bucket\"");
}

#[test]
fn rule1_quoted_key_is_read() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "'outcome': outcome",
            "\"sender_idx\": outcome",
        )
        .findings();
    assert_fires_with(&f, LABEL_KEY_NOT_FORWARDED_RULE_ID, "sender_idx");
}

#[test]
fn rule1_undeclared_wrapper_is_unresolved() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "this.#sink?.counter('dt_client_media_kek_retention_violations_total', this.#base);",
            "this.#emit('dt_client_media_kek_retention_violations_total');",
        )
        .edit(
            MEDIA_METRICS,
            "  frameSent(): void {",
            "  #emit(name: string): void {\n    this.#sink?.counter(name, { ...this.#base, stream_id: 's' });\n  }\n\n  frameSent(): void {",
        )
        .findings();
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "non-literal metric name");
}

#[test]
fn rule1_name_via_const_variable_is_unresolved() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "this.#sink?.counter('dt_client_media_kek_retention_violations_total', this.#base);",
            "const N = 'dt_client_media_kek_retention_violations_total';\n    this.#sink?.counter(N, this.#base);",
        )
        .findings();
    assert_fires_with(
        &f,
        UNRESOLVED_LABEL_BAG_RULE_ID,
        "other than as a call's first argument",
    );
    assert_fires_with(
        &f,
        UNRESOLVED_LABEL_BAG_RULE_ID,
        "non-literal metric name `N`",
    );
}

#[test]
fn rule1_unresolvable_spread_on_exported_emission_is_unresolved() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "{ ...this.#base, reason }",
            "{ ...this.#extra, reason }",
        )
        .findings();
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "this.#extra");
}

#[test]
fn rule1_interpolated_backtick_name_is_unresolved() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "`dt_client_media_frames_dropped_total`",
            "`dt_client_media_${kind}_total`",
        )
        .findings();
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "interpolated template");
}

#[test]
fn rule1_non_literal_sink_call_is_unresolved_even_for_unexported_names() {
    let f = Tree::valid()
        .set(
            "packages/sdk-core/src/session/helper.ts",
            "export function emit(sink: MetricsSink, n: string): void {\n  sink.gauge(n, {}, 1);\n}\n",
        )
        .findings();
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "helper.ts");
}

#[test]
fn rule1_sink_implementation_files_are_exempt() {
    // The baseline OtelMetricsSink forwards `name` — clean. Move the same code
    // to a non-sink file and it fires.
    let body = Tree::valid().files.get(OTEL_SINK).expect("sink").clone();
    let f = Tree::valid()
        .set("packages/sdk-core/src/telemetry/OtherSink.ts", &body)
        .findings();
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "OtherSink.ts");
}

#[test]
fn rule1_unexported_unresolvable_spread_is_out_of_scope_until_exported() {
    // Baseline: MediaTransport's `...this.#metricLabels` is unresolvable there,
    // and dt_client_mh_connection_total is `Exported: no` — clean. Export it:
    let f = Tree::valid()
        .edit(
            CATALOG,
            "### `dt_client_mh_connection_total`\n\n- **Exported**: no",
            "### `dt_client_mh_connection_total`\n\n- **Exported**: yes",
        )
        .findings();
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "this.#metricLabels");
}

#[test]
fn rule1_declared_wrapper_injected_keys_are_checked_once_exported() {
    // Exporting a join-flow name makes its wrapper-injected meeting_id_hash
    // (and the caller's bare failure_stage) a finding.
    let f = Tree::valid()
        .edit(
            CATALOG,
            "### `dt_client_join_attempts_total`\n\n- **Exported**: no",
            "### `dt_client_join_attempts_total`\n\n- **Exported**: yes",
        )
        .findings();
    assert_fires_with(&f, LABEL_KEY_NOT_FORWARDED_RULE_ID, "\"meeting_id_hash\"");
    assert_fires_with(&f, LABEL_KEY_NOT_FORWARDED_RULE_ID, "\"failure_stage\"");
}

#[test]
fn rule1_base_assignment_extended_by_spread_is_unresolved() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "this.#base = mediaMetricLabels(identity);",
            "this.#base = { ...mediaMetricLabels(identity), meeting_id_hash: h };",
        )
        .findings();
    assert_fires_with(
        &f,
        UNRESOLVED_LABEL_BAG_RULE_ID,
        "this.#base = { ...mediaMetricLabels",
    );
}

#[test]
fn rule1_base_member_write_and_object_assign_are_unresolved() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "    this.#sink = sink;\n",
            "    this.#sink = sink;\n    this.#base.meeting_id_hash = 'h';\n    Object.assign(this.#base, { x: 1 });\n",
        )
        .findings();
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "member write");
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "Object.assign(this.#base");
}

#[test]
fn rule1_no_conforming_base_assignment_is_empty_input() {
    let f = Tree::valid()
        .edit(
            MEDIA_METRICS,
            "    this.#base = mediaMetricLabels(identity);\n",
            "",
        )
        .findings();
    assert_fires_with(&f, EMPTY_INPUT_RULE_ID, "this.#base = mediaMetricLabels");
}

#[test]
fn rule1_injected_bag_non_literal_rhs_is_unresolved() {
    let f = Tree::valid()
        .edit(
            MEETING_SESSION,
            "    this.#histogram('dt_client_time_to_signaling_ready_ms');",
            "    this.#metricLabels = buildLabels(options);\n    this.#histogram('dt_client_time_to_signaling_ready_ms');",
        )
        .findings();
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "non-literal assignment");
}

#[test]
fn rule1_injected_bag_member_write_is_unresolved_but_comparison_is_not() {
    let f = Tree::valid()
        .edit(
            MEETING_SESSION,
            "    this.#histogram('dt_client_time_to_signaling_ready_ms');",
            "    this.#metricLabels[key] += 'x';\n    this.#histogram('dt_client_time_to_signaling_ready_ms');",
        )
        .findings();
    assert_fires_with(&f, UNRESOLVED_LABEL_BAG_RULE_ID, "member write");
    let f = Tree::valid()
        .edit(
            MEETING_SESSION,
            "    this.#histogram('dt_client_time_to_signaling_ready_ms');",
            "    if (this.#metricLabels === prev || this.#metricLabels.org_id == 'x') {}\n    this.#histogram('dt_client_time_to_signaling_ready_ms');",
        )
        .findings();
    assert_clean(&f);
}

#[test]
fn rule1_exported_name_through_non_sink_callee_is_unresolved() {
    let f = Tree::valid()
        .set(
            "packages/sdk-core/src/media/probe.ts",
            "emitVia('dt_client_media_frames_sent_total', {});\n",
        )
        .set(
            "packages/sdk-core/src/media/probeHelper.ts",
            "export function emitVia(name: string, labels: MetricLabels): void {\n  const { counter } = globalThis.sink;\n  counter(name, { ...globalThis.joinBag, ...labels });\n}\n",
        )
        .findings();
    assert_fires_with(
        &f,
        UNRESOLVED_LABEL_BAG_RULE_ID,
        "non-sink callee `emitVia`",
    );
}

#[test]
fn rule1_media_base_resolution_positive_control() {
    let f = Tree::valid()
        .edit(MEDIA_METRICS, "    key_custody: KEY_CUSTODY,\n", "")
        .findings();
    assert_empty_input_for(&f, MEDIA_METRICS);
}

#[test]
fn rule1_declared_wrapper_gone_is_empty_input() {
    let f = Tree::valid()
        .edit(
            MEETING_SESSION,
            "#histogram(name: string): void {",
            "#hist(name: string): void {",
        )
        .findings();
    assert_empty_input_for(&f, MEETING_SESSION);
}

#[test]
fn rule1_injected_bag_assignments_gone_is_empty_input() {
    let s = Tree::valid()
        .files
        .get(MEETING_SESSION)
        .expect("ms")
        .replace("this.#metricLabels = {", "this.#metricLabels = build({");
    let f = Tree::valid().set(MEETING_SESSION, &s).findings();
    assert_empty_input_for(&f, MEETING_SESSION);
}

// -----------------------------------------------------------------------------
// Rule 2: loaded dt_client alert references.
// -----------------------------------------------------------------------------

fn with_rule(t: &mut Tree, expr: &str) -> Vec<Finding> {
    t.edit(
        CLIENT_ALERTS,
        "        for: 1m\n",
        &format!("        for: 1m\n      - alert: Extra\n        expr: {expr}\n        for: 1m\n"),
    )
    .findings()
}

#[test]
fn rule2_non_exported_name_fires() {
    let f = with_rule(
        &mut Tree::valid(),
        "sum(rate(dt_client_join_attempts_total[5m])) > 0",
    );
    assert_fires_with(
        &f,
        DEAD_ALERT_REFERENCE_RULE_ID,
        "dt_client_join_attempts_total",
    );
}

#[test]
fn rule2_selector_label_not_kept_fires() {
    let f = with_rule(
        &mut Tree::valid(),
        "sum(rate(dt_client_media_frames_sent_total{status=\"x\"}[5m])) > 0",
    );
    assert_fires_with(&f, DEAD_ALERT_REFERENCE_RULE_ID, "\"status\"");
}

#[test]
fn rule2_by_label_not_kept_fires() {
    let f = with_rule(
        &mut Tree::valid(),
        "sum by (meeting_id_hash) (rate(dt_client_media_frames_sent_total[5m])) > 0",
    );
    assert_fires_with(&f, DEAD_ALERT_REFERENCE_RULE_ID, "meeting_id_hash");
}

#[test]
fn rule2_by_instance_fires_its_own_rule() {
    let f = with_rule(
        &mut Tree::valid(),
        "sum by (instance) (rate(dt_client_media_frames_sent_total[5m])) > 0",
    );
    assert_fires(&f, CLIENT_ALERT_GROUPS_BY_INSTANCE_RULE_ID);
    assert!(!has(&f, DEAD_ALERT_REFERENCE_RULE_ID), "{f:#?}");
}

#[test]
fn rule2_without_or_ignoring_instance_is_clean() {
    let f = with_rule(
        &mut Tree::valid(),
        "sum without (instance) (rate(dt_client_media_frames_sent_total[5m])) / ignoring(instance) sum(rate(dt_client_media_frames_received_total[5m])) > 0",
    );
    assert_clean(&f);
}

#[test]
fn rule2_instance_as_matcher_and_le_job_pass() {
    let f = with_rule(
        &mut Tree::valid(),
        "sum by (le, job) (rate(dt_client_media_frames_sent_total{instance=~\".+\", reason!=\"\"}[5m])) > 0",
    );
    assert_clean(&f);
}

#[test]
fn rule2_comment_only_mentions_do_not_count() {
    // YAML comment (baseline) plus a PromQL `#` comment inside the expr.
    let f = with_rule(
        &mut Tree::valid(),
        "|\n          sum(dt_client_media_kek_retention_violations_total) > 0 # dt_client_ghost_total{bad=\"x\"}\n",
    );
    assert_clean(&f);
}

#[test]
fn rule2_no_dt_client_reference_is_empty_input() {
    let mut t = Tree::valid();
    for p in [MC_ALERTS, CLIENT_ALERTS] {
        let s = t
            .files
            .get(p)
            .expect("rules")
            .replace("dt_client_", "mc_other_");
        t.set(p, &s);
    }
    let f = t.findings();
    assert_empty_input_for(&f, ALERTS_DIR);
    // The dependent rules are skipped, not reported on a broken extraction.
    assert!(!has(&f, TRIPWIRE_LIST_STALE_RULE_ID), "{f:#?}");
}

// -----------------------------------------------------------------------------
// Tripwire shape.
// -----------------------------------------------------------------------------

#[test]
fn tripwire_increase_fires() {
    let f = Tree::valid()
        .edit(
            CLIENT_ALERTS,
            "sum(dt_client_media_kek_retention_violations_total) > 0",
            "increase(dt_client_media_kek_retention_violations_total[15m]) > 0",
        )
        .findings();
    assert_fires_with(&f, TRIPWIRE_RATE_WRAPPED_RULE_ID, "increase()");
}

#[test]
fn tripwire_rate_fires() {
    let f = Tree::valid()
        .edit(
            CLIENT_ALERTS,
            "sum(dt_client_media_kek_retention_violations_total) > 0",
            "rate(dt_client_media_kek_retention_violations_total[5m]) > 0",
        )
        .findings();
    assert_fires_with(&f, TRIPWIRE_RATE_WRAPPED_RULE_ID, "rate()");
}

#[test]
fn tripwire_sum_by_increase_fires() {
    let f = Tree::valid()
        .edit(
            MC_ALERTS,
            "sum(dt_client_media_kek_install_refusals_total{outcome=\"conflicting_key\"}) > 0",
            "sum by(outcome)(increase(dt_client_media_kek_install_refusals_total{outcome=\"conflicting_key\"}[15m])) > 0",
        )
        .findings();
    assert_fires_with(
        &f,
        TRIPWIRE_RATE_WRAPPED_RULE_ID,
        "dt_client_media_kek_install_refusals_total",
    );
}

#[test]
fn tripwire_list_stale_fires() {
    let f = Tree::valid().remove(CLIENT_ALERTS).findings();
    assert_fires_with(
        &f,
        TRIPWIRE_LIST_STALE_RULE_ID,
        "dt_client_media_kek_retention_violations_total",
    );
}

// -----------------------------------------------------------------------------
// G7: reject-token partition.
// -----------------------------------------------------------------------------

fn vectors_with(extra: &[(&str, &str, bool)], drop: &[&str]) -> String {
    let mut rows: Vec<(&str, &str, bool)> = REJECT_TOKENS
        .iter()
        .copied()
        .filter(|(t, _, _)| !drop.contains(t))
        .collect();
    rows.extend_from_slice(extra);
    vectors_json(&rows)
}

#[test]
fn g7_new_dropping_token_unclassified_fires() {
    let f = Tree::valid()
        .set(
            VECTORS,
            &vectors_with(&[("kek_withdrawn", "key", true)], &[]),
        )
        .findings();
    assert_fires_with(&f, REJECT_TOKEN_UNCLASSIFIED_RULE_ID, "kek_withdrawn");
}

#[test]
fn g7_new_non_dropping_token_needs_no_classification() {
    let f = Tree::valid()
        .set(
            VECTORS,
            &vectors_with(&[("advisory_only", "key", false)], &[]),
        )
        .findings();
    assert_clean(&f);
}

#[test]
fn g7_token_in_both_sides_fires() {
    let f = Tree::valid()
        .edit(MC_ALERTS, "|unwrap_failed\"", "|unwrap_failed|truncated\"")
        .findings();
    assert_fires_with(&f, REJECT_TOKEN_MISCLASSIFIED_RULE_ID, "truncated");
}

#[test]
fn g7_non_dropping_token_in_numerator_fires() {
    let f = Tree::valid()
        .edit(
            MC_ALERTS,
            "|unwrap_failed\"",
            "|unwrap_failed|wrap_key_id_mismatch\"",
        )
        .findings();
    assert_fires_with(
        &f,
        REJECT_TOKEN_MISCLASSIFIED_RULE_ID,
        "wrap_key_id_mismatch",
    );
}

#[test]
fn g7_key_delivery_token_dropped_from_alert_is_unclassified() {
    let f = Tree::valid()
        .edit(MC_ALERTS, "|unwrap_failed\"", "\"")
        .findings();
    assert_fires_with(&f, REJECT_TOKEN_UNCLASSIFIED_RULE_ID, "unwrap_failed");
}

#[test]
fn g7_stale_not_key_delivery_entry_fires() {
    let f = Tree::valid()
        .set(VECTORS, &vectors_with(&[], &["sender_not_assigned"]))
        .findings();
    assert_fires_with(
        &f,
        REJECT_TOKEN_MISCLASSIFIED_RULE_ID,
        "sender_not_assigned",
    );
}

#[test]
fn g7_alternation_canary_missing_is_empty_input() {
    let f = Tree::valid()
        .edit(MC_ALERTS, "reason=~\"no_kek_for_generation|", "reason=~\"")
        .findings();
    assert_empty_input_for(&f, ALERTS_DIR);
}

#[test]
fn g7_alert_missing_is_empty_input() {
    let f = Tree::valid()
        .edit(
            MC_ALERTS,
            "alert: MCMediaMissingKeyMaterial",
            "alert: SomethingElse",
        )
        .findings();
    assert_empty_input_for(&f, ALERTS_DIR);
}

#[test]
fn g7_no_dropping_tokens_is_empty_input() {
    let rows: Vec<(&str, &str, bool)> = REJECT_TOKENS
        .iter()
        .map(|(t, l, _)| (*t, *l, false))
        .collect();
    let f = Tree::valid().set(VECTORS, &vectors_json(&rows)).findings();
    assert_empty_input_for(&f, VECTORS);
}

#[test]
fn g7_renamed_required_field_fails_the_run_naming_the_file() {
    let renamed = vectors_json(REJECT_TOKENS).replace("\"drops_frame\"", "\"dropsFrame\"");
    let dir = tempfile::tempdir().expect("tempdir");
    for (rel, content) in &Tree::valid().set(VECTORS, &renamed).files {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, content).expect("write");
    }
    let err = format!(
        "{:#}",
        collect_findings(dir.path()).expect_err("a renamed required field must not default")
    );
    assert!(
        err.contains(VECTORS) && err.contains("drops_frame"),
        "{err}"
    );
}
