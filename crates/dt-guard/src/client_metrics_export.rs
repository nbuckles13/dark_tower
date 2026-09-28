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
//! * **G1** `client.md` `Exported: yes` set == the collector's metric-name
//!   include list, compared in BOTH directions. A name in the config and not the
//!   catalog is an undocumented exported series; a name catalogued as exported
//!   and missing from the config fails closed and silently.
//! * **G2** every `dt_client_*` literal in `mediaMetrics.ts` carries an explicit
//!   `Exported:` marker in the catalog. Absent is not "no" — a new media metric
//!   must force an export DECISION rather than defaulting to invisible.
//! * **G3** every `Exported: yes` name has an emitter somewhere in sdk-core
//!   outside tests, so a catalogued-and-exported series cannot be permanently
//!   empty.
//! * **G4** the collector's `keep_keys` set is a subset of GC's
//!   `ALLOWLIST ∪ MEDIA_DATAPOINT_EXTRA`. A kept key that GC always strips is a
//!   label that can never arrive.
//! * **G5** `metric_expiration` strictly exceeds the longest `dt_client_*`
//!   range-vector window in a loaded rule, and `max_stale >= metric_expiration +
//!   that window`. Both keys must be PRESENT, so a key renamed by a future
//!   collector version goes red instead of quietly passing.
//! * **G6** the collector's rewrite sentinel collides with no real token in
//!   EITHER the TypeScript media vocabularies or MH's `MediaDropReason`.
//!
//! # Anti-vacuity contract, uniform across all six rules
//!
//! Every extractor has a positive control and fails CLOSED on an empty or short
//! parse, with [`EMPTY_INPUT_RULE_ID`] — a reason token deliberately DISTINCT
//! from every content-failure reason. If an extraction break (a moved file, a
//! renamed export, a regex that stops matching after a refactor) reported the
//! same reason as a content failure, it would be mis-triaged as a content
//! result and "fixed" by loosening the extractor, leaving a guard that is green
//! forever while checking nothing.

use crate::common::metric_catalog::CATALOG_HEAD_RE;
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

const CATALOG: &str = "docs/observability/metrics/client.md";
const COLLECTOR_CONFIG: &str = "infra/services/otel-collector/collector.yaml";
const GC_FILTER: &str = "crates/gc-service/src/services/telemetry_filter.rs";
const MEDIA_METRICS_TS: &str = "packages/sdk-core/src/media/setup/mediaMetrics.ts";
const REJECT_REASON_TS: &str = "packages/sdk-core/src/media/frame/rejectReason.ts";
const RECEIVE_PATH_TS: &str = "packages/sdk-core/src/media/frame/receivePath.ts";
const MH_METRICS_RS: &str = "crates/mh-service/src/observability/metrics.rs";
const SDK_SRC: &str = "packages/sdk-core/src";
const ALERTS_SUBDIR: &str = "infra/docker/prometheus/rules";

/// The value the collector rewrites a shape-failing label value to.
///
/// Mirrors `transform/client_metric_labels` in the collector config. If that
/// string changes, change it here — G6 is the check that the two agree about
/// what is reserved, not a derivation of it.
const REWRITE_SENTINEL: &str = "invalid";

/// Positive-control token for the TypeScript vocabulary extractor.
///
/// `no_roster_entry` is one of the two reasons `MCMediaMissingKeyMaterial`
/// selects on, so if the extractor ever stops seeing it, the guard is blind to
/// precisely the vocabulary the alert depends on.
const TS_VOCAB_CANARY: &str = "no_roster_entry";

pub const EMPTY_INPUT_RULE_ID: &str = "extractor_empty_input";
pub const EXPORT_SET_MISMATCH_RULE_ID: &str = "export_set_mismatch";
pub const MISSING_EXPORT_MARKER_RULE_ID: &str = "missing_export_marker";
pub const EXPORTED_WITHOUT_EMITTER_RULE_ID: &str = "exported_without_emitter";
pub const KEEP_KEY_NOT_FORWARDED_RULE_ID: &str = "keep_key_not_forwarded";
pub const EXPIRATION_RELATION_RULE_ID: &str = "expiration_relation";
pub const SENTINEL_COLLISION_RULE_ID: &str = "sentinel_collision";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static EXPORTED_MARKER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*(?:[-*]\s*)?(?:\*\*)?Exported(?:\*\*)?\s*:\s*(yes|no)\b")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static DT_CLIENT_LITERAL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"'(dt_client_[a-z0-9_]+)'").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static TS_TOKEN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"'([a-z][a-z0-9_]*)'").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static RANGE_WINDOW_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[(\d+)([smhd])\]").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static DURATION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(\d+)([smhd])\s*$").expect("static pattern compiles"));

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

#[derive(Debug)]
struct Finding {
    file: String,
    rule_id: &'static str,
    message: String,
}

impl Finding {
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

// -----------------------------------------------------------------------------
// Rust array / enum-arm extraction.
//
// SHARED MECHANISM, PER-CALLER POLICY. This helper PARSES ONLY. Every caller
// keeps its OWN expected-count assertion and its OWN empty-parse reason,
// deliberately: the counts are DIFFERENT FACTS that merely sit behind one
// parser (GC's two allowlists on one side, `MediaDropReason`'s compile-checked
// `ALL` length on the other), so collapsing them would be a false SSoT over
// unrelated values. `crates/dt-guard/src/common/metric_catalog.rs` documents the
// same hazard for a shared regex: two callers reading one parser in opposite
// directions means a degraded parse "fails LOUD in one and SILENT in the other".
// If you are here to hoist the duplicated count checks — that is the thing this
// comment exists to stop.
// -----------------------------------------------------------------------------

/// Collect the string literals inside a `const NAME: [&str; N] = [ ... ];` block.
fn rust_str_array(src: &str, const_name: &str) -> Vec<String> {
    let Some(start) = src.find(&format!("{const_name}: [&str;")) else {
        return Vec::new();
    };
    let Some(open) = src[start..].find('[').map(|i| start + i) else {
        return Vec::new();
    };
    // Skip past the type's own `[&str; N]` bracket pair to the initializer.
    let Some(eq) = src[open..].find('=').map(|i| open + i) else {
        return Vec::new();
    };
    let Some(body_start) = src[eq..].find('[').map(|i| eq + i) else {
        return Vec::new();
    };
    let Some(body_end) = src[body_start..].find("];").map(|i| body_start + i) else {
        return Vec::new();
    };
    let body = &src[body_start..body_end];
    // COMMA-SPLIT, NOT LINE-SPLIT. `cargo fmt` reflows a short array onto one
    // line, so anything keyed on "one entry per line" silently under-counts
    // after a formatting pass — and under-counting here reads as an extraction
    // failure on a perfectly healthy file. The element separator is the stable
    // structure; the line breaks are not.
    let mut out = Vec::new();
    for raw in body.trim_start_matches('[').split(',') {
        let token = raw
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(caps) = QUOTED_STR_RE.captures(token) {
            if let Some(m) = caps.get(1) {
                out.push(m.as_str().to_string());
                continue;
            }
        }
        // A bare identifier entry (e.g. a `const` imported from another module)
        // is a real element this parser cannot resolve to its value. Record it
        // as a placeholder rather than skipping it, so a caller's length check
        // cannot be fooled into reporting a short parse by a const it can't read.
        if token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            out.push(format!("<const:{token}>"));
        }
    }
    out
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

/// The declared length of a `const ALL: [Self; N]` array — a compile-checked
/// number, which is what makes it a structural positive control rather than a
/// hand-picked token.
fn declared_all_len(src: &str) -> Option<usize> {
    ALL_LEN_RE.captures(src)?.get(1)?.as_str().parse().ok()
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

/// A `key: <duration>` scalar under the collector config, as seconds.
fn collector_duration(cfg: &str, key: &str) -> Option<u64> {
    for line in cfg.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(&format!("{key}:")) {
            if let Some(caps) = DURATION_RE.captures(rest) {
                let n: u64 = caps.get(1)?.as_str().parse().ok()?;
                return Some(n * unit_seconds(caps.get(2)?.as_str()));
            }
        }
    }
    None
}

fn unit_seconds(unit: &str) -> u64 {
    match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => 0,
    }
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
// Alert-rule window extraction.
// -----------------------------------------------------------------------------

/// The longest range-vector window, in seconds, over any `dt_client_*` selector
/// in a LOADED rule file (the `_`-prefixed templates are not loaded).
fn longest_dt_client_window(alerts_dir: &Path) -> Result<(u64, usize)> {
    let mut longest = 0u64;
    let mut files_scanned = 0usize;
    if !alerts_dir.is_dir() {
        return Ok((0, 0));
    }
    for entry in std::fs::read_dir(alerts_dir).context("read alerts dir")? {
        let entry = entry.context("read alerts dir entry")?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with('_') || !name.ends_with("-alerts.yaml") {
            continue;
        }
        files_scanned += 1;
        let src = std::fs::read_to_string(entry.path())
            .with_context(|| format!("read rule file {name}"))?;
        for line in src.lines() {
            if !line.contains("dt_client_") {
                continue;
            }
            for caps in RANGE_WINDOW_RE.captures_iter(line) {
                let (Some(n), Some(u)) = (caps.get(1), caps.get(2)) else {
                    continue;
                };
                let secs = n.as_str().parse::<u64>().unwrap_or(0) * unit_seconds(u.as_str());
                longest = longest.max(secs);
            }
        }
    }
    Ok((longest, files_scanned))
}

// -----------------------------------------------------------------------------
// Run.
// -----------------------------------------------------------------------------

#[expect(
    clippy::too_many_lines,
    reason = "six rules over six artifacts; splitting per rule would scatter the shared extraction results and the uniform empty-input contract that binds them"
)]
pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let catalog_path = repo_root.join(CATALOG);
    let config_path = repo_root.join(COLLECTOR_CONFIG);
    match (catalog_path.is_file(), config_path.is_file()) {
        // Neither artifact exists (a tree without the client-metrics surface):
        // nothing to compare, and saying so is the honest result.
        (false, false) => {
            emit_ok("client-metrics-export-no-artifacts");
            return Ok(());
        }
        // ONE side missing is a moved or deleted file, not an absent surface.
        // Treating it as "no artifacts" made this guard go quiet the moment the
        // collector config moved (ADR-0038 renamed configmap.yaml): fail loudly.
        (true, false) | (false, true) => {
            let (missing, present) = if catalog_path.is_file() {
                (COLLECTOR_CONFIG, CATALOG)
            } else {
                (CATALOG, COLLECTOR_CONFIG)
            };
            return report(
                vec![empty_input(
                    missing,
                    &format!("file at all (it is missing while {present} exists)"),
                )],
                explain,
            );
        }
        (true, true) => {}
    }

    let catalog =
        std::fs::read_to_string(&catalog_path).with_context(|| format!("read {CATALOG}"))?;
    let config = std::fs::read_to_string(&config_path)
        .with_context(|| format!("read {COLLECTOR_CONFIG}"))?;

    let mut findings: Vec<Finding> = Vec::new();

    // ---- extraction, with per-source empty-input checks ----
    let markers = catalog_export_markers(&catalog);
    if markers.is_empty() {
        findings.push(empty_input(CATALOG, "`### `dt_client_*`` metric headings"));
    }
    let config_names = collector_metric_names(&config);
    if config_names.is_empty() {
        findings.push(empty_input(
            COLLECTOR_CONFIG,
            "metric names in the filter/client_metric_names include list",
        ));
    }
    let keep_keys = collector_keep_keys(&config);
    if keep_keys.is_empty() {
        findings.push(empty_input(COLLECTOR_CONFIG, "keys in the keep_keys call"));
    }

    // If extraction is already broken there is nothing trustworthy to compare;
    // reporting content findings on top would bury the real cause.
    if !findings.is_empty() {
        return report(findings, explain);
    }

    let exported: BTreeSet<String> = markers
        .iter()
        .filter(|(_, e)| *e == Some(true))
        .map(|(n, _)| n.clone())
        .collect();
    let config_set: BTreeSet<String> = config_names.iter().cloned().collect();

    // ---- G1: set equality, both directions, naming which side diverged ----
    for name in config_set.difference(&exported) {
        findings.push(Finding {
            file: COLLECTOR_CONFIG.to_string(),
            rule_id: EXPORT_SET_MISMATCH_RULE_ID,
            message: format!(
                "{name} is exported by the collector but is not marked `Exported: yes` in \
                 {CATALOG} — an undocumented exported series. Add the catalog entry, or \
                 remove the name from the include list."
            ),
        });
    }
    for name in exported.difference(&config_set) {
        findings.push(Finding {
            file: CATALOG.to_string(),
            rule_id: EXPORT_SET_MISMATCH_RULE_ID,
            message: format!(
                "{name} is marked `Exported: yes` but is missing from the \
                 filter/client_metric_names list in {COLLECTOR_CONFIG}. It will be dropped \
                 silently and look identical to 'no browser is running' — the exact failure \
                 this guard exists to prevent."
            ),
        });
    }

    // ---- G2: every emitted media literal has an explicit marker ----
    let media_ts_path = repo_root.join(MEDIA_METRICS_TS);
    if media_ts_path.is_file() {
        let media_src = std::fs::read_to_string(&media_ts_path)
            .with_context(|| format!("read {MEDIA_METRICS_TS}"))?;
        let literals: BTreeSet<String> = DT_CLIENT_LITERAL_RE
            .captures_iter(&media_src)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();
        if literals.is_empty() {
            findings.push(empty_input(
                MEDIA_METRICS_TS,
                "`dt_client_*` metric literals",
            ));
        }
        let declared: HashSet<&str> = markers
            .iter()
            .filter(|(_, e)| e.is_some())
            .map(|(n, _)| n.as_str())
            .collect();
        for name in &literals {
            if !declared.contains(name.as_str()) {
                findings.push(Finding {
                    file: CATALOG.to_string(),
                    rule_id: MISSING_EXPORT_MARKER_RULE_ID,
                    message: format!(
                        "{name} is emitted in {MEDIA_METRICS_TS} but has no explicit \
                         `Exported: yes|no` marker in {CATALOG}. Absent is NOT 'no': a new \
                         client metric must force an export decision, not default to invisible."
                    ),
                });
            }
        }
    }

    // ---- G3: every exported name has an emitter ----
    let sdk_src = repo_root.join(SDK_SRC);
    if sdk_src.is_dir() {
        let emitters = sdk_emitter_literals(&sdk_src)?;
        if emitters.is_empty() {
            findings.push(empty_input(SDK_SRC, "`dt_client_*` emitter literals"));
        } else {
            for name in &exported {
                if !emitters.contains(name) {
                    findings.push(Finding {
                        file: CATALOG.to_string(),
                        rule_id: EXPORTED_WITHOUT_EMITTER_RULE_ID,
                        message: format!(
                            "{name} is marked `Exported: yes` but no non-test source under \
                             {SDK_SRC} emits it — it would be a permanently empty panel that \
                             reads as a quiet system."
                        ),
                    });
                }
            }
        }
    }

    // ---- G4: keep_keys ⊆ GC's forwarded key set ----
    let gc_path = repo_root.join(GC_FILTER);
    if gc_path.is_file() {
        let gc_src =
            std::fs::read_to_string(&gc_path).with_context(|| format!("read {GC_FILTER}"))?;
        let base = rust_str_array(&gc_src, "ALLOWLIST");
        let extra = rust_str_array(&gc_src, "MEDIA_DATAPOINT_EXTRA");
        // Per-caller count assertion: these are GC's own declared lengths, a
        // different fact from G6's enum-arm count. Do not merge them.
        if base.len() < 12 || extra.len() < 5 {
            findings.push(empty_input(
                GC_FILTER,
                "the full ALLOWLIST (12) and MEDIA_DATAPOINT_EXTRA (5) entries",
            ));
        } else {
            let mut forwarded: HashSet<String> = base.into_iter().collect();
            forwarded.extend(extra);
            // `<const:KEY_CUSTODY_LABEL>` stands for the key_custody const the
            // parser cannot resolve; accept its resolved spelling explicitly.
            forwarded.insert("key_custody".to_string());
            for key in &keep_keys {
                if !forwarded.contains(key) {
                    findings.push(Finding {
                        file: COLLECTOR_CONFIG.to_string(),
                        rule_id: KEEP_KEY_NOT_FORWARDED_RULE_ID,
                        message: format!(
                            "keep_keys retains {key:?}, but GC's filter never forwards it \
                             ({GC_FILTER}) — the label can never arrive, so keeping it is a \
                             no-op that reads as coverage."
                        ),
                    });
                }
            }
        }
    }

    // ---- G5: expiration / max_stale relations, both keys required ----
    let expiration = collector_duration(&config, "metric_expiration");
    let max_stale = collector_duration(&config, "max_stale");
    let (window, rule_files) = longest_dt_client_window(&repo_root.join(ALERTS_SUBDIR))?;
    match (expiration, max_stale) {
        (None, _) | (_, None) => findings.push(Finding {
            file: COLLECTOR_CONFIG.to_string(),
            rule_id: EMPTY_INPUT_RULE_ID,
            message: format!(
                "could not read both `metric_expiration` and `max_stale` from \
                 {COLLECTOR_CONFIG}. These key names have MOVED between collector versions; \
                 a rename must fail here rather than silently skip the relation check."
            ),
        }),
        (Some(exp), Some(stale)) => {
            if rule_files == 0 {
                findings.push(empty_input(
                    ALERTS_SUBDIR,
                    "loadable `*-alerts.yaml` rule files",
                ));
            }
            if window > 0 && exp <= window {
                findings.push(Finding {
                    file: COLLECTOR_CONFIG.to_string(),
                    rule_id: EXPIRATION_RELATION_RULE_ID,
                    message: format!(
                        "metric_expiration ({exp}s) must strictly EXCEED the longest \
                         dt_client_* range window in a loaded rule ({window}s), or a series \
                         expires mid-window while a client is merely slow and the alert stops \
                         evaluating instead of firing."
                    ),
                });
            }
            if stale < exp + window {
                findings.push(Finding {
                    file: COLLECTOR_CONFIG.to_string(),
                    rule_id: EXPIRATION_RELATION_RULE_ID,
                    message: format!(
                        "max_stale ({stale}s) must be >= metric_expiration + longest window \
                         ({exp}s + {window}s). Otherwise the accumulator can forget while \
                         samples of the exposed series are still inside a rate() window, and \
                         the re-entering low value under-counts with no visible reset."
                    ),
                });
            }
        }
    }

    // ---- G6: the rewrite sentinel collides with no real token ----
    findings.extend(sentinel_collisions(repo_root)?);

    report(findings, explain)
}

/// `dt_client_*` literals emitted anywhere in sdk-core outside test files.
fn sdk_emitter_literals(sdk_src: &Path) -> Result<HashSet<String>> {
    let mut out = HashSet::new();
    for entry in walkdir::WalkDir::new(sdk_src)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|e| e != "ts") {
            continue;
        }
        let as_str = path.to_string_lossy();
        if as_str.contains("__tests__") || as_str.contains(".test.") {
            continue;
        }
        let src = std::fs::read_to_string(path)
            .with_context(|| format!("read sdk source {}", path.display()))?;
        for caps in DT_CLIENT_LITERAL_RE.captures_iter(&src) {
            if let Some(m) = caps.get(1) {
                out.insert(m.as_str().to_string());
            }
        }
    }
    Ok(out)
}

/// G6. Reads BOTH vocabularies — the TypeScript media enums AND MH's Rust
/// `MediaDropReason` — because four of the client's send-drop spellings are
/// deliberately mirrored from MH's so `sum by(reason)` compares across the hop.
/// A sentinel colliding on either side makes a rewritten junk value
/// indistinguishable from a legitimate one on a query spanning both.
fn sentinel_collisions(repo_root: &Path) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();

    // --- TypeScript leg ---
    let mut ts_tokens: HashSet<String> = HashSet::new();
    let mut ts_files_read = 0usize;
    for rel in [MEDIA_METRICS_TS, REJECT_REASON_TS, RECEIVE_PATH_TS] {
        let path = repo_root.join(rel);
        if !path.is_file() {
            continue;
        }
        ts_files_read += 1;
        let src = std::fs::read_to_string(&path).with_context(|| format!("read {rel}"))?;
        for caps in TS_TOKEN_RE.captures_iter(&src) {
            if let Some(m) = caps.get(1) {
                ts_tokens.insert(m.as_str().to_string());
            }
        }
    }
    if ts_files_read > 0 && !ts_tokens.contains(TS_VOCAB_CANARY) {
        // Positive control: if the extractor cannot see the token the alert
        // itself selects on, it is not seeing the vocabulary at all.
        findings.push(empty_input(
            MEDIA_METRICS_TS,
            &format!("the media vocabularies (canary {TS_VOCAB_CANARY} absent)"),
        ));
    } else if ts_tokens.contains(REWRITE_SENTINEL) {
        findings.push(Finding {
            file: MEDIA_METRICS_TS.to_string(),
            rule_id: SENTINEL_COLLISION_RULE_ID,
            message: format!(
                "{REWRITE_SENTINEL:?} is the collector's rewrite sentinel \
                 ({COLLECTOR_CONFIG}) and must never be a real token in a media vocabulary — \
                 a rewritten junk value from a patched client would be indistinguishable \
                 from a legitimate one."
            ),
        });
    }

    // --- MH Rust leg ---
    let mh_path = repo_root.join(MH_METRICS_RS);
    if mh_path.is_file() {
        let src =
            std::fs::read_to_string(&mh_path).with_context(|| format!("read {MH_METRICS_RS}"))?;
        let arms = rust_match_arm_literals(&src, "enum MediaDropReason");
        // Per-caller count assertion, structural rather than hand-picked: the
        // declared `ALL: [Self; N]` length is compile-checked on MH's side, so
        // extracting fewer arms than that means the parse degraded. This is a
        // DIFFERENT fact from G4's allowlist lengths — see the helper comment.
        match declared_all_len(&src) {
            Some(declared) if arms.len() >= declared => {
                if arms.iter().any(|a| a == REWRITE_SENTINEL) {
                    findings.push(Finding {
                        file: MH_METRICS_RS.to_string(),
                        rule_id: SENTINEL_COLLISION_RULE_ID,
                        message: format!(
                            "{REWRITE_SENTINEL:?} is the collector's rewrite sentinel \
                             ({COLLECTOR_CONFIG}) and must never be a MediaDropReason value: \
                             four of these spellings are mirrored by the client, so a \
                             collision poisons `sum by(reason)` across the hop."
                        ),
                    });
                }
            }
            _ => findings.push(empty_input(
                MH_METRICS_RS,
                "MediaDropReason arms matching its declared ALL length",
            )),
        }
    }

    Ok(findings)
}

fn report(findings: Vec<Finding>, explain: bool) -> Result<()> {
    if findings.is_empty() {
        emit_ok("client-metrics-export-clean");
        return Ok(());
    }
    for f in &findings {
        f.print(explain);
    }
    anyhow::bail!("client-metrics-export: {} violation(s)", findings.len());
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let got = rust_str_array(one_line, "MEDIA_DATAPOINT_EXTRA");
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
            vec!["client_version", "org_id", "error.code"]
        );
        let extra = rust_str_array(src, "MEDIA_DATAPOINT_EXTRA");
        assert!(extra.contains(&"reason".to_string()));
        assert!(
            extra.iter().any(|e| e.starts_with("<const:")),
            "an unresolved const entry must still be counted, so the caller's \
             length check cannot be fooled by a const it can't read"
        );
    }

    #[test]
    fn extracts_match_arm_literals_and_declared_all_len() {
        let src = r#"
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
        assert_eq!(declared_all_len(src), Some(2));
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
        assert!(arms.len() < declared_all_len(src).unwrap());
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
        assert!(rust_str_array("fn main() {}", "ALLOWLIST").is_empty());
        assert!(rust_match_arm_literals("fn main() {}", "enum MediaDropReason").is_empty());
    }

    #[test]
    fn window_extraction_reads_the_longest_dt_client_range() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("mc-alerts.yaml"),
            "expr: sum(rate(dt_client_media_frames_dropped_total[5m])) / \
             sum(rate(dt_client_media_frames_received_total[5m]))\n\
             other: rate(mc_something_total[30m])\n",
        )
        .unwrap();
        // A template file must NOT be read: it is never loaded by Prometheus.
        std::fs::write(
            dir.path().join("_template-service-alerts.yaml"),
            "expr: rate(dt_client_foo[99h])\n",
        )
        .unwrap();
        let (window, files) = longest_dt_client_window(dir.path()).unwrap();
        assert_eq!(
            window, 300,
            "30m is not on a dt_client_ selector; 99h is a template"
        );
        assert_eq!(files, 1);
    }

    /// One artifact present and the other missing is a moved/deleted file, not
    /// an absent surface, so it must be red, never `no-artifacts`.
    #[test]
    fn one_missing_artifact_fails_instead_of_reporting_no_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let catalog = root.join(CATALOG);
        std::fs::create_dir_all(catalog.parent().unwrap()).unwrap();
        std::fs::write(&catalog, "### `dt_client_x`\n").unwrap();
        assert!(
            run(root, false).is_err(),
            "catalog without collector config"
        );
        std::fs::remove_file(&catalog).unwrap();
        let cfg = root.join(COLLECTOR_CONFIG);
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, CFG).unwrap();
        assert!(
            run(root, false).is_err(),
            "collector config without catalog"
        );
        std::fs::remove_file(&cfg).unwrap();
        assert!(
            run(root, false).is_ok(),
            "neither present is still no-artifacts"
        );
    }
}
