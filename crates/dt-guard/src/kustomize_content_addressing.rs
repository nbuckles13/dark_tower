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
//!   reference walker went blind, this fails too. One exemption, in non-root
//!   targets only: a SHARED generator's ConfigMap (a generator-only base,
//!   whose consumers live in other bases) — it is held to the control at the
//!   environment root, which renders every consumer.
//!
//! **Cross-base references** (a workload referencing a shared generator owned
//! by a generator-only base, e.g. `media-codec-profile`) resolve only at the
//! environment root, so a non-root target DEFERS them to the root — exact
//! name from the ONE `SharedGeneratorIndex`, never an ambiguous name, never at
//! the root itself, and only if the root rendered in the same run. See
//! [`RenderContext`].

use crate::common::kustomize_generators::{parse_config_map_generators, SharedGeneratorIndex};
use crate::common::pod_spec::{
    meta_str, parse_docs, pod_configmap_refs, pod_spec, ConfigMapRefKind,
};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
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
    /// References this (non-root) target DEFERRED to the environment root —
    /// see [`RenderContext`]. Each is a human-readable description. The caller
    /// MUST turn every one into a finding unless the environment root rendered
    /// successfully in the same run.
    pub deferred: Vec<String>,
}

/// What [`check_rendered`] needs to know beyond the render itself.
///
/// # Cross-base references (generator-only bases)
///
/// A workload in one base may reference a SHARED generator owned by a
/// generator-only base (`infra/services/media-codec-profile/`). Only the
/// environment root includes both, so only the root rewrites that reference to
/// the hashed name; a per-service base or overlay render leaves it bare and
/// absent. In a NON-root target such a reference is therefore DEFERRED, not a
/// finding — on three exact conditions: the name carries no hash suffix, it is
/// EXACTLY a name in the [`SharedGeneratorIndex`] (unambiguous; an ambiguous
/// name is never deferred), and no ConfigMap of that name is in this render.
/// The environment root NEVER defers, so a shared generator missing from the
/// root still fails there; and the deferral is conditional on the root having
/// rendered in the same run (`TargetReport::deferred`, enforced by the caller).
pub struct RenderContext<'a> {
    pub declared_generators: &'a BTreeSet<String>,
    pub shared: &'a SharedGeneratorIndex,
    /// This render is the environment root (deploy.sh's one apply).
    pub is_env_root: bool,
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

fn ref_kind_label(k: ConfigMapRefKind) -> &'static str {
    match k {
        ConfigMapRefKind::KeyRef => "configMapKeyRef",
        ConfigMapRefKind::EnvFrom => "envFrom.configMapRef",
        ConfigMapRefKind::Volume => "volume",
        ConfigMapRefKind::Projected => "projected volume",
    }
}

/// Check one rendered multi-document YAML stream.
pub fn check_rendered(rendered: &str, label: &str, ctx: &RenderContext<'_>) -> TargetReport {
    let mut report = TargetReport::default();
    let mut findings: Vec<ContentAddressingFinding> = Vec::new();
    let mut push = |rule_id, detail: String| {
        findings.push(ContentAddressingFinding {
            rule_id,
            detail: format!("{label} — {detail}"),
        });
    };

    let docs = match parse_docs(rendered) {
        Ok(d) => d,
        Err(e) => {
            push(
                UNRESOLVED_REFERENCE_RULE_ID,
                format!("rendered output is not parseable YAML ({e}); nothing can be checked"),
            );
            report.findings = findings;
            return report;
        }
    };

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
    let rendered_names: BTreeSet<&str> = configmaps.iter().map(|(_, n)| n.as_str()).collect();
    let ambiguous: BTreeSet<&str> = ctx
        .shared
        .ambiguous
        .iter()
        .map(|a| a.name.as_str())
        .collect();

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
            let hashed = HASH_SUFFIX_RE.captures(name).and_then(|c| c.get(1));
            // Cross-base deferral (see RenderContext). Exact name, unambiguous,
            // bare, absent from this render, and never at the root.
            if !ctx.is_env_root && hashed.is_none() && !rendered_names.contains(name) {
                if let Some(shared_gen) = ctx.shared.generators.get(name) {
                    report.deferred.push(format!(
                        "{label} — {kind}/{wname}: {how} ConfigMap {name:?} (shared generator in base {})",
                        shared_gen.base
                    ));
                    continue;
                }
            }
            if ambiguous.contains(name) {
                push(
                    UNRESOLVED_REFERENCE_RULE_ID,
                    format!(
                        "{kind}/{wname}: {how} ConfigMap {name:?} is declared by more than one base under infra/services/ (env-config configmap_name_ambiguous_across_bases); it is never resolved or deferred"
                    ),
                );
            }
            referenced.insert((ns.to_string(), name.to_string()));
            match hashed {
                None => push(
                    NOT_CONTENT_ADDRESSED_RULE_ID,
                    format!(
                        "{kind}/{wname}: {how} ConfigMap {name:?} has no content hash — make it a configMapGenerator entry (a plain ConfigMap change rolls nothing; ADR-0038 §2)"
                    ),
                ),
                Some(base) if !ctx.declared_generators.contains(base.as_str()) => push(
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

    // Positive control: every rendered ConfigMap is consumed — always run.
    // One exemption, in NON-root targets only: a SHARED generator's ConfigMap
    // (hash-stripped name in the shared index) is consumed by workloads in
    // OTHER bases, which only the environment root renders; it is held to this
    // control there. Any other unreferenced ConfigMap fails, including in a
    // render with no pod template at all (so a walk that went blind still
    // fails here rather than passing a pod-template-free render).
    for (ns, name) in &configmaps {
        if referenced.contains(&(ns.clone(), name.clone())) {
            continue;
        }
        let generator_name = HASH_SUFFIX_RE
            .captures(name)
            .and_then(|c| c.get(1))
            .map_or(name.as_str(), |m| m.as_str());
        if !ctx.is_env_root && ctx.shared.generators.contains_key(generator_name) {
            continue;
        }
        push(
            UNREFERENCED_CONFIGMAP_RULE_ID,
            format!(
                "ConfigMap {ns}/{name} is referenced by no pod template in this render (dead config, or a reference shape this check cannot see)"
            ),
        );
    }

    report.findings = findings;
    report.consumed = referenced.len();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> BTreeSet<String> {
        ["app-config".to_string()].into_iter().collect()
    }

    fn check(render: &str, declared: &BTreeSet<String>) -> TargetReport {
        let shared = SharedGeneratorIndex::default();
        check_rendered(
            render,
            "t",
            &RenderContext {
                declared_generators: declared,
                shared: &shared,
                is_env_root: false,
            },
        )
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
        check(&docs.join("---\n"), &declared())
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
        // With a pod template that consumes nothing, too.
        let r = run(&[
            cm(HASHED),
            deployment("      containers:\n      - name: c\n"),
        ]);
        assert_eq!(rules(&r), vec![UNREFERENCED_CONFIGMAP_RULE_ID]);
    }

    /// A generator-only base renders its SHARED ConfigMap and no consumer;
    /// in a non-root target that ConfigMap is exempt (it is held to the
    /// control at the root). FAILS against the pre-story-3 rule.
    #[test]
    fn shared_generator_configmap_is_exempt_only_outside_the_root() {
        let shared = shared_index("app-config");
        let r = check_with(&cm(HASHED), &shared, false);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        let r = check_with(&cm(HASHED), &shared, true);
        assert_eq!(
            rules(&r),
            vec![UNREFERENCED_CONFIGMAP_RULE_ID],
            "the root holds it to the control"
        );
    }

    /// The exemption is by shared NAME, not by "no pod template": a
    /// pod-template-free render with a non-shared ConfigMap still fails.
    #[test]
    fn pod_template_free_render_with_a_non_shared_configmap_still_fails() {
        let shared = shared_index("media-codec-profile");
        let r = check_with(&cm(HASHED), &shared, false);
        assert_eq!(rules(&r), vec![UNREFERENCED_CONFIGMAP_RULE_ID]);
    }

    // ---- Cross-base deferral (generator-only bases) -----------------------

    fn shared_index(name: &str) -> SharedGeneratorIndex {
        let mut idx = SharedGeneratorIndex::default();
        idx.generators.insert(
            name.to_string(),
            crate::common::kustomize_generators::SharedGenerator {
                base: "media-codec-profile".into(),
                source: "infra/services/media-codec-profile/profile.env".into(),
                keys: BTreeSet::new(),
            },
        );
        idx
    }

    fn keyref_to(name: &str) -> String {
        deployment(&format!(
            "      containers:\n      - name: c\n        env:\n        - name: K\n          valueFrom:\n            configMapKeyRef: {{name: {name}, key: K}}\n"
        ))
    }

    fn check_with(render: &str, shared: &SharedGeneratorIndex, is_env_root: bool) -> TargetReport {
        check_rendered(
            render,
            "t",
            &RenderContext {
                declared_generators: &declared(),
                shared,
                is_env_root,
            },
        )
    }

    /// FAILS against the pre-story-3 rule: a non-root base render's bare ref
    /// to a shared generator was `not_content_addressed` + `unresolved`.
    #[test]
    fn non_root_bare_ref_to_a_shared_generator_is_deferred() {
        let shared = shared_index("media-codec-profile");
        let r = check_with(&keyref_to("media-codec-profile"), &shared, false);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert_eq!(r.deferred.len(), 1);
        assert!(r.deferred[0].contains("media-codec-profile"));
    }

    #[test]
    fn the_environment_root_never_defers() {
        // The profile excluded from the root: the root must go red.
        let shared = shared_index("media-codec-profile");
        let r = check_with(&keyref_to("media-codec-profile"), &shared, true);
        assert!(r.deferred.is_empty());
        assert_eq!(
            rules(&r),
            vec![NOT_CONTENT_ADDRESSED_RULE_ID, UNRESOLVED_REFERENCE_RULE_ID]
        );
    }

    #[test]
    fn deferral_is_exact_name_only() {
        let shared = shared_index("media-codec-profile");
        for near in [
            "media-codec",
            "media-codec-profile-extra",
            "x-media-codec-profile",
        ] {
            let r = check_with(&keyref_to(near), &shared, false);
            assert!(r.deferred.is_empty(), "{near} must not defer");
            assert!(!r.findings.is_empty(), "{near} must fail");
        }
    }

    #[test]
    fn an_ambiguous_shared_name_is_never_deferred() {
        let mut shared = SharedGeneratorIndex::default();
        shared
            .ambiguous
            .push(crate::common::kustomize_generators::AmbiguousGenerator {
                name: "media-codec-profile".into(),
                bases: vec!["a".into(), "b".into()],
            });
        let r = check_with(&keyref_to("media-codec-profile"), &shared, false);
        assert!(r.deferred.is_empty());
        assert!(
            r.findings
                .iter()
                .any(|f| f.detail.contains("more than one base")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn a_shared_generator_present_in_the_render_is_not_deferred() {
        // If the render includes the generator (bare name = a plain ConfigMap
        // of that name), the ordinary rule applies — no deferral escape hatch.
        let shared = shared_index("media-codec-profile");
        let plain = cm("media-codec-profile");
        let r = check_with(
            &[plain, keyref_to("media-codec-profile")].join("---\n"),
            &shared,
            false,
        );
        assert!(r.deferred.is_empty());
        assert_eq!(rules(&r), vec![NOT_CONTENT_ADDRESSED_RULE_ID]);
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
        let r = check(&render, &declared);
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
