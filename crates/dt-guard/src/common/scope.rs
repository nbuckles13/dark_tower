//! Scope-liveness assertion — "a guard whose scope is configuration must fail
//! when that configuration resolves to nothing."
//!
//! # Why this is a shared helper on its first consumer
//!
//! `docs/TODO.md` §"Guard Coverage Gaps — path roots and globs that resolve
//! to nothing" (S16/S17, sub-entry S16b) files this as a general predicate
//! and enumerates **twelve hardcoded path roots across ten `dt-guard`
//! modules** that need it — `application_metrics`, `kustomize`,
//! `knowledge_index`, `cite_extract`, `todo_tracking`, `histogram_buckets`,
//! `ts_dev_trust`, `grafana_datasources`. That entry's own words: *"a guard's
//! path roots and path globs are inputs and must be asserted present; 'path
//! absent' is a hard error, never an empty result set. The assertion is the
//! load-bearing deliverable."*
//!
//! `media_telemetry_deny` is the first guard whose *purpose* is this
//! assertion, which makes it the natural home rather than an awkward one.
//!
//! **Scope boundary for the loop that introduced this (2026-09-07):** the ten
//! S16b modules are deliberately NOT re-pointed here — that is S16b's task
//! and its owner's call. This module is the reference implementation they
//! should consume. The signature is shaped for the general case, not for this
//! guard's manifest: it takes a plain slice of repo-relative roots, so a
//! single hardcoded root calls it with a one-element slice and no wrapper.
//! If a natural S16b call site needed a wrapper, the extraction would not
//! have happened.
//!
//! # Two distinct failures, deliberately not one
//!
//! "Directory does not exist" and "directory exists but holds no matching
//! files" get **different** reason tokens. The in-tree counter-example is
//! `gsa_sync`'s `CANON_MISSING_RULE_ID`, which means both "string absent from
//! a mirror" and "filesystem path does not exist" — S16b's verdict on it is
//! that *"the module's own emitted behaviour teaches the wrong inference."*
//! The two conditions here have opposite operator first-actions (restore the
//! directory / update the manifest, versus the guard's discovery broke) and a
//! shared token would make the runbook row unwriteable.
//!
//! # Polarity: empty is FATAL here, unlike `metric_coverage`
//!
//! `metric_coverage` documents a *safe*-empty scope — zero matches there is a
//! legitimate state. This module is the opposite polarity and says so at the
//! definition, because the contrast is exactly what S16b says is missing
//! everywhere else: an empty result must never be indistinguishable from a
//! passing one.
//!
//! # [`ScopeRoot`] covers PROVENANCE; the tokens cover CARDINALITY
//!
//! [`ScopeRoot`] has no public constructor, so a caller cannot hold one it
//! did not obtain from [`assert_scope_live`] — every root a walker sees has
//! passed the assertion. That is a *provenance* guarantee and it is all it is.
//!
//! It says nothing about holding **zero** roots: `Ok(vec![])` type-checks
//! perfectly and would hand a walker an empty slice that walks nothing and
//! passes. Cardinality is covered by the reason tokens
//! ([`ScopeFailure::NoDirectoriesConfigured`] and the per-root variants),
//! which exist independently of the type.
//!
//! **Neither subsumes the other, and a reader who concludes "the type
//! guarantees the scope is live" will eventually wonder why the tokens are
//! still there and delete one.** This is ADR-0036 §11's alive-versus-applied
//! split one level down: the type makes a widened walk hard to write, the
//! tokens make a vacuous walk impossible to pass. (@operations, 2026-09-07.)

use crate::common::path_safety::resolve_cited_path;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A configured scope directory that has been proven live.
///
/// **No public constructor.** The only way to obtain one is
/// [`assert_scope_live`], so a file walker whose signature takes
/// `&[ScopeRoot]` structurally cannot be pointed at an unvalidated path.
/// Widening a walk from the resolved directories to a crate or repo root
/// stops being a one-character path edit that compiles and becomes an
/// "add a constructor" diff, which is visible in review.
///
/// Same move as `common::explain::SecretFinding` having no `matched` field:
/// prefer structural impossibility over a control that has to notice
/// (ADR-0036 §11).
#[derive(Debug, Clone)]
pub struct ScopeRoot {
    /// Canonicalized absolute path, proven to be inside the repo root.
    abs: PathBuf,
    /// The repo-relative path as configured, for diagnostics.
    rel: String,
    /// Matching files found under `abs`, sorted. Non-empty by construction.
    files: Vec<PathBuf>,
}

impl ScopeRoot {
    /// The repo-relative path as it appeared in configuration.
    pub fn rel(&self) -> &str {
        &self.rel
    }

    /// Canonicalized absolute path.
    pub fn abs(&self) -> &Path {
        &self.abs
    }

    /// Matching files under this root. Guaranteed non-empty — a root with
    /// zero matches is a [`ScopeFailure::DirectoryEmpty`], never a
    /// `ScopeRoot`.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
}

/// Why a configured scope was not live.
///
/// Each variant maps to a distinct reason-token suffix via [`Self::token`].
/// Callers prefix it with their subcommand name, e.g.
/// `format!("media-telemetry-deny-{}", failure.token())`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeFailure {
    /// The configuration named no directories at all — an empty list, or one
    /// whose every entry was removed. A total disarm with no other detector:
    /// every per-directory check passes vacuously because there is nothing to
    /// check.
    NoDirectoriesConfigured,
    /// A configured entry resolves outside the repository root (traversal, an
    /// absolute path, or a symlink pointing out of the tree).
    ///
    /// Deliberately distinct from [`Self::DirectoryMissing`]: the "missing"
    /// remediation is *restore the directory or update the manifest path*,
    /// which applied to a symlink escape walks the operator toward
    /// re-pointing the manifest at whatever the symlink targets — completing
    /// the evasion rather than closing it.
    DirectoryEscapesRoot { rel: String },
    /// A configured entry does not exist on disk, or is not a directory.
    DirectoryMissing { rel: String },
    /// A configured entry exists but contains no files with a matching
    /// extension. The guard would run and check nothing.
    DirectoryEmpty { rel: String },
}

impl ScopeFailure {
    /// Kebab-case reason-token suffix. Callers prefix with their subcommand.
    pub const fn token(&self) -> &'static str {
        match self {
            Self::NoDirectoriesConfigured => "scope-no-directories-configured",
            Self::DirectoryEscapesRoot { .. } => "scope-directory-escapes-root",
            Self::DirectoryMissing { .. } => "scope-directory-missing",
            Self::DirectoryEmpty { .. } => "scope-directory-empty",
        }
    }

    /// The configured entry this failure concerns, if it names one.
    pub fn rel(&self) -> Option<&str> {
        match self {
            Self::NoDirectoriesConfigured => None,
            Self::DirectoryEscapesRoot { rel }
            | Self::DirectoryMissing { rel }
            | Self::DirectoryEmpty { rel } => Some(rel),
        }
    }

    /// One-line operator-facing description. Deliberately carries **no file
    /// content** — only the configured path string and the condition.
    pub fn detail(&self) -> String {
        match self {
            Self::NoDirectoriesConfigured => {
                "configuration names zero directories — the guard would check nothing".to_string()
            }
            Self::DirectoryEscapesRoot { rel } => format!(
                "configured directory `{rel}` resolves OUTSIDE the repository root (traversal, absolute path, or a symlink out of the tree)"
            ),
            Self::DirectoryMissing { rel } => {
                format!("configured directory `{rel}` does not exist on disk, or is not a directory")
            }
            Self::DirectoryEmpty { rel } => format!(
                "configured directory `{rel}` exists but contains no matching files — the guard would check nothing"
            ),
        }
    }
}

/// Assert every configured root is live, returning the validated roots.
///
/// A root is live when it (a) resolves inside `repo_root`, (b) is an existing
/// directory, and (c) contains at least one file whose name ends with one of
/// `extensions`.
///
/// Returns **all** failures rather than the first, so an operator fixing a
/// multi-root configuration sees the whole picture in one run. An empty
/// `rel_roots` is itself a failure — never a vacuous success.
///
/// Symlinks are not followed during the walk (`walkdir`'s default), so a
/// symlinked subdirectory cannot walk out of the validated scope after
/// containment has been checked.
pub fn assert_scope_live(
    repo_root: &Path,
    rel_roots: &[String],
    extensions: &[&str],
) -> Result<Vec<ScopeRoot>, Vec<ScopeFailure>> {
    if rel_roots.is_empty() {
        return Err(vec![ScopeFailure::NoDirectoriesConfigured]);
    }

    let mut roots: Vec<ScopeRoot> = Vec::new();
    let mut failures: Vec<ScopeFailure> = Vec::new();

    for rel in rel_roots {
        let trimmed = rel.trim_end_matches('/');
        let joined = repo_root.join(trimmed);

        // Order matters: check existence FIRST so that a genuinely absent
        // path reports as `DirectoryMissing`, and only an *existing* path
        // that fails containment reports as `DirectoryEscapesRoot`.
        // `resolve_cited_path` returns `None` for both conditions, and
        // collapsing them would put a symlinked-away directory on the
        // "restore it or re-point the manifest" remediation path.
        if !joined.exists() {
            failures.push(ScopeFailure::DirectoryMissing { rel: rel.clone() });
            continue;
        }

        let Some(abs) = resolve_cited_path(repo_root, trimmed) else {
            failures.push(ScopeFailure::DirectoryEscapesRoot { rel: rel.clone() });
            continue;
        };

        if !abs.is_dir() {
            failures.push(ScopeFailure::DirectoryMissing { rel: rel.clone() });
            continue;
        }

        let mut files: Vec<PathBuf> = Vec::new();
        for entry in WalkDir::new(&abs).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if extensions.iter().any(|ext| name.ends_with(ext)) {
                files.push(path.to_path_buf());
            }
        }
        files.sort();

        if files.is_empty() {
            failures.push(ScopeFailure::DirectoryEmpty { rel: rel.clone() });
            continue;
        }

        roots.push(ScopeRoot {
            abs,
            rel: rel.clone(),
            files,
        });
    }

    if failures.is_empty() {
        Ok(roots)
    } else {
        Err(failures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for (rel, body) in files {
            let path = dir.path().join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("mkdir");
            }
            fs::write(&path, body).expect("write");
        }
        dir
    }

    #[test]
    fn live_scope_returns_roots_with_files() {
        let d = repo(&[
            ("src/media/a.rs", "fn a() {}"),
            ("src/media/b.rs", "fn b() {}"),
        ]);
        let roots =
            assert_scope_live(d.path(), &["src/media/".to_string()], &[".rs"]).expect("live");
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].rel(), "src/media/");
        assert_eq!(roots[0].files().len(), 2);
    }

    #[test]
    fn empty_root_list_is_a_failure_not_a_vacuous_pass() {
        let d = repo(&[]);
        let err = assert_scope_live(d.path(), &[], &[".rs"]).expect_err("must fail");
        assert_eq!(err, vec![ScopeFailure::NoDirectoriesConfigured]);
        assert_eq!(err[0].token(), "scope-no-directories-configured");
    }

    #[test]
    fn missing_directory_reports_missing() {
        let d = repo(&[("src/other/a.rs", "fn a() {}")]);
        let err = assert_scope_live(d.path(), &["src/media/".to_string()], &[".rs"])
            .expect_err("must fail");
        assert_eq!(err[0].token(), "scope-directory-missing");
    }

    #[test]
    fn existing_but_fileless_directory_reports_empty() {
        let d = repo(&[("src/media/README.txt", "not rust")]);
        let err = assert_scope_live(d.path(), &["src/media/".to_string()], &[".rs"])
            .expect_err("must fail");
        assert_eq!(err[0].token(), "scope-directory-empty");
    }

    /// The two tokens must be DISTINGUISHABLE, not merely both failures.
    /// This is the whole point of not overloading one token — see the module
    /// doc on `gsa_sync::CANON_MISSING_RULE_ID`.
    #[test]
    fn missing_and_empty_are_distinct_tokens() {
        let d = repo(&[("src/empty/README.txt", "x")]);
        let err = assert_scope_live(
            d.path(),
            &["src/gone/".to_string(), "src/empty/".to_string()],
            &[".rs"],
        )
        .expect_err("must fail");
        let tokens: Vec<&str> = err.iter().map(|f| f.token()).collect();
        assert!(tokens.contains(&"scope-directory-missing"));
        assert!(tokens.contains(&"scope-directory-empty"));
        assert_ne!(tokens[0], tokens[1]);
    }

    #[test]
    fn traversal_escape_reports_escapes_root_not_missing() {
        let d = repo(&[("src/media/a.rs", "fn a() {}")]);
        // `..` resolves above the root; it exists, so it must NOT be reported
        // as "missing" — that remediation would send the operator the wrong way.
        let err =
            assert_scope_live(d.path(), &["../".to_string()], &[".rs"]).expect_err("must fail");
        assert_eq!(err[0].token(), "scope-directory-escapes-root");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_out_of_tree_reports_escapes_root() {
        let outside = tempfile::tempdir().expect("outside");
        fs::write(outside.path().join("leak.rs"), "fn l() {}").expect("write");
        let d = repo(&[("src/keep.rs", "fn k() {}")]);
        std::os::unix::fs::symlink(outside.path(), d.path().join("src/media")).expect("symlink");

        let err = assert_scope_live(d.path(), &["src/media/".to_string()], &[".rs"])
            .expect_err("must fail");
        assert_eq!(
            err[0].token(),
            "scope-directory-escapes-root",
            "a symlinked-away scope must never report as the benign `missing` case"
        );
    }

    #[test]
    fn a_file_where_a_directory_was_expected_reports_missing() {
        let d = repo(&[("src/media", "I am a file, not a directory")]);
        let err = assert_scope_live(d.path(), &["src/media".to_string()], &[".rs"])
            .expect_err("must fail");
        assert_eq!(err[0].token(), "scope-directory-missing");
    }

    #[test]
    fn all_failures_are_reported_not_just_the_first() {
        let d = repo(&[("src/empty/x.txt", "x")]);
        let err = assert_scope_live(
            d.path(),
            &["src/gone/".to_string(), "src/empty/".to_string()],
            &[".rs"],
        )
        .expect_err("must fail");
        assert_eq!(
            err.len(),
            2,
            "an operator fixing config sees the whole picture"
        );
    }

    /// Failure details name only the configured path and the condition —
    /// never file content. @semantic-guard's constraint applies to every
    /// output surface of this guard family.
    #[test]
    fn detail_carries_no_file_content() {
        let f = ScopeFailure::DirectoryEmpty {
            rel: "crates/x/src/media/".to_string(),
        };
        let detail = f.detail();
        assert!(detail.contains("crates/x/src/media/"));
        assert!(detail.contains("contains no matching files"));
    }
}
