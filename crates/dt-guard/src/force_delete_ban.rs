//! `force-delete-ban` subcommand — ADR-0039 §9, R-28 (story 4 task I-3).
//!
//! A zero-grace force delete removes a pod's API object while its process may
//! still run, so the scheduler can start a second MH on the same hostPort slot:
//! the ONE way to defeat the slot lock (one live process per client-facing
//! address). Validation fails on any command, in tooling, tests or demos, that
//! force-deletes a pod.
//!
//! POLICY CONTENT: security (CLAUDE.md guard-crate ownership); the matcher
//! shapes are infrastructure. **There is no allowlist and no suppression
//! marker** — nothing here reads `guard:ignore` or any other annotation.
//!
//! # Scope
//!
//! Tracked and untracked-not-ignored files under [`SCOPE_ROOTS`] and
//! `packages/*/e2e/`. `docs/` is out: runbooks state the ban in prose.
//!
//! # What is a violation (one "command unit")
//!
//! * **CLI form** — a `delete` token, a pod target (`pod|pods|po`, with an
//!   optional `.group` such as `pods.v1` and `/name`, in a comma list too, or
//!   `all`) and a force flag that is ON, in ANY order. `--force` alone is a
//!   force delete: kubectl sets the grace period to 0 when `--force` is given
//!   without one, so requiring `--grace-period=0` as well would leave a
//!   one-flag bypass. `--force=<v>` is off only for Go `ParseBool` false
//!   spellings ([`FORCE_FALSE_VALUES`]). A bare `--grace-period=0` is NOT a
//!   force delete (kubectl turns it into 1).
//! * **Unresolvable target, fail closed** — `delete` + force ON with a target
//!   the guard cannot read: a shell/template expansion (`$KIND`, `{kind}`) or
//!   a manifest (`-f`, `--filename`, `-k`, `--kustomize`).
//! * **API form** — `grace_period_seconds: Some(0)` / `.grace_period(0)` (Rust),
//!   `gracePeriodSeconds: 0` (TS/JS, JSON bodies, shell `curl`/`--raw` bodies).
//! * **Split Rust/TS builder** — per FILE, `.arg("…")`/`.args([…])` literals
//!   that together spell `delete` + a pod target + force ON.
//!
//! Comments and Markdown prose are not commands: Rust via
//! `test_code_filter::strip_comments`, TS/JS via `ts_lex`, Markdown via
//! `markdown_code` (code spans and fences ARE scanned), everything else via a
//! quote-aware `#` stripper. Units: shell-like text splits on newlines (after
//! joining `\` continuations), `;`, `|`, `&`; code splits on `;`; YAML is one
//! unit per document (a `command:` block sequence spans lines).
//!
//! # Recorded residuals (security-accepted, 2026-11-13)
//!
//! * A builder whose arguments are NOT string literals in `.arg`/`.args`
//!   (e.g. pushed from variables across statements) needs an AST pass.
//! * A raw API call whose zero grace is not spelled `gracePeriodSeconds: 0`
//!   in the file (built at run time, or read from a file outside scope).

use crate::common::explain::{print_finding, Finding};
use crate::common::scan::warn_skip;
use crate::common::status::{emit_ok, emit_scope};
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

pub const FORCE_DELETE_RULE_ID: &str = "pod_force_delete";
pub const UNRESOLVED_TARGET_RULE_ID: &str = "force_delete_unresolved_target";
pub const ZERO_GRACE_API_RULE_ID: &str = "pod_zero_grace_api";
pub const SPLIT_BUILDER_RULE_ID: &str = "pod_force_delete_split_builder";
pub const NO_FILES_RULE_ID: &str = "no_candidate_files";
pub const UNREADABLE_RULE_ID: &str = "unreadable_candidate_file";

/// Scanned roots (plus `packages/*/e2e/`, see [`in_scope`]).
pub const SCOPE_ROOTS: &[&str] = &[
    "infra/",
    "scripts/",
    "crates/env-tests/",
    // The host-side cluster helper (ADR-0030): it runs kubectl/kind against the
    // cluster, so a force-deleting "recovery" verb there is in scope (security).
    "crates/devloop-helper/",
];
/// The packages root; only its `<pkg>/e2e/` subtrees are in scope ([`in_scope`]).
pub const PACKAGES_ROOT: &str = "packages/";

/// Go `strconv.ParseBool` false spellings: `--force=<one of these>` is off.
/// Every other value is treated as ON — including values ParseBool rejects
/// (e.g. `yes`, on which kubectl errors instead of forcing): fail closed.
pub const FORCE_FALSE_VALUES: &[&str] = &["false", "0", "f", "F", "FALSE", "False"];

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at \
        load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static POD_TARGET_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?i:pods?|po|all)(?:\.[A-Za-z0-9.-]*)?(?:/.*)?$")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at \
        load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static RUST_ZERO_GRACE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"grace_period_seconds\s*:\s*Some\s*\(\s*0\s*\)|\.grace_period\s*\(\s*0\s*\)")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at \
        load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static JSON_ZERO_GRACE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\\?["']?gracePeriodSeconds\\?["']?\s*[:=]\s*\\?["']?0\b"#)
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at \
        load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static ARG_CALL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"\.args?\s*\(\s*&?\s*(?:vec!\s*)?(\[[^\]]*\]|"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')"#,
    )
    .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at \
        load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static STRING_LIT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#""((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'"#).expect("static pattern compiles")
});

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanFinding {
    pub rule_id: &'static str,
    pub file: String,
    pub line: usize,
    pub detail: String,
}

/// Is `rel` in scope?
#[must_use]
pub fn in_scope(rel: &str) -> bool {
    if SCOPE_ROOTS.iter().any(|r| rel.starts_with(r)) {
        return true;
    }
    let Some(rest) = rel.strip_prefix(PACKAGES_ROOT) else {
        return false;
    };
    let mut parts = rest.split('/');
    parts.next().is_some() && parts.next() == Some("e2e")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Ts,
    Markdown,
    Yaml,
    Json,
    /// Shell, Dockerfile, TOML, Python, env files, anything else: `#` comments.
    Hash,
}

fn lang_of(rel: &str) -> Lang {
    let ext = rel.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "rs" => Lang::Rust,
        "ts" | "tsx" | "js" | "mjs" | "cjs" | "svelte" | "mts" | "cts" => Lang::Ts,
        "md" => Lang::Markdown,
        "yaml" | "yml" => Lang::Yaml,
        "json" => Lang::Json,
        _ => Lang::Hash,
    }
}

/// Strip `#` comments: a `#` outside quotes, at line start or after
/// whitespace, starts a comment (shell/YAML/TOML/Python/Dockerfile).
fn strip_hash_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let mut quote: Option<char> = None;
        let mut prev_ws = true;
        let mut escaped = false;
        for c in line.chars() {
            if escaped {
                escaped = false;
                out.push(c);
                prev_ws = false;
                continue;
            }
            match quote {
                Some(q) => {
                    if c == '\\' && q == '"' {
                        escaped = true;
                    } else if c == q {
                        quote = None;
                    }
                }
                None => {
                    if c == '#' && prev_ws {
                        break;
                    }
                    if c == '\\' {
                        escaped = true;
                    } else if c == '"' || c == '\'' {
                        quote = Some(c);
                    }
                }
            }
            out.push(c);
            prev_ws = c.is_whitespace();
        }
    }
    out
}

/// `(1-based start line, text)` units split at any char in `seps`.
fn split_units(code: &str, first_line: usize, seps: &[char]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut line = first_line;
    let mut start = first_line;
    for c in code.chars() {
        if seps.contains(&c) {
            out.push((start, std::mem::take(&mut cur)));
            if c == '\n' {
                line += 1;
            }
            start = line;
            continue;
        }
        if c == '\n' {
            line += 1;
        }
        cur.push(c);
    }
    out.push((start, cur));
    out
}

/// Shell-like units: `\`-continuations joined (the ONE joiner,
/// `common::cargo_cli::logical_lines`, keeping the first physical line), then
/// split on `;`, `|` and `&`. Each unit carries the line it starts on.
fn shell_units(code: &str, first_line: usize) -> Vec<(usize, String)> {
    crate::common::cargo_cli::logical_lines(code)
        .into_iter()
        .flat_map(|(line, text)| {
            text.split([';', '|', '&'])
                .filter(|u| !u.trim().is_empty())
                .map(|u| (line + first_line - 1, u.to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn tokens(unit: &str) -> Vec<String> {
    unit.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '(' | ')' | '[' | ']'))
        .map(|t| t.trim_matches(|c| c == ',' || c == ';'))
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

fn force_on(t: &str) -> bool {
    if t == "--force" {
        return true;
    }
    t.strip_prefix("--force=")
        .is_some_and(|v| !FORCE_FALSE_VALUES.contains(&v))
}

fn is_manifest_flag(t: &str) -> bool {
    matches!(t, "-f" | "--filename" | "-k" | "--kustomize")
        || ["-f=", "--filename=", "-k=", "--kustomize="]
            .iter()
            .any(|p| t.starts_with(p))
}

/// Classify one token list. `Some(rule)` when it is a violation.
/// kubectl flags that consume the NEXT token as their value (so that token
/// is not a positional target).
const VALUE_FLAGS: &[&str] = &[
    "-n",
    "--namespace",
    "-l",
    "--selector",
    "--field-selector",
    "--context",
    "--cluster",
    "--kubeconfig",
    "--grace-period",
    "--timeout",
    "--cascade",
    "-o",
    "--output",
];

/// The positional tokens after `delete` (flags and flag values skipped).
fn positionals(after: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for t in after {
        if skip_next {
            skip_next = false;
            continue;
        }
        if t.starts_with('-') {
            skip_next = VALUE_FLAGS.contains(&t.as_str());
            continue;
        }
        out.push(t);
    }
    out
}

/// Classify one token list. `Some(rule)` when it is a violation.
fn classify(toks: &[String]) -> Option<(&'static str, String)> {
    let del = toks.iter().position(|t| t == "delete")?;
    let force = toks.iter().find(|t| force_on(t))?;
    let after = toks.get(del + 1..).unwrap_or_default();
    if let Some(p) = after
        .iter()
        .find(|t| t.split(',').any(|piece| POD_TARGET_RE.is_match(piece)))
    {
        return Some((
            FORCE_DELETE_RULE_ID,
            format!("`delete` of `{p}` with `{force}` (a zero-grace force delete)"),
        ));
    }
    let unresolved = after
        .iter()
        .find(|t| {
            t.starts_with('$')
                || t.starts_with('{')
                || t.contains("/$")
                || t.contains("/{")
                || is_manifest_flag(t)
        })
        .map(|u| format!("`{u}`"))
        .or_else(|| {
            toks.iter()
                .any(|t| t == "xargs")
                .then(|| "targets from `xargs`".to_string())
        })
        .or_else(|| {
            positionals(after)
                .is_empty()
                .then(|| "no target named (stdin)".to_string())
        });
    unresolved.map(|u| {
        (
            UNRESOLVED_TARGET_RULE_ID,
            format!(
                "`delete` with `{force}` and a target the guard cannot resolve ({u}) — fail \
                closed: it may be a pod"
            ),
        )
    })
}

/// Every finding in one file's content.
#[must_use]
pub fn scan_content(rel: &str, content: &str) -> Vec<BanFinding> {
    let lang = lang_of(rel);
    let mut out = Vec::new();
    let mut push = |rule_id, line, detail: String| {
        out.push(BanFinding {
            rule_id,
            file: rel.to_string(),
            line,
            detail,
        });
    };
    let units: Vec<(usize, String)> = match lang {
        Lang::Rust => split_units(
            &crate::common::test_code_filter::strip_comments_file(content),
            1,
            &[';'],
        ),
        Lang::Ts => split_units(&crate::common::ts_lex::lex(content).text, 1, &[';']),
        Lang::Markdown => crate::common::markdown_code::scan(content)
            .spans
            .iter()
            .flat_map(|s| {
                let first = if matches!(s.kind, crate::common::markdown_code::CodeKind::Inline) {
                    s.line
                } else {
                    s.line + 1
                };
                shell_units(&strip_hash_comments(&s.text), first)
            })
            .collect(),
        Lang::Yaml => {
            let stripped = strip_hash_comments(content);
            let mut units = Vec::new();
            let mut start = 1;
            let mut cur = String::new();
            for (i, l) in stripped.split('\n').enumerate() {
                if l.starts_with("---") {
                    units.push((start, std::mem::take(&mut cur)));
                    start = i + 2;
                    continue;
                }
                cur.push_str(l);
                cur.push('\n');
            }
            units.push((start, cur));
            units
        }
        Lang::Json => shell_units(content, 1),
        Lang::Hash => shell_units(&strip_hash_comments(content), 1),
    };
    for (line, unit) in &units {
        if let Some((rule, detail)) = classify(&tokens(unit)) {
            push(rule, *line, detail);
        }
        let api = match lang {
            Lang::Rust => RUST_ZERO_GRACE_RE.find(unit),
            _ => JSON_ZERO_GRACE_RE.find(unit),
        };
        if let Some(m) = api {
            push(
                ZERO_GRACE_API_RULE_ID,
                *line,
                format!(
                    "`{}` — a zero grace period through the API is a force delete",
                    m.as_str().trim()
                ),
            );
        }
    }
    // Split builder: per file, literals passed to `.arg`/`.args`.
    if matches!(lang, Lang::Rust | Lang::Ts) {
        let code = if lang == Lang::Rust {
            crate::common::test_code_filter::strip_comments_file(content)
        } else {
            crate::common::ts_lex::lex(content).text
        };
        let mut lits: Vec<String> = Vec::new();
        let mut first_line = 0usize;
        for m in ARG_CALL_RE.captures_iter(&code) {
            let Some(g) = m.get(1) else { continue };
            if first_line == 0 {
                first_line = code.get(..g.start()).map_or(0, |p| p.matches('\n').count()) + 1;
            }
            for s in STRING_LIT_RE.captures_iter(g.as_str()) {
                if let Some(v) = s.get(1).or_else(|| s.get(2)) {
                    lits.extend(tokens(v.as_str()));
                }
            }
        }
        if let Some((_, detail)) = classify(&lits) {
            let already = out.iter().any(|f| {
                f.rule_id == FORCE_DELETE_RULE_ID || f.rule_id == UNRESOLVED_TARGET_RULE_ID
            });
            if !already {
                out.push(BanFinding {
                    rule_id: SPLIT_BUILDER_RULE_ID,
                    file: rel.to_string(),
                    line: first_line,
                    detail: format!("command-builder literals across the file spell {detail}"),
                });
            }
        }
    }
    out
}

fn list_files(repo_root: &Path) -> Result<Vec<String>> {
    use crate::common::git_changes::{get_tracked_files, get_untracked_files};
    let mut v: Vec<String> = Vec::new();
    for root in SCOPE_ROOTS.iter().chain(std::iter::once(&PACKAGES_ROOT)) {
        let spec = root.trim_end_matches('/');
        let tracked = get_tracked_files(repo_root, spec, &[])?;
        let untracked = get_untracked_files(repo_root, Path::new(spec), &[])?;
        v.extend(
            tracked
                .into_iter()
                .chain(untracked)
                .map(|p| p.to_string_lossy().into_owned()),
        );
    }
    v.retain(|p| in_scope(p));
    v.sort();
    v.dedup();
    Ok(v)
}

fn token_for(rule: &str) -> &'static str {
    match rule {
        FORCE_DELETE_RULE_ID => "pod-force-delete",
        UNRESOLVED_TARGET_RULE_ID => "force-delete-unresolved-target",
        ZERO_GRACE_API_RULE_ID => "pod-zero-grace-api",
        SPLIT_BUILDER_RULE_ID => "pod-force-delete-split-builder",
        NO_FILES_RULE_ID => "force-delete-ban-no-candidate-files",
        UNREADABLE_RULE_ID => "force-delete-ban-unreadable-candidate-file",
        _ => "force-delete-ban",
    }
}

/// Run the policy.
pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let files = list_files(repo_root)?;
    if files.is_empty() {
        let t = token_for(NO_FILES_RULE_ID);
        println!(
            "ERROR: PRECONDITION [{t}] {} — zero files in scope; this guard checked nothing",
            repo_root.display()
        );
        anyhow::bail!("{t}-1");
    }
    let mut findings: Vec<BanFinding> = Vec::new();
    let mut binary = 0usize;
    for rel in &files {
        let abs = repo_root.join(rel);
        let bytes = match std::fs::read(&abs) {
            Ok(b) => b,
            // Deleted in the work tree but still in the index.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                warn_skip("force-delete-ban candidate read", &abs, &e);
                findings.push(BanFinding {
                    rule_id: UNREADABLE_RULE_ID,
                    file: rel.clone(),
                    line: 0,
                    detail: e.to_string(),
                });
                continue;
            }
        };
        // A binary file (NUL byte or not UTF-8) is not a command.
        let Ok(text) = String::from_utf8(bytes) else {
            binary += 1;
            continue;
        };
        if text.contains('\0') {
            binary += 1;
            continue;
        }
        findings.extend(scan_content(rel, &text));
    }
    emit_scope(format!(
        "{} files under {} and {PACKAGES_ROOT}*/e2e/ ({} binary, not commands); no allowlist, \
            no suppression",
        files.len() - binary,
        SCOPE_ROOTS.join(", "),
        binary
    ));
    if findings.is_empty() {
        emit_ok(format!(
            "force-delete-ban-clean-{}-files",
            files.len() - binary
        ));
        return Ok(());
    }
    for f in &findings {
        if explain {
            let policy = format!("force_delete_ban::{}", f.rule_id);
            print_finding(&Finding {
                file: &f.file,
                row: f.line,
                col: 0,
                policy: &policy,
                matched: &f.detail,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {}:{} [{}] {} — never force-delete a pod (ADR-0039 §9: defeats the \
                    MH hostPort slot lock); delete gracefully and wait",
                f.file, f.line, f.rule_id, f.detail
            );
        }
    }
    let first = findings
        .first()
        .map_or("force-delete-ban", |f| token_for(f.rule_id));
    anyhow::bail!("{first}-{}", findings.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(rel: &str, src: &str) -> Vec<&'static str> {
        scan_content(rel, src)
            .into_iter()
            .map(|f| f.rule_id)
            .collect()
    }

    const ORDERINGS: &[&str] = &[
        "kubectl delete pod x --force --grace-period=0",
        "kubectl delete pod x --grace-period=0 --force",
        "kubectl --force delete pod x --grace-period=0",
        "kubectl delete --grace-period=0 --force pod x",
        "kubectl -n ns delete --force pods/x",
        "kubectl delete po x --force",
        "kubectl delete pods,svc -l app=mh --force=true",
        "kubectl delete pod.v1 x --force=1",
        "kubectl delete pods.v1 x --force=yes",
        "kubectl delete all --all --force",
        "kubectl delete pod -l app=x -n ns --grace-period 0 --force",
        "kubectl delete Pod mh-s1 --force",
        "kubectl delete PODS/x --force",
    ];

    #[test]
    fn every_ordering_and_spelling_in_shell_fails() {
        for cmd in ORDERINGS {
            assert_eq!(
                rules("infra/x.sh", &format!("{cmd}\n")),
                vec![FORCE_DELETE_RULE_ID],
                "{cmd}"
            );
        }
    }

    #[test]
    fn continuation_lines_join() {
        let src = "set -e\nkubectl delete pod x \\\n  -n ns \\\n  --force\n";
        let f = scan_content("scripts/a.sh", src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 2);
    }

    #[test]
    fn graceful_or_non_pod_or_false_force_passes() {
        for cmd in [
            "kubectl delete pod x",
            "kubectl delete pod x --grace-period=0",
            "kubectl delete pod x --now",
            "kubectl delete pod x --force=false",
            "kubectl delete pod x --force=0",
            "kubectl delete pod x --force=False",
            "kubectl delete deployment mh-s1 --cascade=foreground",
            "kubectl delete deployment mh-s1 --force",
            "git push origin main --force",
            "./scripts/generate-dev-certs.sh --force",
            "kubectl wait --for=delete pod -l app=x --timeout=60s",
        ] {
            assert_eq!(
                rules("infra/x.sh", &format!("{cmd}\n")),
                Vec::<&str>::new(),
                "{cmd}"
            );
        }
    }

    #[test]
    fn separate_commands_on_separate_lines_do_not_combine() {
        let src = "kubectl delete pod x\ngit push --force\n";
        assert!(rules("scripts/a.sh", src).is_empty());
        let src = "kubectl delete pod x && git push --force\n";
        assert!(rules("scripts/a.sh", src).is_empty());
    }

    #[test]
    fn unresolvable_target_fails_closed() {
        for cmd in [
            "kubectl delete \"$KIND\" x --force",
            "kubectl delete $RES/x --force",
            "kubectl delete -f manifest.yaml --force",
            "kubectl delete -k overlays/x --force",
            "kubectl get pods -o name -l app=mh | xargs kubectl delete --force",
            "xargs kubectl delete -n ns --force",
            "kubectl delete --force -l app=mh",
        ] {
            assert_eq!(
                rules("infra/x.sh", &format!("{cmd}\n")),
                vec![UNRESOLVED_TARGET_RULE_ID],
                "{cmd}"
            );
        }
    }

    #[test]
    fn comments_and_prose_are_not_commands_but_code_spans_are() {
        let sh = "# Never `kubectl delete pod --force --grace-period=0` an MH pod.\nexit 0\n";
        assert!(rules("infra/kind/scripts/mh.sh", sh).is_empty());
        let yaml = "# Never kubectl delete pod --force --grace-period=0\nkind: X\n";
        assert!(rules("infra/services/x.yaml", yaml).is_empty());
        let rs = "//! never `kubectl delete pod --force --grace-period=0`\n/* kubectl delete pod --force */\nfn f() {}\n";
        assert!(rules("crates/env-tests/src/a.rs", rs).is_empty());
        let ts = "// kubectl delete pod --force\nconst a = 1;\n";
        assert!(rules("packages/web-app/e2e/a.ts", ts).is_empty());
        let md =
            "Never kubectl delete pod --force.\n\n```bash\nkubectl delete pod x --force\n```\n";
        assert_eq!(rules("scripts/a.md", md), vec![FORCE_DELETE_RULE_ID]);
        let md_inline = "Run `kubectl delete pod x --force` to break things.\n";
        assert_eq!(rules("scripts/a.md", md_inline), vec![FORCE_DELETE_RULE_ID]);
    }

    #[test]
    fn a_suppression_comment_does_not_silence() {
        let sh = "kubectl delete pod x --force # guard:ignore(force-delete-ban) reason\n";
        assert_eq!(rules("infra/x.sh", sh), vec![FORCE_DELETE_RULE_ID]);
        let rs = "// guard:ignore\nlet _ = Command::new(\"kubectl\").args([\"delete\", \"pod\", &p, \"--force\"]).output();\n";
        assert_eq!(
            rules("crates/env-tests/src/a.rs", rs),
            vec![FORCE_DELETE_RULE_ID]
        );
    }

    #[test]
    fn rust_args_array_in_any_order_fails() {
        let rs = "fn f(p: &str) {\n    let _ = Command::new(\"kubectl\")\n        .args([\"--grace-period=0\", \"delete\",\n            \"--force\", \"pod\", p])\n        .status();\n}\n";
        let f = scan_content("crates/env-tests/src/a.rs", rs);
        assert_eq!(
            f.iter().map(|f| f.rule_id).collect::<Vec<_>>(),
            vec![FORCE_DELETE_RULE_ID]
        );
    }

    #[test]
    fn rust_format_string_with_placeholder_target_fails_closed() {
        let rs = "fn f() { let c = format!(\"kubectl delete {kind} {name} --force\"); }\n";
        assert_eq!(
            rules("crates/env-tests/src/a.rs", rs),
            vec![UNRESOLVED_TARGET_RULE_ID]
        );
    }

    #[test]
    fn split_builder_across_statements_fails() {
        let rs = "fn f(p: &str) {\n    let mut c = Command::new(\"kubectl\");\n    c.arg(\"delete\");\n    c.arg(\"pod\");\n    c.arg(p);\n    c.arg(\"--force\");\n}\n";
        assert_eq!(
            rules("crates/env-tests/src/a.rs", rs),
            vec![SPLIT_BUILDER_RULE_ID]
        );
        let ts = "const c = spawn('kubectl');\nc.arg('delete');\nc.args(['pods', name]);\nc.arg('--force=true');\n";
        assert_eq!(
            rules("packages/web-app/e2e/a.ts", ts),
            vec![SPLIT_BUILDER_RULE_ID]
        );
    }

    #[test]
    fn negative_assertions_on_literals_pass() {
        let rs = "#[test]\nfn t() {\n    let cmd = graceful_delete_command(\"p\", \"ns\");\n    assert!(!cmd.contains(\"--force\"));\n    assert!(!drain.iter().any(|a| a == \"--force\"));\n}\n";
        assert!(rules("crates/env-tests/src/canary.rs", rs).is_empty());
    }

    #[test]
    fn ts_args_array_fails() {
        let ts = "execFileSync('kubectl', ['delete', 'pod', name, '--force']);\n";
        assert_eq!(
            rules("packages/web-app/e2e/a.ts", ts),
            vec![FORCE_DELETE_RULE_ID]
        );
    }

    #[test]
    fn yaml_block_sequence_command_fails() {
        let y = "apiVersion: batch/v1\nkind: Job\nspec:\n  template:\n    spec:\n      containers:\n      - command:\n        - kubectl\n        - delete\n        - pod\n        - --all\n        - --force\n---\nkind: Other\n";
        let f = scan_content("infra/services/x/job.yaml", y);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 1);
    }

    #[test]
    fn api_zero_grace_forms_fail() {
        assert_eq!(
            rules(
                "crates/env-tests/src/a.rs",
                "let dp = DeleteParams { grace_period_seconds: Some(0), ..Default::default() };\n"
            ),
            vec![ZERO_GRACE_API_RULE_ID]
        );
        assert_eq!(
            rules(
                "crates/env-tests/src/a.rs",
                "let dp = DeleteParams::default().grace_period(0);\n"
            ),
            vec![ZERO_GRACE_API_RULE_ID]
        );
        assert_eq!(
            rules("packages/web-app/e2e/a.ts", "await api.deleteNamespacedPod({ name, namespace, body: { gracePeriodSeconds: 0 } });\n"),
            vec![ZERO_GRACE_API_RULE_ID]
        );
        assert_eq!(
            rules(
                "scripts/a.sh",
                "curl -X DELETE \"$API/pods/x\" -d '{\"gracePeriodSeconds\":0}'\n"
            ),
            vec![ZERO_GRACE_API_RULE_ID]
        );
        // Positive arms.
        assert!(rules(
            "crates/env-tests/src/a.rs",
            "let dp = DeleteParams { grace_period_seconds: Some(30), ..Default::default() };\n"
        )
        .is_empty());
        assert!(rules(
            "packages/web-app/e2e/a.ts",
            "const body = { gracePeriodSeconds: 30 };\n"
        )
        .is_empty());
    }

    #[test]
    fn scope() {
        assert!(in_scope("infra/kind/scripts/iterate.sh"));
        assert!(in_scope("scripts/layer7.sh"));
        assert!(in_scope("crates/env-tests/src/canary.rs"));
        assert!(in_scope("packages/web-app/e2e/fixtures.ts"));
        assert!(in_scope("crates/devloop-helper/src/commands.rs"));
        assert!(!in_scope("packages/web-app/src/a.ts"));
        assert!(!in_scope("docs/runbooks/mh-incident-response.md"));
        assert!(!in_scope("crates/dt-guard/src/force_delete_ban.rs"));
    }
}
