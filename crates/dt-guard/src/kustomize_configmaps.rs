//! A local check over kustomize-GENERATED ConfigMaps, run by the `kustomize`
//! subcommand alongside R-16 and R-20.
//!
//! * **`configmap_annotation_size`** — every `configMapGenerator` entry under
//!   `infra/` must stay under [`ANNOTATION_HEADROOM_PERCENT`] of Kubernetes'
//!   annotation cap once client-side apply serialises it.
//!
//! (A second rule, `dashboard_configmap_label`, checked Grafana dashboard
//! generators for the label a k8s-sidecar selected on. ADR-0038 Implementation step 1 replaced the
//! sidecar with one projected volume that names every group, so the label no
//! longer selects anything; a group left off that volume is now caught by R-21
//! (`kustomize_content_addressing`: a generated ConfigMap no pod references).)
//!
//! # SCOPE: GENERATED ConfigMaps ONLY — not every ConfigMap that is applied
//!
//! The size rule covers `configMapGenerator` entries. It does **NOT** cover a
//! literal `kind: ConfigMap` manifest listed under `resources:`, which is
//! applied the same way and is capped by the same annotation. Do not read a
//! green result as "every applied ConfigMap fits".
//!
//! That gap is deliberate and measured: covering literal manifests needs a
//! parse of rendered `data:` block scalars, a different input from the
//! generator sources this rule reads. The one
//! non-trivial literal ConfigMap in the tree when this was written is
//! `otel-collector-config` (`infra/services/otel-collector/collector.yaml`):
//! **30,084 bytes, 11.5% of the cap** (kubectl-measured), about 7x headroom.
//! Tracked in `docs/TODO.md`, "The ConfigMap-size guard covers generated
//! ConfigMaps only".
//!
//! Added at story 2 task 12, whose Layer 7 cluster setup failed with
//! `The ConfigMap "grafana-dashboards-mc" is invalid: metadata.annotations:
//! Too long: must have at most 262144 bytes`, after a dashboard grew. Every
//! Layer-3 check was green, and the failure surfaced only at cluster setup.
//!
//! # Why the size is MODELLED, not read off the file
//!
//! `kubectl apply` (client-side) stores the whole object, JSON-encoded, in the
//! `kubectl.kubernetes.io/last-applied-configuration` annotation. The dashboard
//! file is therefore embedded as an escaped JSON **string**. The escaping is not
//! small: `mc-overview.json` was 236,747 bytes on disk and ~261 KB escaped, a
//! ~10% inflation. So a check comparing file bytes to the cap would have PASSED
//! the exact file that broke cluster setup, with apparent headroom to spare —
//! worse than no check, because it reads as coverage.
//!
//! The model is Go's `encoding/json` with HTML escaping ON (kubectl's default):
//! standard JSON escapes, plus `<`, `>` and `&` as six-byte `<` sequences
//! (PromQL is full of `>` and `&`), plus U+2028/U+2029. Non-ASCII stays raw
//! UTF-8, and map keys are sorted. It was pinned byte-for-byte against
//! `kubectl create configmap --save-config --dry-run=client` on three real
//! ConfigMaps and on a fixture exercising every escape class (the unit test
//! [`tests::the_model_matches_kubectl_byte_for_byte_on_every_escape_class`]
//! keeps that pin).
//!
//! # What is NOT modelled, and why that is the safe direction
//!
//! Only the name and the data are measured exactly. Namespace, generator
//! labels and anything an OVERLAY adds (for example the observability overlay's
//! `managed-by` label) are covered by a fixed [`METADATA_ALLOWANCE_BYTES`]
//! rather than resolved, because one kustomization cannot see the overlays
//! that include it. The model also keeps `"creationTimestamp":null`, which
//! the apply path may omit. Both err toward OVER-estimating, which is the
//! safe direction for a size guard.
//!
//! # The cap is an artifact of CLIENT-SIDE apply, not a law of Kubernetes
//!
//! 262144 bytes is the `metadata.annotations` total-size cap. It is NOT the
//! ConfigMap data limit (~1 MiB, etcd-side). The only reason a large ConfigMap
//! trips it is that client-side `kubectl apply` writes the whole submitted
//! object into `kubectl.kubernetes.io/last-applied-configuration`. **Server-side
//! apply (`kubectl apply --server-side`) writes no such annotation**, so it
//! would remove this failure mode entirely from
//! `infra/kind/scripts/deploy.sh::apply_env_root` (the one environment-root
//! apply, ADR-0038). It was deliberately NOT
//! adopted with this guard: switching carries field-manager and ownership
//! migration consequences for every existing object, which makes it a task of
//! its own. If it lands, this size rule can be retired — do not keep treating
//! 262144 as a constraint after the apply mode that creates it is gone.
//!
//! # Generators are read through the ONE shared parser
//!
//! Entries come from [`crate::common::kustomize_generators`] (a YAML parse —
//! key order, zero-indent block sequences and comments cannot lose or
//! mis-attribute an entry), and their data from its [`generator_data`], which
//! resolves every source through the repository containment gate and reads
//! env files and literals under one `KEY=VALUE` contract. Anything that cannot
//! be read — invalid YAML, a null/non-list section, a nameless entry, an
//! escaping or missing source, a malformed env line — is a finding naming the
//! file, never a skip. (Until ADR-0038 devloop 2 this module carried its own
//! line-oriented parser; it was folded into the shared one.)
//!
//! # Names are measured as written, hash suffixes included by allowance
//!
//! Generators are read from source, so a hash-suffixed name
//! (`prometheus-rules-<hash>`) is measured without its suffix. The ~11 bytes the
//! suffix adds are inside [`METADATA_ALLOWANCE_BYTES`].
//!
//! # Ownership
//!
//! Machinery (`infrastructure`), per CLAUDE.md's guard-ownership split.

use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::common::kustomize_generators::{generator_data, parse_config_map_generators};
use crate::common::scan::warn_skip;

pub const CONFIGMAP_ANNOTATION_SIZE_RULE_ID: &str = "configmap_annotation_size";

/// Kubernetes' cap on the total size of an object's annotations
/// (`TotalAnnotationSizeLimitB` in `k8s.io/apimachinery`'s object-meta
/// validation). This is the number in the error message
/// ("must have at most 262144 bytes"), not a guess.
pub const ANNOTATION_SIZE_LIMIT_BYTES: usize = 262_144;

/// Fail at this share of the cap, not at the cap itself.
///
/// **Derived from the measured failure, not chosen for roundness.** The
/// threshold must leave room for at least one ordinary task's growth, or the
/// guard is green on a ConfigMap that the very next task breaks — the ratchet
/// this guard was added in, where whoever breaks cluster setup is not whoever
/// filled the budget. Measured with the kubectl-pinned model when the guard
/// was written (story 2 task 12):
///
/// * before that task, `grafana-dashboards-mc` was ALREADY at **95.2%** of the
///   cap (249,492 bytes: the kubectl-exact annotation). The guard itself
///   REPORTS that state as 95.5% (250,516 bytes), because it adds
///   [`METADATA_ALLOWANCE_BYTES`]. Two numbers, two quantities: 95.2% is the
///   measurement, 95.5% is the guard's conservative view of it. The trap was
///   armed a task earlier either way;
/// * that one task (nine panels plus description edits) added 29,697 bytes,
///   **11.3% of the cap**, taking it to 106.5% and failing cluster setup.
///
/// So the threshold must sit at or below `100% − 11.3% ≈ 88.7%`. A 90% threshold
/// FAILS that test: a ConfigMap at 89.9% passes, and one such task takes it to
/// 101%. **80% leaves ~1.7 tasks of runway**, and every generated ConfigMap in
/// the tree passed it when written (the largest, `grafana-dashboards-mh`, was at
/// 67%). Proposed by @infrastructure from rendered sizes; the 90% first shipped
/// was superseded on this evidence. If a single task is ever observed to add
/// more than 20% of the cap, lower this.
pub const ANNOTATION_HEADROOM_PERCENT: usize = 80;

/// Fixed allowance for metadata this guard cannot resolve from one
/// kustomization: namespace, generator labels, and labels or annotations added
/// by an overlay that includes it. See the module doc: over-estimating is
/// the safe direction.
pub const METADATA_ALLOWANCE_BYTES: usize = 1_024;

/// The byte count at or below which a ConfigMap passes.
pub const fn headroom_threshold_bytes() -> usize {
    ANNOTATION_SIZE_LIMIT_BYTES * ANNOTATION_HEADROOM_PERCENT / 100
}

/// One finding, in the shape `kustomize::run` reports.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ConfigMapFinding {
    pub rule_id: &'static str,
    pub detail: String,
    pub file: PathBuf,
}

/// Bytes Go's `encoding/json` (HTML escaping on) writes for `s` as a string,
/// quotes included.
pub(crate) fn go_json_string_len(s: &str) -> usize {
    2 + s
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\n' | '\r' | '\t' => 2,
            '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => 6,
            c if (c as u32) < 0x20 => 6,
            c => c.len_utf8(),
        })
        .sum::<usize>()
}

/// The length of the `last-applied-configuration` annotation client-side
/// apply would write for a ConfigMap with this name and data, EXCLUDING the
/// metadata allowance (see [`METADATA_ALLOWANCE_BYTES`]).
///
/// Shape, byte for byte as kubectl writes it:
/// `{"kind":"ConfigMap","apiVersion":"v1","metadata":{"name":N,"creationTimestamp":null},"data":{K:V,...}}\n`
pub(crate) fn last_applied_len(name: &str, data: &BTreeMap<String, String>) -> usize {
    const HEAD: &str = r#"{"kind":"ConfigMap","apiVersion":"v1","metadata":{"name":"#;
    const MID: &str = r#","creationTimestamp":null},"data":{"#;
    const TAIL: &str = "}}\n";
    let entries: usize = data
        .iter()
        .map(|(k, v)| go_json_string_len(k) + 1 + go_json_string_len(v))
        .sum();
    let commas = data.len().saturating_sub(1);
    HEAD.len() + go_json_string_len(name) + MID.len() + entries + commas + TAIL.len()
}

/// Judge one generator's measured size. Pure, so the threshold arithmetic is
/// tested without a filesystem.
pub(crate) fn judge_size(name: &str, measured: usize, file: &Path) -> Option<ConfigMapFinding> {
    let with_allowance = measured + METADATA_ALLOWANCE_BYTES;
    if with_allowance <= headroom_threshold_bytes() {
        return None;
    }
    // Tenths of a percent in integer arithmetic (no float casts).
    let permille = with_allowance * 1000 / ANNOTATION_SIZE_LIMIT_BYTES;
    Some(ConfigMapFinding {
        rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
        detail: format!(
            "generated ConfigMap `{name}` would carry an estimated {with_allowance}-byte \
             last-applied-configuration annotation — {measured} bytes measured from its name \
             and data, plus a {METADATA_ALLOWANCE_BYTES}-byte allowance for metadata this \
             guard cannot resolve from one kustomization (namespace, generator labels, and \
             anything an overlay adds) — ({}.{}% of Kubernetes' {} byte cap; \
             this guard fails above {}%). Client-side apply stores the WHOLE ConfigMap, \
             JSON-escaped, in that annotation, so past the cap cluster setup fails with \
             `metadata.annotations: Too long`. Remedy: split a file out into its own \
             configMapGenerator entry (as mc-media.json was split from mc-overview.json), \
             and, for a dashboard, add the new group to Grafana's projected dashboards \
             volume (R-21 fails a group no pod mounts).",
            permille / 10,
            permille % 10,
            ANNOTATION_SIZE_LIMIT_BYTES,
            ANNOTATION_HEADROOM_PERCENT,
        ),
        file: file.to_path_buf(),
    })
}

/// Find every `kustomization.yaml` under `infra/`.
fn kustomizations(infra: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![infra.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                warn_skip("kustomization walk", &dir, &e);
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some("kustomization.yaml") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn rel(repo_root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(repo_root).unwrap_or(path).to_path_buf()
}

/// Run both checks over the repository.
pub(crate) fn check(repo_root: &Path) -> Result<Vec<ConfigMapFinding>> {
    let mut findings = Vec::new();
    let infra = repo_root.join("infra");
    if !infra.is_dir() {
        return Ok(findings);
    }

    // Size: every generator, in every kustomization.
    //
    // EVERY read failure on this path is a FINDING, not an early `Err`. An `Err`
    // aborts the whole `kustomize` run and takes the other checks' findings with
    // it, so an unreadable kustomization would hide real R-16/R-18/R-19
    // violations behind one I/O error — the same masking the per-file branch
    // below was written to avoid.
    let mut measured = 0usize;
    for kust in kustomizations(&infra) {
        let content = match std::fs::read_to_string(&kust) {
            Ok(c) => c,
            Err(e) => {
                findings.push(ConfigMapFinding {
                    rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
                    detail: format!(
                        "cannot read {}: {e}. Its configMapGenerator entries were NOT measured, \
                         so this run says nothing about their annotation size.",
                        rel(repo_root, &kust).display()
                    ),
                    file: rel(repo_root, &kust),
                });
                continue;
            }
        };
        let dir = kust.parent().unwrap_or(repo_root);
        // The ONE generator parser (common::kustomize_generators): a real YAML
        // parse, so key order, block-sequence indentation and comments are the
        // YAML library's problem, not a line scanner's. Anything it cannot read
        // — invalid YAML, a `configMapGenerator:` that is null or not a list, a
        // nameless entry — is a FINDING naming the file, never an early `Err`
        // (that would abort the run and hide the other checks' findings) and
        // never a skip (that would report unmeasured generators as clean).
        let generators = match parse_config_map_generators(&content) {
            Ok(g) => g,
            Err(e) => {
                findings.push(ConfigMapFinding {
                    rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
                    detail: format!(
                        "{}: configMapGenerator unreadable ({e:#}), so NOTHING in it was \
                         measured and a clean result for this file would be vacuous.",
                        rel(repo_root, &kust).display()
                    ),
                    file: rel(repo_root, &kust),
                });
                continue;
            }
        };
        for gen in generators {
            measured += 1;
            // Sources resolve through the shared containment gate; env files
            // and literals through the shared KEY=VALUE contract. An escaping,
            // unreadable or contract-violating source is a finding.
            match generator_data(repo_root, dir, &gen) {
                Ok(data) => {
                    if let Some(f) = judge_size(
                        &gen.name,
                        last_applied_len(&gen.name, &data),
                        &rel(repo_root, &kust),
                    ) {
                        findings.push(f);
                    }
                }
                Err(e) => findings.push(ConfigMapFinding {
                    rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
                    detail: format!("{e:#}; its annotation size was NOT measured"),
                    file: rel(repo_root, &kust),
                }),
            }
        }
    }

    // Vacuity guard: a run that collected NOTHING and reported clean would be
    // indistinguishable from a real pass. The tree has generators (Grafana's,
    // Prometheus's), so finding none means the parser or the walk broke — for
    // example a reformatted `configMapGenerator:` key — not that all is well.
    if measured == 0 {
        findings.push(ConfigMapFinding {
            rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
            detail: "found ZERO configMapGenerator entries under infra/ — the size check \
                     measured nothing, so a clean result would be vacuous. The parser or the \
                     kustomization walk is broken, or every generator was removed."
                .to_string(),
            file: PathBuf::from("infra"),
        });
    }

    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::kustomize_generators::GeneratorFile;

    /// Pins the model to kubectl's real output, byte for byte, on a fixture
    /// exercising every escape class: `<`, `>`, `&`, `"`, `\`, tab, CR, LF, a
    /// C0 control, U+2028, and non-ASCII (which stays raw). The expected 209 is
    /// what `kubectl create configmap fixture-cm --from-file=f.json=<fixture>
    /// --save-config --dry-run=client` wrote (kubectl v1.32.3), measured when
    /// this guard was written — not derived from this code.
    #[test]
    fn the_model_matches_kubectl_byte_for_byte_on_every_escape_class() {
        let fixture =
            "a > b && c < d\n\"quoted\" back\\slash\ttab\u{2028}ls caf\u{e9} \u{1}ctl\r\n";
        let mut data = BTreeMap::new();
        data.insert("f.json".to_string(), fixture.to_string());
        assert_eq!(last_applied_len("fixture-cm", &data), 209);
    }

    #[test]
    fn html_sensitive_characters_cost_six_bytes_not_one() {
        // The whole reason file size is the wrong measurement.
        assert_eq!(go_json_string_len(">"), 2 + 6);
        assert_eq!(go_json_string_len("&"), 2 + 6);
        assert_eq!(go_json_string_len("<"), 2 + 6);
        assert_eq!(go_json_string_len("a"), 2 + 1);
        assert_eq!(
            go_json_string_len("\u{e9}"),
            2 + 2,
            "non-ASCII stays raw UTF-8"
        );
    }

    #[test]
    fn map_keys_are_joined_with_commas_in_sorted_order() {
        let mut data = BTreeMap::new();
        data.insert("b".to_string(), String::new());
        data.insert("a".to_string(), String::new());
        // {"a":"","b":""} inside the envelope: 2 keys, one comma.
        let one = {
            let mut d = BTreeMap::new();
            d.insert("a".to_string(), String::new());
            last_applied_len("n", &d)
        };
        assert_eq!(
            last_applied_len("n", &data),
            one + 1 + go_json_string_len("b") + 1 + 2
        );
    }

    #[test]
    fn the_threshold_is_eighty_percent_of_the_kubernetes_cap() {
        assert_eq!(ANNOTATION_SIZE_LIMIT_BYTES, 262_144);
        assert_eq!(headroom_threshold_bytes(), 209_715);
    }

    #[test]
    fn a_configmap_over_the_headroom_fails_naming_it_and_the_remedy() {
        let at = headroom_threshold_bytes() - METADATA_ALLOWANCE_BYTES;
        assert_eq!(
            judge_size("cm", at, Path::new("k.yaml")),
            None,
            "exactly at the threshold passes"
        );
        let f = judge_size("grafana-dashboards-mc", at + 1, Path::new("k.yaml")).unwrap();
        assert_eq!(f.rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(f.detail.contains("grafana-dashboards-mc"));
        assert!(
            f.detail.contains("split a file out"),
            "the remedy is non-obvious; say it"
        );
        // The reported number is an ESTIMATE, and the message must say so plus
        // where the difference comes from — a responder who measures the real
        // annotation with kubectl gets a smaller number and must not read that
        // as the guard being wrong.
        assert!(
            f.detail.contains("estimated")
                && f.detail
                    .contains(&format!("{METADATA_ALLOWANCE_BYTES}-byte allowance"))
                && f.detail.contains(&format!("{} bytes measured", at + 1)),
            "{}",
            f.detail
        );
    }

    /// The live failure, as a test: the pre-split MC ConfigMap measured
    /// 279,189 bytes by kubectl. The guard must reject it.
    #[test]
    fn the_configmap_that_broke_cluster_setup_is_rejected() {
        assert!(judge_size("grafana-dashboards-mc", 279_189, Path::new("k.yaml")).is_some());
    }

    const GRAFANA: &str = r#"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: dark-tower-observability
configMapGenerator:
  - name: grafana-dashboards-config
    files:
      - dashboards.yaml=provisioning/dashboards/dashboards.yaml
  - name: grafana-dashboards-mc   # the overview
    options:
      labels:
        grafana_dashboard: "1"
    files:
      - mc-overview.json=dashboards/mc-overview.json  # inline comment
      # standalone comment between items
      - dashboards/mc-slos.json
  # a standalone comment between entries
  - name: grafana-dashboards-unlabelled
    files:
      - x.json=dashboards/x.json
  - name: lits
    literals:
      - KEY=plain value
generatorOptions:
  disableNameSuffixHash: true
"#;

    /// The comment-hardening the old line parser needed, now pinned against the
    /// SHARED parser: inline and standalone comments never corrupt an entry, a
    /// path, or a literal, and a bare path keys on its basename.
    #[test]
    fn the_shared_parser_reads_entries_files_and_literals_through_comments() {
        let g = parse_config_map_generators(GRAFANA).expect("parse");
        let names: Vec<&str> = g.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "grafana-dashboards-config",
                "grafana-dashboards-mc",
                "grafana-dashboards-unlabelled",
                "lits"
            ]
        );
        assert_eq!(
            g[1].files,
            vec![
                GeneratorFile {
                    key: "mc-overview.json".into(),
                    path: "dashboards/mc-overview.json".into()
                },
                GeneratorFile {
                    key: "mc-slos.json".into(),
                    path: "dashboards/mc-slos.json".into()
                },
            ],
            "an inline comment must not corrupt a path, and a bare path keys on its basename"
        );
        assert_eq!(
            g[3].literal_pairs().expect("literals"),
            vec![("KEY".to_string(), "plain value".to_string())]
        );
    }

    #[test]
    fn a_top_level_key_ends_the_section() {
        // `generatorOptions:` follows; nothing after it may become an entry.
        assert_eq!(
            parse_config_map_generators(GRAFANA).expect("parse").len(),
            4
        );
    }

    /// The quirk the old line parser was built around: a `name:` key INSIDE an
    /// `options: labels:` block is a label, not the generator's name.
    #[test]
    fn a_label_called_name_is_not_the_generator_name() {
        let g = parse_config_map_generators(
            "configMapGenerator:\n  - options:\n      labels:\n        name: a-label\n    name: the-generator\n    literals:\n      - K=V\n",
        )
        .expect("parse");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].name, "the-generator");
    }

    #[test]
    fn check_runs_end_to_end_on_a_temp_tree() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let g = root.join("infra/grafana");
        std::fs::create_dir_all(g.join("dashboards")).unwrap();
        std::fs::write(
            g.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: big\n    options:\n      labels:\n        grafana_dashboard: \"1\"\n    files:\n      - big.json=dashboards/big.json\n  - name: small\n    files:\n      - small.json=dashboards/small.json\n",
        )
        .unwrap();
        // `>` costs six bytes escaped: 40,000 of them is ~240 KB, over 80%,
        // while the file itself is only 40 KB — the case file size would miss.
        std::fs::write(g.join("dashboards/big.json"), ">".repeat(40_000)).unwrap();
        std::fs::write(g.join("dashboards/small.json"), "{}").unwrap();
        let findings = check(root).unwrap();
        let rules: Vec<(&str, bool)> = findings
            .iter()
            .map(|f| {
                (
                    f.rule_id,
                    f.detail.contains("`big`") || f.detail.contains("`small`"),
                )
            })
            .collect();
        assert_eq!(
            rules,
            vec![(CONFIGMAP_ANNOTATION_SIZE_RULE_ID, true)],
            "big is too large though its file is 40 KB; small is fine"
        );
    }

    /// Mechanism 5 (review-protocol vacuity): a tree with `infra/` but no
    /// generators must NOT report clean.
    #[test]
    fn collecting_zero_generators_is_a_finding_not_a_pass() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(k.join("kustomization.yaml"), "resources:\n  - a.yaml\n").unwrap();
        let findings = check(dir.path()).unwrap();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].detail.contains("ZERO"));
    }

    /// The ratchet the headroom exists for: the measured single-task growth
    /// (11.3% of the cap) must fit above the threshold.
    #[test]
    fn one_measured_task_of_growth_fits_between_the_threshold_and_the_cap() {
        const OBSERVED_SINGLE_TASK_GROWTH_BYTES: usize = 29_697;
        assert!(
            headroom_threshold_bytes() + OBSERVED_SINGLE_TASK_GROWTH_BYTES
                <= ANNOTATION_SIZE_LIMIT_BYTES,
            "a ConfigMap just under the threshold must survive one more task like story 2 \
             task 12; if this fails, the threshold is too close to the cap"
        );
    }

    /// A kustomize entry is a YAML MAP, so its keys may come in any order. All
    /// three orderings below were silently lost or mis-attributed when `- name:`
    /// was treated as the entry boundary.
    const REORDERED: &str = r#"configMapGenerator:
  - files:
      - a.json=dashboards/a.json
    options:
      labels:
        grafana_dashboard: "1"
    name: reordered-first
  - name: after-files
    files:
      - b.json=dashboards/b.json
  - literals:
      - K=V
    name: entry-after-a-files-list
  - name: entry-after-a-literals-list
    files:
      - c.json=dashboards/c.json
generatorOptions:
  disableNameSuffixHash: true
"#;

    /// The three reordered shapes, with their names, sources AND labels — not
    /// just a count, because the old parser's failure was mis-ATTRIBUTION as
    /// much as loss: a following entry's `- files:` bullet became a *file* of
    /// the preceding entry.
    #[test]
    fn entry_keys_may_come_in_any_order_because_an_entry_is_a_yaml_map() {
        let g = parse_config_map_generators(REORDERED).expect("parse");
        let names: Vec<&str> = g.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            [
                // Shape 1: a reordered entry FIRST in the section (the whole
                // entry used to vanish, taking its dashboard with it).
                "reordered-first",
                "after-files",
                // Shape 2: an entry opening on `- literals:` right after one
                // that ended in a `files:` list.
                "entry-after-a-files-list",
                // Shape 3: an entry right after one that ended in a
                // `literals:` list.
                "entry-after-a-literals-list",
            ]
        );
        assert_eq!(
            g[0].files,
            vec![GeneratorFile {
                key: "a.json".into(),
                path: "dashboards/a.json".into()
            }]
        );
        assert_eq!(
            g[1].files,
            vec![GeneratorFile {
                key: "b.json".into(),
                path: "dashboards/b.json".into()
            }],
            "the NEXT entry's `- literals:` bullet must not be read as a file of this one"
        );
        assert!(g[1].literals.is_empty());
        assert_eq!(g[2].literals, vec!["K=V"]);
        assert!(g[2].files.is_empty());
        assert_eq!(
            g[3].files,
            vec![GeneratorFile {
                key: "c.json".into(),
                path: "dashboards/c.json".into()
            }]
        );
    }

    /// The consequence that matters: a large dashboard in a reordered entry is
    /// MEASURED. Swallowed as a file of the preceding entry it was never
    /// attributed, and the guard reported on a ConfigMap that does not exist
    /// while saying nothing about the one that would break cluster setup.
    #[test]
    fn a_dashboard_in_a_reordered_entry_is_measured_and_can_fail_the_size_rule() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let k = root.join("infra/x");
        std::fs::create_dir_all(k.join("dashboards")).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: first\n    files:\n      - small.json=dashboards/small.json\n  - files:\n      - big.json=dashboards/big.json\n    name: reordered\n",
        )
        .unwrap();
        std::fs::write(k.join("dashboards/small.json"), "{}").unwrap();
        // 40,000 `>` escape to six bytes each: ~240 KB, over the 80% threshold.
        std::fs::write(k.join("dashboards/big.json"), ">".repeat(40_000)).unwrap();
        let findings = check(root).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(
            findings[0].detail.contains("`reordered`"),
            "the finding must name the reordered generator, not its neighbour: {}",
            findings[0].detail
        );
    }

    #[test]
    fn a_nameless_entry_is_a_finding_because_kustomize_requires_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - literals:\n      - K=V\n",
        )
        .unwrap();
        // A well-formed sibling keeps the whole-tree vacuity guard out of it.
        let y = dir.path().join("infra/y");
        std::fs::create_dir_all(&y).unwrap();
        std::fs::write(
            y.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: ok\n    literals:\n      - K=V\n",
        )
        .unwrap();
        let findings = check(dir.path()).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(
            findings[0].detail.contains("no string `name`")
                && findings[0].detail.contains("infra/x/kustomization.yaml"),
            "{}",
            findings[0].detail
        );
    }

    /// A YAML-supplied source path must face the repo's containment gate, and
    /// an escape must be a FINDING — never a skip, which would report a
    /// generator as measured when it was not.
    ///
    /// Both escape shapes: `../` traversal, and an ABSOLUTE path (which
    /// `Path::join` lets replace the base outright).
    #[test]
    fn a_source_path_resolving_outside_the_repository_is_a_finding_not_a_measurement() {
        let outer = tempfile::tempdir().unwrap();
        // A real, readable file OUTSIDE the repository root.
        let outside = outer.path().join("outside.json");
        std::fs::write(&outside, ">".repeat(40_000)).unwrap();
        let root = outer.path().join("repo");
        let k = root.join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            format!(
                "configMapGenerator:\n  - name: traverser\n    files:\n      \
                 - t.json=../../../outside.json\n  - name: absoluter\n    files:\n      \
                 - a.json={}\n",
                outside.display()
            ),
        )
        .unwrap();
        let findings = check(&root).unwrap();
        assert_eq!(findings.len(), 2, "{findings:#?}");
        for f in &findings {
            assert_eq!(f.rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
            assert!(
                f.detail.contains("resolves outside the repository"),
                "an escaping path must be reported as an escape, not measured or \
                 mislabelled unreadable: {}",
                f.detail
            );
        }
        assert!(
            findings.iter().any(|f| f.detail.contains("`traverser`"))
                && findings.iter().any(|f| f.detail.contains("`absoluter`")),
            "both escape shapes must be reported: {findings:#?}"
        );
    }

    /// A block sequence at its parent key's indent is valid YAML, accepted by
    /// kustomize, and `yq`'s DEFAULT output — including the sub-list sitting at
    /// its own `files:` key's indent. Every entry must still be found.
    const ZERO_INDENT: &str = r#"apiVersion: kustomize.config.k8s.io/v1beta1
configMapGenerator:
- name: zero-indent-first
  options:
    labels:
      grafana_dashboard: "1"
  files:
  - a.json=dashboards/a.json
  - dashboards/b.json
- literals:
  - K=V
  name: zero-indent-reordered
generatorOptions:
  disableNameSuffixHash: true
"#;

    /// The old fixture's trailing shape — a `- ` bullet after `generatorOptions:`
    /// closed the section — which the line parser had to IGNORE. It is not
    /// YAML at all (a sequence item inside a mapping), so the shared parser
    /// REJECTS it, and `check` reports the file rather than measuring half of it.
    const ZERO_INDENT_WITH_STRAY_BULLET: &str = r#"configMapGenerator:
- name: zero-indent-first
  literals:
  - K=V
generatorOptions:
  disableNameSuffixHash: true
- name: after-the-top-level-key
"#;

    #[test]
    fn a_block_sequence_at_its_parent_keys_indent_is_still_the_section() {
        let g = parse_config_map_generators(ZERO_INDENT).expect("parse");
        let names: Vec<&str> = g.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            ["zero-indent-first", "zero-indent-reordered"],
            "a column-0 `- ` bullet is an ENTRY, not a new top-level key"
        );
        assert_eq!(
            g[0].files,
            vec![
                GeneratorFile {
                    key: "a.json".into(),
                    path: "dashboards/a.json".into()
                },
                GeneratorFile {
                    key: "b.json".into(),
                    path: "dashboards/b.json".into()
                },
            ],
            "a sub-list at its own `files:` key's indent is still a sub-list"
        );
        assert_eq!(
            g[1].literals,
            vec!["K=V"],
            "the reordered zero-indent entry keeps its own literals"
        );
    }

    #[test]
    fn a_stray_bullet_after_the_section_is_rejected_and_reported_not_half_measured() {
        assert!(parse_config_map_generators(ZERO_INDENT_WITH_STRAY_BULLET).is_err());
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(k.join("kustomization.yaml"), ZERO_INDENT_WITH_STRAY_BULLET).unwrap();
        let findings = check(dir.path()).unwrap();
        // The unreadable file, plus the whole-tree vacuity guard (nothing else
        // in this tree was measured).
        assert!(
            findings
                .iter()
                .any(|f| f.detail.contains("configMapGenerator unreadable")
                    && f.detail.contains("infra/x/kustomization.yaml")),
            "{findings:#?}"
        );
    }

    /// The consequence, measured: the whole section used to be skipped, so an
    /// oversized dashboard passed while `measured` stayed non-zero — fail-open
    /// with the vacuity guard none the wiser.
    #[test]
    fn a_zero_indent_section_is_measured_and_can_fail_the_size_rule() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let k = root.join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "apiVersion: kustomize.config.k8s.io/v1beta1\nconfigMapGenerator:\n- name: big\n  files:\n  - big.json=big.json\n",
        )
        .unwrap();
        std::fs::write(k.join("big.json"), ">".repeat(40_000)).unwrap();
        let findings = check(root).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(
            findings[0].detail.contains("`big`"),
            "{}",
            findings[0].detail
        );
    }

    /// The guard that makes the class self-reporting rather than silent: one
    /// file declaring the section but yielding nothing measurable is invisible
    /// to the whole-tree `measured` counter, because the OTHER files keep it
    /// non-zero. Under the YAML parser the only such shape is a section that is
    /// null / not a list — an error, hence a FINDING naming the file. The flow
    /// spelling, which the line parser lost, now parses and is measured.
    #[test]
    fn a_declared_but_null_section_is_a_finding_even_when_other_files_parse() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for d in ["good", "flow", "lost"] {
            std::fs::create_dir_all(root.join("infra").join(d)).unwrap();
        }
        std::fs::write(
            root.join("infra/good/kustomization.yaml"),
            "configMapGenerator:\n  - name: ok\n    literals:\n      - K=V\n",
        )
        .unwrap();
        std::fs::write(
            root.join("infra/flow/kustomization.yaml"),
            "configMapGenerator:\n  [{name: flow, literals: [K=V]}]\n",
        )
        .unwrap();
        std::fs::write(
            root.join("infra/lost/kustomization.yaml"),
            "configMapGenerator:\nresources: []\n",
        )
        .unwrap();
        let findings = check(root).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(
            findings[0].detail.contains("configMapGenerator unreadable")
                && findings[0].detail.contains("infra/lost/kustomization.yaml"),
            "{}",
            findings[0].detail
        );
    }

    /// Consolidation TIGHTENS the size rule: env files and literals now go
    /// through the shared KEY=VALUE contract, so a malformed env line or a
    /// quote-wrapped literal is a FINDING — it used to be silently skipped or
    /// unquoted and measured as something kustomize would not generate.
    #[test]
    fn a_malformed_env_file_is_now_a_finding() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: env-cm\n    envs:\n      - c.env\n",
        )
        .unwrap();
        std::fs::write(k.join("c.env"), "GOOD=1\nNOT A PAIR\n").unwrap();
        let findings = check(dir.path()).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(
            findings[0].detail.contains("not KEY=VALUE")
                && findings[0].detail.contains("NOT measured"),
            "{}",
            findings[0].detail
        );
    }

    #[test]
    fn a_quote_wrapped_literal_is_a_finding() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: lit\n    literals:\n      - 'KEY=\"quoted\"'\n",
        )
        .unwrap();
        let findings = check(dir.path()).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(
            findings[0].detail.contains("quote-wrapped"),
            "{}",
            findings[0].detail
        );
    }

    /// Env values are measured, not just keys: an oversized env value fails.
    #[test]
    fn env_file_values_are_measured() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: big-env\n    envs:\n      - c.env\n",
        )
        .unwrap();
        std::fs::write(k.join("c.env"), format!("BIG={}\n", ">".repeat(40_000))).unwrap();
        let findings = check(dir.path()).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(
            findings[0].detail.contains("`big-env`"),
            "{}",
            findings[0].detail
        );
    }

    /// Containment is fail-CLOSED, so a root spelled non-canonically (`--root .`
    /// or a trailing `/.`, both of which Layer 3 could pass) must NOT make every
    /// in-repo source look like an escape — that would turn a whole-repo guard
    /// red on an artifact of how the root was spelled.
    #[test]
    fn a_non_canonical_repo_root_does_not_make_every_source_look_like_an_escape() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: cm\n    files:\n      - small.json\n",
        )
        .unwrap();
        std::fs::write(k.join("small.json"), "{}").unwrap();
        let dotted = dir.path().join(".");
        assert_eq!(check(&dotted).unwrap(), Vec::new());
    }

    #[test]
    fn an_unreadable_referenced_file_is_a_finding_not_a_silent_undercount() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: cm\n    files:\n      - missing.yaml\n",
        )
        .unwrap();
        let findings = check(dir.path()).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(findings[0].detail.contains("cannot be read"));
    }
}
