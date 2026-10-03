//! `workspace-deps` subcommand — Rust dependency-version single source of truth.
//!
//! A crate version written in more than one place drifts: one copy is bumped,
//! the other is not, and two majors of the same crate end up in the graph with
//! nothing flagging the split. This guard makes `[workspace.dependencies]` the
//! one place a version is written for every dependency the workspace shares,
//! and checks the two places that cannot inherit from it.
//!
//! | Rule | REASON token | Fires when |
//! |---|---|---|
//! | R1 | `workspace-dep-redeclared` | a member declares a dep that IS in `[workspace.dependencies]` without `workspace = true` |
//! | R2 | `dep-pinned-in-multiple-members` | a registry dep carries a literal version in ≥2 distinct crates (members and root-`exclude` crates alike) and is NOT in `[workspace.dependencies]` |
//! | R3 | `excluded-workspace-version-drift` | a crate under root `[workspace] exclude` (an independent workspace, e.g. the fuzz crates) pins a workspace dep at a different requirement, or keeps default features the root disables |
//! | R6 | `dependabot-fuzz-block-missing` / `dependabot-fuzz-allow-missing` / `dependabot-fuzz-allow-extra` | `.github/dependabot.yml` has no cargo block whose `directories` are exactly the root-`exclude` crates (or another cargo block overlaps them); or that block's `allow:` omits / adds a crate relative to the fuzz-only set (declared by an excluded crate, used by no member). Fuzz-only crates are absent from the root Cargo.lock, so dependabot-core never offers them from the root block |
//! | R4 | `facade-crate-duplicated` | `Cargo.lock` holds more than one version of a [`SINGLE_VERSION_CRATES`] crate |
//! | R5 | `member-default-features-ignored` | a member writes `{ workspace = true, default-features = false }` against a workspace entry that keeps default features (cargo ignores the member flag with only a warning, so features silently turn on) |
//!
//! `path` and `git` dependencies are exempt everywhere: they carry no registry
//! version requirement to drift.
//!
//! PRECONDITION classes (the guard could not check what it is responsible
//! for — an operator problem, never a PASS): workspace manifest unreadable,
//! no/empty `[workspace] members`, glob members, no/empty
//! `[workspace.dependencies]`, a member or excluded manifest unreadable, and
//! `Cargo.lock` unreadable. R2 is still evaluated when
//! `[workspace.dependencies]` is absent — that is exactly when every shared
//! literal is an independent pin.
//!
//! Parsing goes through `common::cargo_manifest`, the same reader
//! `release_build_profile` uses.

use crate::common::cargo_manifest::{
    classify_dep, member_dependencies, members_contain_glob, read_root_manifest, DepDecl, DepSpec,
    ManifestError, WorkspaceTable,
};
use crate::common::explain::{print_finding, Finding};
use crate::common::status::{emit_ok, emit_scope};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Crates whose duplication in the resolved graph is silent telemetry loss.
///
/// INCLUSION CRITERION: the crate holds a process-global (metrics recorder,
/// tracing dispatcher, OpenTelemetry provider/propagator), so a second copy in
/// the graph gives the code that links it an EMPTY global with no error — the
/// recorder is installed on one copy while instrumented code emits to the
/// other. Policy content owned by observability; judge additions against the
/// criterion, not against "crates we would rather not duplicate".
pub const SINGLE_VERSION_CRATES: &[&str] = &["metrics", "tracing-core", "opentelemetry"];

pub const WORKSPACE_DEP_REDECLARED: &str = "workspace_dep_redeclared";
pub const DEP_PINNED_IN_MULTIPLE_MEMBERS: &str = "dep_pinned_in_multiple_members";
pub const EXCLUDED_WORKSPACE_VERSION_DRIFT: &str = "excluded_workspace_version_drift";
pub const FACADE_CRATE_DUPLICATED: &str = "facade_crate_duplicated";
pub const MEMBER_DEFAULT_FEATURES_IGNORED: &str = "member_default_features_ignored";
pub const DEPENDABOT_FUZZ_BLOCK_MISSING: &str = "dependabot_fuzz_block_missing";
pub const DEPENDABOT_FUZZ_ALLOW_MISSING: &str = "dependabot_fuzz_allow_missing";
pub const DEPENDABOT_FUZZ_ALLOW_EXTRA: &str = "dependabot_fuzz_allow_extra";

pub const WORKSPACE_MANIFEST_UNREADABLE: &str = "workspace_manifest_unreadable";
pub const WORKSPACE_MEMBERS_EMPTY: &str = "workspace_members_empty";
pub const WORKSPACE_MEMBERS_GLOB: &str = "workspace_members_glob";
pub const WORKSPACE_DEPENDENCIES_EMPTY: &str = "workspace_dependencies_empty";
pub const MEMBER_MANIFEST_UNREADABLE: &str = "member_manifest_unreadable";
pub const CARGO_LOCK_UNREADABLE: &str = "cargo_lock_unreadable";
pub const DEPENDABOT_CONFIG_UNREADABLE: &str = "dependabot_config_unreadable";
/// A root `[workspace] exclude` entry has no Cargo.toml. R3 and R6 compare
/// against the exclude list; a moved or renamed crate must not shrink it.
pub const EXCLUDED_MANIFEST_MISSING: &str = "excluded_manifest_missing";
/// A [`SINGLE_VERSION_CRATES`] entry is absent from the lock: either the lock
/// lost its packages or the policy list went stale. R4 cannot have looked at
/// it, so this is a precondition, never a pass.
pub const FACADE_ABSENT_FROM_LOCK: &str = "facade_absent_from_lock";

/// Rule-id → REASON token, in precedence order. PRECONDITION classes first so
/// they survive `run-guards.sh`'s `head -5` re-emit and win the REASON slot.
const RULE_ORDER: &[(&str, &str)] = &[
    (
        WORKSPACE_MANIFEST_UNREADABLE,
        "workspace-deps-workspace-manifest-unreadable",
    ),
    (
        WORKSPACE_MEMBERS_EMPTY,
        "workspace-deps-workspace-members-empty",
    ),
    (
        WORKSPACE_MEMBERS_GLOB,
        "workspace-deps-workspace-members-glob",
    ),
    (
        WORKSPACE_DEPENDENCIES_EMPTY,
        "workspace-deps-workspace-dependencies-empty",
    ),
    (
        MEMBER_MANIFEST_UNREADABLE,
        "workspace-deps-member-manifest-unreadable",
    ),
    (
        CARGO_LOCK_UNREADABLE,
        "workspace-deps-cargo-lock-unreadable",
    ),
    (
        FACADE_ABSENT_FROM_LOCK,
        "workspace-deps-facade-absent-from-lock",
    ),
    (
        DEPENDABOT_CONFIG_UNREADABLE,
        "workspace-deps-dependabot-config-unreadable",
    ),
    (
        EXCLUDED_MANIFEST_MISSING,
        "workspace-deps-excluded-manifest-missing",
    ),
    (WORKSPACE_DEP_REDECLARED, "workspace-dep-redeclared"),
    (
        DEP_PINNED_IN_MULTIPLE_MEMBERS,
        "dep-pinned-in-multiple-members",
    ),
    (
        EXCLUDED_WORKSPACE_VERSION_DRIFT,
        "excluded-workspace-version-drift",
    ),
    (FACADE_CRATE_DUPLICATED, "facade-crate-duplicated"),
    (
        MEMBER_DEFAULT_FEATURES_IGNORED,
        "member-default-features-ignored",
    ),
    (
        DEPENDABOT_FUZZ_BLOCK_MISSING,
        "dependabot-fuzz-block-missing",
    ),
    (
        DEPENDABOT_FUZZ_ALLOW_MISSING,
        "dependabot-fuzz-allow-missing",
    ),
    (DEPENDABOT_FUZZ_ALLOW_EXTRA, "dependabot-fuzz-allow-extra"),
];

const PRECONDITION_RULES: &[&str] = &[
    WORKSPACE_MANIFEST_UNREADABLE,
    WORKSPACE_MEMBERS_EMPTY,
    WORKSPACE_MEMBERS_GLOB,
    WORKSPACE_DEPENDENCIES_EMPTY,
    MEMBER_MANIFEST_UNREADABLE,
    CARGO_LOCK_UNREADABLE,
    FACADE_ABSENT_FROM_LOCK,
    DEPENDABOT_CONFIG_UNREADABLE,
    EXCLUDED_MANIFEST_MISSING,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub rule_id: &'static str,
    pub file: String,
    pub detail: String,
}

/// What the guard examined — the anti-vacuity counts carried on the PASS line.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scope {
    pub members: usize,
    pub excluded: usize,
    pub workspace_deps: usize,
    pub lock_packages: usize,
}

/// A workspace dependency entry reduced to what the rules compare.
#[derive(Debug, Clone)]
struct WsDep {
    req: Option<String>,
    default_features: Option<bool>,
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let (hits, scope) = evaluate(repo_root);

    emit_scope(format!(
        "{} members, {} excluded crates, {} workspace deps, {} lock packages, {} hits",
        scope.members,
        scope.excluded,
        scope.workspace_deps,
        scope.lock_packages,
        hits.len()
    ));

    if hits.is_empty() {
        emit_ok(format!(
            "workspace-deps-clean-members-{}-excluded-{}",
            scope.members, scope.excluded
        ));
        return Ok(());
    }

    let mut ordered: Vec<&Hit> = hits.iter().collect();
    ordered.sort_by_key(|h| rule_precedence(h.rule_id));
    for hit in ordered {
        let token = token_for_rule(hit.rule_id);
        if explain {
            let policy = format!("workspace-deps::{}", hit.rule_id);
            print_finding(&Finding {
                file: &hit.file,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &hit.detail,
                extras: &[("reason", token)],
                src_file: file!(),
                src_line: line!(),
            });
        } else if PRECONDITION_RULES.contains(&hit.rule_id) {
            println!(
                "ERROR: PRECONDITION [{token}] {} — {} (discovery/precondition failure, \
                 NOT a diff defect; see docs/runbooks/devloop-validation.md §6.3.1)",
                hit.file, hit.detail
            );
        } else {
            println!("VIOLATION: [{token}] {} {}", hit.file, hit.detail);
        }
    }

    anyhow::bail!("{}", reason_for(&hits))
}

/// Evaluate every rule against the tree at `repo_root`. Pure apart from reads.
pub fn evaluate(repo_root: &Path) -> (Vec<Hit>, Scope) {
    let mut hits: Vec<Hit> = Vec::new();
    let mut scope = Scope::default();

    let manifest = match read_root_manifest::<toml::Value>(repo_root) {
        Ok(m) => m,
        Err(e) => {
            hits.push(precondition(
                WORKSPACE_MANIFEST_UNREADABLE,
                "Cargo.toml",
                format!("workspace Cargo.toml {e} — no dependency rule could be evaluated"),
            ));
            return (hits, scope);
        }
    };
    let workspace = manifest.workspace.unwrap_or_default();

    if workspace.members.is_empty() {
        hits.push(precondition(
            WORKSPACE_MEMBERS_EMPTY,
            "Cargo.toml",
            "no `[workspace] members` — there is nothing to check, which is never a pass"
                .to_string(),
        ));
        return (hits, scope);
    }
    if members_contain_glob(&workspace.members) {
        hits.push(precondition(
            WORKSPACE_MEMBERS_GLOB,
            "Cargo.toml",
            "`[workspace] members` contains glob entries; enumerate them so every member \
             manifest is provably checked"
                .to_string(),
        ));
        return (hits, scope);
    }

    let ws_deps = workspace_dep_table(&workspace);
    scope.workspace_deps = ws_deps.len();
    if ws_deps.is_empty() {
        // Precondition AND keep going: R2 is exactly the rule that matters when
        // there is no workspace table to inherit from.
        hits.push(precondition(
            WORKSPACE_DEPENDENCIES_EMPTY,
            "Cargo.toml",
            "no/empty `[workspace.dependencies]` — the single source of truth this guard \
             enforces does not exist"
                .to_string(),
        ));
    }

    // package -> distinct crates (members, then root-`exclude` crates) declaring
    // it with a literal version; R2 reads it after both phases.
    let mut literal_pins: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut member_packages: BTreeSet<String> = BTreeSet::new();
    let mut excluded_seen = Excluded::default();
    scope.members = check_members(
        repo_root,
        &workspace.members,
        &ws_deps,
        &mut literal_pins,
        &mut member_packages,
        &mut hits,
    );
    scope.excluded = check_excluded(
        repo_root,
        &workspace.exclude,
        &ws_deps,
        &mut literal_pins,
        &mut excluded_seen,
        &mut hits,
    );
    hits.extend(check_multi_pins(&literal_pins));
    scope.lock_packages = check_lock(repo_root, &mut hits);
    if !excluded_seen.dirs.is_empty() {
        let fuzz_only: BTreeSet<String> = excluded_seen
            .packages
            .difference(&member_packages)
            .cloned()
            .collect();
        hits.extend(check_dependabot(repo_root, &excluded_seen.dirs, &fuzz_only));
    }

    (hits, scope)
}

/// Root `[workspace.dependencies]` reduced to what the rules compare.
fn workspace_dep_table(workspace: &WorkspaceTable) -> BTreeMap<String, WsDep> {
    workspace
        .dependencies
        .as_ref()
        .map(|deps| {
            deps.iter()
                .map(|(key, value)| {
                    let (package, spec) = classify_dep(key, value);
                    let ws = match spec {
                        DepSpec::Version {
                            req,
                            default_features,
                        } => WsDep {
                            req: Some(req),
                            default_features,
                        },
                        _ => WsDep {
                            req: None,
                            default_features: None,
                        },
                    };
                    (package, ws)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Record a literal-version declaration not covered by the workspace table (R2 input).
fn record_literal_pin(
    dep: &DepDecl,
    ws_deps: &BTreeMap<String, WsDep>,
    declarer: &str,
    literal_pins: &mut BTreeMap<String, BTreeSet<String>>,
) {
    if let DepSpec::Version { .. } = dep.spec {
        if !ws_deps.contains_key(&dep.package) {
            literal_pins
                .entry(dep.package.clone())
                .or_default()
                .insert(declarer.to_string());
        }
    }
}

/// Root-`exclude` crates: every listed directory, and the registry packages
/// the readable ones declare (R6 input).
#[derive(Debug, Default)]
struct Excluded {
    dirs: BTreeSet<String>,
    packages: BTreeSet<String>,
}

/// Members: R1 + R5 per dependency, and R2/R6 input. Returns the members evaluated.
fn check_members(
    repo_root: &Path,
    members: &[String],
    ws_deps: &BTreeMap<String, WsDep>,
    literal_pins: &mut BTreeMap<String, BTreeSet<String>>,
    member_packages: &mut BTreeSet<String>,
    hits: &mut Vec<Hit>,
) -> usize {
    let mut evaluated = 0;
    for member in members {
        let rel = format!("{}/Cargo.toml", member.trim_end_matches('/'));
        let deps = match read_deps(&repo_root.join(&rel)) {
            Ok(d) => d,
            Err(e) => {
                hits.push(precondition(
                    MEMBER_MANIFEST_UNREADABLE,
                    &rel,
                    format!("member manifest {e}"),
                ));
                continue;
            }
        };
        evaluated += 1;
        for dep in deps {
            check_member_dep(&rel, &dep, ws_deps, hits);
            record_literal_pin(&dep, ws_deps, member, literal_pins);
            if !matches!(dep.spec, DepSpec::Path | DepSpec::Git) {
                member_packages.insert(dep.package.clone());
            }
        }
    }
    evaluated
}

/// Root-`exclude` independent workspaces: R3 per dependency, and R2 input (an
/// excluded crate cannot inherit, but a version it shares with another crate is
/// still written twice — R2 moves it into the root table, where R3 binds it).
/// Returns the excluded crates evaluated.
fn check_excluded(
    repo_root: &Path,
    exclude: &[String],
    ws_deps: &BTreeMap<String, WsDep>,
    literal_pins: &mut BTreeMap<String, BTreeSet<String>>,
    seen: &mut Excluded,
    hits: &mut Vec<Hit>,
) -> usize {
    let mut evaluated = 0;
    for excluded in exclude {
        let rel = format!("{}/Cargo.toml", excluded.trim_end_matches('/'));
        let path = repo_root.join(&rel);
        seen.dirs.insert(excluded.trim_end_matches('/').to_string());
        if !path.is_file() {
            // Never a silent skip: a moved or renamed crate would quietly shrink
            // the set R3 and R6 compare against.
            hits.push(precondition(
                EXCLUDED_MANIFEST_MISSING,
                &rel,
                "root `[workspace] exclude` names a crate with no Cargo.toml — R3 and R6 \
                 could not evaluate it; fix the exclude list or restore the crate"
                    .to_string(),
            ));
            continue;
        }
        let deps = match read_deps(&path) {
            Ok(d) => d,
            Err(e) => {
                hits.push(precondition(
                    MEMBER_MANIFEST_UNREADABLE,
                    &rel,
                    format!("excluded-crate manifest {e}"),
                ));
                continue;
            }
        };
        evaluated += 1;
        for dep in deps {
            check_excluded_dep(&rel, &dep, ws_deps, hits);
            record_literal_pin(&dep, ws_deps, excluded, literal_pins);
            if let DepSpec::Version { .. } = dep.spec {
                seen.packages.insert(dep.package.clone());
            }
        }
    }
    evaluated
}

/// R6: the Dependabot cargo block for the root-`exclude` crates.
///
/// dependabot-core's cargo parser skips any `[workspace.dependencies]` entry the
/// root Cargo.lock does not resolve (`file_parser.rb`: `next if lockfile &&
/// !version_from_lockfile(...)`), and the fuzz crates' own lockfiles are not
/// tracked. So a crate only the fuzz crates use gets a PR solely from a cargo
/// block listing their directories, and that block must `allow:` exactly those
/// crates: shared crates stay on the root block's PR (R3 forces the matching
/// fuzz edit there), and a fuzz-only crate missing from the list never gets one.
fn check_dependabot(
    repo_root: &Path,
    excluded_dirs: &BTreeSet<String>,
    fuzz_only: &BTreeSet<String>,
) -> Vec<Hit> {
    const FILE: &str = ".github/dependabot.yml";
    let raw = match std::fs::read_to_string(repo_root.join(FILE)) {
        Ok(r) => r,
        Err(e) => {
            return vec![precondition(
                DEPENDABOT_CONFIG_UNREADABLE,
                FILE,
                format!("{e} — R6 (fuzz-only Dependabot coverage) could not be evaluated"),
            )]
        }
    };
    let doc: serde_norway::Value = match serde_norway::from_str(&raw) {
        Ok(d) => d,
        Err(e) => {
            return vec![precondition(
                DEPENDABOT_CONFIG_UNREADABLE,
                FILE,
                format!("unparseable YAML ({e}) — R6 could not be evaluated"),
            )]
        }
    };
    let hit = |rule_id: &'static str, detail: String| Hit {
        rule_id,
        file: FILE.to_string(),
        detail,
    };
    let mut hits = Vec::new();
    let mut fuzz_block: Option<BTreeSet<String>> = None;
    let updates = doc
        .get("updates")
        .and_then(serde_norway::Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    for block in &updates {
        if block
            .get("package-ecosystem")
            .and_then(serde_norway::Value::as_str)
            != Some("cargo")
        {
            continue;
        }
        let dirs: BTreeSet<String> = match (block.get("directory"), block.get("directories")) {
            (Some(d), _) => d.as_str().into_iter().map(norm_dir).collect(),
            (None, Some(ds)) => ds
                .as_sequence()
                .map(|v| {
                    v.iter()
                        .filter_map(serde_norway::Value::as_str)
                        .map(norm_dir)
                        .collect()
                })
                .unwrap_or_default(),
            (None, None) => BTreeSet::new(),
        };
        if &dirs == excluded_dirs {
            let allowed = block
                .get("allow")
                .and_then(serde_norway::Value::as_sequence)
                .map(|v| {
                    v.iter()
                        .filter_map(|a| {
                            a.get("dependency-name")
                                .and_then(serde_norway::Value::as_str)
                        })
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            fuzz_block = Some(allowed);
        } else if dirs.iter().any(|d| excluded_dirs.contains(d)) {
            hits.push(hit(
                DEPENDABOT_FUZZ_BLOCK_MISSING,
                format!(
                    "a cargo block lists {dirs:?}, overlapping the root-`exclude` crates \
                     {excluded_dirs:?}: give those crates exactly one block of their own"
                ),
            ));
        }
    }
    let Some(allowed) = fuzz_block else {
        hits.push(hit(
            DEPENDABOT_FUZZ_BLOCK_MISSING,
            format!(
                "no cargo block has `directories` equal to the root-`exclude` crates \
                 {excluded_dirs:?}; their fuzz-only crates {fuzz_only:?} would never get a \
                 Dependabot PR"
            ),
        ));
        return hits;
    };
    for name in fuzz_only.difference(&allowed) {
        hits.push(hit(
            DEPENDABOT_FUZZ_ALLOW_MISSING,
            format!(
                "`{name}` is used only by the root-`exclude` crates but is not in the fuzz \
                 block's `allow:` — it would never get a Dependabot PR; add it"
            ),
        ));
    }
    for name in allowed.difference(fuzz_only) {
        hits.push(hit(
            DEPENDABOT_FUZZ_ALLOW_EXTRA,
            format!(
                "`{name}` is in the fuzz block's `allow:` but is not fuzz-only (unused by the \
                 excluded crates, or used by a member and so bumped by the root block's PR); \
                 remove it"
            ),
        ));
    }
    hits
}

/// `"/crates/x/fuzz/"` -> `"crates/x/fuzz"` (Dependabot paths are repo-rooted).
fn norm_dir(d: &str) -> String {
    d.trim_matches('/').to_string()
}

/// R2 over the collected literal pins.
fn check_multi_pins(literal_pins: &BTreeMap<String, BTreeSet<String>>) -> Vec<Hit> {
    literal_pins
        .iter()
        .filter(|(_, declarers)| declarers.len() >= 2)
        .map(|(package, declarers)| Hit {
            rule_id: DEP_PINNED_IN_MULTIPLE_MEMBERS,
            file: "Cargo.toml".to_string(),
            detail: format!(
                "`{package}` is pinned with a literal version in {} crates ({}); hoist it \
                 into [workspace.dependencies] — members inherit it with `workspace = true`, \
                 root-`exclude` crates repeat it and R3 binds them to it",
                declarers.len(),
                declarers.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        })
        .collect()
}

/// R4 over Cargo.lock. Returns the lock packages counted.
fn check_lock(repo_root: &Path, hits: &mut Vec<Hit>) -> usize {
    match lock_versions(repo_root) {
        Ok(versions) => {
            hits.extend(check_facades(&versions));
            versions.values().map(BTreeSet::len).sum()
        }
        Err(e) => {
            hits.push(precondition(
                CARGO_LOCK_UNREADABLE,
                "Cargo.lock",
                format!("Cargo.lock {e} — the facade single-version rule could not be evaluated"),
            ));
            0
        }
    }
}

fn precondition(rule_id: &'static str, file: &str, detail: String) -> Hit {
    Hit {
        rule_id,
        file: file.to_string(),
        detail,
    }
}

fn read_deps(path: &Path) -> std::result::Result<Vec<DepDecl>, ManifestError> {
    let src = std::fs::read_to_string(path).map_err(ManifestError::Unreadable)?;
    member_dependencies(&src).map_err(ManifestError::Unparseable)
}

/// R1 + R5 for one member dependency.
fn check_member_dep(
    rel: &str,
    dep: &DepDecl,
    ws_deps: &BTreeMap<String, WsDep>,
    hits: &mut Vec<Hit>,
) {
    let Some(ws) = ws_deps.get(&dep.package) else {
        return;
    };
    match &dep.spec {
        DepSpec::Version { req, .. } => hits.push(Hit {
            rule_id: WORKSPACE_DEP_REDECLARED,
            file: rel.to_string(),
            detail: format!(
                "[{}] `{}` pins \"{req}\" but `{}` is in [workspace.dependencies]; use \
                 `{{ workspace = true }}`",
                dep.table, dep.key, dep.package
            ),
        }),
        DepSpec::WorkspaceInherit {
            default_features: Some(false),
        } if ws.default_features != Some(false) => hits.push(Hit {
            rule_id: MEMBER_DEFAULT_FEATURES_IGNORED,
            file: rel.to_string(),
            detail: format!(
                "[{}] `{}` sets `default-features = false` but the workspace entry keeps default \
                 features — cargo ignores the member flag (warning only); disable them on the \
                 workspace entry instead",
                dep.table, dep.key
            ),
        }),
        _ => {}
    }
}

/// R3 for one dependency of an excluded independent workspace.
fn check_excluded_dep(
    rel: &str,
    dep: &DepDecl,
    ws_deps: &BTreeMap<String, WsDep>,
    hits: &mut Vec<Hit>,
) {
    let Some(ws) = ws_deps.get(&dep.package) else {
        return;
    };
    let DepSpec::Version {
        req,
        default_features,
    } = &dep.spec
    else {
        return;
    };
    if let Some(ws_req) = &ws.req {
        if ws_req != req {
            hits.push(Hit {
                rule_id: EXCLUDED_WORKSPACE_VERSION_DRIFT,
                file: rel.to_string(),
                detail: format!(
                    "[{}] `{}` requires \"{req}\" but root [workspace.dependencies] requires \
                     \"{ws_req}\" — this crate cannot inherit, so its literal must match",
                    dep.table, dep.key
                ),
            });
        }
    }
    if ws.default_features == Some(false) && *default_features != Some(false) {
        hits.push(Hit {
            rule_id: EXCLUDED_WORKSPACE_VERSION_DRIFT,
            file: rel.to_string(),
            detail: format!(
                "[{}] `{}` keeps default features but root [workspace.dependencies] sets \
                 `default-features = false` — the excluded crate must disable them too",
                dep.table, dep.key
            ),
        });
    }
}

/// `Cargo.lock` package name → set of resolved versions.
fn lock_versions(
    repo_root: &Path,
) -> std::result::Result<BTreeMap<String, BTreeSet<String>>, ManifestError> {
    let src =
        std::fs::read_to_string(repo_root.join("Cargo.lock")).map_err(ManifestError::Unreadable)?;
    parse_lock(&src).map_err(ManifestError::Unparseable)
}

fn parse_lock(
    src: &str,
) -> std::result::Result<BTreeMap<String, BTreeSet<String>>, toml::de::Error> {
    #[derive(serde::Deserialize)]
    struct Lock {
        #[serde(default)]
        package: Vec<LockPackage>,
    }
    #[derive(serde::Deserialize)]
    struct LockPackage {
        name: String,
        version: String,
    }
    let lock: Lock = toml::from_str(src)?;
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in lock.package {
        out.entry(p.name).or_default().insert(p.version);
    }
    Ok(out)
}

fn check_facades(versions: &BTreeMap<String, BTreeSet<String>>) -> Vec<Hit> {
    SINGLE_VERSION_CRATES
        .iter()
        .filter_map(|name| {
            let Some(v) = versions.get(*name) else {
                return Some(precondition(
                    FACADE_ABSENT_FROM_LOCK,
                    "Cargo.lock",
                    format!(
                        "`{name}` (SINGLE_VERSION_CRATES) has no [[package]] entry in Cargo.lock — \
                         either the lock lost its packages or the policy list is stale; R4 did \
                         not evaluate it. Fix the lock or update the list, never loosen R4"
                    ),
                ));
            };
            (v.len() > 1).then(|| Hit {
                rule_id: FACADE_CRATE_DUPLICATED,
                file: "Cargo.lock".to_string(),
                detail: format!(
                    "`{name}` resolves to {} versions ({}); it holds a process-global, so one \
                     copy's global is empty — align every dependent on one version",
                    v.len(),
                    v.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
            })
        })
        .collect()
}

fn rule_precedence(rule_id: &str) -> usize {
    RULE_ORDER
        .iter()
        .position(|(id, _)| *id == rule_id)
        .unwrap_or(usize::MAX)
}

fn token_for_rule(rule_id: &str) -> &'static str {
    RULE_ORDER
        .iter()
        .find(|(id, _)| *id == rule_id)
        .map(|(_, token)| *token)
        .unwrap_or("workspace-deps-violation")
}

/// Highest-precedence class names the REASON token, suffixed with its count.
fn reason_for(hits: &[Hit]) -> String {
    for (rule_id, token) in RULE_ORDER {
        let n = hits.iter().filter(|h| h.rule_id == *rule_id).count();
        if n > 0 {
            return format!("{token}-{n}");
        }
    }
    format!("workspace-deps-violations-{}", hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Build a workspace: root manifest body, `(member_path, manifest)` pairs,
    /// and an optional Cargo.lock body (a single-version default when `None`).
    fn ws(root: &str, crates: &[(&str, &str)], lock: Option<&str>) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), root).unwrap();
        for (path, body) in crates {
            let d = dir.path().join(path);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("Cargo.toml"), body).unwrap();
        }
        let lock = lock.unwrap_or(LOCK_OK);
        fs::write(dir.path().join("Cargo.lock"), lock).unwrap();
        dir
    }

    /// Every SINGLE_VERSION_CRATES entry exactly once (R4's anchor).
    const LOCK_OK: &str = "version = 4\n\
                           [[package]]\nname = \"metrics\"\nversion = \"0.24.6\"\n\
                           [[package]]\nname = \"tracing-core\"\nversion = \"0.1.34\"\n\
                           [[package]]\nname = \"opentelemetry\"\nversion = \"0.24.0\"\n";

    /// Write `.github/dependabot.yml` with a root cargo block and, when `fuzz_dirs`
    /// is non-empty, a fuzz block over those directories allowing `allow`.
    fn dependabot(dir: &TempDir, fuzz_dirs: &[&str], allow: &[&str]) {
        let mut y = String::from(
            "version: 2\nupdates:\n  - package-ecosystem: \"cargo\"\n    directory: \"/\"\n",
        );
        if !fuzz_dirs.is_empty() {
            y.push_str("  - package-ecosystem: \"cargo\"\n    directories:\n");
            for d in fuzz_dirs {
                y.push_str(&format!("      - \"/{d}\"\n"));
            }
            y.push_str("    allow:\n");
            for a in allow {
                y.push_str(&format!("      - dependency-name: \"{a}\"\n"));
            }
        }
        fs::create_dir_all(dir.path().join(".github")).unwrap();
        fs::write(dir.path().join(".github/dependabot.yml"), y).unwrap();
    }

    fn rules(dir: &TempDir) -> Vec<&'static str> {
        evaluate(dir.path()).0.iter().map(|h| h.rule_id).collect()
    }

    const ROOT: &str = "[workspace]\nmembers = [\"a\", \"b\"]\n\
                        [workspace.dependencies]\nserde = \"1.0\"\n\
                        base64 = { version = \"0.23\", default-features = false, features = [\"std\"] }\n\
                        tokio = { version = \"1\", features = [\"full\"] }\n";

    fn pkg(name: &str, deps: &str) -> String {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n{deps}")
    }

    #[test]
    fn clean_workspace_passes_with_counts() {
        let a = pkg(
            "a",
            "[dependencies]\nserde = { workspace = true, features = [\"derive\"] }\n",
        );
        let b = pkg(
            "b",
            "[dependencies]\nbase64 = { workspace = true }\nlocal = { path = \"../a\" }\n",
        );
        let dir = ws(ROOT, &[("a", &a), ("b", &b)], None);
        let (hits, scope) = evaluate(dir.path());
        assert!(hits.is_empty(), "{hits:?}");
        assert_eq!(scope.members, 2);
        assert_eq!(scope.workspace_deps, 3);
    }

    #[test]
    fn r1_literal_redeclaration_fires() {
        let a = pkg("a", "[dependencies]\nserde = \"1.0\"\n");
        let b = pkg("b", "");
        assert_eq!(
            rules(&ws(ROOT, &[("a", &a), ("b", &b)], None)),
            vec![WORKSPACE_DEP_REDECLARED]
        );
    }

    #[test]
    fn r1_honours_package_rename_and_target_dev_build_tables() {
        let a = pkg(
            "a",
            "[target.'cfg(unix)'.dev-dependencies]\nmy-serde = { package = \"serde\", version = \"1\" }\n\
             [build-dependencies]\ntokio = \"1\"\n",
        );
        let b = pkg("b", "");
        let got = rules(&ws(ROOT, &[("a", &a), ("b", &b)], None));
        assert_eq!(
            got,
            vec![WORKSPACE_DEP_REDECLARED, WORKSPACE_DEP_REDECLARED]
        );
    }

    #[test]
    fn path_and_git_deps_are_exempt() {
        let a = pkg("a", "[dependencies]\nserde = { path = \"../vendored-serde\" }\nx = { git = \"https://e.org/x\" }\n");
        let b = pkg("b", "[dependencies]\nx = { git = \"https://e.org/x\" }\n");
        assert!(rules(&ws(ROOT, &[("a", &a), ("b", &b)], None)).is_empty());
    }

    #[test]
    fn r2_fires_for_literal_in_two_distinct_members() {
        let a = pkg("a", "[dependencies]\nregex = \"1\"\n");
        let b = pkg("b", "[dev-dependencies]\nregex = \"1.10\"\n");
        assert_eq!(
            rules(&ws(ROOT, &[("a", &a), ("b", &b)], None)),
            vec![DEP_PINNED_IN_MULTIPLE_MEMBERS]
        );
    }

    #[test]
    fn r2_counts_distinct_members_not_declarations() {
        let a = pkg(
            "a",
            "[dependencies]\nregex = \"1\"\n[dev-dependencies]\nregex = \"1\"\n\
             [target.'cfg(unix)'.dependencies]\nregex = \"1\"\n",
        );
        let b = pkg("b", "");
        assert!(rules(&ws(ROOT, &[("a", &a), ("b", &b)], None)).is_empty());
    }

    #[test]
    fn r2_still_fires_without_workspace_dependencies() {
        let root = "[workspace]\nmembers = [\"a\", \"b\"]\n";
        let a = pkg("a", "[dependencies]\nregex = \"1\"\n");
        let b = pkg("b", "[dependencies]\nregex = \"1\"\n");
        let got = rules(&ws(root, &[("a", &a), ("b", &b)], None));
        assert!(got.contains(&WORKSPACE_DEPENDENCIES_EMPTY), "{got:?}");
        assert!(got.contains(&DEP_PINNED_IN_MULTIPLE_MEMBERS), "{got:?}");
        assert!(!got.contains(&WORKSPACE_DEP_REDECLARED));
    }

    #[test]
    fn r3_version_and_default_features_drift() {
        let root = ROOT.replace(
            "members = [\"a\", \"b\"]\n",
            "members = [\"a\", \"b\"]\nexclude = [\"fuzz\"]\n",
        );
        let a = pkg("a", "");
        let b = pkg("b", "");
        // Drift: serde requirement differs; base64 keeps default features.
        let fuzz = "[package]\nname = \"f\"\nversion = \"0.0.0\"\n[workspace]\n\
                    [dependencies]\nserde = \"1.0.200\"\nbase64 = \"0.23\"\n";
        let dir = ws(&root, &[("a", &a), ("b", &b), ("fuzz", fuzz)], None);
        dependabot(&dir, &["fuzz"], &["base64", "serde"]);
        let got = rules(&dir);
        assert_eq!(
            got,
            vec![
                EXCLUDED_WORKSPACE_VERSION_DRIFT,
                EXCLUDED_WORKSPACE_VERSION_DRIFT
            ]
        );

        // Allowed direction: matching requirement; excluded crate disabling
        // defaults the root keeps is fine.
        let fuzz_ok = "[package]\nname = \"f\"\nversion = \"0.0.0\"\n[workspace]\n\
                       [dependencies]\nserde = { version = \"1.0\", default-features = false }\n\
                       base64 = { version = \"0.23\", default-features = false }\n";
        let dir = ws(&root, &[("a", &a), ("b", &b), ("fuzz", fuzz_ok)], None);
        dependabot(&dir, &["fuzz"], &["base64", "serde"]);
        assert!(rules(&dir).is_empty());
    }

    #[test]
    fn r4_listed_facade_duplicated_fires_unlisted_does_not() {
        let a = pkg("a", "");
        let b = pkg("b", "");
        let two = "version = 4\n[[package]]\nname = \"metrics\"\nversion = \"0.24.3\"\n\
                   [[package]]\nname = \"tracing-core\"\nversion = \"0.1.34\"\n\
                   [[package]]\nname = \"opentelemetry\"\nversion = \"0.24.0\"\n\
                   [[package]]\nname = \"metrics\"\nversion = \"0.25.0\"\n\
                   [[package]]\nname = \"base64\"\nversion = \"0.22.1\"\n\
                   [[package]]\nname = \"base64\"\nversion = \"0.23.1\"\n";
        assert_eq!(
            rules(&ws(ROOT, &[("a", &a), ("b", &b)], Some(two))),
            vec![FACADE_CRATE_DUPLICATED]
        );
    }

    #[test]
    fn r4_listed_facade_absent_is_a_precondition() {
        let a = pkg("a", "");
        let b = pkg("b", "");
        // opentelemetry missing (stale list / lock lost it) -> precondition, not pass.
        let missing = "version = 4\n[[package]]\nname = \"metrics\"\nversion = \"0.24.6\"\n\
                       [[package]]\nname = \"tracing-core\"\nversion = \"0.1.34\"\n";
        assert_eq!(
            rules(&ws(ROOT, &[("a", &a), ("b", &b)], Some(missing))),
            vec![FACADE_ABSENT_FROM_LOCK]
        );
        // A lock that parses but holds no packages: one precondition per listed crate.
        assert_eq!(
            rules(&ws(ROOT, &[("a", &a), ("b", &b)], Some("version = 4\n"))),
            vec![FACADE_ABSENT_FROM_LOCK; SINGLE_VERSION_CRATES.len()]
        );
    }

    #[test]
    fn r2_counts_excluded_crates_sharing_a_literal() {
        let root = ROOT.replace(
            "members = [\"a\", \"b\"]\n",
            "members = [\"a\", \"b\"]\nexclude = [\"fuzz1\", \"fuzz2\"]\n",
        );
        let a = pkg("a", "");
        let b = pkg("b", "");
        let f = |n: &str| {
            format!(
                "[package]\nname = \"{n}\"\nversion = \"0.0.0\"\n[workspace]\n\
                 [dependencies]\nlibfuzzer-sys = \"0.4\"\n"
            )
        };
        let (f1, f2) = (f("f1"), f("f2"));
        let with_fuzz_block = |dir: TempDir| {
            dependabot(&dir, &["fuzz1", "fuzz2"], &["libfuzzer-sys"]);
            dir
        };
        assert_eq!(
            rules(&with_fuzz_block(ws(
                &root,
                &[("a", &a), ("b", &b), ("fuzz1", &f1), ("fuzz2", &f2)],
                None
            ))),
            vec![DEP_PINNED_IN_MULTIPLE_MEMBERS]
        );
        // Hoisted to the root table: R2 is satisfied and R3 binds both literals.
        let hoisted = format!("{root}libfuzzer-sys = \"0.4\"\n");
        assert!(rules(&with_fuzz_block(ws(
            &hoisted,
            &[("a", &a), ("b", &b), ("fuzz1", &f1), ("fuzz2", &f2)],
            None
        )))
        .is_empty());
        let f2_drift = f2.replace("\"0.4\"", "\"0.4.10\"");
        assert_eq!(
            rules(&with_fuzz_block(ws(
                &hoisted,
                &[("a", &a), ("b", &b), ("fuzz1", &f1), ("fuzz2", &f2_drift)],
                None
            ))),
            vec![EXCLUDED_WORKSPACE_VERSION_DRIFT]
        );
    }

    /// One excluded crate declaring a fuzz-only crate (`libfuzzer-sys`) and a
    /// shared one (`serde`, also used by member `a`).
    fn r6_fixture() -> TempDir {
        let root = ROOT.replace(
            "members = [\"a\", \"b\"]\n",
            "members = [\"a\", \"b\"]\nexclude = [\"fuzz\"]\n",
        );
        let a = pkg("a", "[dependencies]\nserde = { workspace = true }\n");
        let b = pkg("b", "");
        let fuzz = "[package]\nname = \"f\"\nversion = \"0.0.0\"\n[workspace]\n\
                    [dependencies]\nserde = \"1.0\"\nlibfuzzer-sys = \"0.4\"\n";
        ws(&root, &[("a", &a), ("b", &b), ("fuzz", fuzz)], None)
    }

    #[test]
    fn r6_fuzz_block_allows_exactly_the_fuzz_only_crates() {
        let dir = r6_fixture();
        dependabot(&dir, &["fuzz"], &["libfuzzer-sys"]);
        assert!(rules(&dir).is_empty());
    }

    #[test]
    fn r6_allow_missing_and_extra_have_distinct_tokens() {
        let dir = r6_fixture();
        dependabot(&dir, &["fuzz"], &[]);
        assert_eq!(rules(&dir), vec![DEPENDABOT_FUZZ_ALLOW_MISSING]);
        // `serde` is shared (a member uses it): it moves with the root PR.
        dependabot(&dir, &["fuzz"], &["libfuzzer-sys", "serde"]);
        assert_eq!(rules(&dir), vec![DEPENDABOT_FUZZ_ALLOW_EXTRA]);
    }

    #[test]
    fn r6_block_missing_or_overlapping_fires() {
        let dir = r6_fixture();
        dependabot(&dir, &[], &[]);
        assert_eq!(rules(&dir), vec![DEPENDABOT_FUZZ_BLOCK_MISSING]);
        // Directories that do not equal the exclude list are not the fuzz block.
        dependabot(&dir, &["fuzz", "elsewhere"], &["libfuzzer-sys"]);
        let got = rules(&dir);
        assert!(got.contains(&DEPENDABOT_FUZZ_BLOCK_MISSING), "{got:?}");
        // The root block also listing the fuzz crate overlaps it.
        let y = "version: 2\nupdates:\n  - package-ecosystem: \"cargo\"\n    directories: [\"/\", \"/fuzz\"]\n  \
                 - package-ecosystem: \"cargo\"\n    directories: [\"/fuzz\"]\n    allow:\n      - dependency-name: \"libfuzzer-sys\"\n";
        fs::write(dir.path().join(".github/dependabot.yml"), y).unwrap();
        assert_eq!(rules(&dir), vec![DEPENDABOT_FUZZ_BLOCK_MISSING]);
    }

    #[test]
    fn r6_preconditions_never_pass() {
        let dir = r6_fixture();
        assert_eq!(rules(&dir), vec![DEPENDABOT_CONFIG_UNREADABLE]);
        fs::create_dir_all(dir.path().join(".github")).unwrap();
        fs::write(dir.path().join(".github/dependabot.yml"), "updates: [\n").unwrap();
        assert_eq!(rules(&dir), vec![DEPENDABOT_CONFIG_UNREADABLE]);

        // An exclude entry with no manifest is never silently skipped.
        let root = ROOT.replace(
            "members = [\"a\", \"b\"]\n",
            "members = [\"a\", \"b\"]\nexclude = [\"gone\"]\n",
        );
        let a = pkg("a", "");
        let b = pkg("b", "");
        let dir = ws(&root, &[("a", &a), ("b", &b)], None);
        dependabot(&dir, &["gone"], &[]);
        assert_eq!(rules(&dir), vec![EXCLUDED_MANIFEST_MISSING]);
    }

    #[test]
    fn r5_member_default_features_ignored() {
        let a = pkg(
            "a",
            "[dependencies]\ntokio = { workspace = true, default-features = false }\n",
        );
        let b = pkg(
            "b",
            "[dependencies]\nbase64 = { workspace = true, default-features = false }\n",
        );
        // tokio keeps defaults at the workspace → fires; base64 disables them → fine.
        assert_eq!(
            rules(&ws(ROOT, &[("a", &a), ("b", &b)], None)),
            vec![MEMBER_DEFAULT_FEATURES_IGNORED]
        );
    }

    #[test]
    fn preconditions_never_pass() {
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(rules(&empty), vec![WORKSPACE_MANIFEST_UNREADABLE]);

        let dir = ws("[workspace\n", &[], None);
        assert_eq!(rules(&dir), vec![WORKSPACE_MANIFEST_UNREADABLE]);

        let dir = ws("[workspace]\nmembers = []\n", &[], None);
        assert_eq!(rules(&dir), vec![WORKSPACE_MEMBERS_EMPTY]);

        let dir = ws(
            "[package]\nname = \"solo\"\nversion = \"0.1.0\"\n",
            &[],
            None,
        );
        assert_eq!(rules(&dir), vec![WORKSPACE_MEMBERS_EMPTY]);

        let dir = ws("[workspace]\nmembers = [\"crates/*\"]\n", &[], None);
        assert_eq!(rules(&dir), vec![WORKSPACE_MEMBERS_GLOB]);

        let b = pkg("b", "");
        let dir = ws(ROOT, &[("b", &b)], None);
        assert_eq!(rules(&dir), vec![MEMBER_MANIFEST_UNREADABLE]);

        let a = pkg("a", "");
        let dir = ws(ROOT, &[("a", &a), ("b", &b)], None);
        fs::remove_file(dir.path().join("Cargo.lock")).unwrap();
        assert_eq!(rules(&dir), vec![CARGO_LOCK_UNREADABLE]);

        let dir = ws(ROOT, &[("a", &a), ("b", &b)], Some("[[package]\n"));
        assert_eq!(rules(&dir), vec![CARGO_LOCK_UNREADABLE]);
    }

    #[test]
    fn reason_token_prefers_precondition_and_counts() {
        let hits = vec![
            Hit {
                rule_id: WORKSPACE_DEP_REDECLARED,
                file: String::new(),
                detail: String::new(),
            },
            Hit {
                rule_id: CARGO_LOCK_UNREADABLE,
                file: String::new(),
                detail: String::new(),
            },
        ];
        assert_eq!(reason_for(&hits), "workspace-deps-cargo-lock-unreadable-1");
    }

    #[test]
    fn every_rule_has_a_token() {
        for id in [
            WORKSPACE_DEP_REDECLARED,
            DEP_PINNED_IN_MULTIPLE_MEMBERS,
            EXCLUDED_WORKSPACE_VERSION_DRIFT,
            FACADE_CRATE_DUPLICATED,
            MEMBER_DEFAULT_FEATURES_IGNORED,
            DEPENDABOT_FUZZ_BLOCK_MISSING,
            DEPENDABOT_FUZZ_ALLOW_MISSING,
            DEPENDABOT_FUZZ_ALLOW_EXTRA,
        ]
        .iter()
        .chain(PRECONDITION_RULES)
        {
            assert_ne!(token_for_rule(id), "workspace-deps-violation", "{id}");
        }
    }
}
