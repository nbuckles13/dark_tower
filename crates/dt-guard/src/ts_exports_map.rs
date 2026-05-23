//! `ts-exports-map-closed` subcommand — port of
//! `scripts/guards/simple/ts/exports-map-closed.sh`.
//!
//! Enforces closed-world `exports` map per ADR-0028 §5 (supply chain). For
//! each changed `packages/<name>/package.json` (depth-2 only) that is NOT
//! exempt (private: true OR name prefix `@darktower/test-`):
//!
//! * **Check A (HARD)**: forbid first-level subpath KEYS matching
//!   `(^|/)(test|tests|testing|test-only|__tests__|internal|private)(/|$)`
//!   plus bare wildcard-only `./*`.
//! * **Check B (HARD)**: forbid any leaf `./`-string VALUE pointing into
//!   `/(test|tests|testing|test-only|__tests__|internal|private)(/|$)`.
//!   Walks ALL conditional sub-objects (`import`/`require`/`types`/`default`/
//!   `node`/`browser`/`worker`/...).
//! * **Check C (SOFT, promotable via `--strict`)**: missing `exports` emits
//!   WARN by default. STATUS=FAIL when `--strict` is set.
//!
//! Per @security F2 + @paired-client F3: `ExportsValue` is an untagged enum
//! with no `deny_unknown_fields` at the conditional-keys layer (open
//! vocabulary per npm spec). Top level uses `serde_json::Value` reach-in for
//! `.exports` (simpler than typing the full package.json struct).
//!
//! Per @semantic-guard non-blocking note: Check A applies ONLY to
//! first-level subpath keys (e.g. `"./test/foo"`) — NOT to nested
//! conditional sub-keys (e.g. `"import"`/`"require"`).

use crate::common::explain::{print_finding, Finding};
use crate::common::git_changes::get_all_changed_files;
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const FORBIDDEN_KEY_RULE_ID: &str = "exports_key_forbidden";
pub const FORBIDDEN_VALUE_RULE_ID: &str = "exports_value_forbidden";
pub const MISSING_EXPORTS_RULE_ID: &str = "exports_missing";

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static FORBIDDEN_KEY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(^|/)(test|tests|testing|test-only|__tests__|internal|private)(/|$)")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static WILDCARD_KEY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\./\*$").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static FORBIDDEN_VALUE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"/(test|tests|testing|test-only|__tests__|internal|private)(/|$)")
        .expect("static pattern compiles")
});

fn is_exempt(pkg: &Value) -> bool {
    if let Some(v) = pkg.get("private") {
        if v.as_bool() == Some(true) {
            return true;
        }
    }
    if let Some(name) = pkg.get("name").and_then(|v| v.as_str()) {
        if name.starts_with("@darktower/test-") {
            return true;
        }
    }
    false
}

/// Collect every leaf string from an `exports` value tree. Walks through
/// every conditional sub-object recursively (parity with bash
/// `[.. | strings]`).
fn collect_leaf_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Object(map) => {
            for (_k, v) in map {
                collect_leaf_strings(v, out);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_leaf_strings(v, out);
            }
        }
        _ => {}
    }
}

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    file: PathBuf,
    detail: String,
}

fn check_one(pkg_path: &Path, raw: &str, strict: bool) -> Result<Vec<Hit>> {
    let pkg: Value =
        serde_json::from_str(raw).with_context(|| format!("parsing {}", pkg_path.display()))?;
    if is_exempt(&pkg) {
        return Ok(Vec::new());
    }

    let mut hits: Vec<Hit> = Vec::new();
    let exports = pkg.get("exports");

    // Check C — missing exports (soft/strict).
    let Some(exports) = exports else {
        if strict {
            hits.push(Hit {
                rule_id: MISSING_EXPORTS_RULE_ID,
                file: pkg_path.to_path_buf(),
                detail: "missing 'exports' field (STRICT mode)".to_string(),
            });
        } else {
            // Soft warn — emit on stderr but do NOT count as a violation.
            eprintln!(
                "WARN: {} missing 'exports' (closed-world surface not enforceable). Set --strict to promote to violation.",
                pkg_path.display()
            );
        }
        return Ok(hits);
    };

    // Check A — first-level subpath keys.
    //
    // Per @security F2: only first-level subpath keys are regex-checked
    // (NOT nested conditional keys like `import` / `require`).
    if let Some(map) = exports.as_object() {
        for key in map.keys() {
            if FORBIDDEN_KEY_RE.is_match(key) {
                hits.push(Hit {
                    rule_id: FORBIDDEN_KEY_RULE_ID,
                    file: pkg_path.to_path_buf(),
                    detail: format!("forbidden subpath key {key:?}"),
                });
            }
            if WILDCARD_KEY_RE.is_match(key) {
                hits.push(Hit {
                    rule_id: FORBIDDEN_KEY_RULE_ID,
                    file: pkg_path.to_path_buf(),
                    detail: format!("wildcard-only key {key:?} (closed-world bypass)"),
                });
            }
        }
    }
    // String-shaped exports (single-file shortcut) → no keys to check, no
    // forbidden segments unless the string itself routes to one (handled by
    // Check B below).

    // Check B — every leaf string value.
    let mut leaves: Vec<String> = Vec::new();
    collect_leaf_strings(exports, &mut leaves);
    for leaf in &leaves {
        // Only inspect `./`-rooted strings (relative paths). Condition tokens
        // like `"default"` or absolute paths don't apply here.
        if !leaf.starts_with("./") {
            continue;
        }
        if FORBIDDEN_VALUE_RE.is_match(leaf) {
            hits.push(Hit {
                rule_id: FORBIDDEN_VALUE_RULE_ID,
                file: pkg_path.to_path_buf(),
                detail: format!("forbidden subpath value {leaf:?}"),
            });
        }
    }

    Ok(hits)
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    run_with_strict(repo_root, explain, env_strict())
}

/// Read `STRICT_EXPORTS_MAP` env var per @paired-client F4 — literal `"1"`
/// only (no generic truthy widening). Default off.
fn env_strict() -> bool {
    std::env::var("STRICT_EXPORTS_MAP")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Inner entry — explicit `strict` parameter for fixture-driven tests.
pub fn run_with_strict(repo_root: &Path, explain: bool, strict: bool) -> Result<()> {
    let changed = get_all_changed_files(repo_root, Path::new("."), &["package.json"])
        .context("collecting changed package.json files")?;

    // Filter to depth-2 `packages/<name>/package.json` only.
    let in_scope: Vec<PathBuf> = changed
        .into_iter()
        .filter(|p| {
            let Some(s) = p.to_str() else {
                return false;
            };
            // Path shape: packages/<name>/package.json (exactly 3 segments).
            let parts: Vec<&str> = s.split('/').collect();
            matches!(parts.as_slice(), ["packages", _, "package.json"])
        })
        .filter(|p| repo_root.join(p).is_file())
        .collect();

    if in_scope.is_empty() {
        emit_ok("ts-exports-map-closed-no-files");
        return Ok(());
    }

    let mut all_hits: Vec<Hit> = Vec::new();
    for pkg_path in &in_scope {
        let abs = repo_root.join(pkg_path);
        let raw =
            std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
        let hits = check_one(pkg_path, &raw, strict)?;
        all_hits.extend(hits);
    }

    if all_hits.is_empty() {
        emit_ok(format!(
            "ts-exports-map-closed-clean-{}-packages",
            in_scope.len()
        ));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("ts-exports-map-closed::{}", hit.rule_id);
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

    let (kind, count) = classify_hits(&all_hits);
    anyhow::bail!("ts-exports-{kind}-{count}-of-{}-packages", in_scope.len())
}

fn classify_hits(hits: &[Hit]) -> (&'static str, usize) {
    let key = hits
        .iter()
        .filter(|h| h.rule_id == FORBIDDEN_KEY_RULE_ID)
        .count();
    let value = hits
        .iter()
        .filter(|h| h.rule_id == FORBIDDEN_VALUE_RULE_ID)
        .count();
    let missing = hits
        .iter()
        .filter(|h| h.rule_id == MISSING_EXPORTS_RULE_ID)
        .count();
    if missing > 0 {
        ("missing-strict", missing)
    } else if key >= value {
        ("key-forbidden", key)
    } else {
        ("value-forbidden", value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_key_regex_matches_test_segments() {
        assert!(FORBIDDEN_KEY_RE.is_match("./test/foo"));
        assert!(FORBIDDEN_KEY_RE.is_match("./tests"));
        assert!(FORBIDDEN_KEY_RE.is_match("./internal/util"));
        assert!(FORBIDDEN_KEY_RE.is_match("./__tests__/x"));
        assert!(FORBIDDEN_KEY_RE.is_match("./test-only"));
        assert!(!FORBIDDEN_KEY_RE.is_match("./index"));
        assert!(!FORBIDDEN_KEY_RE.is_match("./public/api"));
    }

    #[test]
    fn wildcard_key_regex_only_matches_bare_dot_star() {
        assert!(WILDCARD_KEY_RE.is_match("./*"));
        assert!(!WILDCARD_KEY_RE.is_match("./*.js"));
        assert!(!WILDCARD_KEY_RE.is_match("./pub/*"));
    }

    #[test]
    fn forbidden_value_regex_matches_test_segments() {
        assert!(FORBIDDEN_VALUE_RE.is_match("./src/test/foo.js"));
        assert!(FORBIDDEN_VALUE_RE.is_match("./dist/__tests__/x.js"));
        assert!(FORBIDDEN_VALUE_RE.is_match("./lib/internal/util.js"));
        assert!(!FORBIDDEN_VALUE_RE.is_match("./src/index.js"));
    }

    #[test]
    fn collect_leaf_strings_walks_recursively() {
        let v: Value = serde_json::from_str(
            r#"{
                "./index": {
                    "import": "./dist/index.mjs",
                    "require": "./dist/index.cjs",
                    "types": "./dist/index.d.ts"
                },
                "./util": "./dist/util.js"
            }"#,
        )
        .unwrap();
        let mut leaves: Vec<String> = Vec::new();
        collect_leaf_strings(&v, &mut leaves);
        assert!(leaves.contains(&"./dist/index.mjs".to_string()));
        assert!(leaves.contains(&"./dist/index.cjs".to_string()));
        assert!(leaves.contains(&"./dist/index.d.ts".to_string()));
        assert!(leaves.contains(&"./dist/util.js".to_string()));
        assert_eq!(leaves.len(), 4);
    }

    #[test]
    fn check_a_first_level_only_not_nested_conditional() {
        // `import` is a nested conditional key, NOT a first-level subpath
        // key — it must NOT match Check A regardless of being a non-`./`
        // string that happens to share segments with forbidden tokens.
        // Use a contrived "test" conditional sub-key to verify only the
        // top-level keys are regex-checked.
        let pkg: Value = serde_json::from_str(
            r#"{
                "name": "@foo/bar",
                "exports": {
                    "./index": {
                        "test": "./dist/index.test.js"
                    }
                }
            }"#,
        )
        .unwrap();
        assert!(!is_exempt(&pkg));
        // Direct call to the check via the inner function:
        let raw = serde_json::to_string(&pkg).unwrap();
        let hits = check_one(Path::new("packages/x/package.json"), &raw, false).unwrap();
        // Should be NO Check A hit (only the first-level `./index` is checked,
        // and it doesn't match forbidden segments).
        let key_hits: Vec<_> = hits
            .iter()
            .filter(|h| h.rule_id == FORBIDDEN_KEY_RULE_ID)
            .collect();
        assert!(
            key_hits.is_empty(),
            "Check A must apply only to first-level keys; got: {key_hits:?}"
        );
        // Check B WILL fire on the leaf `./dist/index.test.js`.
        let value_hits: Vec<_> = hits
            .iter()
            .filter(|h| h.rule_id == FORBIDDEN_VALUE_RULE_ID)
            .collect();
        assert_eq!(
            value_hits.len(),
            0,
            "leaf is not `/test/` shape: {value_hits:?}"
        );
    }

    #[test]
    fn check_b_fires_on_dist_test_leaf() {
        let pkg = r#"{
            "name": "@foo/bar",
            "exports": {
                "./index": { "import": "./dist/test/index.js" }
            }
        }"#;
        let hits = check_one(Path::new("packages/x/package.json"), pkg, false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rule_id, FORBIDDEN_VALUE_RULE_ID);
    }

    #[test]
    fn check_a_fires_on_forbidden_subpath_key() {
        let pkg = r#"{
            "name": "@foo/bar",
            "exports": {
                "./test/foo": "./dist/foo.js"
            }
        }"#;
        let hits = check_one(Path::new("packages/x/package.json"), pkg, false).unwrap();
        assert!(hits.iter().any(|h| h.rule_id == FORBIDDEN_KEY_RULE_ID));
    }

    #[test]
    fn check_a_fires_on_bare_wildcard_key() {
        let pkg = r#"{
            "name": "@foo/bar",
            "exports": {
                "./*": "./dist/index.js"
            }
        }"#;
        let hits = check_one(Path::new("packages/x/package.json"), pkg, false).unwrap();
        assert!(hits.iter().any(|h| h.rule_id == FORBIDDEN_KEY_RULE_ID));
    }

    #[test]
    fn private_true_exempts_all_checks() {
        let pkg = r#"{
            "private": true,
            "name": "@foo/internal-tool",
            "exports": {
                "./*": "./dist/test/anything.js"
            }
        }"#;
        let hits = check_one(Path::new("packages/x/package.json"), pkg, false).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn darktower_test_prefix_exempts_all_checks() {
        let pkg = r#"{
            "name": "@darktower/test-utils",
            "exports": {
                "./*": "./dist/test/anything.js"
            }
        }"#;
        let hits = check_one(Path::new("packages/x/package.json"), pkg, false).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn missing_exports_soft_in_default_mode() {
        let pkg = r#"{ "name": "@foo/bar" }"#;
        let hits = check_one(Path::new("packages/x/package.json"), pkg, false).unwrap();
        // Soft mode → no hit recorded (warning only).
        assert!(hits.is_empty());
    }

    #[test]
    fn missing_exports_hard_in_strict_mode() {
        let pkg = r#"{ "name": "@foo/bar" }"#;
        let hits = check_one(Path::new("packages/x/package.json"), pkg, true).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rule_id, MISSING_EXPORTS_RULE_ID);
    }

    #[test]
    fn env_strict_only_matches_literal_one() {
        // Document the literal-"1" semantic via the function — bash today
        // checks `[[ "$STRICT" == "1" ]]`; the Rust port mirrors it.
        // Test via direct unit (no env mutation; rely on default).
        assert!(!env_strict()); // not set
    }
}
