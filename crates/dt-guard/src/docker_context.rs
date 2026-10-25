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

pub fn run(repo_root: &Path, _explain: bool) -> Result<()> {
    let ignore_path = repo_root.join(DOCKERIGNORE);
    let content = std::fs::read_to_string(&ignore_path).with_context(|| {
        format!(
            "docker-context-dockerignore-unreadable: {}",
            ignore_path.display()
        )
    })?;
    let (allow, mut violations) = parse_dockerignore(&content);
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
}
