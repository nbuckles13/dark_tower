//! `gsa-sync` subcommand — port of
//! `scripts/guards/simple/validate-gsa-sync.sh` (ADR-0024 §6.8 item #2).
//!
//! ## GSA enumeration mirrors (MUST update together):
//!   1. `docs/decisions/adr-0024-agent-teams-workflow.md` §6.4
//!   2. `.claude/skills/devloop/SKILL.md`
//!   3. `.claude/skills/devloop/review-protocol.md`
//!   4. `scripts/guards/simple/cross-boundary-ownership.yaml`
//!   5. **This module's `CANON` const below (the canonical fully-expanded list)**
//!
//! Per @operations Wave-2 flag (carries forward from bash
//! `validate-gsa-sync.sh:3-17,38`): the 5-mirror enumeration is documented
//! explicitly so a future contributor extending GSA via micro-debate is
//! pointed at all 5 places to update.

use crate::common::explain::{print_finding, Finding};
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use std::path::Path;

pub const CANON_MISSING_RULE_ID: &str = "canon_missing";
pub const COUNT_MISMATCH_RULE_ID: &str = "count_mismatch";
pub const STRAY_YAML_KEY_RULE_ID: &str = "stray_yaml_key";

/// Canonical GSA path list. Fully expanded. Update all five mirrors together.
const CANON: &[&str] = &[
    "proto/**",
    "proto-gen/**",
    "build.rs",
    "crates/media-protocol/**",
    "crates/common/src/jwt.rs",
    "crates/common/src/meeting_token.rs",
    "crates/common/src/token_manager.rs",
    "crates/common/src/secret.rs",
    "crates/common/src/webtransport/**",
    "crates/ac-service/src/jwks/**",
    "crates/ac-service/src/token/**",
    "crates/ac-service/src/crypto/**",
    "crates/ac-service/src/audit/**",
    "db/migrations/**",
];

/// YAML-only intersection-rule sub-paths per ADR-0003 §5.7.
const INTERSECTION_SUBPATHS: &[&str] = &["proto/dark_tower/internal/v1/internal.proto"];

const ADR_PATH: &str = "docs/decisions/adr-0024-agent-teams-workflow.md";
const SKILL_PATH: &str = ".claude/skills/devloop/SKILL.md";
const PROTOCOL_PATH: &str = ".claude/skills/devloop/review-protocol.md";
const YAML_PATH: &str = "scripts/guards/simple/cross-boundary-ownership.yaml";

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    detail: String,
}

/// Slice a markdown mirror's GSA enumeration section: starts at the anchor
/// comment ("Mirror of ADR-0024 §6.4" or "Source of truth for GSA
/// enumeration"), enters "list mode" on the first `- ` bullet, exits at the
/// first non-bullet non-blank line.
fn slice_markdown_section(content: &str) -> String {
    let mut armed = false;
    let mut in_list = false;
    let mut out = String::new();
    for line in content.lines() {
        if !armed {
            if line.contains("Mirror of ADR-0024 §6.4")
                || line.contains("Source of truth for GSA enumeration")
            {
                armed = true;
            }
            continue;
        }
        let trimmed_start = line.trim_start();
        if trimmed_start.starts_with("- ") {
            in_list = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_list && line.trim().is_empty() {
            continue;
        }
        if in_list {
            break;
        }
    }
    out
}

/// Shorthand forms a canon path may appear as in markdown.
fn shorthand_forms(canon_path: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![canon_path.to_string()];
    if let Some(stem) = canon_path.strip_suffix("/**") {
        // `foo/bar/baz/**` → `baz/**`.
        if let Some(last) = stem.rsplit('/').next() {
            if !last.is_empty() && last != stem {
                out.push(format!("{last}/**"));
            }
            // Two-segment: `bar/baz/**`.
            let mut segments: Vec<&str> = stem.split('/').collect();
            if segments.len() >= 2 {
                if let Some(last) = segments.pop() {
                    let second = segments.last().copied().unwrap_or("");
                    if !second.is_empty() {
                        out.push(format!("{second}/{last}/**"));
                    }
                }
            }
        }
    } else {
        // Bare basename.
        if let Some(basename) = canon_path.rsplit('/').next() {
            if basename != canon_path {
                out.push(basename.to_string());
            }
        }
    }
    out
}

fn check_markdown_mirror(label: &str, content: &str) -> Vec<Hit> {
    let mut hits: Vec<Hit> = Vec::new();
    let slice = slice_markdown_section(content);
    if slice.trim().is_empty() {
        hits.push(Hit {
            rule_id: CANON_MISSING_RULE_ID,
            detail: format!(
                "{label} enumeration section empty — anchor comment missing or structure changed"
            ),
        });
        return hits;
    }

    for &canon in CANON {
        let forms = shorthand_forms(canon);
        let matched = forms
            .iter()
            .any(|form| slice.contains(&format!("`{form}`")));
        if !matched {
            hits.push(Hit {
                rule_id: CANON_MISSING_RULE_ID,
                detail: format!(
                    "canon `{canon}` missing from {label} (tried: {})",
                    forms.join(", ")
                ),
            });
        }
    }

    // Count-check: markdown should have exactly len(CANON) backticked tokens.
    let count = slice.matches('`').count() / 2;
    if count != CANON.len() {
        hits.push(Hit {
            rule_id: COUNT_MISMATCH_RULE_ID,
            detail: format!(
                "{label} has {count} backticked paths, canon has {}",
                CANON.len()
            ),
        });
    }
    hits
}

fn check_yaml_mirror(content: &str) -> Vec<Hit> {
    let mut hits: Vec<Hit> = Vec::new();
    // Extract `"<key>":` lines.
    let mut yaml_keys: Vec<String> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        let Some(stripped) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some(end_q) = stripped.find('"') else {
            continue;
        };
        let key = &stripped[..end_q];
        let after = &stripped[end_q + 1..];
        if !after.trim_start().starts_with(':') {
            continue;
        }
        yaml_keys.push(key.to_string());
    }
    yaml_keys.sort();
    yaml_keys.dedup();

    // Every CANON entry must appear as a YAML key.
    for &canon in CANON {
        if !yaml_keys.iter().any(|k| k == canon) {
            hits.push(Hit {
                rule_id: CANON_MISSING_RULE_ID,
                detail: format!("canon `{canon}` missing from cross-boundary-ownership.yaml"),
            });
        }
    }
    // Every YAML key must be in CANON or INTERSECTION_SUBPATHS.
    for key in &yaml_keys {
        let allowed =
            CANON.iter().any(|c| c == key) || INTERSECTION_SUBPATHS.iter().any(|s| s == key);
        if !allowed {
            hits.push(Hit {
                rule_id: STRAY_YAML_KEY_RULE_ID,
                detail: format!("cross-boundary-ownership.yaml has stray key `{key}` — not in CANON or INTERSECTION_SUBPATHS"),
            });
        }
    }
    hits
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let mut all_hits: Vec<Hit> = Vec::new();

    let mirrors = [
        ("ADR-0024 §6.4", ADR_PATH),
        ("SKILL.md", SKILL_PATH),
        ("review-protocol.md", PROTOCOL_PATH),
    ];

    for (label, path) in &mirrors {
        let abs = repo_root.join(path);
        if !abs.is_file() {
            all_hits.push(Hit {
                rule_id: CANON_MISSING_RULE_ID,
                detail: format!("mirror file missing: {path}"),
            });
            continue;
        }
        let content =
            std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
        all_hits.extend(check_markdown_mirror(label, &content));
    }

    let yaml_abs = repo_root.join(YAML_PATH);
    if !yaml_abs.is_file() {
        all_hits.push(Hit {
            rule_id: CANON_MISSING_RULE_ID,
            detail: format!("mirror file missing: {YAML_PATH}"),
        });
    } else {
        let content = std::fs::read_to_string(&yaml_abs)
            .with_context(|| format!("reading {}", yaml_abs.display()))?;
        all_hits.extend(check_yaml_mirror(&content));
    }

    if all_hits.is_empty() {
        emit_ok("gsa-sync-all-5-mirrors-in-sync");
        return Ok(());
    }

    for hit in &all_hits {
        if explain {
            let policy = format!("gsa-sync::{}", hit.rule_id);
            print_finding(&Finding {
                file: "gsa-sync",
                row: 0,
                col: 0,
                policy: &policy,
                matched: &hit.detail,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!("VIOLATION: [{}] {}", hit.rule_id, hit.detail);
        }
    }

    // Dominant-class fold: ONE REASON token names the largest finding class
    // (per @test F3 2026-05-23 — deliberate operator-affordance, not a bug).
    // Per-finding detail is still emitted via VIOLATION lines; the wire token
    // points the runbook to the most-frequent rule.
    let canon_missing = all_hits
        .iter()
        .filter(|h| h.rule_id == CANON_MISSING_RULE_ID)
        .count();
    let count_mismatch = all_hits
        .iter()
        .filter(|h| h.rule_id == COUNT_MISMATCH_RULE_ID)
        .count();
    let stray_yaml = all_hits
        .iter()
        .filter(|h| h.rule_id == STRAY_YAML_KEY_RULE_ID)
        .count();
    let token = if canon_missing >= count_mismatch && canon_missing >= stray_yaml {
        "canon-missing"
    } else if count_mismatch >= stray_yaml {
        "count-mismatch"
    } else {
        "stray-yaml-key"
    };
    anyhow::bail!("gsa-sync-{token}-{}", all_hits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_forms_for_starstar_path() {
        let forms = shorthand_forms("crates/ac-service/src/jwks/**");
        // Should include the full form + `jwks/**` last-dir + `src/jwks/**` two-segment.
        assert!(forms.contains(&"crates/ac-service/src/jwks/**".to_string()));
        assert!(forms.contains(&"jwks/**".to_string()));
        assert!(forms.contains(&"src/jwks/**".to_string()));
    }

    #[test]
    fn shorthand_forms_for_file_path() {
        let forms = shorthand_forms("crates/common/src/jwt.rs");
        assert!(forms.contains(&"crates/common/src/jwt.rs".to_string()));
        assert!(forms.contains(&"jwt.rs".to_string()));
    }

    #[test]
    fn slice_markdown_section_extracts_bullet_list() {
        let md = "Some prose.\n<!-- Mirror of ADR-0024 §6.4 -->\nMore prose.\n\n- `proto/**`\n- `build.rs`\n\nAfter the list.\n";
        let slice = slice_markdown_section(md);
        assert!(slice.contains("`proto/**`"));
        assert!(slice.contains("`build.rs`"));
        assert!(!slice.contains("After the list"));
    }

    #[test]
    fn check_yaml_mirror_flags_canon_missing() {
        let yaml = r#"# header
"build.rs": [protocol]
"#;
        let hits = check_yaml_mirror(yaml);
        // Many canons missing; just verify at least one was flagged.
        assert!(hits.iter().any(|h| h.rule_id == CANON_MISSING_RULE_ID));
    }

    #[test]
    fn check_yaml_mirror_flags_stray_key() {
        // All canons present + 1 stray.
        let mut yaml = String::new();
        for c in CANON {
            yaml.push_str(&format!("\"{c}\": [protocol]\n"));
        }
        yaml.push_str("\"crates/typod/**\": [protocol]\n");
        let hits = check_yaml_mirror(&yaml);
        assert!(hits.iter().any(|h| h.rule_id == STRAY_YAML_KEY_RULE_ID));
    }

    #[test]
    fn check_yaml_mirror_allows_intersection_subpaths() {
        let mut yaml = String::new();
        for c in CANON {
            yaml.push_str(&format!("\"{c}\": [protocol]\n"));
        }
        for s in INTERSECTION_SUBPATHS {
            yaml.push_str(&format!("\"{s}\": [protocol]\n"));
        }
        let hits = check_yaml_mirror(&yaml);
        assert!(
            !hits.iter().any(|h| h.rule_id == STRAY_YAML_KEY_RULE_ID),
            "INTERSECTION_SUBPATHS should not be flagged as stray: {hits:?}"
        );
    }

    #[test]
    fn check_markdown_mirror_clean_when_all_canon_present() {
        let mut md = String::from("<!-- Mirror of ADR-0024 §6.4 -->\n\n");
        for c in CANON {
            md.push_str(&format!("- `{c}`\n"));
        }
        md.push_str("\nAfter.\n");
        let hits = check_markdown_mirror("test-mirror", &md);
        assert!(hits.is_empty(), "expected no hits, got: {hits:?}");
    }

    #[test]
    fn check_markdown_mirror_count_mismatch_when_extra_token() {
        // All canons present, plus one extra backticked token.
        let mut md = String::from("<!-- Mirror of ADR-0024 §6.4 -->\n\n");
        for c in CANON {
            md.push_str(&format!("- `{c}`\n"));
        }
        md.push_str("- `extra/path`\n");
        let hits = check_markdown_mirror("test-mirror", &md);
        assert!(hits.iter().any(|h| h.rule_id == COUNT_MISMATCH_RULE_ID));
    }

    #[test]
    fn check_markdown_mirror_shorthand_basename_matches() {
        // Use basename form for the file-path entries.
        let mut md = String::from("<!-- Mirror of ADR-0024 §6.4 -->\n\n");
        for c in CANON {
            let display = if c.ends_with("/**") {
                c.to_string()
            } else if let Some(basename) = c.rsplit('/').next() {
                basename.to_string()
            } else {
                c.to_string()
            };
            md.push_str(&format!("- `{display}`\n"));
        }
        let hits = check_markdown_mirror("test-mirror", &md);
        assert!(hits.is_empty(), "shorthand basename should match: {hits:?}");
    }
}
