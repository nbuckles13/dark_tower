//! Extraction kernels for "are the alert rules Prometheus is meant to load
//! actually loaded", plus their FIRE fixtures.
//!
//! # Why the kernels are here and not inline in the cluster-gated test
//!
//! `cargo test` cannot reach a cluster-gated file, so an extractor written
//! inline in one gets **zero unit coverage** — and this repo has already paid
//! for that. `tests/32_media_metric_hygiene.rs` records an earlier check that
//! line-parsed `/api/v1/status/config`, which Prometheus returns as an
//! **escaped JSON string with zero real newlines**: every extraction matched
//! nothing, and the check passed on every possible input including a real
//! violation. Same split applied here — the kernels are pure and unit-tested
//! against known input; the cluster-gated test supplies only real bytes and
//! renders failures.
//!
//! Both kernels therefore parse **structured data as structured data**: the
//! on-disk rule files as YAML, `/api/v1/rules` as JSON. A `grep 'alert:'` over
//! the YAML would sweep in `mh-alerts.yaml`'s header comment lines
//! (`# - No RegisterMeeting RPC alert: ...`) and produce expectations that can
//! never be satisfied.
//!
//! # There is NO file-set predicate here, deliberately
//!
//! An earlier version of this module hand-wrote one (`is_loadable_rules_file`)
//! and its header claimed the two were "kept honest by `dt-guard
//! alert-rules-policy`'s `rule_file_loading` rule". **That claim was false**:
//! `check_rule_file_loading` reads the on-disk predicate, the two Prometheus
//! `rule_files` globs and the kustomize `files:` list, and never opens anything
//! under `crates/env-tests/`. Nothing bound them, and they already disagreed —
//! for `Foo-alerts.yaml` dt-guard's predicate said loadable and the local copy
//! said not. A comment asserting coverage that does not exist is the exact
//! pattern this whole task exists to close, and it was sitting in the task's own
//! artifact.
//!
//! So the predicate is gone rather than documented. [`mounted_rules_files`]
//! reads the **kustomize `configMapGenerator` file list** — the artifact that
//! decides which rule files are actually in the pod this test queries. That is
//! the thing under test, so the extraction structurally cannot disagree with it.
//!
//! **Why deriving from THIS member of the set is safe** — the reasoning that
//! should stop the next reader "simplifying" it back to a local predicate:
//! `check_rule_file_loading` pins four things as SET-EQUAL — the K8s
//! `rule_files` glob, the docker `rule_files` glob, this `configMapGenerator`
//! list, and the on-disk `loadable_rules_files` predicate — and fails in either
//! direction. Deriving from **any member of that pinned set** closes the loop;
//! writing a fifth predicate here opens it.
//!
//! **Why the ConfigMap list rather than the glob**, which @test and
//! @infrastructure both named as the preferred source: deriving from the glob
//! would require re-implementing Go's `filepath.Match` subset in this crate,
//! which is a *new* duplicate of `dt-guard`'s `glob_matches` — trading one
//! unguarded encoding for another. The ConfigMap list needs no matcher, and it
//! is additionally the most direct answer to "what is in the pod this test
//! queries". Both are members of the pinned set, so the safety argument is
//! identical; this one costs less. Say so if you disagree — the requirement
//! (derive from the pinned set, never hand-write a predicate) is met either way.
//!
//! **What keeps that honest, stated so it is checkable rather than asserted**:
//! `dt-guard alert-rules-policy`'s `rule_file_loading` rule *does* read this
//! list, and asserts set equality between it, `loadable_rules_files()` and both
//! `rule_files` globs, failing in either direction. So a file added to the glob
//! but not the ConfigMap fails Layer 3 before this test can silently skip it —
//! which is the fail-open case a hand-written mirror leaves open.

use std::collections::BTreeSet;

/// Rules directory, repo-relative. Single on-disk copy; the in-cluster
/// ConfigMap is generated from it.
pub const RULES_DIR: &str = "infra/docker/prometheus/rules";

/// The kustomization whose `configMapGenerator` decides which rule files are
/// mounted into the cluster Prometheus this test queries.
pub const RULES_KUSTOMIZATION: &str = "infra/docker/prometheus/kustomization.yaml";

/// The rule-file basenames that are actually MOUNTED in-cluster, read out of the
/// `configMapGenerator` list rather than decided by a predicate here.
///
/// Kustomize cannot glob, so this list is an enumeration — which makes it the
/// literal answer to "what is in the pod", and therefore the right thing for a
/// cluster test to derive from. Returns `None` if no entry is found, so an
/// unparseable or restructured kustomization is a could-not-evaluate rather than
/// an empty-and-therefore-clean set.
///
/// Anchored on the `- ` bullet and comment-stripped for the same two reasons
/// `dt-guard`'s `extract_declared_generator_files` is: an inline comment
/// otherwise leaves the tail ending in comment text, and a *standalone* comment
/// naming a path otherwise counts as declaring it — the second being fail-open.
pub fn mounted_rules_files(kustomization: &str) -> Option<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for line in kustomization.lines() {
        let Some(rest) = line.trim().strip_prefix("- ") else {
            continue;
        };
        let rest = rest
            .split_once('#')
            .map_or(rest, |(before, _)| before)
            .trim();
        let Some((_, base)) = rest.rsplit_once('/') else {
            continue;
        };
        if base.ends_with(".yaml") || base.ends_with(".yml") {
            out.insert(base.to_string());
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Extract every `alert:` name from one rules file's YAML.
///
/// Returns `None` when the document does not parse or declares no groups —
/// distinct from `Some(empty)`, because "could not read this file" and "this
/// file declares no alerts" have different causes and the caller must not
/// collapse them into a clean result.
pub fn alert_names_in_yaml(content: &str) -> Option<BTreeSet<String>> {
    #[derive(serde::Deserialize)]
    struct Doc {
        groups: Vec<Group>,
    }
    #[derive(serde::Deserialize)]
    struct Group {
        #[serde(default)]
        rules: Vec<Rule>,
    }
    #[derive(serde::Deserialize)]
    struct Rule {
        #[serde(default)]
        alert: Option<String>,
    }
    let doc: Doc = serde_norway::from_str(content).ok()?;
    Some(
        doc.groups
            .into_iter()
            .flat_map(|g| g.rules)
            .filter_map(|r| r.alert)
            .filter(|a| !a.is_empty())
            .collect(),
    )
}

/// Extract every loaded alert name from a Prometheus `/api/v1/rules` response.
///
/// Parsed as JSON, never line-matched. Returns `None` when the envelope is not
/// a successful, well-formed rules response — again distinct from an empty set.
pub fn loaded_alert_names(body: &str) -> Option<BTreeSet<String>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v.get("status").and_then(serde_json::Value::as_str) != Some("success") {
        return None;
    }
    let groups = v.get("data")?.get("groups")?.as_array()?;
    let mut out = BTreeSet::new();
    for g in groups {
        let Some(rules) = g.get("rules").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for r in rules {
            if r.get("type").and_then(serde_json::Value::as_str) == Some("alerting") {
                if let Some(name) = r.get("name").and_then(serde_json::Value::as_str) {
                    out.insert(name.to_string());
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // FIRE fixtures. Every branch below is one the cluster-gated test cannot
    // exercise: on a healthy cluster it passes, so without these the extractors
    // would ship having never been shown to produce the RIGHT answer on known
    // input — only to have produced no complaint on unknown input.

    #[test]
    fn mounted_set_is_read_from_the_configmap_list() {
        let k = concat!(
            "configMapGenerator:\n",
            "  - name: prometheus-rules\n",
            "    files:\n",
            "      # rules/_template-service-alerts.yaml is deliberately absent\n",
            "      - rules/mc-alerts.yaml\n",
            "      - rules/mh-alerts.yaml   # media handler\n",
        );
        let got = mounted_rules_files(k).expect("parses");
        assert_eq!(
            got,
            ["mc-alerts.yaml".to_string(), "mh-alerts.yaml".to_string()]
                .into_iter()
                .collect(),
            "a standalone comment must not declare a file (fail-open), and an \
             inline comment must not un-declare one"
        );
    }

    #[test]
    fn mounted_set_distinguishes_unreadable_from_empty() {
        // An empty result is a broken extraction, not a clean one: an empty
        // expected set makes every downstream assertion pass vacuously.
        assert!(mounted_rules_files("resources:\n  - deployment.yaml\n").is_none());
        assert!(mounted_rules_files("").is_none());
    }

    #[test]
    fn yaml_kernel_returns_the_right_names_and_a_non_empty_set() {
        let yaml = concat!(
            "# - No RegisterMeeting RPC alert: this comment must NOT be extracted\n",
            "groups:\n",
            "  - name: mc-service-page\n",
            "    rules:\n",
            "      - alert: MCDown\n",
            "        expr: up == 0\n",
            "      - record: some:recording:rule\n",
            "        expr: vector(1)\n",
            "      - alert: MCActorPanic\n",
            "        expr: increase(x[5m]) > 0\n",
        );
        let names = alert_names_in_yaml(yaml).expect("parses");
        assert_eq!(
            names,
            ["MCActorPanic".to_string(), "MCDown".to_string()]
                .into_iter()
                .collect()
        );
        assert!(!names.is_empty(), "positive control");
    }

    #[test]
    fn yaml_kernel_distinguishes_unparseable_from_empty() {
        assert!(alert_names_in_yaml("this: [is: not: valid").is_none());
        assert!(alert_names_in_yaml("groups: []\n")
            .expect("parses")
            .is_empty());
    }

    #[test]
    fn json_kernel_returns_the_right_names() {
        let body = r#"{"status":"success","data":{"groups":[
            {"name":"mc-service-page","rules":[
              {"type":"alerting","name":"MCDown"},
              {"type":"recording","name":"some:rule"},
              {"type":"alerting","name":"MCActorPanic"}
            ]}]}}"#;
        let names = loaded_alert_names(body).expect("parses");
        assert_eq!(
            names,
            ["MCActorPanic".to_string(), "MCDown".to_string()]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn json_kernel_rejects_non_success_and_garbage() {
        // The trap this repo already stepped in, in its /api/v1/rules form: a
        // body that is not the expected envelope must be a COULD-NOT-EVALUATE,
        // never an empty-and-therefore-clean result.
        assert!(loaded_alert_names(r#"{"status":"error","error":"boom"}"#).is_none());
        assert!(loaded_alert_names("global:\n  scrape_interval: 15s\n").is_none());
        assert!(loaded_alert_names("").is_none());
    }

    #[test]
    fn json_kernel_empty_groups_is_empty_not_none() {
        // A Prometheus that loaded NOTHING answers successfully with zero
        // groups. That is a real, distinguishable state and it must reach the
        // caller as `Some(empty)` so the caller can fail on it — not as `None`,
        // which would be triaged as a broken extractor.
        let names = loaded_alert_names(r#"{"status":"success","data":{"groups":[]}}"#)
            .expect("well-formed envelope");
        assert!(names.is_empty());
    }
}
