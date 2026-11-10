//! `docker-context` subcommand — the image build context must hold no key
//! material or secret files.
//!
//! # Why
//!
//! Every first-party image builds from the repository root. Whatever the
//! context holds lands in builder-stage layers, and with a layer cache export
//! (`--cache-to ... mode=max`) those layers leave the runner. The cache-spike
//! run of 2026-10-25 shipped provision-generated TLS private keys
//! (`infra/docker/certs/`) and the dev `secret.yaml` files into the GitHub
//! Actions cache that way. The same flow is the basis for official image
//! publishing, so the context is an allowlist and this guard proves it.
//!
//! # Contract on `.dockerignore`
//!
//! The file is an allowlist in a deliberately tiny dialect, so this guard can
//! read it exactly instead of re-implementing Docker's matcher (a second
//! implementation would drift — single source of truth):
//!
//! * the first rule is `*` (exclude everything);
//! * `!<path>` re-includes a literal repo-relative path (no glob characters,
//!   no `..`, no leading `/`);
//! * `**/<name>/` excludes a directory name anywhere below an included path.
//!
//! Comments and blank lines are ignored. Any other line is a violation.
//!
//! # What is scanned
//!
//! Every file under every re-included path (tracked or not — generated files
//! are the realistic leak), minus `**/<name>/` directories:
//!
//! * **name rules** — key/cert containers, env files, kubeconfigs,
//!   `secret*.{yaml,yml,json}`, Terraform state;
//! * **PEM private-key blocks** in any file;
//! * **value-shape secrets** ([`source_scan_patterns`]) in non-source data
//!   files. `.rs`/`.ts` source is left to `rust-no-hardcoded-secrets` /
//!   `ts-no-secrets`, which own source scanning and its test-fixture handling.
//!
//! A symlink in the context is itself a violation: it can point outside the
//! allowlist. An unreadable file is a violation, never a skip.
//!
//! # `[patch.*]` path targets must be in the context
//!
//! Every image resolves the workspace lock, so a root `[patch.<registry>]`
//! entry with a `path` (a vendored crate) must sit under an admitted `!` path,
//! or the planner/cook stage of EVERY image fails — at a host image build,
//! long after this guard could have said so. Paths come from the ONE
//! derivation, `common::cargo_manifest::patch_path_targets`. Reason token:
//! `patch-path-not-admitted`; an unreadable or unparseable root manifest is
//! `patch-manifest-unreadable`, never a pass.
//!
//! Admitted is not enough: cargo-chef's recipe skeletonizes workspace MEMBERS
//! only, so in every Dockerfile stage under `infra/docker/` that runs
//! `cargo chef cook`, each `[patch.*]` path must be COPYed into that stage
//! (from the context, not `--from=`) BEFORE the cook line, or the cook fails
//! to resolve the graph — at a host image build (a Layer-7 deploy failure
//! once, 2026-11-10). Reason token: `patch-path-not-in-cook-stage`. Anti-vacuity:
//! with any `[patch.*]` path, an unreadable `infra/docker` is
//! `cook-stage-scan-unreadable` and finding NO cook stage at all is
//! `patch-path-no-cook-stage-found` (a renamed or relocated cook would otherwise
//! check nothing). A cook line is any line containing `chef cook` (`cargo chef
//! cook` and `cargo-chef cook`). Known false red, failing closed: a stage that
//! inherits `vendor/` from its `FROM` parent stage is not credited — copies are
//! counted per stage; no first-party Dockerfile does that today.

use crate::common::cargo_manifest::{patch_path_targets, read_root_manifest};
use crate::common::status::emit_ok;
use crate::secret_patterns::source_scan_patterns;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

pub const DOCKERIGNORE: &str = ".dockerignore";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PEM_PRIVATE_KEY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"-----BEGIN ([A-Z0-9]+ )*PRIVATE KEY-----").expect("static pattern compiles")
});

/// (rule id, predicate over the lower-cased file name).
type NameRule = (&'static str, fn(&str) -> bool);

/// File-name rules.
const NAME_RULES: &[NameRule] = &[
    ("key-container", |n| {
        [
            ".pem",
            ".key",
            ".p12",
            ".pfx",
            ".jks",
            ".crt",
            ".cer",
            ".der",
            ".keystore",
        ]
        .iter()
        .any(|ext| n.ends_with(ext))
    }),
    ("ssh-key", |n| {
        n.starts_with("id_rsa") || n.starts_with("id_ed25519") || n.starts_with("id_ecdsa")
    }),
    ("env-file", |n| n == ".env" || n.starts_with(".env.")),
    ("kubeconfig", |n| n.contains("kubeconfig")),
    ("secret-manifest", |n| {
        n.starts_with("secret")
            && [".yaml", ".yml", ".json"]
                .iter()
                .any(|ext| n.ends_with(ext))
    }),
    ("terraform-state", |n| {
        n.ends_with(".tfstate") || n.ends_with(".tfstate.backup")
    }),
];

const SOURCE_EXTENSIONS: &[&str] = &["rs", "ts", "js", "svelte"];

#[derive(Debug, Default)]
struct Allowlist {
    includes: Vec<String>,
    excluded_dir_names: Vec<String>,
}

#[derive(Debug)]
struct Violation {
    path: String,
    rule: String,
}

fn is_literal_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.split('/').any(|seg| seg == "..")
        && !p.contains(['*', '?', '[', ']', '\\'])
}

fn parse_dockerignore(content: &str) -> (Allowlist, Vec<Violation>) {
    let mut allow = Allowlist::default();
    let mut bad = Vec::new();
    let mut seen_star = false;
    for (idx, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = format!("{DOCKERIGNORE}:{}", idx + 1);
        if !seen_star {
            if line == "*" {
                seen_star = true;
            } else {
                bad.push(Violation {
                    path: at,
                    rule: "first-rule-must-be-star".into(),
                });
            }
            continue;
        }
        if let Some(p) = line.strip_prefix('!') {
            let p = p.trim_end_matches('/');
            if is_literal_path(p) {
                allow.includes.push(p.to_string());
            } else {
                bad.push(Violation {
                    path: at,
                    rule: "include-not-literal-path".into(),
                });
            }
        } else if let Some(name) = line.strip_prefix("**/") {
            let name = name.trim_end_matches('/');
            if !name.is_empty() && is_literal_path(name) && !name.contains('/') {
                allow.excluded_dir_names.push(name.to_string());
            } else {
                bad.push(Violation {
                    path: at,
                    rule: "exclude-not-dir-name".into(),
                });
            }
        } else {
            bad.push(Violation {
                path: at,
                rule: "unsupported-rule".into(),
            });
        }
    }
    if !seen_star {
        bad.push(Violation {
            path: DOCKERIGNORE.into(),
            rule: "not-an-allowlist".into(),
        });
    }
    (allow, bad)
}

fn check_file(rel: &str, path: &Path, out: &mut Vec<Violation>) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    for (rule, pred) in NAME_RULES {
        if pred(&name) {
            out.push(Violation {
                path: rel.into(),
                rule: format!("name:{rule}"),
            });
        }
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            out.push(Violation {
                path: rel.into(),
                rule: format!("unreadable:{e}"),
            });
            return;
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    if PEM_PRIVATE_KEY_RE.is_match(&text) {
        out.push(Violation {
            path: rel.into(),
            rule: "content:pem-private-key".into(),
        });
    }
    let is_source = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SOURCE_EXTENSIONS.contains(&e));
    if !is_source {
        for (pname, re) in source_scan_patterns() {
            if re.is_match(&text) {
                out.push(Violation {
                    path: rel.into(),
                    rule: format!("content:{pname}"),
                });
            }
        }
    }
}

fn scan(repo_root: &Path, allow: &Allowlist, out: &mut Vec<Violation>) -> usize {
    let mut files = 0;
    for inc in &allow.includes {
        let base = repo_root.join(inc);
        if !base.exists() && base.symlink_metadata().is_err() {
            out.push(Violation {
                path: inc.clone(),
                rule: "include-missing".into(),
            });
            continue;
        }
        let walker = walkdir::WalkDir::new(&base).follow_links(false).into_iter();
        let walker = walker.filter_entry(|e| {
            !(e.file_type().is_dir()
                && e.depth() > 0
                && allow
                    .excluded_dir_names
                    .iter()
                    .any(|n| e.file_name().to_string_lossy() == n.as_str()))
        });
        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    out.push(Violation {
                        path: inc.clone(),
                        rule: format!("walk-error:{e}"),
                    });
                    continue;
                }
            };
            let rel = entry
                .path()
                .strip_prefix(repo_root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .into_owned();
            if entry.path_is_symlink() {
                out.push(Violation {
                    path: rel,
                    rule: "symlink".into(),
                });
            } else if entry.file_type().is_file() {
                files += 1;
                check_file(&rel, entry.path(), out);
            }
        }
    }
    files
}

/// Every root `[patch.*]` path target must be admitted by an include. Returns
/// the targets (empty when unreadable — already a violation).
fn check_patch_paths(repo_root: &Path, allow: &Allowlist, out: &mut Vec<Violation>) -> Vec<String> {
    let targets = read_root_manifest::<toml::Value>(repo_root)
        .map_err(|e| e.to_string())
        .and_then(|m| patch_path_targets(&m));
    let targets = match targets {
        Ok(t) => t,
        Err(e) => {
            out.push(Violation {
                path: "Cargo.toml".into(),
                rule: format!("patch-manifest-unreadable:{e}"),
            });
            return Vec::new();
        }
    };
    let paths = targets.iter().map(|t| t.path.clone()).collect();
    for t in targets {
        let admitted = allow
            .includes
            .iter()
            .any(|inc| t.path == *inc || t.path.starts_with(&format!("{inc}/")));
        if !admitted {
            out.push(Violation {
                path: t.path,
                rule: "patch-path-not-admitted".into(),
            });
        }
    }
    paths
}

/// The context paths a `COPY`/`ADD` brings into its stage (never `--from=`).
fn copy_sources(line: &str) -> Option<Vec<String>> {
    let mut words = line.split_whitespace();
    let op = words.next()?.to_ascii_uppercase();
    if op != "COPY" && op != "ADD" {
        return None;
    }
    let args: Vec<&str> = words.collect();
    if args.iter().any(|a| a.starts_with("--from")) {
        return None;
    }
    let operands: Vec<&str> = args.into_iter().filter(|a| !a.starts_with("--")).collect();
    let (_, sources) = operands.split_last()?;
    Some(
        sources
            .iter()
            .map(|s| s.trim_start_matches("./").trim_end_matches('/').to_string())
            .collect(),
    )
}

/// True iff a COPY source brings `path` (repo-relative) into the stage.
fn covers(source: &str, path: &str) -> bool {
    source.is_empty() || source == "." || path == source || path.starts_with(&format!("{source}/"))
}

/// In `dockerfile`'s text, every stage running `cargo chef cook` must COPY each
/// of `patch_paths` before its cook line.
/// (uncovered paths, number of cook lines seen).
fn scan_cook_stages(dockerfile: &str, patch_paths: &[String]) -> (Vec<String>, usize) {
    let mut cooks = 0;
    let mut missing = Vec::new();
    let mut copied: Vec<String> = Vec::new();
    for line in dockerfile.lines().map(str::trim) {
        if line.starts_with('#') {
            continue;
        }
        if line.to_ascii_uppercase().starts_with("FROM ") {
            copied.clear();
            continue;
        }
        if let Some(sources) = copy_sources(line) {
            copied.extend(sources);
            continue;
        }
        if line.contains("chef cook") {
            cooks += 1;
            for p in patch_paths {
                if !copied.iter().any(|s| covers(s, p)) && !missing.contains(p) {
                    missing.push(p.clone());
                }
            }
        }
    }
    (missing, cooks)
}

/// Every first-party Dockerfile that cooks must bring the `[patch.*]` paths in
/// before the cook.
fn check_cook_stages(repo_root: &Path, patch_paths: &[String], out: &mut Vec<Violation>) {
    if patch_paths.is_empty() {
        return;
    }
    let entries = match std::fs::read_dir(repo_root.join("infra/docker")) {
        Ok(entries) => entries,
        Err(e) => {
            out.push(Violation {
                path: "infra/docker".into(),
                rule: format!("cook-stage-scan-unreadable:{e}"),
            });
            return;
        }
    };
    let mut cooks = 0;
    let mut dockerfiles: Vec<_> = entries
        .flatten()
        .map(|e| e.path().join("Dockerfile"))
        .filter(|p| p.is_file())
        .collect();
    dockerfiles.sort();
    for df in dockerfiles {
        let rel = df
            .strip_prefix(repo_root)
            .unwrap_or(&df)
            .to_string_lossy()
            .into_owned();
        let text = match std::fs::read_to_string(&df) {
            Ok(t) => t,
            Err(e) => {
                out.push(Violation {
                    path: rel,
                    rule: format!("unreadable:{e}"),
                });
                continue;
            }
        };
        let (missing, found) = scan_cook_stages(&text, patch_paths);
        cooks += found;
        for p in missing {
            out.push(Violation {
                path: rel.clone(),
                rule: format!("patch-path-not-in-cook-stage:{p}"),
            });
        }
    }
    if cooks == 0 {
        out.push(Violation {
            path: "infra/docker".into(),
            rule: "patch-path-no-cook-stage-found".into(),
        });
    }
}

pub fn run(repo_root: &Path, _explain: bool) -> Result<()> {
    let ignore_path = repo_root.join(DOCKERIGNORE);
    let content = std::fs::read_to_string(&ignore_path).with_context(|| {
        format!(
            "docker-context-dockerignore-unreadable: {}",
            ignore_path.display()
        )
    })?;
    let (allow, mut violations) = parse_dockerignore(&content);
    let patch_paths = check_patch_paths(repo_root, &allow, &mut violations);
    check_cook_stages(repo_root, &patch_paths, &mut violations);
    let files = scan(repo_root, &allow, &mut violations);
    if violations.is_empty() {
        emit_ok(format!(
            "docker-context-clean-{}-includes-{files}-files",
            allow.includes.len()
        ));
        return Ok(());
    }
    for v in &violations {
        println!("VIOLATION: {} [{}]", v.path, v.rule);
    }
    anyhow::bail!("docker-context-violations-{}", violations.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_dialect_parses() {
        let (a, bad) = parse_dockerignore("# c\n*\n!Cargo.toml\n!crates/\n**/target/\n");
        assert!(bad.is_empty(), "{bad:?}");
        assert_eq!(a.includes, vec!["Cargo.toml", "crates"]);
        assert_eq!(a.excluded_dir_names, vec!["target"]);
    }

    #[test]
    fn denylist_style_is_rejected() {
        let (_, bad) = parse_dockerignore("target/\n.git/\n");
        assert!(bad.iter().any(|v| v.rule == "first-rule-must-be-star"));
        assert!(bad.iter().any(|v| v.rule == "not-an-allowlist"));
    }

    #[test]
    fn globs_and_escapes_are_rejected() {
        let (_, bad) = parse_dockerignore("*\n!crates/*\n!../x\n!/abs\n*.log\n**/a/b/\n");
        let rules: Vec<_> = bad.iter().map(|v| v.rule.as_str()).collect();
        assert_eq!(
            rules,
            vec![
                "include-not-literal-path",
                "include-not-literal-path",
                "include-not-literal-path",
                "unsupported-rule",
                "exclude-not-dir-name"
            ]
        );
    }

    #[test]
    fn name_and_content_rules_fire() {
        let dir =
            std::env::temp_dir().join(format!("dt-guard-docker-context-{}", std::process::id()));
        let inc = dir.join("ctx");
        std::fs::create_dir_all(inc.join("target")).unwrap();
        std::fs::write(inc.join("ok.rs"), "fn main() {}").unwrap();
        std::fs::write(inc.join("mh.key"), "x").unwrap();
        std::fs::write(inc.join("secret.yaml"), "a: b").unwrap();
        // Assembled at runtime so this file does not itself trip the guard.
        let pem = format!("-----BEGIN EC {}-----\nabc\n", ["PRIVATE", "KEY"].join(" "));
        std::fs::write(inc.join("notes.txt"), pem).unwrap();
        std::fs::write(inc.join("target").join("leak.pem"), "x").unwrap();
        let allow = Allowlist {
            includes: vec!["ctx".into()],
            excluded_dir_names: vec!["target".into()],
        };
        let mut out = Vec::new();
        let files = scan(&dir, &allow, &mut out);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(files, 4);
        let mut got: Vec<_> = out
            .iter()
            .map(|v| format!("{} {}", v.path, v.rule))
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                "ctx/mh.key name:key-container",
                "ctx/notes.txt content:pem-private-key",
                "ctx/secret.yaml name:secret-manifest",
            ]
        );
    }

    /// A fresh scratch root for one test (removed first, so a crashed earlier
    /// run cannot leak state into this one).
    fn scratch(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dt-guard-docker-context-{test}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rules(out: &[Violation]) -> Vec<String> {
        let mut got: Vec<_> = out
            .iter()
            .map(|v| format!("{} {}", v.path, v.rule))
            .collect();
        got.sort();
        got
    }

    #[test]
    fn run_passes_a_clean_context_and_fails_a_dirty_one() {
        let dir = scratch("run");
        std::fs::create_dir_all(dir.join("ctx")).unwrap();
        std::fs::write(dir.join("ctx").join("lib.rs"), "fn f() {}").unwrap();
        // No .dockerignore: unreadable is an error, never a pass.
        let missing = run(&dir, false).unwrap_err().to_string();
        assert!(
            missing.contains("docker-context-dockerignore-unreadable"),
            "{missing}"
        );

        std::fs::write(dir.join(DOCKERIGNORE), "*\n!ctx/\n").unwrap();
        // The root manifest is read too: unreadable is a violation, not a pass.
        let no_manifest = run(&dir, false).unwrap_err().to_string();
        assert_eq!(no_manifest, "docker-context-violations-1");
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\n").unwrap();
        assert!(run(&dir, false).is_ok());

        std::fs::write(dir.join("ctx").join("id_ed25519"), "x").unwrap();
        let dirty = run(&dir, false).unwrap_err().to_string();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(dirty, "docker-context-violations-1");
    }

    #[test]
    fn value_shape_secrets_are_flagged_in_data_files_not_source() {
        let dir = scratch("value-shape");
        let inc = dir.join("ctx");
        std::fs::create_dir_all(&inc).unwrap();
        // Assembled at runtime so this file does not itself trip secret scanners.
        let key = format!("key = \"{}{}\"\n", "AKIA", "ABCDEFGHIJKLMNOP");
        std::fs::write(inc.join("settings.toml"), &key).unwrap();
        std::fs::write(inc.join("fixture.rs"), &key).unwrap();
        let allow = Allowlist {
            includes: vec!["ctx".into()],
            excluded_dir_names: vec![],
        };
        let mut out = Vec::new();
        scan(&dir, &allow, &mut out);
        std::fs::remove_dir_all(&dir).unwrap();
        // .rs source belongs to rust-no-hardcoded-secrets; the data file is ours.
        assert_eq!(
            rules(&out),
            vec!["ctx/settings.toml content:AWS access key"]
        );
    }

    #[test]
    fn patch_path_targets_must_be_admitted() {
        let dir = scratch("patch-paths");
        std::fs::create_dir_all(dir.join("vendor").join("w")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\n[patch.crates-io]\nw = { path = \"vendor/w\" }\n",
        )
        .unwrap();
        // Negative: the vendored crate is outside every include.
        let allow = Allowlist {
            includes: vec!["crates".into()],
            excluded_dir_names: vec![],
        };
        let mut out = Vec::new();
        check_patch_paths(&dir, &allow, &mut out);
        assert_eq!(rules(&out), vec!["vendor/w patch-path-not-admitted"]);
        // Positive: admitted by the crate's own include, or by a parent one.
        for inc in ["vendor/w", "vendor"] {
            let allow = Allowlist {
                includes: vec![inc.into()],
                excluded_dir_names: vec![],
            };
            let mut out = Vec::new();
            check_patch_paths(&dir, &allow, &mut out);
            assert!(out.is_empty(), "{inc}: {:?}", rules(&out));
        }
        // A sibling prefix is not a parent (`vendor/wx` does not admit `vendor/w`).
        let allow = Allowlist {
            includes: vec!["vendor/wx".into()],
            excluded_dir_names: vec![],
        };
        let mut out = Vec::new();
        check_patch_paths(&dir, &allow, &mut out);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(rules(&out), vec!["vendor/w patch-path-not-admitted"]);
    }

    #[test]
    fn a_cook_stage_must_copy_every_patch_path_before_cooking() {
        let paths = vec!["vendor/w".to_string()];
        let base = "FROM chef AS planner\nCOPY . .\nRUN cargo chef prepare\n\
                    FROM chef AS builder\nCOPY --from=planner /build/recipe.json recipe.json\n";
        // Negative: the planner's `COPY . .` and a `--from=` copy do not count
        // for the builder stage; the cook there cannot see vendor/w.
        let bad = format!("{base}RUN cargo chef cook --recipe-path recipe.json\nCOPY . .\n");
        assert_eq!(
            scan_cook_stages(&bad, &paths).0,
            vec!["vendor/w".to_string()]
        );
        // A COPY AFTER the cook is too late.
        let late = format!("{base}RUN cargo chef cook\nCOPY vendor/ vendor/\n");
        assert_eq!(
            scan_cook_stages(&late, &paths).0,
            vec!["vendor/w".to_string()]
        );
        // Positive: the parent dir, the exact dir, or the whole context, before the cook.
        for copy in [
            "COPY vendor/ vendor/",
            "COPY vendor/w/ vendor/w/",
            "COPY . .",
        ] {
            let ok = format!("{base}{copy}\nRUN cargo chef cook\n");
            assert!(scan_cook_stages(&ok, &paths).0.is_empty(), "{copy}");
        }
        // A sibling prefix is not a parent.
        let sib = format!("{base}COPY vendor/wx/ vendor/wx/\nRUN cargo chef cook\n");
        assert_eq!(
            scan_cook_stages(&sib, &paths).0,
            vec!["vendor/w".to_string()]
        );
        // No patch paths, nothing to require.
        assert!(scan_cook_stages(&bad, &[]).0.is_empty());
        // `cargo-chef cook` is a cook line too.
        let hyphen = format!("{base}RUN cargo-chef cook\n");
        assert_eq!(
            scan_cook_stages(&hyphen, &paths),
            (vec!["vendor/w".to_string()], 1)
        );
    }

    #[test]
    fn the_cook_stage_rule_cannot_pass_vacuously() {
        let paths = vec!["vendor/w".to_string()];
        // No infra/docker at all: a violation, not a pass.
        let dir = scratch("cook-none");
        let mut out = Vec::new();
        check_cook_stages(&dir, &paths, &mut out);
        assert_eq!(out.len(), 1);
        assert!(
            out[0].rule.starts_with("cook-stage-scan-unreadable:"),
            "{}",
            out[0].rule
        );
        // Dockerfiles exist but none cooks (renamed/moved cook): a violation.
        std::fs::create_dir_all(dir.join("infra/docker/x")).unwrap();
        std::fs::write(dir.join("infra/docker/x/Dockerfile"), "FROM a\nRUN make\n").unwrap();
        let mut out = Vec::new();
        check_cook_stages(&dir, &paths, &mut out);
        assert_eq!(
            rules(&out),
            vec!["infra/docker patch-path-no-cook-stage-found"]
        );
        // Positive control: a covered cook stage passes.
        std::fs::write(
            dir.join("infra/docker/x/Dockerfile"),
            "FROM a\nCOPY vendor/ vendor/\nRUN cargo chef cook\n",
        )
        .unwrap();
        let mut out = Vec::new();
        check_cook_stages(&dir, &paths, &mut out);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(out.is_empty(), "{:?}", rules(&out));
    }

    #[cfg(unix)]
    #[test]
    fn missing_includes_and_symlinks_are_violations() {
        let dir = scratch("missing-symlink");
        let inc = dir.join("ctx");
        std::fs::create_dir_all(&inc).unwrap();
        std::os::unix::fs::symlink("/etc/hostname", inc.join("link")).unwrap();
        let allow = Allowlist {
            includes: vec!["ctx".into(), "absent".into()],
            excluded_dir_names: vec![],
        };
        let mut out = Vec::new();
        scan(&dir, &allow, &mut out);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            rules(&out),
            vec!["absent include-missing", "ctx/link symlink"]
        );
    }

    /// Needs a non-root user: root reads mode-000 files regardless (CI runners
    /// and the devloop container run tests unprivileged).
    #[cfg(unix)]
    #[test]
    fn unreadable_files_and_directories_are_violations_not_skips() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("unreadable");
        let inc = dir.join("ctx");
        std::fs::create_dir_all(inc.join("locked")).unwrap();
        std::fs::write(inc.join("sealed.txt"), "x").unwrap();
        let none = std::fs::Permissions::from_mode(0o000);
        std::fs::set_permissions(inc.join("sealed.txt"), none.clone()).unwrap();
        std::fs::set_permissions(inc.join("locked"), none).unwrap();
        let allow = Allowlist {
            includes: vec!["ctx".into()],
            excluded_dir_names: vec![],
        };
        let mut out = Vec::new();
        scan(&dir, &allow, &mut out);
        let open = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(inc.join("locked"), open.clone()).unwrap();
        std::fs::set_permissions(inc.join("sealed.txt"), open).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let got = rules(&out);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got[0].starts_with("ctx walk-error:"), "{got:?}");
        assert!(got[1].starts_with("ctx/sealed.txt unreadable:"), "{got:?}");
    }
}
