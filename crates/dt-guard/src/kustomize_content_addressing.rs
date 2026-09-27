//! `kustomize` R-21 — every ConfigMap a pod template consumes is
//! content-addressed (ADR-0038 §2).
//!
//! Kustomize hashes only GENERATED ConfigMaps: it appends a hash of the final
//! data to the name and rewrites every reference, so a config change changes
//! the pod template and applying the environment root rolls exactly the
//! workloads whose configuration changed. A plain `kind: ConfigMap` resource
//! keeps its literal name: editing it rolls nothing, and the pod runs on stale
//! config until something else restarts it (the 2026-09-03 MC CrashLoop). This
//! rule is what keeps the invariant complete when someone adds a plain one.
//!
//! Scope: **ConfigMaps only.** Secrets are provision-layer material (ADR-0038
//! §1, devloop 3) and are not checked here.
//!
//! Per rendered build target:
//!
//! * every pod-template ConfigMap reference (`configMapKeyRef`,
//!   `envFrom.configMapRef`, `volumes[].configMap`, projected sources) carries a
//!   kustomize hash suffix, and the name without it is a generator declared
//!   somewhere under `infra/` — closing the look-alike case of a hand-written
//!   name that happens to end in ten characters of the hash alphabet;
//! * the reference RESOLVES to a ConfigMap in the render in the workload's own
//!   namespace. Kustomize's name-reference rewrite silently does nothing across
//!   a namespace mismatch, leaving a reference to a name that will not exist;
//! * positive control, derived from the render (no hard-coded count): EVERY
//!   ConfigMap in the render is referenced by some pod template. No exemption:
//!   Grafana dashboards are mounted through a projected volume, so a dashboard
//!   group generated but never added to that volume fails here. If the
//!   reference walker went blind, this fails too.

use crate::common::kustomize_generators::parse_config_map_generators;
use crate::common::pod_spec::{pod_configmap_refs, pod_spec, ConfigMapRefKind};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize as _;
use serde_norway::Value;
use std::collections::BTreeSet;
use std::path::Path;

pub const NOT_CONTENT_ADDRESSED_RULE_ID: &str = "configmap_not_content_addressed";
pub const UNRESOLVED_REFERENCE_RULE_ID: &str = "configmap_reference_unresolved";
pub const UNREFERENCED_CONFIGMAP_RULE_ID: &str = "configmap_unreferenced";
/// Nothing was checked: zero build targets, or the environment root rendered
/// with zero pod-template ConfigMap references. Distinct from the content
/// findings — "measured nothing" is never "measured and clean".
pub const VACUOUS_RULE_ID: &str = "content_addressing_vacuous";
/// No kustomize tool, so nothing could be rendered. A FAIL, never the WARN the
/// other build-dependent checks degrade to: this rule is the invariant's only
/// enforcement.
pub const UNVERIFIABLE_RULE_ID: &str = "content_addressing_unverifiable";

/// The kustomize name-hash suffix. SSoT: kustomize's `hasher.encode`
/// (sigs.k8s.io/kustomize/api/hasher) takes the first 10 hex characters of the
/// SHA-256 of the object and maps `0,1,3,a,e` to `g,h,k,m,t` (to avoid
/// forming words), so the alphabet is `2 4 5 6 7 8 9 b c d f g h k m t`.
/// Verified against kubectl v1.32.3 (kustomize v5.5.0). This pattern is
/// defined here ONLY; tests compare against real renders, never restate it.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static HASH_SUFFIX_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(.+)-[245-9bcdfghkmt]{10}$").expect("static pattern compiles"));

/// One R-21 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentAddressingFinding {
    pub rule_id: &'static str,
    pub detail: String,
}

/// Outcome over one rendered target.
#[derive(Debug, Default)]
pub struct TargetReport {
    pub findings: Vec<ContentAddressingFinding>,
    /// Distinct `(namespace, name)` ConfigMaps referenced by pod templates.
    pub consumed: usize,
}

/// Every `configMapGenerator` name declared by a kustomization under `infra/`.
/// A kustomization this cannot parse is an error, not a skipped file.
pub fn declared_generator_names(repo_root: &Path) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in walkdir::WalkDir::new(repo_root.join("infra")) {
        let entry = entry.context("walking infra/")?;
        if entry.file_name() != "kustomization.yaml" {
            continue;
        }
        let content = std::fs::read_to_string(entry.path())
            .with_context(|| format!("reading {}", entry.path().display()))?;
        for g in parse_config_map_generators(&content)
            .with_context(|| format!("parsing {}", entry.path().display()))?
        {
            names.insert(g.name);
        }
    }
    Ok(names)
}

fn meta_str<'a>(doc: &'a Value, field: &str) -> Option<&'a str> {
    doc.get("metadata")?.get(field)?.as_str()
}

fn ref_kind_label(k: ConfigMapRefKind) -> &'static str {
    match k {
        ConfigMapRefKind::KeyRef => "configMapKeyRef",
        ConfigMapRefKind::EnvFrom => "envFrom.configMapRef",
        ConfigMapRefKind::Volume => "volume",
        ConfigMapRefKind::Projected => "projected volume",
    }
}

/// Check one rendered multi-document YAML stream.
pub fn check_rendered(
    rendered: &str,
    label: &str,
    declared_generators: &BTreeSet<String>,
) -> TargetReport {
    let mut report = TargetReport::default();
    let mut push = |rule_id, detail: String| {
        report.findings.push(ContentAddressingFinding {
            rule_id,
            detail: format!("{label} — {detail}"),
        });
    };

    let mut docs: Vec<Value> = Vec::new();
    for de in serde_norway::Deserializer::from_str(rendered) {
        match Value::deserialize(de) {
            Ok(v) if !v.is_null() => docs.push(v),
            Ok(_) => {}
            Err(e) => {
                push(
                    UNRESOLVED_REFERENCE_RULE_ID,
                    format!("rendered output is not parseable YAML ({e}); nothing can be checked"),
                );
                return report;
            }
        }
    }

    // Every ConfigMap in the render, by (namespace, name).
    let mut configmaps: BTreeSet<(String, String)> = BTreeSet::new();
    for doc in &docs {
        if doc.get("kind").and_then(Value::as_str) != Some("ConfigMap") {
            continue;
        }
        let ns = meta_str(doc, "namespace").unwrap_or("").to_string();
        let name = meta_str(doc, "name").unwrap_or("").to_string();
        configmaps.insert((ns, name));
    }

    let mut referenced: BTreeSet<(String, String)> = BTreeSet::new();
    for doc in &docs {
        let Some(pod) = pod_spec(doc) else {
            continue;
        };
        let kind = doc.get("kind").and_then(Value::as_str).unwrap_or("?");
        let wname = meta_str(doc, "name").unwrap_or("<unnamed>");
        let ns = meta_str(doc, "namespace").unwrap_or("");
        for r in pod_configmap_refs(pod) {
            let how = ref_kind_label(r.kind);
            let Some(name) = r.name else {
                push(
                    UNRESOLVED_REFERENCE_RULE_ID,
                    format!("{kind}/{wname}: a {how} ConfigMap reference has no name"),
                );
                continue;
            };
            referenced.insert((ns.to_string(), name.to_string()));
            match HASH_SUFFIX_RE.captures(name).and_then(|c| c.get(1)) {
                None => push(
                    NOT_CONTENT_ADDRESSED_RULE_ID,
                    format!(
                        "{kind}/{wname}: {how} ConfigMap {name:?} has no content hash — make it a configMapGenerator entry (a plain ConfigMap change rolls nothing; ADR-0038 §2)"
                    ),
                ),
                Some(base) if !declared_generators.contains(base.as_str()) => push(
                    NOT_CONTENT_ADDRESSED_RULE_ID,
                    format!(
                        "{kind}/{wname}: {how} ConfigMap {name:?} looks hash-suffixed but no configMapGenerator under infra/ declares {:?}",
                        base.as_str()
                    ),
                ),
                Some(_) => {}
            }
            if !configmaps.contains(&(ns.to_string(), name.to_string())) {
                push(
                    UNRESOLVED_REFERENCE_RULE_ID,
                    format!(
                        "{kind}/{wname}: {how} ConfigMap {name:?} does not exist in namespace {ns:?} in this render (kustomize skips the name rewrite across a namespace mismatch — check the generator's `namespace:`)"
                    ),
                );
            }
        }
    }

    for (ns, name) in &configmaps {
        if !referenced.contains(&(ns.clone(), name.clone())) {
            push(
                UNREFERENCED_CONFIGMAP_RULE_ID,
                format!(
                    "ConfigMap {ns}/{name} is referenced by no pod template in this render (dead config, or a reference shape this check cannot see)"
                ),
            );
        }
    }

    report.consumed = referenced.len();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> BTreeSet<String> {
        ["app-config".to_string()].into_iter().collect()
    }

    fn cm(name: &str) -> String {
        format!("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {name}\n  namespace: ns\ndata:\n  K: v\n")
    }

    fn deployment(pod_body: &str) -> String {
        format!(
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: d\n  namespace: ns\nspec:\n  template:\n    spec:\n{pod_body}"
        )
    }

    const HASHED: &str = "app-config-2b4c5d6f7g";

    fn run(docs: &[String]) -> TargetReport {
        check_rendered(&docs.join("---\n"), "t", &declared())
    }

    fn rules(r: &TargetReport) -> Vec<&'static str> {
        r.findings.iter().map(|f| f.rule_id).collect()
    }

    #[test]
    fn hashed_generator_reference_passes() {
        let r = run(&[
            cm(HASHED),
            deployment(&format!(
                "      containers:\n      - name: c\n        env:\n        - name: K\n          valueFrom:\n            configMapKeyRef: {{name: {HASHED}, key: K}}\n"
            )),
        ]);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert_eq!(r.consumed, 1);
    }

    #[test]
    fn plain_configmap_fails_via_every_reference_shape() {
        let shapes = [
            "      containers:\n      - name: c\n        env:\n        - name: K\n          valueFrom:\n            configMapKeyRef: {name: plain, key: K}\n",
            "      containers:\n      - name: c\n        envFrom:\n        - configMapRef: {name: plain}\n",
            "      containers:\n      - name: c\n      volumes:\n      - name: v\n        configMap: {name: plain}\n",
            "      containers:\n      - name: c\n      volumes:\n      - name: v\n        projected:\n          sources:\n          - configMap: {name: plain}\n",
        ];
        for body in shapes {
            let r = run(&[cm("plain"), deployment(body)]);
            assert_eq!(
                rules(&r),
                vec![NOT_CONTENT_ADDRESSED_RULE_ID],
                "shape not caught:\n{body}\n{:?}",
                r.findings
            );
        }
    }

    #[test]
    fn hashed_looking_reference_absent_from_render_fails() {
        let r = run(&[deployment(&format!(
            "      containers:\n      - name: c\n        envFrom:\n        - configMapRef: {{name: {HASHED}}}\n"
        ))]);
        assert_eq!(rules(&r), vec![UNRESOLVED_REFERENCE_RULE_ID]);
    }

    #[test]
    fn namespace_mismatch_does_not_resolve() {
        let other_ns = cm(HASHED).replace("namespace: ns", "namespace: elsewhere");
        let r = run(&[
            other_ns,
            deployment(&format!(
                "      containers:\n      - name: c\n      volumes:\n      - name: v\n        configMap: {{name: {HASHED}}}\n"
            )),
        ]);
        assert!(
            rules(&r).contains(&UNRESOLVED_REFERENCE_RULE_ID),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn look_alike_name_without_a_generator_fails() {
        // Ends in ten hash-alphabet characters, but nothing generates it.
        let lookalike = "handmade-2b4c5d6f7g";
        let r = run(&[
            cm(lookalike),
            deployment(&format!(
                "      containers:\n      - name: c\n        envFrom:\n        - configMapRef: {{name: {lookalike}}}\n"
            )),
        ]);
        assert_eq!(rules(&r), vec![NOT_CONTENT_ADDRESSED_RULE_ID]);
    }

    #[test]
    fn unreferenced_configmap_is_the_positive_control() {
        let r = run(&[cm(HASHED)]);
        assert_eq!(rules(&r), vec![UNREFERENCED_CONFIGMAP_RULE_ID]);
    }

    #[test]
    fn dashboard_group_generated_but_not_mounted_fails() {
        // The replacement for the old unguarded "missing labels block" hazard:
        // Grafana mounts dashboard groups through ONE projected volume, so a
        // group added to the generator but not to that volume must FAIL here.
        // No label or name-prefix exemption exists for dashboards.
        let mounted = "grafana-dashboards-ac-2b4c5d6f7g";
        let forgotten = "grafana-dashboards-mh-4c5d6f7g2b";
        let declared: BTreeSet<String> = ["grafana-dashboards-ac", "grafana-dashboards-mh"]
            .into_iter()
            .map(String::from)
            .collect();
        let dashboard_cm = |name: &str| {
            format!("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: {name}\n  namespace: ns\ndata:\n  a.json: '{{}}'\n")
        };
        let grafana = deployment(&format!(
            "      containers:\n      - name: grafana\n      volumes:\n      - name: dashboards\n        projected:\n          sources:\n          - configMap: {{name: {mounted}}}\n"
        ));
        let render = [dashboard_cm(mounted), dashboard_cm(forgotten), grafana].join("---\n");
        let r = check_rendered(&render, "t", &declared);
        assert_eq!(
            rules(&r),
            vec![UNREFERENCED_CONFIGMAP_RULE_ID],
            "{:?}",
            r.findings
        );
        assert!(
            r.findings[0].detail.contains("grafana-dashboards-mh"),
            "{:?}",
            r.findings
        );
    }
}
