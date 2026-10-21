//! Canonical service enumeration — single source of truth.
//!
//! Per @dry-reviewer F-DRY-3 2026-05-19: the `{ac, gc, mc, mh}` enumeration
//! was mirrored across 5 sites (2 Rust const arrays, 2 byte-identical Rust
//! regex statics, 1 shell array). The 2 Rust regex statics were the most
//! load-bearing duplication — both walked PromQL expressions for service-
//! prefixed metric refs. This module is the SoT; downstream consumers
//! `application_metrics` and `dashboard_panels` import from here.
//!
//! `scripts/guards/common.sh:CANONICAL_SERVICES` remains a separate Bash
//! mirror — the cross-stack collapse to a single SoT requires either Bash
//! code-generation from Rust or a JSON/TOML intermediary, and is tracked
//! as Wave 2+ work in `docs/TODO.md` §Cross-Service Duplication (DRY).

use crate::common::path_safety::resolve_cited_path;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

/// Repo-relative directory holding every Kustomize service base.
pub const INFRA_SERVICES_DIR: &str = "infra/services";

/// Every Kustomize base under `infra/services/` — the directory names, sorted.
///
/// THE ONE enumeration of service bases. `kustomize` R-15/R-16 build and
/// orphan-check every base it returns, and `env-config` discovers generator-only
/// bases (shared ConfigMaps such as `media-codec-profile`) from it. It replaced
/// a hand-written list in `kustomize.rs` that a new base could silently miss —
/// the listing IS the source of truth.
///
/// Every subdirectory is yielded, with or without a `kustomization.yaml`:
/// R-16 reports a base missing one, so dropping it here would turn that loud
/// finding into an invisible omission.
///
/// Containment: each base directory is resolved through
/// [`resolve_cited_path`]; a base that is (or resolves through) a symlink out
/// of the repository is a hard error, never a skip — a guard must not parse a
/// kustomization from outside the tree it vouches for. A missing
/// `infra/services/` yields no bases (the callers' own vacuity checks fire).
pub fn infra_service_bases(repo_root: &Path) -> Result<Vec<String>> {
    let dir = repo_root.join(INFRA_SERVICES_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry.with_context(|| format!("reading entry in {}", dir.display()))?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            anyhow::bail!("{INFRA_SERVICES_DIR}: a base directory name is not UTF-8");
        };
        let rel = format!("{INFRA_SERVICES_DIR}/{name}");
        let Some(resolved) = resolve_cited_path(repo_root, &rel) else {
            anyhow::bail!(
                "{rel} resolves outside the repository (a symlink?) or cannot be resolved; refusing to read a service base from outside the tree"
            );
        };
        if resolved.is_dir() {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

/// `(metric_prefix, directory_name)` for each service that emits metrics.
/// Adding a new service requires updating only this one site — the
/// `SERVICE_METRIC_PREFIX_RE` regex below is derived from this list at
/// `Lazy::new` time, so prefix and regex cannot drift.
///
/// # Two consumers, and the invariant between them
///
/// This array was written to answer "which services **emit metrics**"
/// (`application_metrics`, `dashboard_panels`). `release_build_profile` also
/// reads it, to answer a different question: "which services **ship as a
/// container**" — it cross-checks this roster when asserting that every
/// service is inside the release-build premise gate.
///
/// **Those two sets coincide today, and that coincidence is now load-bearing.**
/// Editing this array for a metrics reason would otherwise silently move a
/// release-premise gate's floor. Rather than leave that to a doc comment a
/// future editor must notice, it is **machine-checked**:
/// `release_build_profile`'s `canonical_services_roster_drift` rule derives the
/// service roster from `[workspace] members` matching `crates/*-service` — a
/// true single source of truth, since cargo itself fails to build if that list
/// is wrong — and FAILs when this array disagrees with it. So this array is a
/// *guarded* mirror, not a trusted one.
///
/// ANCHOR (DRY): `scripts/guards/common.sh:CANONICAL_SERVICES` is a separate,
/// un-guarded Bash mirror (tracked in `docs/TODO.md` §Cross-Service
/// Duplication). The Rust array here is authoritative for the release-premise
/// gate.
pub const CANONICAL_SERVICES: &[(&str, &str)] = &[
    ("ac", "ac-service"),
    ("gc", "gc-service"),
    ("mc", "mc-service"),
    ("mh", "mh-service"),
];

/// Service-metric reference in PromQL expressions: matches the prefix list
/// in `CANONICAL_SERVICES` followed by `_<metric-name>`. Capture group 1 =
/// full metric name (`ac_token_validations_total`, `mh_active_connections`,
/// etc.). Consumers walk `captures_iter` and read group 1.
///
/// Pattern is `\b((?:ac|gc|mc|mh)_[a-z][a-z0-9_]*)`. The alternation is
/// constructed dynamically from `CANONICAL_SERVICES` so adding a service
/// updates the regex automatically — no separate mirror to maintain.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
pub static SERVICE_METRIC_PREFIX_RE: Lazy<Regex> = Lazy::new(|| {
    let alternation: Vec<&str> = CANONICAL_SERVICES.iter().map(|(p, _)| *p).collect();
    let pattern = format!(r"\b((?:{})_[a-z][a-z0-9_]*)", alternation.join("|"));
    Regex::new(&pattern).expect("static pattern compiles")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_services_count_matches_regex_alternation() {
        // Regression net: if a future contributor adds a service to
        // CANONICAL_SERVICES but the regex Lazy somehow forks (it can't
        // structurally, but tests cheap), this fires.
        let captured: Vec<&str> = SERVICE_METRIC_PREFIX_RE
            .captures_iter("ac_x_total gc_y_total mc_z_total mh_w_total")
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert_eq!(captured.len(), CANONICAL_SERVICES.len());
    }

    #[test]
    fn infra_service_bases_lists_every_dir_sorted_and_skips_files() {
        let td = tempfile::tempdir().expect("tempdir");
        let svc = td.path().join(INFRA_SERVICES_DIR);
        for d in ["zeta", "alpha", "no-kustomization"] {
            std::fs::create_dir_all(svc.join(d)).expect("mkdir");
        }
        std::fs::write(svc.join("alpha/kustomization.yaml"), "").expect("write");
        std::fs::write(svc.join("README.md"), "").expect("write");
        let bases = infra_service_bases(td.path()).expect("bases");
        assert_eq!(bases, vec!["alpha", "no-kustomization", "zeta"]);
    }

    #[test]
    fn infra_service_bases_missing_dir_is_empty() {
        let td = tempfile::tempdir().expect("tempdir");
        assert!(infra_service_bases(td.path()).expect("bases").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn infra_service_bases_symlink_escaping_repo_is_an_error() {
        let outer = tempfile::tempdir().expect("tempdir");
        let repo = outer.path().join("repo");
        let outside = outer.path().join("outside-base");
        std::fs::create_dir_all(repo.join(INFRA_SERVICES_DIR)).expect("mkdir");
        std::fs::create_dir_all(&outside).expect("mkdir");
        std::fs::write(outside.join("kustomization.yaml"), "resources: []\n").expect("write");
        std::os::unix::fs::symlink(&outside, repo.join(INFRA_SERVICES_DIR).join("evil"))
            .expect("symlink");
        let err = infra_service_bases(&repo).expect_err("escape must fail");
        assert!(
            format!("{err:#}").contains("infra/services/evil"),
            "error names the base: {err:#}"
        );
    }

    #[test]
    fn service_prefix_re_rejects_unknown_prefix() {
        let captured: Vec<&str> = SERVICE_METRIC_PREFIX_RE
            .captures_iter("xx_unknown_total")
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert!(captured.is_empty());
    }

    #[test]
    fn service_prefix_re_requires_underscore_after_prefix() {
        // `accept` should NOT match `ac_*` — no underscore after `ac`.
        let captured: Vec<&str> = SERVICE_METRIC_PREFIX_RE
            .captures_iter("accept")
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert!(captured.is_empty());
    }
}
