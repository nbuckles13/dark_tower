//! `resolved-features` subcommand — every crate that picks a process-level
//! crypto provider from Cargo features resolves to exactly its approved
//! provider, and aws-lc is reachable only through jsonwebtoken.
//!
//! jsonwebtoken (>=10) and rustls each choose their crypto backend from
//! features at compile time. Zero providers or two providers both compile
//! cleanly and fail only at runtime (jsonwebtoken panics at the first
//! sign/verify; rustls panics in any implicit-provider `builder()`). A
//! dependency bump re-enabling a default feature — e.g. restoring
//! `metrics-exporter-prometheus`'s defaults, whose `push-gateway` turns on
//! rustls `aws_lc_rs` — is invisible to every other gate. This guard reads the
//! RESOLVED features cargo actually computes and compares them with
//! [`policy`] (ADR-0027 is the authority).
//!
//! ## How the graph is read
//!
//! `cargo metadata --format-version 1 --offline --locked --manifest-path
//! <root>/Cargo.toml` (spawned via [`run_cargo`]; never the caller's cwd).
//! `resolve.nodes[].features` is the feature set cargo resolved for each
//! package across the WHOLE workspace with default features, dev-dependencies
//! included (tests link these crates too). No `--filter-platform`: the
//! all-platform union over-approximates, which is fail-closed for the deny
//! side (a provider that must be absent is absent from every binary if it is
//! absent from the union). On the require side the union could show
//! `aws_lc_rs` while one binary lacked it; that side is held by the feature
//! living on the root `[workspace.dependencies]` entry, which workspace-deps
//! R1/R5 force every member to inherit (a member can add features, never
//! remove them).
//!
//! ## Scope
//!
//! The ROOT workspace only. The root-`exclude` fuzz workspaces have no tracked
//! Cargo.lock, so `--offline --locked` cannot resolve them; their jsonwebtoken
//! entry is bound to the root's (version, default-features, features ⊇) by
//! `workspace-deps` R3 instead.
//!
//! ## Residual gap
//!
//! A provider feature a crate adds upstream that is NOT in our vocabulary is
//! invisible to the partition check. If it pulls aws-lc, the reverse-dependent
//! backstop ([`policy::REVERSE_DEP_POLICY`]) catches it; if it pulls a third,
//! unlisted backend, nothing here does. A vocabulary term that stops being
//! declared (an upstream rename) IS caught, as a precondition.
//!
//! ## Why this runs in Layer 1, not Layer 3
//!
//! It invokes cargo, and Layer 3 invoking cargo zero times is structural
//! (`scripts/lang/rust/compile.sh` has the reasoning). It is wired in
//! `compile.sh` after `service-dep-exclusion`, NOT as a
//! `scripts/guards/simple/*.sh` wrapper. `--offline` needs every platform's
//! package sources (no `--filter-platform`), which the host-only workspace
//! build does NOT download; `compile.sh` therefore runs an all-platform
//! `cargo fetch --locked` as its own labelled step first, so a cold registry is
//! a network failure in THAT lane, not a false red here.
//!
//! ## Vacuity
//!
//! cargo failing, output that does not parse, an empty graph, a policy crate
//! absent from the graph, and an undeclared vocabulary term are each a
//! PRECONDITION with its own token — never a pass.
//!
//! ## Ownership
//!
//! Machinery (this file): infrastructure. Policy content ([`policy`]):
//! security with auth-controller.

pub mod policy;

use crate::common::cargo_invoke::run_cargo;
use crate::common::explain::{print_finding, Finding};
use crate::common::status::{emit_ok, emit_scope};
use anyhow::Result;
use policy::{ProviderPolicy, ReverseDepPolicy, PROVIDER_POLICY, REVERSE_DEP_POLICY};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const CARGO_FAILED: &str = "resolved_features_cargo_failed";
pub const METADATA_UNPARSEABLE: &str = "resolved_features_metadata_unparseable";
pub const POLICY_CRATE_ABSENT: &str = "resolved_features_policy_crate_absent";
pub const VOCABULARY_TERM_UNDECLARED: &str = "resolved_features_vocabulary_term_undeclared";
pub const PROVIDER_FEATURES_MISMATCH: &str = "provider_features_mismatch";
pub const REVERSE_DEPENDENT_DISALLOWED: &str = "reverse_dependent_disallowed";

/// PRECONDITION classes, in precedence order (first wins the REASON slot).
const PRECONDITION_RULES: &[&str] = &[
    CARGO_FAILED,
    METADATA_UNPARSEABLE,
    POLICY_CRATE_ABSENT,
    VOCABULARY_TERM_UNDECLARED,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub rule_id: &'static str,
    /// `crate@version`, or `cargo metadata` for whole-run preconditions.
    pub subject: String,
    pub detail: String,
}

/// What was examined — the anti-vacuity counts on the PASS line.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scope {
    pub nodes: usize,
    pub provider_instances: usize,
    pub reverse_dep_crates: usize,
}

// Minimal typed view of `cargo metadata --format-version 1`.
#[derive(Debug, serde::Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Option<Resolve>,
}

#[derive(Debug, serde::Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    /// Declared features (explicit `[features]` plus implicit optional-dep ones).
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Debug, serde::Deserialize)]
struct Node {
    id: String,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    deps: Vec<NodeDep>,
}

#[derive(Debug, serde::Deserialize)]
struct NodeDep {
    pkg: String,
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    // ABSOLUTE: `run_cargo` also sets `current_dir(repo_root)`, so a relative
    // `--root` joined into a relative manifest path would be applied twice.
    let raw = match std::path::absolute(repo_root.join("Cargo.toml")) {
        Ok(manifest) => run_metadata(repo_root, &manifest),
        Err(e) => Err(format!(
            "cannot make `{}` absolute: {e}",
            repo_root.display()
        )),
    };
    report(evaluate(raw, PROVIDER_POLICY, REVERSE_DEP_POLICY), explain)
}

fn run_metadata(repo_root: &Path, manifest: &Path) -> std::result::Result<String, String> {
    let manifest = manifest.to_string_lossy();
    run_cargo(
        repo_root,
        &[
            "metadata",
            "--format-version",
            "1",
            "--offline",
            "--locked",
            "--manifest-path",
            &manifest,
        ],
    )
}

fn report((hits, scope): (Vec<Hit>, Scope), explain: bool) -> Result<()> {
    emit_scope(format!(
        "{} resolved nodes, {} provider-policy crate instances, {} reverse-dependent \
         policies, {} hits",
        scope.nodes,
        scope.provider_instances,
        scope.reverse_dep_crates,
        hits.len()
    ));

    if hits.is_empty() {
        emit_ok(format!(
            "resolved-features-clean-instances-{}",
            scope.provider_instances
        ));
        return Ok(());
    }
    let mut ordered: Vec<&Hit> = hits.iter().collect();
    ordered.sort_by_key(|h| precedence(h.rule_id));
    for hit in &ordered {
        let token = hit.rule_id.replace('_', "-");
        if explain {
            let policy = format!("resolved-features::{}", hit.rule_id);
            print_finding(&Finding {
                file: &hit.subject,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &hit.detail,
                extras: &[("reason", &token)],
                src_file: file!(),
                src_line: line!(),
            });
        } else if PRECONDITION_RULES.contains(&hit.rule_id) {
            println!(
                "ERROR: PRECONDITION [{token}] {} — {} (resolution/precondition failure: \
                 nothing was checked, never a pass; see docs/runbooks/devloop-validation.md §6.1)",
                hit.subject, hit.detail
            );
        } else {
            println!("VIOLATION: [{token}] {} {}", hit.subject, hit.detail);
        }
    }
    let first = ordered.first().map_or("resolved-features", |h| h.rule_id);
    anyhow::bail!("{}-{}", first.replace('_', "-"), hits.len())
}

fn precedence(rule_id: &str) -> usize {
    PRECONDITION_RULES
        .iter()
        .position(|r| *r == rule_id)
        .unwrap_or(PRECONDITION_RULES.len())
}

/// Evaluate `raw` (`Ok(stdout)` of `cargo metadata`, or `Err(diagnostic)`)
/// against the policies. Pure: no cargo, no filesystem.
pub fn evaluate(
    raw: std::result::Result<String, String>,
    providers: &[ProviderPolicy],
    reverse_deps: &[ReverseDepPolicy],
) -> (Vec<Hit>, Scope) {
    let mut hits = Vec::new();
    let mut scope = Scope::default();
    let (packages, resolve) = match parse(raw) {
        Ok(parsed) => parsed,
        Err(hit) => {
            hits.push(hit);
            return (hits, scope);
        }
    };
    scope.nodes = resolve.nodes.len();
    let graph = Graph {
        packages: packages.iter().map(|p| (p.id.as_str(), p)).collect(),
        node_ids: resolve.nodes.iter().map(|n| n.id.as_str()).collect(),
        nodes: &resolve.nodes,
    };
    for policy in providers {
        scope.provider_instances += check_provider_policy(policy, &graph, &mut hits);
    }
    // A backstop crate absent BECAUSE a provider policy already fired (e.g.
    // jsonwebtoken lost `aws_lc_rs`, so aws-lc-rs/aws-lc-sys left the graph) is a
    // consequence of that diff defect, not vacuity: the run's positive controls
    // (non-empty graph, every provider crate present) already held. Reporting it
    // as a precondition would send triage to the operator lane. Without a
    // mismatch, the crate vanishing really is a rename or a vacuous run, and
    // stays a precondition.
    let mismatch_explains_absence = hits.iter().any(|h| h.rule_id == PROVIDER_FEATURES_MISMATCH);
    for policy in reverse_deps {
        match check_reverse_dep(policy, &graph, &mut hits) {
            ReverseDep::Checked => scope.reverse_dep_crates += 1,
            ReverseDep::Absent if !mismatch_explains_absence => hits.push(Hit {
                rule_id: POLICY_CRATE_ABSENT,
                subject: policy.krate.to_string(),
                detail: format!(
                    "`{}` is not in the resolved graph and no provider policy fired — the \
                     reverse-dependent backstop checked nothing (renamed, removed, or a \
                     vacuous metadata run)",
                    policy.krate
                ),
            }),
            ReverseDep::Absent => {}
        }
    }
    (hits, scope)
}

/// The resolved graph, indexed once.
struct Graph<'a> {
    /// package id -> package (declared features, name, version).
    packages: BTreeMap<&'a str, &'a Package>,
    /// ids present in `resolve.nodes`.
    node_ids: BTreeSet<&'a str>,
    nodes: &'a [Node],
}

impl Graph<'_> {
    fn name_of<'b>(&'b self, id: &'b str) -> &'b str {
        self.packages.get(id).map_or(id, |p| p.name.as_str())
    }
}

/// `cargo metadata` output -> (packages, resolve), or the whole-run
/// PRECONDITION hit explaining why nothing can be checked.
fn parse(
    raw: std::result::Result<String, String>,
) -> std::result::Result<(Vec<Package>, Resolve), Hit> {
    let whole_run = |rule_id, detail: String| Hit {
        rule_id,
        subject: "cargo metadata".to_string(),
        detail,
    };
    let raw =
        raw.map_err(|diag| whole_run(CARGO_FAILED, format!("cargo metadata failed: {diag}")))?;
    let metadata: Metadata = serde_json::from_str(&raw).map_err(|e| {
        whole_run(
            METADATA_UNPARSEABLE,
            format!("output is not `cargo metadata --format-version 1` JSON ({e})"),
        )
    })?;
    let Some(resolve) = metadata.resolve else {
        return Err(whole_run(
            METADATA_UNPARSEABLE,
            "`resolve` is null — cargo ran without resolving dependencies (--no-deps?)".to_string(),
        ));
    };
    if metadata.packages.is_empty() || resolve.nodes.is_empty() {
        return Err(whole_run(
            METADATA_UNPARSEABLE,
            format!(
                "empty graph ({} packages, {} resolved nodes) — nothing to check",
                metadata.packages.len(),
                resolve.nodes.len()
            ),
        ));
    }
    Ok((metadata.packages, resolve))
}

/// One provider policy over every resolved instance of its crate. Returns the
/// number of instances checked (0 = absent, reported as a precondition).
fn check_provider_policy(policy: &ProviderPolicy, graph: &Graph, hits: &mut Vec<Hit>) -> usize {
    let instances: Vec<(&Node, &Package)> = graph
        .nodes
        .iter()
        .filter_map(|n| graph.packages.get(n.id.as_str()).map(|p| (n, *p)))
        .filter(|(_, p)| p.name == policy.krate)
        .collect();
    if instances.is_empty() {
        hits.push(Hit {
            rule_id: POLICY_CRATE_ABSENT,
            subject: policy.krate.to_string(),
            detail: format!(
                "`{}` is not in the resolved graph — the policy checked nothing (renamed, \
                 removed, or a vacuous metadata run). Update the policy deliberately; \
                 never let absence read as a pass",
                policy.krate
            ),
        });
        return 0;
    }
    for (node, package) in &instances {
        check_provider(policy, node, package, hits);
    }
    instances.len()
}

/// Outcome of one reverse-dependent backstop check.
enum ReverseDep {
    Checked,
    /// The crate is not in the graph; the caller decides how to report it.
    Absent,
}

/// The reverse-dependent backstop for one crate.
fn check_reverse_dep(policy: &ReverseDepPolicy, graph: &Graph, hits: &mut Vec<Hit>) -> ReverseDep {
    let ids: BTreeSet<&str> = graph
        .packages
        .iter()
        .filter(|(id, p)| p.name == policy.krate && graph.node_ids.contains(*id))
        .map(|(id, _)| *id)
        .collect();
    if ids.is_empty() {
        return ReverseDep::Absent;
    }
    let dependents: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.deps.iter().any(|d| ids.contains(d.pkg.as_str())))
        .map(|n| graph.name_of(n.id.as_str()))
        .collect();
    let disallowed: Vec<&str> = dependents
        .iter()
        .copied()
        .filter(|d| !policy.allowed_dependents.contains(d))
        .collect();
    if !disallowed.is_empty() {
        hits.push(Hit {
            rule_id: REVERSE_DEPENDENT_DISALLOWED,
            subject: policy.krate.to_string(),
            detail: format!(
                "is depended on by [{}]; allowed only [{}]. {}. Find the edge with \
                 `cargo tree -e normal,build,dev --target all -i {}` and remove it — \
                 never widen the policy to pass",
                backticked(&disallowed),
                backticked(policy.allowed_dependents),
                policy.reason,
                policy.krate
            ),
        });
    }
    ReverseDep::Checked
}

fn check_provider(policy: &ProviderPolicy, node: &Node, package: &Package, hits: &mut Vec<Hit>) {
    let subject = format!("{}@{}", package.name, package.version);
    let undeclared: Vec<&str> = policy
        .vocabulary
        .iter()
        .copied()
        .filter(|term| !package.features.contains_key(*term))
        .collect();
    if !undeclared.is_empty() {
        hits.push(Hit {
            rule_id: VOCABULARY_TERM_UNDECLARED,
            subject,
            detail: format!(
                "the policy vocabulary names [{}], which this version does not declare — an \
                 upstream rename would silently drop out of the comparison; re-derive the \
                 vocabulary from this version's `[features]`",
                backticked(&undeclared)
            ),
        });
        return;
    }
    let resolved: BTreeSet<&str> = node
        .features
        .iter()
        .map(String::as_str)
        .filter(|f| policy.vocabulary.contains(f))
        .collect();
    let required: BTreeSet<&str> = policy.required.iter().copied().collect();
    if resolved == required {
        return;
    }
    let unexpected: Vec<&str> = resolved.difference(&required).copied().collect();
    let missing: Vec<&str> = required.difference(&resolved).copied().collect();
    let mut parts = Vec::new();
    if !unexpected.is_empty() {
        parts.push(format!("unexpected [{}]", backticked(&unexpected)));
    }
    if !missing.is_empty() {
        parts.push(format!("missing [{}]", backticked(&missing)));
    }
    hits.push(Hit {
        rule_id: PROVIDER_FEATURES_MISMATCH,
        subject,
        detail: format!(
            "resolves provider features [{}], required exactly [{}]: {}. {}. {} — never widen \
             the policy to pass",
            backticked(&resolved.iter().copied().collect::<Vec<_>>()),
            backticked(policy.required),
            parts.join("; "),
            policy.reason,
            if unexpected.is_empty() {
                "Enable the missing feature on the root `[workspace.dependencies]` entry"
                    .to_string()
            } else {
                format!(
                    "Find the edge that enables it with `cargo tree -e features --target all -i \
                     {}@{}` and remove it",
                    package.name, package.version
                )
            }
        ),
    });
}

fn backticked(items: &[&str]) -> String {
    items
        .iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    /// Each fixture package: (name, version, declared features, resolved features, deps by name).
    type Pkg<'a> = (
        &'a str,
        &'a str,
        &'a [&'a str],
        &'a [&'a str],
        &'a [&'a str],
    );

    const JWT_DECL: &[&str] = &["aws_lc_rs", "rust_crypto", "use_pem", "default"];
    const RUSTLS_DECL: &[&str] = &[
        "ring",
        "aws_lc_rs",
        "aws-lc-rs",
        "fips",
        "prefer-post-quantum",
        "std",
        "tls12",
        "logging",
    ];
    const WEBPKI_DECL: &[&str] = &[
        "ring",
        "aws-lc-rs",
        "aws-lc-rs-fips",
        "aws-lc-rs-unstable",
        "alloc",
        "std",
    ];
    const QUINN_DECL: &[&str] = &[
        "ring",
        "rustls-ring",
        "aws-lc-rs",
        "aws-lc-rs-fips",
        "rustls-aws-lc-rs",
        "rustls-aws-lc-rs-fips",
        "__rustls-post-quantum-test",
        "rustls",
        "log",
    ];

    fn metadata(pkgs: &[Pkg]) -> String {
        let id = |name: &str, version: &str| format!("registry+x#{name}@{version}");
        let id_by_name = |name: &str| {
            pkgs.iter()
                .find(|p| p.0 == name)
                .map(|p| id(p.0, p.1))
                .expect("every fixture dep must name a fixture package")
        };
        let packages: Vec<Value> = pkgs
            .iter()
            .map(|(name, version, declared, _, _)| {
                let features: serde_json::Map<String, Value> =
                    declared.iter().map(|f| (f.to_string(), json!([]))).collect();
                json!({"id": id(name, version), "name": name, "version": version, "features": features})
            })
            .collect();
        let nodes: Vec<Value> = pkgs
            .iter()
            .map(|(name, version, _, resolved, deps)| {
                let deps: Vec<Value> = deps.iter().map(|d| json!({"pkg": id_by_name(d)})).collect();
                json!({"id": id(name, version), "features": resolved, "deps": deps})
            })
            .collect();
        json!({"packages": packages, "resolve": {"nodes": nodes}}).to_string()
    }

    /// The post-fix state, with unrelated features present on every crate.
    fn good() -> Vec<Pkg<'static>> {
        vec![
            (
                "app",
                "0.1.0",
                &[],
                &[],
                &["jsonwebtoken", "rustls", "quinn"],
            ),
            (
                "jsonwebtoken",
                "11.1.0",
                JWT_DECL,
                &["aws_lc_rs"],
                &["aws-lc-rs"],
            ),
            ("aws-lc-rs", "1.18.1", &[], &[], &["aws-lc-sys"]),
            ("aws-lc-sys", "0.45.0", &[], &[], &[]),
            (
                "rustls",
                "0.23.46",
                RUSTLS_DECL,
                &["ring", "std", "tls12", "logging"],
                &["rustls-webpki"],
            ),
            (
                "rustls-webpki",
                "0.103.16",
                WEBPKI_DECL,
                &["ring", "alloc", "std"],
                &[],
            ),
            (
                "quinn",
                "0.11.12",
                QUINN_DECL,
                &["ring", "rustls", "rustls-ring", "log"],
                &["quinn-proto"],
            ),
            (
                "quinn-proto",
                "0.11.19",
                QUINN_DECL,
                &["ring", "rustls-ring"],
                &[],
            ),
        ]
    }

    fn with(name: &str, resolved: &'static [&'static str]) -> Vec<Pkg<'static>> {
        let mut pkgs = good();
        let p = pkgs.iter_mut().find(|p| p.0 == name).unwrap();
        p.3 = resolved;
        pkgs
    }

    fn eval(pkgs: &[Pkg]) -> Vec<Hit> {
        evaluate(Ok(metadata(pkgs)), PROVIDER_POLICY, REVERSE_DEP_POLICY).0
    }

    fn ids(hits: &[Hit]) -> Vec<&'static str> {
        hits.iter().map(|h| h.rule_id).collect()
    }

    fn single_mismatch(hits: &[Hit], subject: &str) -> String {
        assert_eq!(ids(hits), vec![PROVIDER_FEATURES_MISMATCH], "{hits:?}");
        assert_eq!(hits[0].subject, subject);
        hits[0].detail.clone()
    }

    #[test]
    fn good_state_with_unrelated_features_passes_and_counts() {
        let (hits, scope) = evaluate(Ok(metadata(&good())), PROVIDER_POLICY, REVERSE_DEP_POLICY);
        assert!(hits.is_empty(), "{hits:?}");
        assert_eq!(scope.provider_instances, PROVIDER_POLICY.len());
        assert_eq!(scope.reverse_dep_crates, REVERSE_DEP_POLICY.len());
        assert_eq!(scope.nodes, good().len());
    }

    #[test]
    fn jsonwebtoken_zero_providers_fires() {
        // The PR-101 head state: `default, use_pem`, no backend.
        let d = single_mismatch(
            &eval(&with("jsonwebtoken", &["default", "use_pem"])),
            "jsonwebtoken@11.1.0",
        );
        assert!(d.contains("missing [`aws_lc_rs`]"), "{d}");
    }

    #[test]
    fn jsonwebtoken_both_providers_fires() {
        let d = single_mismatch(
            &eval(&with("jsonwebtoken", &["aws_lc_rs", "rust_crypto"])),
            "jsonwebtoken@11.1.0",
        );
        assert!(d.contains("unexpected [`rust_crypto`]"), "{d}");
    }

    #[test]
    fn jsonwebtoken_rust_crypto_only_fires() {
        let d = single_mismatch(
            &eval(&with("jsonwebtoken", &["rust_crypto"])),
            "jsonwebtoken@11.1.0",
        );
        assert!(
            d.contains("unexpected [`rust_crypto`]") && d.contains("missing [`aws_lc_rs`]"),
            "{d}"
        );
    }

    #[test]
    fn rustls_ring_plus_aws_lc_rs_fires_main_state_at_2869bf37() {
        // REGRESSION CASE: rustls's resolved set on `main` / 2869bf37, where
        // metrics-exporter-prometheus's default `push-gateway` enabled
        // hyper-rustls/aws-lc-rs.
        let d = single_mismatch(
            &eval(&with(
                "rustls",
                &["aws-lc-rs", "aws_lc_rs", "ring", "std", "tls12"],
            )),
            "rustls@0.23.46",
        );
        assert!(d.contains("unexpected [`aws-lc-rs`, `aws_lc_rs`]"), "{d}");
    }

    #[test]
    fn rustls_aws_lc_rs_only_fires() {
        let d = single_mismatch(
            &eval(&with("rustls", &["aws_lc_rs", "std"])),
            "rustls@0.23.46",
        );
        assert!(d.contains("missing [`ring`]"), "{d}");
    }

    #[test]
    fn rustls_zero_providers_fires() {
        let d = single_mismatch(&eval(&with("rustls", &["std", "tls12"])), "rustls@0.23.46");
        assert!(d.contains("missing [`ring`]"), "{d}");
    }

    #[test]
    fn webpki_dual_provider_fires() {
        let d = single_mismatch(
            &eval(&with(
                "rustls-webpki",
                &["alloc", "aws-lc-rs", "ring", "std"],
            )),
            "rustls-webpki@0.103.16",
        );
        assert!(d.contains("unexpected [`aws-lc-rs`]"), "{d}");
    }

    #[test]
    fn quinn_missing_rustls_ring_fires() {
        let d = single_mismatch(&eval(&with("quinn", &["ring", "log"])), "quinn@0.11.12");
        assert!(d.contains("missing [`rustls-ring`]"), "{d}");
    }

    #[test]
    fn every_resolved_instance_is_checked() {
        // Two rustls versions: the good one must not mask the bad one.
        let mut pkgs = good();
        pkgs.push((
            "rustls",
            "0.23.99",
            RUSTLS_DECL,
            &["ring", "aws_lc_rs"],
            &[],
        ));
        let hits = eval(&pkgs);
        let d = single_mismatch(&hits, "rustls@0.23.99");
        assert!(d.contains("unexpected [`aws_lc_rs`]"), "{d}");
    }

    #[test]
    fn new_reverse_dependent_of_aws_lc_rs_fires() {
        // A workspace member (or any crate) depending on aws-lc-rs directly.
        let mut pkgs = good();
        pkgs[0].4 = &["jsonwebtoken", "rustls", "quinn", "aws-lc-rs"];
        let hits = eval(&pkgs);
        assert_eq!(ids(&hits), vec![REVERSE_DEPENDENT_DISALLOWED], "{hits:?}");
        assert_eq!(hits[0].subject, "aws-lc-rs");
        assert!(
            hits[0].detail.contains("depended on by [`app`]"),
            "{}",
            hits[0].detail
        );
    }

    #[test]
    fn rustls_reaching_aws_lc_rs_trips_the_backstop_too() {
        let mut pkgs = good();
        pkgs.iter_mut().find(|p| p.0 == "rustls").unwrap().4 = &["rustls-webpki", "aws-lc-rs"];
        let hits = eval(&pkgs);
        assert_eq!(ids(&hits), vec![REVERSE_DEPENDENT_DISALLOWED], "{hits:?}");
        assert!(hits[0].detail.contains("[`rustls`]"), "{}", hits[0].detail);
    }

    #[test]
    fn policy_crate_absent_is_a_precondition() {
        let pkgs: Vec<Pkg> = good()
            .into_iter()
            .filter(|p| p.0 != "quinn-proto")
            .collect();
        let mut pkgs = pkgs;
        pkgs.iter_mut().find(|p| p.0 == "quinn").unwrap().4 = &[];
        let hits = eval(&pkgs);
        assert_eq!(ids(&hits), vec![POLICY_CRATE_ABSENT], "{hits:?}");
        assert_eq!(hits[0].subject, "quinn-proto");
    }

    #[test]
    fn backstop_absence_caused_by_a_provider_mismatch_is_not_a_precondition() {
        // jsonwebtoken lost `aws_lc_rs`, so aws-lc-rs and aws-lc-sys left the
        // graph: ONE diff defect, reported once, in the violation lane.
        let mut pkgs: Vec<Pkg> = good()
            .into_iter()
            .filter(|p| p.0 != "aws-lc-rs" && p.0 != "aws-lc-sys")
            .collect();
        let jwt = pkgs.iter_mut().find(|p| p.0 == "jsonwebtoken").unwrap();
        jwt.3 = &[];
        jwt.4 = &[];
        let hits = eval(&pkgs);
        assert_eq!(ids(&hits), vec![PROVIDER_FEATURES_MISMATCH], "{hits:?}");
        assert_eq!(hits[0].subject, "jsonwebtoken@11.1.0");
    }

    #[test]
    fn reverse_dep_crate_absent_is_a_precondition() {
        let mut pkgs: Vec<Pkg> = good().into_iter().filter(|p| p.0 != "aws-lc-sys").collect();
        pkgs.iter_mut().find(|p| p.0 == "aws-lc-rs").unwrap().4 = &[];
        assert_eq!(ids(&eval(&pkgs)), vec![POLICY_CRATE_ABSENT]);
    }

    #[test]
    fn undeclared_vocabulary_term_is_a_precondition() {
        // An upstream rename: this jsonwebtoken no longer declares `rust_crypto`.
        let mut pkgs = good();
        pkgs.iter_mut().find(|p| p.0 == "jsonwebtoken").unwrap().2 = &["aws_lc_rs", "use_pem"];
        let hits = eval(&pkgs);
        assert_eq!(ids(&hits), vec![VOCABULARY_TERM_UNDECLARED], "{hits:?}");
        assert!(
            hits[0].detail.contains("[`rust_crypto`]"),
            "{}",
            hits[0].detail
        );
    }

    #[test]
    fn cargo_failure_is_a_precondition() {
        let (hits, _) = evaluate(
            Err("exit 101: error: no matching package".into()),
            PROVIDER_POLICY,
            REVERSE_DEP_POLICY,
        );
        assert_eq!(ids(&hits), vec![CARGO_FAILED]);
        assert!(hits[0].detail.contains("no matching package"));
    }

    #[test]
    fn unparseable_null_resolve_and_empty_graph_are_preconditions() {
        for raw in [
            "not json".to_string(),
            json!({"packages": [], "resolve": null}).to_string(),
            json!({"packages": [], "resolve": {"nodes": []}}).to_string(),
        ] {
            let (hits, _) = evaluate(Ok(raw.clone()), PROVIDER_POLICY, REVERSE_DEP_POLICY);
            assert_eq!(ids(&hits), vec![METADATA_UNPARSEABLE], "{raw}: {hits:?}");
        }
    }

    #[test]
    fn policy_is_internally_consistent() {
        for p in PROVIDER_POLICY {
            assert!(
                !p.vocabulary.is_empty() && !p.required.is_empty(),
                "{}",
                p.krate
            );
            for r in p.required {
                assert!(
                    p.vocabulary.contains(r),
                    "{}: required `{r}` not in vocabulary",
                    p.krate
                );
            }
        }
        for r in REVERSE_DEP_POLICY {
            assert!(!r.allowed_dependents.is_empty(), "{}", r.krate);
        }
    }
}
