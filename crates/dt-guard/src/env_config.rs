//! `env-config` subcommand — validates consistency between Rust service
//! config and the Kubernetes manifests that supply it.
//!
//! # Checks
//!
//! 1. Every `MissingEnvVar("VAR")` in `crates/<svc>/src/config.rs` is declared
//!    by **every** workload in `infra/services/<svc>/`. Per-workload, not
//!    union: a var present in `mc-0` but missing from `mc-1` is a real
//!    production defect (that pod CrashLoops) even though its sibling is fine.
//! 2. Every `configMapKeyRef` resolves — the named ConfigMap exists in the
//!    service directory **and** declares the named key. Resolution is
//!    name-scoped: the `(configMapKeyRef.name, key)` pair is looked up against
//!    that specific ConfigMap's `data:` block, never against a directory-wide
//!    union of every key in sight.
//! 3. Every ConfigMap `data:` key is referenced by at least one workload
//!    **naming that ConfigMap**. A key in `mh-0-config` referenced only by
//!    `mh-0-deployment.yaml` is correct; the same key referenced only by
//!    `mh-1-deployment.yaml` (which names `mh-1-config`) is an orphan.
//!
//! # Discovery
//!
//! Manifests are discovered by **parsed `kind:`**, never by filename. Filename
//! probing is what made this guard silently skip `mc-service` and `mh-service`
//! for the whole of their existence — they use per-instance workload names
//! (`mc-0-deployment.yaml`) that a two-name lookup could not see, while the run
//! still reported `STATUS=OK REASON=env-config-clean-4-services`, a clean
//! verdict for a count that included the two services it had skipped. A
//! filename *glob* would only have replaced one literal with a longer one and
//! re-opened the hole at the next naming change. Kind-based discovery has no
//! filename convention to drift from, and it makes an unrecognised manifest a
//! loud failure rather than an invisible omission.
//!
//! # Scope boundary — read this before trusting the status line
//!
//! This guard covers `infra/services/<svc>/` **only**. It does NOT cover
//! `infra/kubernetes/overlays/**`. Kind overlays carry live `data:` keys
//! (`overlays/kind/services/{ac,gc,mc}-service/configmap-otel-patch.yaml`) that
//! are invisible here, and because a Kustomize strategic merge on a ConfigMap
//! is *additive*, an overlay can introduce a key that no base ConfigMap
//! declares and no workload references. Extending coverage to overlays needs a
//! patch-semantics resolution model rather than this declaration-semantics one;
//! it is tracked in `docs/TODO.md` § Infrastructure Validation in Devloops.
//! `env-config-clean-<S>-services-<W>-workloads` therefore means "the base
//! service manifests are consistent", not "MH's ConfigMap surface is checked".
//! Coverage that is stated can be audited; coverage that is assumed cannot.

use crate::common::explain::{print_finding, Finding};
use crate::common::path_safety::resolve_cited_path;
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize as _;
use serde_norway::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// -----------------------------------------------------------------------------
// Rule ids.
// -----------------------------------------------------------------------------

pub const MISSING_IN_MANIFEST_RULE_ID: &str = "missing_in_manifest";
pub const KEY_NOT_IN_CONFIGMAP_RULE_ID: &str = "key_not_in_configmap";
pub const ORPHAN_CONFIGMAP_KEY_RULE_ID: &str = "orphan_configmap_key";
/// A `configMapKeyRef` names a ConfigMap with no manifest in the service dir.
/// Distinct from [`KEY_NOT_IN_CONFIGMAP_RULE_ID`] because the 2am action
/// differs: "I cannot find this ConfigMap" sends the reader to an overlay or a
/// typo'd `name:`, "this key is not in the ConfigMap it names" sends them to
/// that ConfigMap's `data:` block.
pub const CONFIGMAP_NOT_FOUND_RULE_ID: &str = "configmap_not_found";
/// The service has config + an infra dir but no discoverable workload. Hard
/// FAIL, never a warn-and-continue — a skipped service reported as clean is
/// the defect this guard exists to make impossible.
pub const NO_WORKLOAD_MANIFEST_RULE_ID: &str = "no_workload_manifest";
/// `crates/<svc>/src/config.rs` or `infra/services/<svc>/` is absent.
pub const SERVICE_INPUTS_MISSING_RULE_ID: &str = "service_inputs_missing";
/// A manifest document's `kind` is in neither the workload set nor the
/// no-pod-spec set (or the document has no `kind` at all).
pub const UNCLASSIFIED_MANIFEST_KIND_RULE_ID: &str = "unclassified_manifest_kind";
/// A workload document whose pod spec could not be located — refuses to pass
/// vacuously on zero containers.
pub const WORKLOAD_MISSING_POD_SPEC_RULE_ID: &str = "workload_missing_pod_spec";
/// A container uses `envFrom`. The guard cannot reason about bulk env
/// injection, so it declines to claim coverage rather than checking the
/// per-key references it *can* see and reporting clean.
pub const UNSUPPORTED_ENV_SOURCE_RULE_ID: &str = "unsupported_env_source";
/// An `env` entry the guard cannot fully read: no `name`, or a
/// `configMapKeyRef` missing its `name` or `key`. Surfaced rather than
/// silently dropped — a dropped `configMapKeyRef` would misattribute its key
/// as an orphan on the ConfigMap, sending remediation to delete a key that IS
/// referenced, just malformedly.
pub const MALFORMED_ENV_REFERENCE_RULE_ID: &str = "malformed_env_reference";
/// Two ConfigMap documents in one service dir share a `metadata.name`. Fatal to
/// a name-scoped resolver whose entire premise is that `metadata.name`
/// identifies the document — a silent last-wins collapse would hide both.
pub const DUPLICATE_CONFIGMAP_NAME_RULE_ID: &str = "duplicate_configmap_name";

/// Rule-id → FAIL-token, in precedence order. The first id with any hit names
/// the run.
///
/// Coverage faults precede content faults: "the guard could not apply" outranks
/// "the guard applied and found something", because the former means there is
/// unchecked surface and the latter does not. Within the content half,
/// `configmap_not_found` leads — it is runtime-fatal (`CreateContainerConfigError`,
/// the pod never starts) and it is the one content fault that can be a coverage
/// gap in content clothing, since it fires exactly when a ConfigMap might be
/// supplied from outside this guard's scope.
///
/// This table is exhaustive over the rule-id constants above; `all_rule_ids_have_a_precedence_entry`
/// fails the build's test suite if a new id is added without one.
const RULE_PRECEDENCE: &[(&str, &str)] = &[
    (NO_WORKLOAD_MANIFEST_RULE_ID, "no-workload-manifest"),
    (SERVICE_INPUTS_MISSING_RULE_ID, "service-inputs-missing"),
    (
        UNCLASSIFIED_MANIFEST_KIND_RULE_ID,
        "unclassified-manifest-kind",
    ),
    (DUPLICATE_CONFIGMAP_NAME_RULE_ID, "duplicate-configmap-name"),
    (
        WORKLOAD_MISSING_POD_SPEC_RULE_ID,
        "workload-missing-pod-spec",
    ),
    (UNSUPPORTED_ENV_SOURCE_RULE_ID, "unsupported-env-source"),
    (MALFORMED_ENV_REFERENCE_RULE_ID, "malformed-env-reference"),
    (CONFIGMAP_NOT_FOUND_RULE_ID, "configmap-not-found"),
    (MISSING_IN_MANIFEST_RULE_ID, "missing-in-manifest"),
    (KEY_NOT_IN_CONFIGMAP_RULE_ID, "key-not-in-configmap"),
    (ORPHAN_CONFIGMAP_KEY_RULE_ID, "orphan-key"),
];

// -----------------------------------------------------------------------------
// Manifest-kind classification.
// -----------------------------------------------------------------------------

/// Kinds that carry a pod template at `spec.template.spec`.
///
/// ANCHOR (DRY): `crates/dt-guard/src/kustomize_tools.rs::SECURITY_CONTEXT_KINDS`
/// is the *other* answer in this binary to "which kinds carry a pod spec", and
/// the two deliberately differ on `DaemonSet`. That list is scoped to R-18's
/// security-context invariants and is held at `{Deployment, StatefulSet}` to
/// preserve parity with the bash guard it replaced; this list covers every kind
/// that can carry env configuration. Neither is the other's SSoT. Widening
/// R-18's coverage is a behaviour change to a different check and belongs to
/// whoever owns that bash-parity decision — do not quietly fold these together.
///
/// `Job`/`CronJob` are deliberately absent: no service directory contains one,
/// and their pod spec nests one level deeper (`spec.jobTemplate.spec.template.spec`),
/// so supporting them speculatively would add the only per-kind branch in
/// pod-spec location and leave it permanently untested. Their absence is safe
/// only because an unclassified kind is a hard FAIL — see [`NO_POD_SPEC_KINDS`].
const WORKLOAD_KINDS_WITH_POD_SPEC: &[&str] = &["Deployment", "StatefulSet", "DaemonSet"];

/// Kinds that legitimately live in a service directory and **carry no pod
/// spec**. That is the membership rule, and it is stated here rather than in a
/// commit message on purpose.
///
/// If a `Job` (or any other pod-carrying kind) ever reds this guard with
/// `unclassified_manifest_kind`, the fix is to add it to
/// [`WORKLOAD_KINDS_WITH_POD_SPEC`] and implement its pod-spec path — NOT to
/// silence the failure by adding it here. Doing that would restore the exact
/// defect this guard was rewritten to remove: a workload the guard cannot see,
/// reported as clean. A kind belongs in this list only if it has no pod
/// template at all.
const NO_POD_SPEC_KINDS: &[&str] = &[
    "Service",
    "NetworkPolicy",
    "PodDisruptionBudget",
    "ServiceMonitor",
    "Secret",
    "Kustomization",
    "PersistentVolumeClaim",
    "ServiceAccount",
    "Role",
    "RoleBinding",
    "Ingress",
    "HorizontalPodAutoscaler",
];

const CONFIGMAP_KIND: &str = "ConfigMap";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MISSING_ENV_VAR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"MissingEnvVar\("([A-Z_][A-Z0-9_]*)""#).expect("static pattern compiles")
});

// -----------------------------------------------------------------------------
// Model.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hit {
    pub(crate) rule_id: &'static str,
    pub(crate) detail: String,
    pub(crate) file: PathBuf,
}

/// Outcome of a full run. The counts are the honest ones: `checked_services`
/// counts services actually checked (not services iterated), and
/// `checked_workloads` makes a silently-dropped instance manifest visible in
/// the status line rather than only in a diff.
#[derive(Debug, Default)]
pub(crate) struct Report {
    pub(crate) checked_services: usize,
    pub(crate) checked_workloads: usize,
    pub(crate) hits: Vec<Hit>,
}

/// One workload document, reduced to what the checks need.
#[derive(Debug)]
struct Workload {
    /// Repo-relative path of the file this document came from.
    path: PathBuf,
    /// Env var names declared across all containers and initContainers.
    env_names: BTreeSet<String>,
    /// `(configMapKeyRef.name, key)` pairs. The ConfigMap name is retained —
    /// dropping it is what would let a reference to the wrong ConfigMap pass.
    key_refs: BTreeSet<(String, String)>,
    /// A container in this workload uses `envFrom`. When set, the guard cannot
    /// see the full env set, so checks 1 and 3 (which assert about *absence* —
    /// a required var not declared, a ConfigMap key not referenced) must not
    /// run against it: `envFrom` could supply either. The workload still counts
    /// as discovered and still carries an `unsupported_env_source` hit, so the
    /// run fails loudly rather than reporting a finding the guard cannot stand
    /// behind. Check 2 (explicit `configMapKeyRef`s resolve) stays valid — those
    /// references are real regardless of `envFrom`.
    uses_env_from: bool,
    /// ConfigMaps whose key-consumption the guard cannot fully see, so no key
    /// in them can be honestly called an orphan (check 3). Two sources: an
    /// `envFrom` bulk `configMapRef` (all keys might be consumed) and a
    /// malformed `configMapKeyRef` with `name` present but `key` absent (the
    /// intended key is unknowable). Scoped to the NAMED map — an `envFrom` on
    /// `mh-service-config` says nothing about a sibling `mh-1-config`, so
    /// suppression must not spill onto other ConfigMaps in the service.
    ambiguous_cm_names: BTreeSet<String>,
    /// A bulk `envFrom` `configMapRef` whose `name` could not be read. The
    /// imported ConfigMap is unknowable, so orphan checks cannot be scoped and
    /// must be suppressed service-wide *for this service only* — the sole case
    /// where broad suppression is justified, and it is made loud by a
    /// `malformed_env_reference` hit rather than silent.
    imports_unnamed_configmap: bool,
}

/// One ConfigMap document, keyed by its `metadata.name`.
#[derive(Debug)]
struct ConfigMapDoc {
    path: PathBuf,
    name: String,
    keys: BTreeSet<String>,
}

#[derive(Debug, Default)]
struct Discovered {
    workloads: Vec<Workload>,
    configmaps: Vec<ConfigMapDoc>,
    hits: Vec<Hit>,
}

// -----------------------------------------------------------------------------
// Extraction.
// -----------------------------------------------------------------------------

fn extract_required_env_vars(content: &str) -> Vec<String> {
    let mut out: Vec<String> = MISSING_ENV_VAR_RE
        .captures_iter(content)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Locate a workload's pod spec. One path, no per-kind branching — see
/// [`WORKLOAD_KINDS_WITH_POD_SPEC`] on why `CronJob`'s deeper nesting is out.
fn pod_spec(doc: &Value) -> Option<&Value> {
    doc.get("spec")?.get("template")?.get("spec")
}

/// All containers in a pod spec, regular and init.
///
/// `initContainers` is included deliberately: it is one more field on the same
/// pod spec rather than a separate code path, and omitting it is not neutral —
/// a required env var declared only on an init container would produce a false
/// `missing_in_manifest`, and its ConfigMap key a false orphan. A guard that
/// invents findings is worse than one that misses them.
fn containers(pod: &Value) -> Vec<&Value> {
    let mut out: Vec<&Value> = Vec::new();
    for field in ["containers", "initContainers"] {
        if let Some(seq) = pod.get(field).and_then(Value::as_sequence) {
            out.extend(seq.iter());
        }
    }
    out
}

/// Read `metadata.name`, if present.
fn metadata_name(doc: &Value) -> Option<&str> {
    doc.get("metadata")?.get("name")?.as_str()
}

/// Extract env names and `configMapKeyRef` pairs from one workload document.
///
/// Returns `Err`-shaped hits alongside the workload so an `envFrom` container
/// or a missing pod spec becomes a finding rather than a silent zero.
fn parse_workload(doc: &Value, rel_path: &Path, dir: &str) -> (Option<Workload>, Vec<Hit>) {
    let mut hits: Vec<Hit> = Vec::new();

    let Some(pod) = pod_spec(doc) else {
        hits.push(Hit {
            rule_id: WORKLOAD_MISSING_POD_SPEC_RULE_ID,
            detail: format!(
                "{dir}: workload {name} has no spec.template.spec — cannot check its env wiring",
                name = metadata_name(doc).unwrap_or("<unnamed>"),
            ),
            file: rel_path.to_path_buf(),
        });
        return (None, hits);
    };

    let conts = containers(pod);
    if conts.is_empty() {
        // The rule id's doc promises this check ("refuses to pass vacuously on
        // zero containers"); a pod spec with no containers would otherwise be a
        // Workload with empty env, satisfying check 1 trivially.
        hits.push(Hit {
            rule_id: WORKLOAD_MISSING_POD_SPEC_RULE_ID,
            detail: format!(
                "{dir}: workload {name} has a pod spec but declares no containers or initContainers — nothing to check its env against",
                name = metadata_name(doc).unwrap_or("<unnamed>"),
            ),
            file: rel_path.to_path_buf(),
        });
        return (None, hits);
    }

    let mut env_names: BTreeSet<String> = BTreeSet::new();
    let mut key_refs: BTreeSet<(String, String)> = BTreeSet::new();
    let mut ambiguous_cm_names: BTreeSet<String> = BTreeSet::new();
    let mut imports_unnamed_configmap = false;
    let mut uses_env_from = false;

    for container in conts {
        if let Some(env_from) = container.get("envFrom") {
            uses_env_from = true;
            hits.push(Hit {
                rule_id: UNSUPPORTED_ENV_SOURCE_RULE_ID,
                detail: format!(
                    "{dir}: container {c} uses envFrom; this guard checks per-key configMapKeyRef only and will not claim coverage over bulk env injection",
                    c = container.get("name").and_then(Value::as_str).unwrap_or("<unnamed>"),
                ),
                file: rel_path.to_path_buf(),
            });
            // A bulk `configMapRef` import means every key of the named
            // ConfigMap might be consumed — suppress orphan checks for THAT map
            // only (below), not the whole service. A `secretRef`-only envFrom
            // names no ConfigMap and suppresses none. A `configMapRef` whose
            // `name` cannot be read is unknowable: emit a malformed hit and fall
            // back to the conservative service-wide suppression, never a silent
            // widening.
            match env_from.as_sequence() {
                Some(seq) => {
                    for src in seq {
                        let Some(cm_ref) = src.get("configMapRef") else {
                            continue; // secretRef or other source: no ConfigMap
                        };
                        match cm_ref.get("name").and_then(Value::as_str) {
                            Some(cm_name) => {
                                ambiguous_cm_names.insert(cm_name.to_string());
                            }
                            None => {
                                imports_unnamed_configmap = true;
                                hits.push(Hit {
                                    rule_id: MALFORMED_ENV_REFERENCE_RULE_ID,
                                    detail: format!(
                                        "{dir}: an envFrom configMapRef has no readable `name`; the imported ConfigMap is unknowable"
                                    ),
                                    file: rel_path.to_path_buf(),
                                });
                            }
                        }
                    }
                }
                // envFrom present but not a list — e.g. written as a mapping,
                // one of the commonest YAML mistakes. Same treatment as an
                // unreadable `name`: suppress conservatively, but SAY SO. An
                // unexplained service-wide suppression is the silent widening
                // the comment above rules out.
                None => {
                    imports_unnamed_configmap = true;
                    hits.push(Hit {
                        rule_id: MALFORMED_ENV_REFERENCE_RULE_ID,
                        detail: format!(
                            "{dir}: envFrom is not a list (expected a sequence of sources); its imports are unreadable, so orphan checks are suppressed for this service"
                        ),
                        file: rel_path.to_path_buf(),
                    });
                }
            }
        }
        let Some(env) = container.get("env").and_then(Value::as_sequence) else {
            continue;
        };
        for entry in env {
            let Some(name) = entry.get("name").and_then(Value::as_str) else {
                hits.push(Hit {
                    rule_id: MALFORMED_ENV_REFERENCE_RULE_ID,
                    detail: format!("{dir}: an env entry has no `name` field"),
                    file: rel_path.to_path_buf(),
                });
                continue;
            };
            env_names.insert(name.to_string());
            let Some(cm_ref) = entry
                .get("valueFrom")
                .and_then(|v| v.get("configMapKeyRef"))
            else {
                continue;
            };
            let cm_name = cm_ref.get("name").and_then(Value::as_str);
            let cm_key = cm_ref.get("key").and_then(Value::as_str);
            match (cm_name, cm_key) {
                (Some(cm_name), Some(cm_key)) => {
                    key_refs.insert((cm_name.to_string(), cm_key.to_string()));
                }
                // Missing `name` and/or `key`. Dropping this silently would let
                // the key it *should* have named surface as an orphan — a
                // wrong-direction remediation carrying the guard's authority.
                _ => {
                    if let Some(cm_name) = cm_name {
                        // We know the ConfigMap but not the key — suppress
                        // orphan checks for it below.
                        ambiguous_cm_names.insert(cm_name.to_string());
                    }
                    hits.push(Hit {
                        rule_id: MALFORMED_ENV_REFERENCE_RULE_ID,
                        detail: format!(
                            "{dir}: env {name} has a configMapKeyRef missing its `name` and/or `key` (name: {has_n}, key: {has_k})",
                            has_n = cm_name.is_some(),
                            has_k = cm_key.is_some(),
                        ),
                        file: rel_path.to_path_buf(),
                    });
                }
            }
        }
    }

    (
        Some(Workload {
            path: rel_path.to_path_buf(),
            env_names,
            key_refs,
            uses_env_from,
            ambiguous_cm_names,
            imports_unnamed_configmap,
        }),
        hits,
    )
}

/// Extract the `data:` keys of a ConfigMap document.
///
/// No key-shape filter: every `data:` key is a candidate env key. Filtering to
/// uppercase identifiers would silently drop anything that did not match, which
/// is the same class of invisible omission this guard exists to catch. (Verified
/// across all four service directories: no ConfigMap is volume-mounted and every
/// key is an env var name, so the strict reading costs nothing today and fails
/// loudly rather than quietly if that changes.)
fn configmap_keys(doc: &Value) -> BTreeSet<String> {
    doc.get("data")
        .and_then(Value::as_mapping)
        .map(|m| {
            m.iter()
                .filter_map(|(k, _)| k.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Walk one level of `infra/services/<dir>/`, parse every YAML document, and
/// classify each by `kind`.
fn discover_manifests(repo_root: &Path, infra_dir: &Path, dir: &str) -> Result<Discovered> {
    let mut out = Discovered::default();

    // Collect and sort so findings are ordered by path rather than by inode.
    let mut paths: Vec<PathBuf> = Vec::new();
    let entries = std::fs::read_dir(infra_dir)
        .with_context(|| format!("reading directory {}", infra_dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("reading entry in {}", infra_dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "yaml" || e == "yml");
        if !is_yaml {
            continue;
        }
        paths.push(path);
    }
    paths.sort();

    for path in paths {
        // ADR-0034 §5: containment through the single SoT, never a re-implementation.
        let rel_for_resolve = path.strip_prefix(repo_root).unwrap_or(&path);
        let rel_str = rel_for_resolve.to_string_lossy().into_owned();
        if resolve_cited_path(repo_root, &rel_str).is_none() {
            anyhow::bail!(
                "manifest path escapes the repository root or cannot be resolved: {rel_str}"
            );
        }
        let rel = rel_for_resolve.to_path_buf();

        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;

        for de in serde_norway::Deserializer::from_str(&raw) {
            let doc = Value::deserialize(de)
                .with_context(|| format!("parsing YAML in {}", rel.display()))?;

            // A trailing `---` yields a null document. That is not a manifest.
            // A document with *content* but no `kind` is a different case and
            // must not fall through into silence — see below.
            if doc.is_null() {
                continue;
            }

            let Some(kind) = doc.get("kind").and_then(Value::as_str) else {
                out.hits.push(Hit {
                    rule_id: UNCLASSIFIED_MANIFEST_KIND_RULE_ID,
                    detail: format!(
                        "{dir}: manifest document has no `kind` field; cannot classify it as workload, ConfigMap, or neither"
                    ),
                    file: rel.clone(),
                });
                continue;
            };

            if WORKLOAD_KINDS_WITH_POD_SPEC.contains(&kind) {
                let (workload, hits) = parse_workload(&doc, &rel, dir);
                out.hits.extend(hits);
                if let Some(w) = workload {
                    out.workloads.push(w);
                }
            } else if kind == CONFIGMAP_KIND {
                let Some(name) = metadata_name(&doc) else {
                    out.hits.push(Hit {
                        rule_id: UNCLASSIFIED_MANIFEST_KIND_RULE_ID,
                        detail: format!(
                            "{dir}: ConfigMap has no metadata.name; its keys cannot be resolved against any configMapKeyRef"
                        ),
                        file: rel.clone(),
                    });
                    continue;
                };
                out.configmaps.push(ConfigMapDoc {
                    path: rel.clone(),
                    name: name.to_string(),
                    keys: configmap_keys(&doc),
                });
            } else if !NO_POD_SPEC_KINDS.contains(&kind) {
                out.hits.push(Hit {
                    rule_id: UNCLASSIFIED_MANIFEST_KIND_RULE_ID,
                    detail: format!(
                        "{dir}: manifest kind {kind:?} is classified neither as a workload nor as pod-spec-free; if it carries a pod template add it to WORKLOAD_KINDS_WITH_POD_SPEC, otherwise to NO_POD_SPEC_KINDS"
                    ),
                    file: rel.clone(),
                });
            }
        }
    }

    Ok(out)
}

// -----------------------------------------------------------------------------
// Analysis.
// -----------------------------------------------------------------------------

pub(crate) fn analyze(repo_root: &Path) -> Result<Report> {
    let mut report = Report::default();

    for (_, dir) in crate::common::services::CANONICAL_SERVICES {
        let config_rs = repo_root.join("crates").join(dir).join("src/config.rs");
        let infra_dir = repo_root.join("infra/services").join(dir);

        // Formerly a silent `continue`. A canonical service losing its config
        // or its manifests would have vanished from the run with no trace at
        // all — not even the WARN the missing-workload path emitted.
        if !config_rs.is_file() || !infra_dir.is_dir() {
            report.hits.push(Hit {
                rule_id: SERVICE_INPUTS_MISSING_RULE_ID,
                detail: format!(
                    "{dir}: expected both crates/{dir}/src/config.rs and infra/services/{dir}/ (config.rs present: {c}, infra dir present: {i})",
                    c = config_rs.is_file(),
                    i = infra_dir.is_dir(),
                ),
                file: PathBuf::from(format!("infra/services/{dir}")),
            });
            continue;
        }

        let discovered = discover_manifests(repo_root, &infra_dir, dir)?;
        report.hits.extend(discovered.hits);

        if discovered.workloads.is_empty() {
            report.hits.push(Hit {
                rule_id: NO_WORKLOAD_MANIFEST_RULE_ID,
                detail: format!(
                    "{dir}: no workload manifest discovered in infra/services/{dir}/ (looked for kinds {kinds:?} in every *.yaml); this service is deployed but unchecked",
                    kinds = WORKLOAD_KINDS_WITH_POD_SPEC,
                ),
                file: PathBuf::from(format!("infra/services/{dir}")),
            });
            continue;
        }

        report.checked_services += 1;
        report.checked_workloads += discovered.workloads.len();

        let config_content = std::fs::read_to_string(&config_rs)
            .with_context(|| format!("reading {}", config_rs.display()))?;
        let required = extract_required_env_vars(&config_content);

        // ConfigMap name -> its declared keys. A name collision is fatal to a
        // name-scoped resolver — `.collect()` would silently keep the last and
        // hide both — so detect it explicitly rather than dedup by accident.
        let mut by_name: BTreeMap<&str, &ConfigMapDoc> = BTreeMap::new();
        for cm in &discovered.configmaps {
            if let Some(prev) = by_name.insert(cm.name.as_str(), cm) {
                report.hits.push(Hit {
                    rule_id: DUPLICATE_CONFIGMAP_NAME_RULE_ID,
                    detail: format!(
                        "{dir}: two ConfigMaps share metadata.name {name:?} ({a}, {b}); name-scoped resolution cannot tell them apart",
                        name = cm.name,
                        a = prev.path.display(),
                        b = cm.path.display(),
                    ),
                    file: cm.path.clone(),
                });
            }
        }

        // Every (configmap name, key) pair referenced by any workload here.
        let referenced: BTreeSet<(&str, &str)> = discovered
            .workloads
            .iter()
            .flat_map(|w| w.key_refs.iter().map(|(n, k)| (n.as_str(), k.as_str())))
            .collect();

        // Check 1 is suppressed PER WORKLOAD for any workload using envFrom (a
        // required var might be bulk-injected, so "not declared" is unprovable).
        // Check 3 is suppressed PER CONFIGMAP for maps named by an envFrom
        // import or a malformed ref (their key-consumption is unknowable) — NOT
        // service-wide: an orphan in a ConfigMap that no unseeable reference
        // names is still a real orphan. The `unsupported_env_source` /
        // `malformed_env_reference` hits fail the run regardless.
        let ambiguous_cms: BTreeSet<&str> = discovered
            .workloads
            .iter()
            .flat_map(|w| w.ambiguous_cm_names.iter().map(String::as_str))
            .collect();
        // Loop-invariant, hoisted so it reads as "check 3 is entirely off" not
        // "filtered per ConfigMap": true only when a workload imports a
        // ConfigMap we could not identify (unreadable `name`, or an `envFrom`
        // that is not a list), so no map can be excluded precisely.
        //
        // SOUNDNESS: this blanket suppression is safe ONLY because
        // `unsupported_env_source` is emitted UNCONDITIONALLY for any `envFrom`
        // and is a hard FAIL — so a run reaching this line is always red, and
        // the suppressed orphans cannot hide behind a green verdict. The
        // guarantor is that unconditional fatality, NOT the accompanying
        // `malformed_env_reference` hit. Anyone adding an `envFrom` allowlist,
        // or otherwise making `unsupported_env_source` non-fatal, MUST revisit
        // this line first: without it this becomes a silent service-wide orphan
        // blind spot on a green run. The invariant "suppression implies a fatal
        // hit" is encoded in `env_from_suppresses_missing_in_manifest`
        // (asserts the unsupported_env_source hit) and every suppression test's
        // `run(..).is_err()` assertion — weaken the fatality and those fail.
        let suppress_all_orphans = discovered
            .workloads
            .iter()
            .any(|w| w.imports_unnamed_configmap);

        // Check 1 — per workload, not union.
        for workload in &discovered.workloads {
            if !workload.uses_env_from {
                for var in &required {
                    if !workload.env_names.contains(var) {
                        report.hits.push(Hit {
                            rule_id: MISSING_IN_MANIFEST_RULE_ID,
                            detail: format!(
                                "{dir}: config.rs requires {var}, not declared by this workload"
                            ),
                            file: workload.path.clone(),
                        });
                    }
                }
            }

            // Check 2 — name-scoped resolution. Runs regardless of envFrom:
            // an explicit configMapKeyRef either resolves or it does not.
            for (cm_name, key) in &workload.key_refs {
                match by_name.get(cm_name.as_str()) {
                    None => report.hits.push(Hit {
                        rule_id: CONFIGMAP_NOT_FOUND_RULE_ID,
                        detail: format!(
                            "{dir}: configMapKeyRef names ConfigMap {cm_name:?} (key {key:?}) but no ConfigMap with that metadata.name exists in infra/services/{dir}/"
                        ),
                        file: workload.path.clone(),
                    }),
                    Some(cm) if !cm.keys.contains(key) => report.hits.push(Hit {
                        rule_id: KEY_NOT_IN_CONFIGMAP_RULE_ID,
                        detail: format!(
                            "{dir}: configMapKeyRef requests key {key:?} from ConfigMap {cm_name:?}, which does not declare it"
                        ),
                        file: workload.path.clone(),
                    }),
                    Some(_) => {}
                }
            }
        }

        // Check 3 — orphan keys, scoped to the ConfigMap that declares them.
        // A key is an orphan iff no workload references (this ConfigMap's name,
        // this key). Suppressed per-ConfigMap for maps a bulk import or a
        // malformed ref names (their key-consumption is unknowable), and only
        // service-wide in the unreadable-import fallback above.
        for cm in &discovered.configmaps {
            if suppress_all_orphans {
                break;
            }
            if ambiguous_cms.contains(cm.name.as_str()) {
                continue;
            }
            for key in &cm.keys {
                if !referenced.contains(&(cm.name.as_str(), key.as_str())) {
                    report.hits.push(Hit {
                        rule_id: ORPHAN_CONFIGMAP_KEY_RULE_ID,
                        detail: format!(
                            "{dir}: ConfigMap {name:?} declares {key:?}, referenced by no workload naming that ConfigMap",
                            name = cm.name,
                        ),
                        file: cm.path.clone(),
                    });
                }
            }
        }
    }

    Ok(report)
}

// -----------------------------------------------------------------------------
// Rendering.
// -----------------------------------------------------------------------------

/// One `VIOLATION:` line per hit, in discovery order.
///
/// Every hit is rendered regardless of which rule id wins the FAIL token —
/// precedence names the run, it never filters the body. A coverage fault that
/// suppressed the content findings would have the operator fix it, re-run, and
/// be ambushed by findings the guard already knew about.
fn render_lines(hits: &[Hit]) -> Vec<String> {
    hits.iter()
        .map(|hit| {
            format!(
                "VIOLATION: {} [{}] {}",
                hit.file.display(),
                hit.rule_id,
                hit.detail
            )
        })
        .collect()
}

/// `env-config-<kind>-<n_of_kind>-of-<total>-findings`.
///
/// Both numbers are explicit because neither can stand in for the other: with a
/// precedence table the selected kind and the total are fully decoupled, so a
/// single `<kind>-<total>` token would report (say) four undiscoverable
/// workloads when there was one and three orphan keys — a count attached to a
/// claim it does not measure, which is precisely the defect
/// `env-config-clean-4-services` embodied.
fn fail_token(hits: &[Hit]) -> String {
    let total = hits.len();
    for (rule_id, token) in RULE_PRECEDENCE {
        let n = hits.iter().filter(|h| h.rule_id == *rule_id).count();
        if n > 0 {
            return format!("env-config-{token}-{n}-of-{total}-findings");
        }
    }
    format!("env-config-unclassified-rule-0-of-{total}-findings")
}

/// Decide the no-findings outcome: the clean `REASON` token, or an error when
/// the run measured nothing.
///
/// Extracted as a pure function purely so the fail-closed branch is reachable
/// from a test. Through the `analyze()` → `run()` seam it is not: every
/// canonical service that fails discovery pushes a hit, so `hits.is_empty()`
/// implies all four were checked. That unreachability is exactly why the branch
/// needs a direct unit test — @test F-N8 disabled the guard and the
/// integration-style test still passed, because it never entered this block at
/// all. An untested fail-closed branch is an assertion, not a control.
fn clean_outcome(report: &Report) -> Result<String> {
    // Fail closed on "measured nothing". A future refactor that empties or
    // gates CANONICAL_SERVICES must not let the guard go green having verified
    // nothing — that is the exact shape this task exists to remove.
    if report.checked_services == 0 {
        anyhow::bail!("env-config-no-services-checked");
    }
    Ok(format!(
        "env-config-clean-{}-services-{}-workloads",
        report.checked_services, report.checked_workloads
    ))
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let report = analyze(repo_root)?;

    if report.hits.is_empty() {
        emit_ok(clean_outcome(&report)?);
        return Ok(());
    }

    if explain {
        for hit in &report.hits {
            let file_disp = hit.file.display().to_string();
            let policy = format!("env-config::{}", hit.rule_id);
            print_finding(&Finding {
                file: &file_disp,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &hit.detail,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        }
    } else {
        for line in render_lines(&report.hits) {
            println!("{line}");
        }
    }

    anyhow::bail!("{}", fail_token(&report.hits))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Fixture builder.
    // -------------------------------------------------------------------------

    const MINIMAL_DEPLOYMENT: &str = r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: svc
spec:
  template:
    spec:
      containers:
      - name: app
";

    /// `(service_dir, config_rs_body, &[(filename, yaml_body)])`.
    type Custom<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)]);

    /// Build a temp repo root containing all four canonical services.
    ///
    /// Services not named in `custom` get a minimal valid shape (a `config.rs`
    /// requiring nothing, one Deployment declaring nothing), so a fixture with
    /// no overrides is clean and any hit in a test is attributable to that
    /// test's override rather than to fixture noise.
    fn fixture(custom: &[Custom<'_>]) -> tempfile::TempDir {
        let td = tempfile::tempdir().expect("tempdir");
        let root = td.path();
        for (_, dir) in crate::common::services::CANONICAL_SERVICES {
            let over = custom.iter().find(|(d, _, _)| d == dir);
            let crate_dir = root.join("crates").join(dir).join("src");
            std::fs::create_dir_all(&crate_dir).expect("mkdir crate");
            std::fs::write(
                crate_dir.join("config.rs"),
                over.map_or("", |(_, body, _)| body),
            )
            .expect("write config.rs");

            let infra = root.join("infra/services").join(dir);
            std::fs::create_dir_all(&infra).expect("mkdir infra");
            match over {
                Some((_, _, files)) => {
                    for (name, body) in *files {
                        std::fs::write(infra.join(name), body).expect("write manifest");
                    }
                }
                None => {
                    std::fs::write(infra.join("deployment.yaml"), MINIMAL_DEPLOYMENT)
                        .expect("write deployment");
                }
            }
        }
        td
    }

    fn hits_for<'a>(report: &'a Report, rule: &str) -> Vec<&'a Hit> {
        report.hits.iter().filter(|h| h.rule_id == rule).collect()
    }

    /// A Deployment naming `cm_name` for `key`, plus one plain env var.
    fn deployment_ref(name: &str, cm_name: &str, key: &str) -> String {
        format!(
            r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {name}
spec:
  template:
    spec:
      containers:
      - name: app
        env:
        - name: {key}
          valueFrom:
            configMapKeyRef:
              name: {cm_name}
              key: {key}
"
        )
    }

    fn configmap(name: &str, key: &str) -> String {
        format!(
            r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {name}
data:
  {key}: "value"
"#
        )
    }

    // -------------------------------------------------------------------------
    // Baseline.
    // -------------------------------------------------------------------------

    #[test]
    fn baseline_fixture_is_clean_and_counts_honestly() {
        let td = fixture(&[]);
        let report = analyze(td.path()).expect("analyze");
        assert!(report.hits.is_empty(), "unexpected hits: {:?}", report.hits);
        assert_eq!(report.checked_services, 4);
        assert_eq!(report.checked_workloads, 4);
    }

    // -------------------------------------------------------------------------
    // Discovery.
    // -------------------------------------------------------------------------

    #[test]
    fn discovers_per_instance_workloads() {
        // Trap: the old two-name `find_workload` probed only `deployment.yaml`
        // / `statefulset.yaml`, found neither, and warn-skipped the service
        // while still counting it as checked.
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                ("mh-0-deployment.yaml", MINIMAL_DEPLOYMENT),
                ("mh-1-deployment.yaml", MINIMAL_DEPLOYMENT),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert!(report.hits.is_empty(), "unexpected hits: {:?}", report.hits);
        assert_eq!(report.checked_services, 4);
        // 3 defaults + 2 per-instance MH workloads.
        assert_eq!(report.checked_workloads, 5);
    }

    #[test]
    fn single_workload_services_still_checked() {
        // Regression net for the shapes that DID work before: AC's
        // statefulset.yaml and GC's deployment.yaml. Also carries the
        // discrimination controls — a `Service` document must NOT trip
        // `unclassified_manifest_kind`, and plain `env:` must NOT trip
        // `unsupported_env_source`. A rule wired to fire unconditionally would
        // pass its own FAIL-path test; these are what catch that.
        let statefulset = r"apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: ac
spec:
  template:
    spec:
      containers:
      - name: app
        env:
        - name: PLAIN
          value: 'x'
";
        let service = r"apiVersion: v1
kind: Service
metadata:
  name: ac-service
spec:
  ports:
  - port: 8080
";
        let td = fixture(&[(
            "ac-service",
            "",
            &[("statefulset.yaml", statefulset), ("service.yaml", service)],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert!(report.hits.is_empty(), "unexpected hits: {:?}", report.hits);
        assert_eq!(report.checked_services, 4);
        assert_eq!(report.checked_workloads, 4);
    }

    #[test]
    fn multi_document_yaml_file_is_fully_parsed() {
        // `service.yaml` in mc/mh holds three documents. A parser that read
        // only the first would silently miss workloads in multi-doc files.
        let multi = format!(
            "{}---\n{}",
            MINIMAL_DEPLOYMENT,
            configmap("only-cm", "ORPHANED")
        );
        let td = fixture(&[("mh-service", "", &[("all.yaml", &multi)])]);
        let report = analyze(td.path()).expect("analyze");
        // The ConfigMap in the SECOND document was seen — proven by its key
        // being reported as an orphan.
        let orphans = hits_for(&report, ORPHAN_CONFIGMAP_KEY_RULE_ID);
        assert_eq!(orphans.len(), 1, "hits: {:?}", report.hits);
        assert!(orphans[0].detail.contains("ORPHANED"));
    }

    #[test]
    fn null_document_is_skipped_but_kindless_document_fails() {
        // Two cases that look alike and are not: a trailing `---` yields a null
        // document (not a manifest, skip), while a document with content but no
        // `kind` must not fall through into silence.
        let trailing = format!("{MINIMAL_DEPLOYMENT}---\n");
        let td = fixture(&[("mh-service", "", &[("w.yaml", &trailing)])]);
        let report = analyze(td.path()).expect("analyze");
        assert!(report.hits.is_empty(), "unexpected hits: {:?}", report.hits);

        let kindless = format!("{MINIMAL_DEPLOYMENT}---\nmetadata:\n  name: mystery\n");
        let td = fixture(&[("mh-service", "", &[("w.yaml", &kindless)])]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, UNCLASSIFIED_MANIFEST_KIND_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
    }

    // -------------------------------------------------------------------------
    // Multi-ConfigMap resolution.
    // -------------------------------------------------------------------------

    #[test]
    fn per_instance_configmap_key_resolves() {
        // The real MH_WEBTRANSPORT_ADVERTISE_ADDRESS shape: the key lives ONLY
        // in the per-instance ConfigMap. Trap: resolving against a single
        // hardcoded `configmap.yaml` yields a false `key_not_in_configmap` —
        // the false positive the task predicted on the fix's first run.
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                (
                    "mh-0-deployment.yaml",
                    &deployment_ref("mh-0", "mh-0-config", "ADVERTISE"),
                ),
                (
                    "mh-0-configmap.yaml",
                    &configmap("mh-0-config", "ADVERTISE"),
                ),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert!(report.hits.is_empty(), "unexpected hits: {:?}", report.hits);
    }

    #[test]
    fn configmap_key_ref_is_name_scoped() {
        // mh-1's deployment references mh-0's ConfigMap. The key NAME exists in
        // both, so a directory-wide union calls this clean. Name-scoped
        // resolution sees that mh-1-config's key is referenced by nobody.
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                (
                    "mh-0-deployment.yaml",
                    &deployment_ref("mh-0", "mh-0-config", "ADVERTISE"),
                ),
                (
                    "mh-1-deployment.yaml",
                    &deployment_ref("mh-1", "mh-0-config", "ADVERTISE"),
                ),
                (
                    "mh-0-configmap.yaml",
                    &configmap("mh-0-config", "ADVERTISE"),
                ),
                (
                    "mh-1-configmap.yaml",
                    &configmap("mh-1-config", "ADVERTISE"),
                ),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        let orphans = hits_for(&report, ORPHAN_CONFIGMAP_KEY_RULE_ID);
        assert_eq!(orphans.len(), 1, "hits: {:?}", report.hits);
        assert!(
            orphans[0].detail.contains("mh-1-config"),
            "expected mh-1-config orphan, got {}",
            orphans[0].detail
        );
    }

    #[test]
    fn undefined_configmap_ref_flags_configmap_not_found() {
        let td = fixture(&[(
            "mh-service",
            "",
            &[(
                "mh-0-deployment.yaml",
                &deployment_ref("mh-0", "nowhere-config", "ADVERTISE"),
            )],
        )]);
        let report = analyze(td.path()).expect("analyze");
        let hits = hits_for(&report, CONFIGMAP_NOT_FOUND_RULE_ID);
        assert_eq!(hits.len(), 1, "hits: {:?}", report.hits);
        assert!(hits[0].detail.contains("nowhere-config"));
    }

    #[test]
    fn key_absent_from_the_named_configmap_is_flagged() {
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                (
                    "mh-0-deployment.yaml",
                    &deployment_ref("mh-0", "mh-0-config", "ADVERTISE"),
                ),
                (
                    "mh-0-configmap.yaml",
                    &configmap("mh-0-config", "SOMETHING_ELSE"),
                ),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, KEY_NOT_IN_CONFIGMAP_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
    }

    #[test]
    fn orphan_key_detected() {
        // The four MH OTel keys in miniature.
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                ("mh-0-deployment.yaml", MINIMAL_DEPLOYMENT),
                (
                    "configmap.yaml",
                    &configmap("mh-service-config", "OTEL_ENABLED"),
                ),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        let orphans = hits_for(&report, ORPHAN_CONFIGMAP_KEY_RULE_ID);
        assert_eq!(orphans.len(), 1, "hits: {:?}", report.hits);
        assert!(orphans[0].detail.contains("OTEL_ENABLED"));
    }

    // -------------------------------------------------------------------------
    // Check 1 — per workload, not union.
    // -------------------------------------------------------------------------

    #[test]
    fn required_env_var_missing_in_one_instance() {
        // The "easy to half-do" case: present in mh-0, absent from mh-1. A
        // union check reports clean while mh-1 CrashLoops on MissingEnvVar —
        // half capacity, reported green.
        let with_var = r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: mh-0
spec:
  template:
    spec:
      containers:
      - name: app
        env:
        - name: MH_TLS_CERT_PATH
          value: '/etc/tls'
";
        let td = fixture(&[(
            "mh-service",
            r#"ConfigError::MissingEnvVar("MH_TLS_CERT_PATH".to_string())"#,
            &[
                ("mh-0-deployment.yaml", with_var),
                ("mh-1-deployment.yaml", MINIMAL_DEPLOYMENT),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        let missing = hits_for(&report, MISSING_IN_MANIFEST_RULE_ID);
        assert_eq!(missing.len(), 1, "hits: {:?}", report.hits);
        assert!(
            missing[0].file.ends_with("mh-1-deployment.yaml"),
            "expected mh-1 to be blamed, got {}",
            missing[0].file.display()
        );
    }

    #[test]
    fn init_container_env_names_are_collected() {
        // Omitting initContainers is not neutral: it would invent a false
        // `missing_in_manifest` for a var declared only there.
        let with_init = r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: mh-0
spec:
  template:
    spec:
      initContainers:
      - name: setup
        env:
        - name: MH_TLS_CERT_PATH
          value: '/etc/tls'
      containers:
      - name: app
";
        let td = fixture(&[(
            "mh-service",
            r#"ConfigError::MissingEnvVar("MH_TLS_CERT_PATH".to_string())"#,
            &[("mh-0-deployment.yaml", with_init)],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert!(report.hits.is_empty(), "unexpected hits: {:?}", report.hits);
    }

    // -------------------------------------------------------------------------
    // Hard-fail paths. Each asserts `run()` actually errors — a test that only
    // checked for a Hit would pass against the warn-and-continue bug.
    // -------------------------------------------------------------------------

    #[test]
    fn missing_workload_is_hard_fail() {
        let td = fixture(&[(
            "mh-service",
            "",
            &[("configmap.yaml", &configmap("mh-service-config", "K"))],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, NO_WORKLOAD_MANIFEST_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        assert!(
            run(td.path(), false).is_err(),
            "an undiscoverable workload must FAIL the run, not warn-and-continue"
        );
    }

    #[test]
    fn honest_count_excludes_unchecked_service() {
        // The count bug itself: `checked_services += 1` used to fire before the
        // skip, so a skipped service was counted as checked.
        let td = fixture(&[(
            "mh-service",
            "",
            &[("configmap.yaml", &configmap("c", "K"))],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            report.checked_services, 3,
            "mh-service had no workload and must not be counted as checked"
        );
        assert_eq!(report.checked_workloads, 3);
    }

    #[test]
    fn service_inputs_missing_is_hard_fail() {
        let td = fixture(&[]);
        std::fs::remove_dir_all(td.path().join("infra/services/mh-service")).expect("rm");
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, SERVICE_INPUTS_MISSING_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        assert_eq!(report.checked_services, 3);
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn env_from_is_hard_fail() {
        let env_from = r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: mh-0
spec:
  template:
    spec:
      containers:
      - name: app
        envFrom:
        - configMapRef:
            name: mh-service-config
";
        let td = fixture(&[("mh-service", "", &[("mh-0-deployment.yaml", env_from)])]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, UNSUPPORTED_ENV_SOURCE_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn unclassified_kind_is_hard_fail() {
        let cron = r"apiVersion: batch/v1
kind: CronJob
metadata:
  name: reaper
spec:
  schedule: '* * * * *'
";
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                ("mh-0-deployment.yaml", MINIMAL_DEPLOYMENT),
                ("cronjob.yaml", cron),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        let hits = hits_for(&report, UNCLASSIFIED_MANIFEST_KIND_RULE_ID);
        assert_eq!(hits.len(), 1, "hits: {:?}", report.hits);
        assert!(hits[0].detail.contains("CronJob"));
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn workload_missing_pod_spec_is_hard_fail() {
        let no_pod = r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: broken
spec:
  replicas: 1
";
        let td = fixture(&[("mh-service", "", &[("mh-0-deployment.yaml", no_pod)])]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, WORKLOAD_MISSING_POD_SPEC_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        // It also fails discovery, so the service is not counted as checked.
        assert_eq!(report.checked_services, 3);
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn unparseable_manifest_is_an_error_not_a_skip() {
        let td = fixture(&[(
            "mh-service",
            "",
            &[("mh-0-deployment.yaml", "a:\n  - b\n c\n")],
        )]);
        assert!(
            analyze(td.path()).is_err(),
            "a manifest that cannot be parsed must fail loudly, never be skipped"
        );
    }

    // -------------------------------------------------------------------------
    // Rendering / token separation.
    // -------------------------------------------------------------------------

    #[test]
    fn mixed_hit_set_renders_all_hits_but_token_selects_coverage_fault() {
        let hits = vec![
            Hit {
                rule_id: ORPHAN_CONFIGMAP_KEY_RULE_ID,
                detail: "orphan detail".to_string(),
                file: PathBuf::from("a.yaml"),
            },
            Hit {
                rule_id: NO_WORKLOAD_MANIFEST_RULE_ID,
                detail: "coverage detail".to_string(),
                file: PathBuf::from("b"),
            },
        ];
        let lines = render_lines(&hits);
        assert_eq!(lines.len(), 2, "precedence must not filter the output body");
        assert!(lines.iter().any(|l| l.contains("orphan detail")));
        assert!(lines.iter().any(|l| l.contains("coverage detail")));

        // Coverage fault names the run even though it is in the minority.
        assert_eq!(
            fail_token(&hits),
            "env-config-no-workload-manifest-1-of-2-findings"
        );
    }

    #[test]
    fn fail_token_reports_both_counts_separately() {
        let hits = vec![
            Hit {
                rule_id: ORPHAN_CONFIGMAP_KEY_RULE_ID,
                detail: String::new(),
                file: PathBuf::new(),
            },
            Hit {
                rule_id: ORPHAN_CONFIGMAP_KEY_RULE_ID,
                detail: String::new(),
                file: PathBuf::new(),
            },
        ];
        assert_eq!(fail_token(&hits), "env-config-orphan-key-2-of-2-findings");
    }

    #[test]
    fn all_rule_ids_have_a_precedence_entry() {
        // Drift guard: a new rule id without a precedence entry would fall
        // through to the placeholder token and report `0` findings for a run
        // that had some.
        for id in [
            MISSING_IN_MANIFEST_RULE_ID,
            KEY_NOT_IN_CONFIGMAP_RULE_ID,
            ORPHAN_CONFIGMAP_KEY_RULE_ID,
            CONFIGMAP_NOT_FOUND_RULE_ID,
            NO_WORKLOAD_MANIFEST_RULE_ID,
            SERVICE_INPUTS_MISSING_RULE_ID,
            UNCLASSIFIED_MANIFEST_KIND_RULE_ID,
            WORKLOAD_MISSING_POD_SPEC_RULE_ID,
            UNSUPPORTED_ENV_SOURCE_RULE_ID,
            MALFORMED_ENV_REFERENCE_RULE_ID,
            DUPLICATE_CONFIGMAP_NAME_RULE_ID,
        ] {
            assert!(
                RULE_PRECEDENCE.iter().any(|(r, _)| *r == id),
                "rule id {id:?} has no RULE_PRECEDENCE entry"
            );
        }
    }

    #[test]
    fn workload_and_no_pod_spec_kind_lists_are_disjoint() {
        for kind in WORKLOAD_KINDS_WITH_POD_SPEC {
            assert!(
                !NO_POD_SPEC_KINDS.contains(kind),
                "{kind:?} is in both kind lists; a pod-carrying kind silenced as pod-spec-free reopens the coverage hole"
            );
        }
    }

    #[test]
    fn zero_container_pod_spec_is_hard_fail() {
        // F1: `spec.template.spec` exists but declares no containers. Trap: the
        // empty container loop leaves a Workload with empty env that satisfies
        // check 1 vacuously — empty-collection-means-clean, the exact pattern
        // this guard exists to kill, under a doc comment asserting it doesn't.
        let empty_pod = r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: hollow
spec:
  template:
    spec: {}
";
        let td = fixture(&[("mh-service", "", &[("mh-0-deployment.yaml", empty_pod)])]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, WORKLOAD_MISSING_POD_SPEC_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        // Not counted as a checked workload.
        assert_eq!(report.checked_services, 3);
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn malformed_configmap_key_ref_is_surfaced_not_dropped() {
        // F2: a `configMapKeyRef` missing its `key`. Trap: silently dropping it
        // makes the ConfigMap key it should have named surface as an orphan,
        // and the orphan runbook row says "or removing it" — a wrong-direction
        // remediation on a key that IS referenced, just malformedly.
        let malformed = r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: mh-0
spec:
  template:
    spec:
      containers:
      - name: app
        env:
        - name: MH_REGION
          valueFrom:
            configMapKeyRef:
              name: mh-service-config
";
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                ("mh-0-deployment.yaml", malformed),
                (
                    "configmap.yaml",
                    &configmap("mh-service-config", "MH_REGION"),
                ),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, MALFORMED_ENV_REFERENCE_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        // The malformed ref must NOT masquerade as an orphan on MH_REGION.
        assert!(
            hits_for(&report, ORPHAN_CONFIGMAP_KEY_RULE_ID).is_empty(),
            "malformed ref misattributed as orphan: {:?}",
            report.hits
        );
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn duplicate_configmap_name_is_hard_fail() {
        // F3: two documents share metadata.name. Trap: `.collect()` keeps the
        // last silently, and because the referenced set is name-keyed, both
        // read as "referenced" — the collision is entirely invisible.
        let dup = format!(
            "{}---
{}",
            configmap("dup", "A"),
            configmap("dup", "B")
        );
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                ("mh-0-deployment.yaml", &deployment_ref("mh-0", "dup", "A")),
                ("configmaps.yaml", &dup),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, DUPLICATE_CONFIGMAP_NAME_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        assert!(run(td.path(), false).is_err());
    }

    /// A workload whose sole container bulk-imports `cm_name` via envFrom.
    fn env_from_deployment(name: &str, cm_name: &str) -> String {
        format!(
            r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {name}
spec:
  template:
    spec:
      containers:
      - name: app
        envFrom:
        - configMapRef:
            name: {cm_name}
"
        )
    }

    #[test]
    fn env_from_suppresses_missing_in_manifest() {
        // @test trap 1 (check-1 suppression, per-workload). config.rs requires a
        // var the envFrom workload does not explicitly declare; envFrom could
        // inject it, so `missing_in_manifest` must NOT fire — only
        // `unsupported_env_source`. Reverting `if !workload.uses_env_from` fails
        // this.
        let td = fixture(&[(
            "mh-service",
            r#"ConfigError::MissingEnvVar("MH_TLS_CERT_PATH".to_string())"#,
            &[(
                "mh-0-deployment.yaml",
                &env_from_deployment("mh-0", "mh-service-config"),
            )],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, UNSUPPORTED_ENV_SOURCE_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        assert!(
            hits_for(&report, MISSING_IN_MANIFEST_RULE_ID).is_empty(),
            "check 1 must not run against an envFrom workload: {:?}",
            report.hits
        );
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn env_from_suppresses_orphan_key() {
        // @test trap 2 (check-3 suppression, per-ConfigMap). The envFrom import
        // names mh-service-config, whose key is referenced by no explicit
        // configMapKeyRef; envFrom could consume it, so `orphan_key` must NOT
        // fire for that map. Reverting the `ambiguous_cms.contains` skip fails
        // this.
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                (
                    "mh-0-deployment.yaml",
                    &env_from_deployment("mh-0", "mh-service-config"),
                ),
                (
                    "configmap.yaml",
                    &configmap("mh-service-config", "SOME_KEY"),
                ),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert!(
            hits_for(&report, ORPHAN_CONFIGMAP_KEY_RULE_ID).is_empty(),
            "orphan check must be suppressed for a bulk-imported ConfigMap: {:?}",
            report.hits
        );
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn env_from_orphan_suppression_is_per_configmap_not_service_wide() {
        // @test scope-pin. mh-0 bulk-imports mh-service-config; mh-1 does NOT use
        // envFrom and mh-1-config carries a genuine orphan. That orphan MUST
        // still fire — envFrom on one ConfigMap says nothing about a sibling.
        // This pins the DECISION that check-3 suppression is per-ConfigMap, not
        // the service-wide `any_env_from` break an earlier revision used. A
        // service-wide suppression would wrongly silence mh-1-config's orphan.
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                (
                    "mh-0-deployment.yaml",
                    &env_from_deployment("mh-0", "mh-service-config"),
                ),
                ("mh-1-deployment.yaml", MINIMAL_DEPLOYMENT),
                (
                    "configmap.yaml",
                    &configmap("mh-service-config", "IMPORTED_KEY"),
                ),
                (
                    "mh-1-configmap.yaml",
                    &configmap("mh-1-config", "REAL_ORPHAN"),
                ),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        let orphans = hits_for(&report, ORPHAN_CONFIGMAP_KEY_RULE_ID);
        assert_eq!(
            orphans.len(),
            1,
            "exactly mh-1-config's orphan should survive: {:?}",
            report.hits
        );
        assert!(
            orphans[0].detail.contains("REAL_ORPHAN") && orphans[0].detail.contains("mh-1-config"),
            "wrong orphan surfaced: {}",
            orphans[0].detail
        );
        // And mh-service-config's imported key is NOT reported.
        assert!(
            !orphans[0].detail.contains("IMPORTED_KEY"),
            "bulk-imported key must be suppressed: {:?}",
            report.hits
        );
    }

    #[test]
    fn unreadable_env_from_configmap_ref_suppresses_orphans_service_wide() {
        // F5 fallback (@security + @code-reviewer). An envFrom configMapRef with
        // no readable `name` names an unknowable ConfigMap, so orphan checks
        // cannot be scoped and are suppressed service-wide — but LOUDLY, via a
        // malformed_env_reference hit. Trap: removing the `suppress_all_orphans`
        // break lets the sibling orphan fire, so this test fails.
        let unreadable = r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: mh-0
spec:
  template:
    spec:
      containers:
      - name: app
        envFrom:
        - configMapRef:
            optional: true
";
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                ("mh-0-deployment.yaml", unreadable),
                (
                    "configmap.yaml",
                    &configmap("mh-service-config", "WOULD_BE_ORPHAN"),
                ),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        // Loud: the unknowable import is reported.
        assert_eq!(
            hits_for(&report, MALFORMED_ENV_REFERENCE_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        // Conservative: no orphan is asserted while an unreadable import exists.
        assert!(
            hits_for(&report, ORPHAN_CONFIGMAP_KEY_RULE_ID).is_empty(),
            "orphans must be suppressed under an unreadable bulk import: {:?}",
            report.hits
        );
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn env_from_written_as_mapping_emits_malformed_hit() {
        // F6 (@security). `envFrom:` as a mapping rather than a list is a very
        // common YAML mistake. It makes the imports unreadable, which suppresses
        // check 3 SERVICE-WIDE — so the suppression must be explained at the
        // point it happens. Trap: dropping the hit from the `None` arm leaves
        // only the generic unsupported_env_source line, and nothing tells the
        // reader an orphan check was disabled or why.
        let mapping_env_from = r"apiVersion: apps/v1
kind: Deployment
metadata:
  name: mh-0
spec:
  template:
    spec:
      containers:
      - name: app
        envFrom:
          configMapRef:
            name: mh-service-config
";
        let td = fixture(&[(
            "mh-service",
            "",
            &[
                ("mh-0-deployment.yaml", mapping_env_from),
                (
                    "configmap.yaml",
                    &configmap("mh-service-config", "REAL_ORPHAN"),
                ),
            ],
        )]);
        let report = analyze(td.path()).expect("analyze");
        assert_eq!(
            hits_for(&report, MALFORMED_ENV_REFERENCE_RULE_ID).len(),
            1,
            "the unreadable envFrom shape must be reported, not silently widen suppression: {:?}",
            report.hits
        );
        // The suppression itself is still in force (that part is correct)...
        assert!(
            hits_for(&report, ORPHAN_CONFIGMAP_KEY_RULE_ID).is_empty(),
            "hits: {:?}",
            report.hits
        );
        // ...and the run is red regardless, which is what makes the blanket
        // suppression sound in the first place.
        assert_eq!(
            hits_for(&report, UNSUPPORTED_ENV_SOURCE_RULE_ID).len(),
            1,
            "hits: {:?}",
            report.hits
        );
        assert!(run(td.path(), false).is_err());
    }

    #[test]
    fn clean_outcome_fails_closed_when_nothing_was_measured() {
        // @test F-N8. The fail-closed branch is unreachable through
        // analyze()->run() (an empty root pushes service_inputs_missing hits, so
        // the hits.is_empty() block is never entered), which is why the previous
        // run()-level test passed even with the guard disabled. Drive the
        // decision directly. Trap: changing the guard to `if false` makes this
        // assertion fail.
        let nothing_measured = Report::default();
        assert_eq!(nothing_measured.checked_services, 0);
        assert!(nothing_measured.hits.is_empty());
        let err = clean_outcome(&nothing_measured)
            .expect_err("a run that measured nothing must NOT report clean");

        // SSoT drift-guard (CLAUDE.md): the `bail!` string above and the
        // runbook §8 triage row are two encodings of one value with nothing
        // else binding them. Asserting through `reason_token` runs the real
        // `sluggify` on the real error, so this pins the REASON as actually
        // EMITTED — not the argument as written. It is the only one of the
        // eleven env-config tokens without an exact-string pin (every
        // `fail_token` product has one), and it reds on two independent
        // regressions: a changed `bail!` argument, and a `sluggify` change that
        // mangles the slug.
        assert_eq!(
            crate::common::status::reason_token(&err),
            "env-config-no-services-checked",
            "emitted REASON must match the runbook §8 row"
        );
    }

    #[test]
    fn clean_outcome_reports_both_counts_when_services_were_checked() {
        let measured = Report {
            checked_services: 4,
            checked_workloads: 6,
            hits: Vec::new(),
        };
        let token = clean_outcome(&measured).expect("clean run yields a token");
        assert_eq!(token, "env-config-clean-4-services-6-workloads");
    }

    // -------------------------------------------------------------------------
    // Retained unit coverage.
    // -------------------------------------------------------------------------

    #[test]
    fn extract_required_env_vars_finds_missing_env_var() {
        let content = r#"
            return Err(ConfigError::MissingEnvVar("DATABASE_URL".to_string()));
            return Err(ConfigError::MissingEnvVar("JWT_SECRET".to_string()));
        "#;
        let vars = extract_required_env_vars(content);
        assert_eq!(
            vars,
            vec!["DATABASE_URL".to_string(), "JWT_SECRET".to_string()]
        );
    }

    #[test]
    fn configmap_keys_collects_the_data_block() {
        let doc: Value = serde_norway::from_str(
            r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: cm
data:
  LOG_LEVEL: info
  METRICS_PORT: "9090"
"#,
        )
        .expect("parse");
        let keys = configmap_keys(&doc);
        assert!(keys.contains("LOG_LEVEL"));
        assert!(keys.contains("METRICS_PORT"));
        assert_eq!(keys.len(), 2, "metadata.name must not leak into data keys");
    }
}
