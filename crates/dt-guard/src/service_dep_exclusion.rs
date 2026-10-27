//! `service-dep-exclusion` subcommand — a crate barred from a service must not
//! enter that service's resolved dependency graph.
//!
//! The one exclusion today: **no `sqlx` in MC or MH.** `common`'s `db` feature
//! is the only door — MC and MH own no database, and `common` gates
//! `dep:sqlx` behind `db`, which only AC and GC enable. Nothing in cargo stops a
//! later edit from opening that door (MC enabling `common/db`, a new shared
//! dependency that pulls sqlx unconditionally, a "convenience" feature
//! unification in `[workspace.dependencies]`): the build still succeeds, the
//! binary just grows a Postgres driver it never uses, plus its compile cost in
//! every image build. This guard makes that a red line instead of a silent one.
//!
//! ## How the graph is read
//!
//! `cargo tree -p <service>` resolves features **for that one package**, exactly
//! as the per-service image build (`cargo build --release -p <service>`) does.
//! A whole-workspace view (`cargo metadata`, `Cargo.lock`) is the wrong
//! instrument: it unifies features across all members, so `common`'s `db` edge
//! (enabled by AC/GC) would appear under MC too and every run would be a false
//! positive. Edges: `normal,build` (what the shipped binary's build compiles;
//! dev-dependencies are excluded), `--target all` (a cfg-gated dependency is
//! still a dependency). Each service is resolved twice — default features (the
//! image build) and `--all-features` (no service feature may open the door
//! either).
//!
//! ## Why this runs in Layer 1, not Layer 3
//!
//! It invokes cargo, and Layer 3 invoking cargo zero times is structural
//! (`scripts/lang/rust/compile.sh` has the reasoning). It is wired in
//! `compile.sh` right after the dt-guard release build, NOT as a
//! `scripts/guards/simple/*.sh` wrapper (which `run-guards.sh` would
//! auto-discover into Layer 3).
//!
//! ## Vacuity
//!
//! A resolution that fails, or a graph that does not contain the service
//! itself plus at least one dependency, is a PRECONDITION failure, never a
//! pass: "cargo printed nothing" must not read like "no sqlx".
//!
//! ## Ownership
//!
//! Machinery (this file's shape, the cargo invocation, parsing): infrastructure.
//! Policy content — WHICH crates are barred from WHICH service ([`EXCLUSIONS`]):
//! database (it encodes `common`'s `db` boundary), with the owning service
//! specialist for any entry added for another reason.

use crate::common::explain::{print_finding, Finding};
use crate::common::status::{emit_ok, emit_scope};
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// One service and the crates that must never appear in its resolved graph.
#[derive(Debug, Clone, Copy)]
pub struct Exclusion {
    /// Workspace package name of the service (`cargo tree -p` spec).
    pub service: &'static str,
    /// Package names barred from the service's normal+build graph.
    pub crates: &'static [&'static str],
    /// One-line rationale, printed with every violation.
    pub why: &'static str,
}

/// `sqlx` is the facade; `sqlx-core` is listed too because every sqlx-based
/// path (a driver crate, a sqlx-backed library) goes through it, so a route
/// that skips the facade crate is still caught.
const NO_DB_CRATES: &[&str] = &["sqlx", "sqlx-core"];
const NO_DB_WHY: &str = "owns no database; sqlx enters only via `common`'s `db` feature, \
                         which only AC and GC enable";

/// Policy content (see module doc §Ownership).
pub const EXCLUSIONS: &[Exclusion] = &[
    Exclusion {
        service: "mc-service",
        crates: NO_DB_CRATES,
        why: NO_DB_WHY,
    },
    Exclusion {
        service: "mh-service",
        crates: NO_DB_CRATES,
        why: NO_DB_WHY,
    },
];

pub const FORBIDDEN_DEP: &str = "forbidden_dep_in_service_graph";
pub const RESOLUTION_FAILED: &str = "service_dep_exclusion_resolution_failed";
pub const GRAPH_VACUOUS: &str = "service_dep_exclusion_graph_vacuous";

const PRECONDITION_RULES: &[&str] = &[RESOLUTION_FAILED, GRAPH_VACUOUS];

/// The two feature selections each service is resolved under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureMode {
    /// The service's default features — what the image build compiles.
    Default,
    /// `--all-features` — no service feature may pull a barred crate either.
    All,
}

impl FeatureMode {
    pub const BOTH: [FeatureMode; 2] = [FeatureMode::Default, FeatureMode::All];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default-features",
            Self::All => "all-features",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub rule_id: &'static str,
    pub service: String,
    pub detail: String,
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let (hits, graphs) = evaluate(EXCLUSIONS, |service, mode| {
        cargo_tree(repo_root, service, mode)
    });
    emit_scope(format!(
        "{} services x {} feature modes = {graphs} resolved graphs, {} hits",
        EXCLUSIONS.len(),
        FeatureMode::BOTH.len(),
        hits.len()
    ));

    if hits.is_empty() {
        emit_ok(format!("service-dep-exclusion-clean-graphs-{graphs}"));
        return Ok(());
    }
    for hit in &hits {
        let token = hit.rule_id.replace('_', "-");
        if explain {
            let policy = format!("service-dep-exclusion::{}", hit.rule_id);
            print_finding(&Finding {
                file: &hit.service,
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
                "ERROR: PRECONDITION [{token}] {} — {} (resolution/precondition failure, \
                 NOT a diff defect)",
                hit.service, hit.detail
            );
        } else {
            println!("VIOLATION: [{token}] {} {}", hit.service, hit.detail);
        }
    }
    let first = hits
        .iter()
        .find(|h| PRECONDITION_RULES.contains(&h.rule_id))
        .or(hits.first())
        .map_or("service-dep-exclusion", |h| h.rule_id);
    anyhow::bail!("{}-{}", first.replace('_', "-"), hits.len())
}

/// Evaluate `exclusions`, resolving each service's graph with `resolve`
/// (`Ok(stdout)` of a `cargo tree --prefix none` listing, or `Err(diagnostic)`).
/// Returns the hits and the number of graphs that resolved non-vacuously.
pub fn evaluate<F>(exclusions: &[Exclusion], mut resolve: F) -> (Vec<Hit>, usize)
where
    F: FnMut(&str, FeatureMode) -> std::result::Result<String, String>,
{
    let mut hits = Vec::new();
    let mut graphs = 0;
    for ex in exclusions {
        for mode in FeatureMode::BOTH {
            let listing = match resolve(ex.service, mode) {
                Ok(out) => out,
                Err(diag) => {
                    hits.push(Hit {
                        rule_id: RESOLUTION_FAILED,
                        service: ex.service.to_string(),
                        detail: format!("({}) cargo tree failed: {diag}", mode.label()),
                    });
                    continue;
                }
            };
            let packages = parse_listing(&listing);
            if !packages.contains(ex.service) || packages.len() < 2 {
                hits.push(Hit {
                    rule_id: GRAPH_VACUOUS,
                    service: ex.service.to_string(),
                    detail: format!(
                        "({}) resolved graph has {} packages and {} the service itself — the \
                         listing cannot be what cargo resolves for this package",
                        mode.label(),
                        packages.len(),
                        if packages.contains(ex.service) {
                            "includes"
                        } else {
                            "lacks"
                        }
                    ),
                });
                continue;
            }
            graphs += 1;
            for barred in ex.crates {
                if packages.contains(*barred) {
                    hits.push(Hit {
                        rule_id: FORBIDDEN_DEP,
                        service: ex.service.to_string(),
                        detail: format!(
                            "({mode}) depends on `{barred}`: {service} {why}. Find the path with \
                             `cargo tree -p {service} -e normal,build --target all{flag} -i {barred}`",
                            mode = mode.label(),
                            service = ex.service,
                            why = ex.why,
                            flag = if mode == FeatureMode::All {
                                " --all-features"
                            } else {
                                ""
                            },
                        ),
                    });
                }
            }
        }
    }
    (hits, graphs)
}

/// Package names in a `cargo tree --prefix none` listing. Each non-empty line
/// is `<name> v<version>[ (<source>)][ (*)]`; the name is the first token.
pub fn parse_listing(listing: &str) -> BTreeSet<&str> {
    listing
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

/// Resolve `service`'s graph the way its image build does (see module doc).
fn cargo_tree(
    repo_root: &Path,
    service: &str,
    mode: FeatureMode,
) -> std::result::Result<String, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(repo_root).args([
        "tree",
        "--locked",
        "-p",
        service,
        "-e",
        "normal,build",
        "--target",
        "all",
        "--prefix",
        "none",
        "--format",
        "{p}",
    ]);
    if mode == FeatureMode::All {
        cmd.arg("--all-features");
    }
    let output = cmd.output().map_err(|e| format!("spawning cargo: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let head: Vec<&str> = stderr.lines().take(5).collect();
        return Err(format!("exit {}: {}", output.status, head.join(" | ")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MC_CLEAN: &str = "\
mc-service v0.1.0 (/repo/crates/mc-service)
common v0.1.0 (/repo/crates/common)
tokio v1.54.0
tokio v1.54.0 (*)
";

    fn exclusions() -> &'static [Exclusion] {
        &[Exclusion {
            service: "mc-service",
            crates: NO_DB_CRATES,
            why: "test",
        }]
    }

    #[test]
    fn parse_listing_takes_first_token_and_dedupes() {
        let pkgs = parse_listing(MC_CLEAN);
        let want: BTreeSet<&str> = ["mc-service", "common", "tokio"].into_iter().collect();
        assert_eq!(pkgs, want);
    }

    #[test]
    fn parse_listing_ignores_blank_lines() {
        assert!(parse_listing("\n  \n").is_empty());
    }

    #[test]
    fn clean_graphs_pass_and_count_both_modes() {
        let (hits, graphs) = evaluate(exclusions(), |_, _| Ok(MC_CLEAN.to_string()));
        assert!(hits.is_empty(), "{hits:?}");
        assert_eq!(graphs, 2);
    }

    #[test]
    fn sqlx_in_graph_is_a_violation_naming_the_crate_and_mode() {
        let listing = format!("{MC_CLEAN}sqlx v0.9.0\nsqlx-core v0.9.0\n");
        let (hits, graphs) = evaluate(exclusions(), |_, _| Ok(listing.clone()));
        assert_eq!(graphs, 2);
        assert_eq!(hits.len(), 4, "{hits:?}");
        assert!(hits.iter().all(|h| h.rule_id == FORBIDDEN_DEP));
        assert!(hits
            .iter()
            .any(|h| h.detail.contains("`sqlx`") && h.detail.contains("default-features")));
        assert!(hits
            .iter()
            .any(|h| h.detail.contains("`sqlx-core`") && h.detail.contains("--all-features")));
    }

    #[test]
    fn sqlx_core_alone_is_caught() {
        let listing = format!("{MC_CLEAN}sqlx-core v0.9.0\n");
        let (hits, _) = evaluate(exclusions(), |_, _| Ok(listing.clone()));
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(hits.iter().all(|h| h.detail.contains("`sqlx-core`")));
    }

    #[test]
    fn barred_crate_only_under_all_features_is_caught() {
        let (hits, _) = evaluate(exclusions(), |_, mode| match mode {
            FeatureMode::Default => Ok(MC_CLEAN.to_string()),
            FeatureMode::All => Ok(format!("{MC_CLEAN}sqlx v0.9.0\n")),
        });
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits
            .first()
            .is_some_and(|h| h.detail.contains("all-features")));
    }

    #[test]
    fn similarly_named_crate_is_not_a_violation() {
        // Whole-name match only: `sqlxmq` and `my-sqlx` are not `sqlx`.
        let listing = format!("{MC_CLEAN}sqlxmq v0.1.0\nmy-sqlx v0.1.0\n");
        let (hits, _) = evaluate(exclusions(), |_, _| Ok(listing.clone()));
        assert!(hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn resolution_failure_is_a_precondition_not_a_pass() {
        let (hits, graphs) = evaluate(exclusions(), |_, _| Err("boom".to_string()));
        assert_eq!(graphs, 0);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.rule_id == RESOLUTION_FAILED));
    }

    #[test]
    fn empty_listing_is_vacuous_not_a_pass() {
        let (hits, graphs) = evaluate(exclusions(), |_, _| Ok(String::new()));
        assert_eq!(graphs, 0);
        assert!(hits.iter().all(|h| h.rule_id == GRAPH_VACUOUS), "{hits:?}");
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn listing_without_the_service_root_is_vacuous() {
        let (hits, _) = evaluate(exclusions(), |_, _| {
            Ok("gc-service v0.1.0\nsqlx v0.9.0\n".to_string())
        });
        assert!(hits.iter().all(|h| h.rule_id == GRAPH_VACUOUS), "{hits:?}");
    }

    #[test]
    fn policy_covers_mc_and_mh_for_sqlx() {
        for svc in ["mc-service", "mh-service"] {
            let ex = EXCLUSIONS.iter().find(|e| e.service == svc);
            assert!(
                ex.is_some_and(|e| e.crates.contains(&"sqlx")),
                "{svc} must bar sqlx"
            );
        }
    }
}
