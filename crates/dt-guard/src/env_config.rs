//! `env-config` subcommand — port of
//! `scripts/guards/simple/validate-env-config.sh`.
//!
//! Validates consistency between Rust service config and K8s manifests:
//! 1. Required env vars in `crates/<svc>/src/config.rs` (extracted from
//!    `MissingEnvVar("VAR")`) are provided in the workload manifest
//!    (`deployment.yaml` or `statefulset.yaml`).
//! 2. `configMapKeyRef` keys in the workload manifest exist in the
//!    corresponding `configmap.yaml`.
//! 3. configmap keys are referenced by at least one workload env var.

use crate::common::explain::{print_finding, Finding};
use crate::common::scan::warn_skip;
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

pub const MISSING_IN_MANIFEST_RULE_ID: &str = "missing_in_manifest";
pub const KEY_NOT_IN_CONFIGMAP_RULE_ID: &str = "key_not_in_configmap";
pub const ORPHAN_CONFIGMAP_KEY_RULE_ID: &str = "orphan_configmap_key";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MISSING_ENV_VAR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"MissingEnvVar\("([A-Z_][A-Z0-9_]*)""#).expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static WORKLOAD_ENV_NAME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s+-\s+name:\s+([A-Z_][A-Z0-9_]*)").expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static CONFIGMAP_KEY_REF_RE: Lazy<Regex> = Lazy::new(|| {
    // Matches the `key: NAME` line inside a `configMapKeyRef:` block.
    // We approximate by finding `configMapKeyRef` then `key:` within 2 lines.
    Regex::new(r"(?m)^\s+key:\s+(\S+)").expect("static pattern compiles")
});

fn find_workload(infra_dir: &Path) -> Option<PathBuf> {
    let d = infra_dir.join("deployment.yaml");
    if d.is_file() {
        return Some(d);
    }
    let s = infra_dir.join("statefulset.yaml");
    if s.is_file() {
        return Some(s);
    }
    None
}

fn extract_required_env_vars(content: &str) -> Vec<String> {
    let mut out: Vec<String> = MISSING_ENV_VAR_RE
        .captures_iter(content)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn extract_workload_env_names(content: &str) -> Vec<String> {
    let mut out: Vec<String> = WORKLOAD_ENV_NAME_RE
        .captures_iter(content)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn extract_workload_configmap_key_refs(content: &str) -> Vec<String> {
    // For each `configMapKeyRef` occurrence, capture the `key:` value within
    // the next 3 lines. Simple line-based walk.
    let mut out: Vec<String> = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        if !line.contains("configMapKeyRef") {
            continue;
        }
        let end = (idx + 4).min(lines.len());
        for next in lines.get(idx + 1..end).unwrap_or(&[]) {
            if let Some(caps) = CONFIGMAP_KEY_REF_RE.captures(next) {
                if let Some(m) = caps.get(1) {
                    out.push(m.as_str().to_string());
                    break;
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn extract_configmap_data_keys(content: &str) -> Vec<String> {
    // Walk into the `data:` block and capture top-level keys (lines like
    // `  SOME_KEY: ...`).
    let mut out: Vec<String> = Vec::new();
    let mut in_data = false;
    for line in content.lines() {
        if line.trim_start() == "data:" || line.starts_with("data:") {
            in_data = true;
            continue;
        }
        // A non-indented line (start of new top-level key in the YAML doc)
        // exits the data section.
        if in_data
            && !line.is_empty()
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !line.starts_with('-')
        {
            in_data = false;
            continue;
        }
        if !in_data {
            continue;
        }
        // Match `  KEY_NAME:` (indented, uppercase identifier, colon).
        let trimmed = line.trim_start();
        if let Some(colon_idx) = trimmed.find(':') {
            let key = &trimmed[..colon_idx];
            if !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                out.push(key.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    detail: String,
    file: PathBuf,
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let mut all_hits: Vec<Hit> = Vec::new();
    let mut checked_services = 0usize;

    for (_, dir) in crate::common::services::CANONICAL_SERVICES {
        let config_rs = repo_root.join("crates").join(dir).join("src/config.rs");
        let infra_dir = repo_root.join("infra/services").join(dir);
        if !config_rs.is_file() || !infra_dir.is_dir() {
            continue;
        }
        checked_services += 1;

        let Some(workload) = find_workload(&infra_dir) else {
            warn_skip(
                "no workload manifest",
                &infra_dir,
                &std::io::Error::other("no deployment.yaml or statefulset.yaml"),
            );
            continue;
        };
        let configmap = infra_dir.join("configmap.yaml");

        let config_content = std::fs::read_to_string(&config_rs)
            .with_context(|| format!("reading {}", config_rs.display()))?;
        let workload_content = std::fs::read_to_string(&workload)
            .with_context(|| format!("reading {}", workload.display()))?;
        let configmap_content = if configmap.is_file() {
            std::fs::read_to_string(&configmap)
                .with_context(|| format!("reading {}", configmap.display()))?
        } else {
            String::new()
        };

        let required = extract_required_env_vars(&config_content);
        let declared = extract_workload_env_names(&workload_content);
        let referenced_keys = extract_workload_configmap_key_refs(&workload_content);
        let configmap_keys = extract_configmap_data_keys(&configmap_content);

        // Check 1: required env vars present in workload.
        for var in &required {
            if !declared.contains(var) {
                all_hits.push(Hit {
                    rule_id: MISSING_IN_MANIFEST_RULE_ID,
                    detail: format!("{dir}: config.rs requires {var}, missing in workload"),
                    file: workload
                        .strip_prefix(repo_root)
                        .unwrap_or(&workload)
                        .to_path_buf(),
                });
            }
        }

        // Check 2: configMapKeyRef keys exist in configmap.
        if configmap.is_file() {
            for key in &referenced_keys {
                if !configmap_keys.contains(key) {
                    all_hits.push(Hit {
                        rule_id: KEY_NOT_IN_CONFIGMAP_RULE_ID,
                        detail: format!("{dir}: workload refs configMapKeyRef key {key:?} not in configmap.yaml"),
                        file: workload.strip_prefix(repo_root).unwrap_or(&workload).to_path_buf(),
                    });
                }
            }

            // Check 3: configmap keys referenced by workload.
            for key in &configmap_keys {
                if !referenced_keys.contains(key) {
                    all_hits.push(Hit {
                        rule_id: ORPHAN_CONFIGMAP_KEY_RULE_ID,
                        detail: format!("{dir}: configmap key {key:?} not referenced by workload"),
                        file: configmap
                            .strip_prefix(repo_root)
                            .unwrap_or(&configmap)
                            .to_path_buf(),
                    });
                }
            }
        } else if !referenced_keys.is_empty() {
            all_hits.push(Hit {
                rule_id: KEY_NOT_IN_CONFIGMAP_RULE_ID,
                detail: format!(
                    "{dir}: workload references configMapKeyRef but no configmap.yaml exists"
                ),
                file: workload
                    .strip_prefix(repo_root)
                    .unwrap_or(&workload)
                    .to_path_buf(),
            });
        }
    }

    if checked_services == 0 {
        emit_ok("env-config-no-services");
        return Ok(());
    }

    if all_hits.is_empty() {
        emit_ok(format!("env-config-clean-{checked_services}-services"));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
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
        } else {
            println!("VIOLATION: {} [{}] {}", file_disp, hit.rule_id, hit.detail);
        }
    }

    let missing = all_hits
        .iter()
        .filter(|h| h.rule_id == MISSING_IN_MANIFEST_RULE_ID)
        .count();
    let key_miss = all_hits
        .iter()
        .filter(|h| h.rule_id == KEY_NOT_IN_CONFIGMAP_RULE_ID)
        .count();
    let orphan = all_hits
        .iter()
        .filter(|h| h.rule_id == ORPHAN_CONFIGMAP_KEY_RULE_ID)
        .count();
    let kind = if missing >= key_miss && missing >= orphan {
        "missing-in-manifest"
    } else if key_miss >= orphan {
        "key-not-in-configmap"
    } else {
        "orphan-key"
    };
    anyhow::bail!("env-config-{kind}-{}", all_hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn extract_workload_env_names_finds_name_lines() {
        let content = r#"
            env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: db
            - name: JWT_SECRET
              value: "x"
        "#;
        let names = extract_workload_env_names(content);
        assert!(names.contains(&"DATABASE_URL".to_string()));
        assert!(names.contains(&"JWT_SECRET".to_string()));
    }

    #[test]
    fn extract_configmap_data_keys_collects_top_level() {
        let content = r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: cm
data:
  LOG_LEVEL: info
  METRICS_PORT: "9090"
"#;
        let keys = extract_configmap_data_keys(content);
        assert!(keys.contains(&"LOG_LEVEL".to_string()));
        assert!(keys.contains(&"METRICS_PORT".to_string()));
    }

    #[test]
    fn extract_configmap_key_refs_walks_to_key_line() {
        let content = r#"
            env:
            - name: LOG_LEVEL
              valueFrom:
                configMapKeyRef:
                  name: cm
                  key: LOG_LEVEL
        "#;
        let keys = extract_workload_configmap_key_refs(content);
        assert!(keys.contains(&"LOG_LEVEL".to_string()));
    }
}
