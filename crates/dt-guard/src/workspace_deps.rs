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
//! | R6 | `dependabot-cargo-block-names-excluded-crate` | a cargo block in `.github/dependabot.yml` lists a root-`exclude` crate (or a glob) in `directory`/`directories`. Resolution is layout-dependent — dependabot-core cannot resolve `crates/media-protocol/fuzz` (its path dep inherits from a root it never fetches) — and with no tracked lockfile a block could only ever PR a new major |
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
pub const DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE: &str =
    "dependabot_cargo_block_names_excluded_crate";

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
        DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE,
        "dependabot-cargo-block-names-excluded-crate",
    ),
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
    let mut excluded_dirs: BTreeSet<String> = BTreeSet::new();
    scope.members = check_members(
        repo_root,
        &workspace.members,
        &ws_deps,
        &mut literal_pins,
        &mut hits,
    );
    scope.excluded = check_excluded(
        repo_root,
        &workspace.exclude,
        &ws_deps,
        &mut literal_pins,
        &mut excluded_dirs,
        &mut hits,
    );
    hits.extend(check_multi_pins(&literal_pins));
    scope.lock_packages = check_lock(repo_root, &mut hits);
    if !excluded_dirs.is_empty() {
        hits.extend(check_dependabot(repo_root, &excluded_dirs));
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

/// Members: R1 + R5 per dependency, and R2 input. Returns the members evaluated.
fn check_members(
    repo_root: &Path,
    members: &[String],
    ws_deps: &BTreeMap<String, WsDep>,
    literal_pins: &mut BTreeMap<String, BTreeSet<String>>,
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
        }
    }
    evaluated
}

/// Root-`exclude` independent workspaces: R3 per dependency, and R2 input (an
/// excluded crate cannot inherit, but a version it shares with another crate is
/// still written twice — R2 moves it into the root table, where R3 binds it),
/// and every listed directory as R6 input. Returns the excluded crates evaluated.
fn check_excluded(
    repo_root: &Path,
    exclude: &[String],
    ws_deps: &BTreeMap<String, WsDep>,
    literal_pins: &mut BTreeMap<String, BTreeSet<String>>,
    excluded_dirs: &mut BTreeSet<String>,
    hits: &mut Vec<Hit>,
) -> usize {
    let mut evaluated = 0;
    for excluded in exclude {
        let rel = format!("{}/Cargo.toml", excluded.trim_end_matches('/'));
        let path = repo_root.join(&rel);
        excluded_dirs.insert(excluded.trim_end_matches('/').to_string());
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
        }
    }
    evaluated
}

/// R6: no Dependabot cargo block may name a root-`exclude` crate.
///
/// Two reasons, one uniform rule. (1) Resolution is layout-dependent:
/// dependabot-core finds a path dep's workspace root by walking up a depth
/// computed from the dep's job-relative name (`file_fetcher.rb`
/// `find_workspace_root`). From `/crates/media-protocol/fuzz` the dep `..` walks
/// only to `crates/`, the repo root is never fetched, and cargo fails "failed to
/// find a workspace root" on the dep's `*.workspace = true` keys (run
/// 37724549271; upstream dependabot/dependabot-core#11861). Any excluded crate
/// whose path dep undershoots the root the same way fails too. (2) With no tracked
/// lockfile, the cargo updater leaves a requirement alone while it admits the
/// latest release, so even a resolving block could only PR a new major. Their
/// dependencies are covered instead by floating and by `scripts/lang/rust/audit.sh`,
/// which audits their graphs. A glob directory cannot be compared against
/// `exclude`, so it is refused too.
fn check_dependabot(repo_root: &Path, excluded_dirs: &BTreeSet<String>) -> Vec<Hit> {
    const FILE: &str = ".github/dependabot.yml";
    let raw = match std::fs::read_to_string(repo_root.join(FILE)) {
        Ok(r) => r,
        Err(e) => {
            return vec![precondition(
                DEPENDABOT_CONFIG_UNREADABLE,
                FILE,
                format!("{e} — R6 (no cargo block over excluded crates) could not be evaluated"),
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
    let updates = doc
        .get("updates")
        .and_then(serde_norway::Value::as_sequence)
        .cloned()
        .unwrap_or_default();
    let mut hits = Vec::new();
    for block in &updates {
        if block
            .get("package-ecosystem")
            .and_then(serde_norway::Value::as_str)
            != Some("cargo")
        {
            continue;
        }
        // Both keys, unioned: a block carrying both must not hide one behind the other.
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        for key in ["directory", "directories"] {
            let Some(value) = block.get(key) else {
                continue;
            };
            let entries: Vec<&serde_norway::Value> = match value.as_sequence() {
                Some(seq) => seq.iter().collect(),
                None => vec![value],
            };
            for entry in entries {
                let Some(d) = entry.as_str() else {
                    hits.push(precondition(
                        DEPENDABOT_CONFIG_UNREADABLE,
                        FILE,
                        format!(
                            "a cargo block's `{key}` holds a non-string entry ({entry:?}) — R6 \
                             could not be evaluated"
                        ),
                    ));
                    continue;
                };
                dirs.insert(norm_dir(d));
            }
        }
        // Dependabot expands globs; a literal comparison against `exclude` cannot.
        let globbed: Vec<&String> = dirs
            .iter()
            .filter(|d| d.contains(['*', '?', '[', '{']))
            .collect();
        if !globbed.is_empty() {
            hits.push(Hit {
                rule_id: DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE,
                file: FILE.to_string(),
                detail: format!(
                    "a cargo block lists glob directories {globbed:?}: they cannot be checked \
                     against the root `exclude` list; list cargo directories literally"
                ),
            });
        }
        let named: Vec<&String> = dirs.intersection(excluded_dirs).collect();
        if !named.is_empty() {
            hits.push(Hit {
                rule_id: DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE,
                file: FILE.to_string(),
                detail: format!(
                    "a cargo block lists the root-`exclude` crate(s) {named:?}: such a block may \
                     not resolve (layout-dependent; crates/media-protocol/fuzz fails, \
                     dependabot/dependabot-core#11861) and, with no tracked lockfile, could only \
                     PR a major. Remove them from the block; their dependencies float at each \
                     build and are audited by scripts/lang/rust/audit.sh"
                ),
            });
        }
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

    /// Write `.github/dependabot.yml` with `body` verbatim.
    fn write_dependabot(dir: &TempDir, body: &str) {
        fs::create_dir_all(dir.path().join(".github")).unwrap();
        fs::write(dir.path().join(".github/dependabot.yml"), body).unwrap();
    }

    const ROOT_ONLY_DEPENDABOT: &str =
        "version: 2\nupdates:\n  - package-ecosystem: \"cargo\"\n    directory: \"/\"\n";

    /// The repo's shape: one cargo block over the root workspace only.
    fn root_only_dependabot(dir: &TempDir) {
        write_dependabot(dir, ROOT_ONLY_DEPENDABOT);
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
        root_only_dependabot(&dir);
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
        root_only_dependabot(&dir);
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
        let with_dependabot = |dir: TempDir| {
            root_only_dependabot(&dir);
            dir
        };
        assert_eq!(
            rules(&with_dependabot(ws(
                &root,
                &[("a", &a), ("b", &b), ("fuzz1", &f1), ("fuzz2", &f2)],
                None
            ))),
            vec![DEP_PINNED_IN_MULTIPLE_MEMBERS]
        );
        // Hoisted to the root table: R2 is satisfied and R3 binds both literals.
        let hoisted = format!("{root}libfuzzer-sys = \"0.4\"\n");
        assert!(rules(&with_dependabot(ws(
            &hoisted,
            &[("a", &a), ("b", &b), ("fuzz1", &f1), ("fuzz2", &f2)],
            None
        )))
        .is_empty());
        let f2_drift = f2.replace("\"0.4\"", "\"0.4.10\"");
        assert_eq!(
            rules(&with_dependabot(ws(
                &hoisted,
                &[("a", &a), ("b", &b), ("fuzz1", &f1), ("fuzz2", &f2_drift)],
                None
            ))),
            vec![EXCLUDED_WORKSPACE_VERSION_DRIFT]
        );
    }

    /// Two excluded crates at the repo's real depth (`crates/<x>/fuzz`).
    fn r6_fixture() -> TempDir {
        let root = ROOT.replace(
            "members = [\"a\", \"b\"]\n",
            "members = [\"a\", \"b\"]\nexclude = [\"crates/x/fuzz\", \"crates/y/fuzz\"]\n",
        );
        let a = pkg("a", "");
        let b = pkg("b", "");
        let fuzz = "[package]\nname = \"f\"\nversion = \"0.0.0\"\n[workspace]\n";
        ws(
            &root,
            &[
                ("a", &a),
                ("b", &b),
                ("crates/x/fuzz", fuzz),
                ("crates/y/fuzz", fuzz),
            ],
            None,
        )
    }

    fn cargo_block(dirs_yaml: &str) -> String {
        format!("{ROOT_ONLY_DEPENDABOT}  - package-ecosystem: \"cargo\"\n{dirs_yaml}")
    }

    #[test]
    fn r6_root_only_config_passes() {
        let dir = r6_fixture();
        root_only_dependabot(&dir);
        assert!(rules(&dir).is_empty());
    }

    #[test]
    fn r6_fires_on_the_shipped_fuzz_block() {
        // The block run 37724549271 failed on: both fuzz crates, `directories:`.
        let dir = r6_fixture();
        write_dependabot(
            &dir,
            &cargo_block(
                "    directories:\n      - \"/crates/x/fuzz\"\n      - \"/crates/y/fuzz\"\n    \
                 allow:\n      - dependency-name: \"libfuzzer-sys\"\n",
            ),
        );
        let (hits, _) = evaluate(dir.path());
        assert_eq!(
            hits.iter().map(|h| h.rule_id).collect::<Vec<_>>(),
            vec![DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE]
        );
        assert!(hits[0].detail.contains("crates/x/fuzz"), "{hits:?}");
        assert!(hits[0].detail.contains("crates/y/fuzz"), "{hits:?}");
    }

    #[test]
    fn r6_fires_on_either_key_shape_and_slash_variant() {
        let dir = r6_fixture();
        for d in ["/crates/x/fuzz", "crates/x/fuzz", "/crates/x/fuzz/"] {
            write_dependabot(&dir, &cargo_block(&format!("    directory: \"{d}\"\n")));
            assert_eq!(
                rules(&dir),
                vec![DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE],
                "directory: {d}"
            );
            write_dependabot(
                &dir,
                &cargo_block(&format!("    directories:\n      - \"{d}\"\n")),
            );
            assert_eq!(
                rules(&dir),
                vec![DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE],
                "directories: [{d}]"
            );
        }
        // The root block itself widened to an excluded crate fires too.
        write_dependabot(
            &dir,
            "version: 2\nupdates:\n  - package-ecosystem: \"cargo\"\n    \
             directories: [\"/\", \"/crates/y/fuzz\"]\n",
        );
        assert_eq!(
            rules(&dir),
            vec![DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE]
        );
    }

    #[test]
    fn r6_ignores_other_ecosystems_and_prefix_dirs() {
        let dir = r6_fixture();
        write_dependabot(
            &dir,
            &format!(
                "{ROOT_ONLY_DEPENDABOT}  - package-ecosystem: \"npm\"\n    \
                 directory: \"/crates/x/fuzz\"\n"
            ),
        );
        assert!(rules(&dir).is_empty());
        write_dependabot(&dir, &cargo_block("    directory: \"/crates/x\"\n"));
        assert!(rules(&dir).is_empty());
    }

    #[test]
    fn r6_fires_on_glob_directories() {
        let dir = r6_fixture();
        for g in ["/crates/**", "/crates/*/fuzz", "/crates/x/fuz?"] {
            write_dependabot(
                &dir,
                &cargo_block(&format!("    directories:\n      - \"{g}\"\n")),
            );
            assert_eq!(
                rules(&dir),
                vec![DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE],
                "glob {g}"
            );
        }
    }

    #[test]
    fn r6_reads_both_keys_when_both_are_present() {
        let dir = r6_fixture();
        write_dependabot(
            &dir,
            &cargo_block("    directory: \"/elsewhere\"\n    directories: [\"/crates/x/fuzz\"]\n"),
        );
        assert_eq!(
            rules(&dir),
            vec![DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE]
        );
        write_dependabot(
            &dir,
            &cargo_block("    directory: \"/crates/x/fuzz\"\n    directories: [\"/elsewhere\"]\n"),
        );
        assert_eq!(
            rules(&dir),
            vec![DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE]
        );
    }

    #[test]
    fn r6_non_string_directory_is_a_precondition_not_a_drop() {
        let dir = r6_fixture();
        write_dependabot(&dir, &cargo_block("    directories:\n      - 7\n"));
        assert_eq!(rules(&dir), vec![DEPENDABOT_CONFIG_UNREADABLE]);
        write_dependabot(&dir, &cargo_block("    directory: {a: 1}\n"));
        assert_eq!(rules(&dir), vec![DEPENDABOT_CONFIG_UNREADABLE]);
    }

    #[test]
    fn r6_preconditions_never_pass() {
        let dir = r6_fixture();
        assert_eq!(rules(&dir), vec![DEPENDABOT_CONFIG_UNREADABLE]);
        write_dependabot(&dir, "updates: [\n");
        assert_eq!(rules(&dir), vec![DEPENDABOT_CONFIG_UNREADABLE]);

        // An exclude entry with no manifest is never silently skipped.
        let root = ROOT.replace(
            "members = [\"a\", \"b\"]\n",
            "members = [\"a\", \"b\"]\nexclude = [\"gone\"]\n",
        );
        let a = pkg("a", "");
        let b = pkg("b", "");
        let dir = ws(&root, &[("a", &a), ("b", &b)], None);
        root_only_dependabot(&dir);
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
            DEPENDABOT_CARGO_BLOCK_NAMES_EXCLUDED_CRATE,
        ]
        .iter()
        .chain(PRECONDITION_RULES)
        {
            assert_ne!(token_for_rule(id), "workspace-deps-violation", "{id}");
        }
    }
}
