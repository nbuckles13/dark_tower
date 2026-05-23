//! Sibling helpers for `kustomize.rs` — tool detection, build invocation,
//! and pure-policy functions for R-17/R-18/R-19 on rendered multi-doc YAML.
//!
//! Per @dry-reviewer Gate-1 nudge: single-consumer extraction doesn't warrant
//! a `common/` siting (which by convention houses cross-subcommand kernels).
//! This sibling module keeps the policy / tooling split visible without
//! falsely signaling reuse. If a second consumer of kustomize tooling ever
//! lands, promotion to `common/` becomes the obvious move.
//!
//! Tool-absent fallback: per bash today and team-lead 2026-05-22 directive,
//! missing `kustomize`/`kubectl kustomize`/`kubeconform` degrades to WARN
//! per check — never FAIL on missing tools (devloop containers may not have
//! kubeconform).

use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Which kustomize-build tool is available, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KustomizeTool {
    /// Standalone `kustomize` binary.
    Standalone,
    /// `kubectl kustomize` subcommand.
    Kubectl,
}

impl KustomizeTool {
    fn command(&self) -> Command {
        match self {
            Self::Standalone => {
                let mut c = Command::new("kustomize");
                c.arg("build");
                c
            }
            Self::Kubectl => {
                let mut c = Command::new("kubectl");
                c.arg("kustomize");
                c
            }
        }
    }
}

/// Detect which kustomize-build tool is available, if any.
pub fn detect_kustomize_tool() -> Option<KustomizeTool> {
    if Command::new("kustomize")
        .arg("version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Some(KustomizeTool::Standalone);
    }
    if Command::new("kubectl")
        .args(["kustomize", "--help"])
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Some(KustomizeTool::Kubectl);
    }
    None
}

/// True iff `kubeconform` is available on PATH.
pub fn detect_kubeconform() -> bool {
    Command::new("kubeconform")
        .arg("-v")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Outcome of a single `kustomize build <dir>` invocation.
pub struct BuildResult {
    /// Rendered multi-document YAML (empty on failure).
    pub stdout: String,
    /// First ~10 lines of stderr (truncated for diagnostic embedding).
    pub stderr_head: String,
    /// True iff the build succeeded.
    pub success: bool,
}

/// Invoke `kustomize build <dir>` (or `kubectl kustomize <dir>`). On
/// success returns rendered stdout; on failure returns the first ~10
/// stderr lines as the diagnostic.
pub fn run_kustomize_build(tool: KustomizeTool, dir: &Path) -> Result<BuildResult> {
    let output = tool
        .command()
        .arg(dir)
        .output()
        .map_err(|e| anyhow::anyhow!("running kustomize on {}: {e}", dir.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr_full = String::from_utf8_lossy(&output.stderr);
    let stderr_head: String = stderr_full
        .lines()
        .take(10)
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(BuildResult {
        stdout,
        stderr_head,
        success: output.status.success(),
    })
}

/// One R-17 / R-18 / R-19 finding produced from rendered multi-doc YAML.
#[derive(Debug, Clone)]
pub struct YamlFinding {
    /// Resource label like `Deployment/ac-service (infra/services/ac-service)`
    /// or `Secret/db-secret (infra/services/ac-service)` — never the actual
    /// secret value.
    pub resource: String,
    /// Short reason token like `missing-runAsNonRoot`.
    pub detail: String,
}

/// Workload `kind`s subject to R-18 security-context invariants. Matches
/// bash today (`Deployment` + `StatefulSet`). Does NOT include `DaemonSet`
/// or `Job` (bash today doesn't check them either; preserving parity).
pub const SECURITY_CONTEXT_KINDS: &[&str] = &["Deployment", "StatefulSet"];

/// Workload names that bash today exempts from `readOnlyRootFilesystem: true`
/// (substrings — match bash's `[[ "$name" != *postgres* ]]` shape).
pub const READ_ONLY_ROOT_FS_EXEMPT_SUBSTRINGS: &[&str] =
    &["postgres", "prometheus", "loki", "grafana"];

/// R-18 security-context check. Walks multi-doc rendered YAML; for each
/// `Deployment`/`StatefulSet`, asserts four required substrings appear:
/// `runAsNonRoot: true`, `allowPrivilegeEscalation: false`,
/// `capabilities.drop: [ALL]` (loose match per bash), and
/// `readOnlyRootFilesystem: true` (skipped for names containing
/// `postgres|prometheus|loki|grafana`).
///
/// Bash semantics: substring-matches across the rendered doc, NOT a YAML
/// parse. Preserves bash today's shape for port fidelity — false-positive
/// on string-literal-mention-of-the-token is bash's behavior too.
pub fn check_security_context(rendered: &str, source_label: &str) -> Vec<YamlFinding> {
    let mut findings = Vec::new();
    for doc in split_yaml_docs(rendered) {
        let Some(kind) = field_one_line(&doc, "kind") else {
            continue;
        };
        if !SECURITY_CONTEXT_KINDS.contains(&kind.as_str()) {
            continue;
        }
        let name = field_one_line(&doc, "name").unwrap_or_else(|| "<unknown>".to_string());
        let resource_label = format!("{kind}/{name} ({source_label})");

        if !doc.contains("runAsNonRoot: true") {
            findings.push(YamlFinding {
                resource: resource_label.clone(),
                detail: "missing runAsNonRoot: true".to_string(),
            });
        }
        if !doc.contains("allowPrivilegeEscalation: false") {
            findings.push(YamlFinding {
                resource: resource_label.clone(),
                detail: "missing allowPrivilegeEscalation: false".to_string(),
            });
        }
        if !doc.contains("drop:") || !contains_all_capability_line(&doc) {
            findings.push(YamlFinding {
                resource: resource_label.clone(),
                detail: "missing capabilities.drop: [ALL]".to_string(),
            });
        }
        let exempt = READ_ONLY_ROOT_FS_EXEMPT_SUBSTRINGS
            .iter()
            .any(|s| name.contains(s));
        if !exempt && !doc.contains("readOnlyRootFilesystem: true") {
            findings.push(YamlFinding {
                resource: resource_label,
                detail: "missing readOnlyRootFilesystem: true".to_string(),
            });
        }
    }
    findings
}

/// R-19 empty-secret check. Walks multi-doc rendered YAML; for each
/// `Secret`, finds lines inside `data:` / `stringData:` sections whose
/// value is empty (`key:` / `key: ""` / `key: ''`). Reports only key
/// names — NEVER echoes values (preserves bash today's redaction).
pub fn check_empty_secret_data(rendered: &str, source_label: &str) -> Vec<YamlFinding> {
    let mut findings = Vec::new();
    for doc in split_yaml_docs(rendered) {
        let Some(kind) = field_one_line(&doc, "kind") else {
            continue;
        };
        if kind != "Secret" {
            continue;
        }
        let name = field_one_line(&doc, "name").unwrap_or_else(|| "<unknown>".to_string());
        let mut in_data_section = false;
        for line in doc.lines() {
            // Section start: `data:` or `stringData:` (no leading whitespace
            // in bash's pattern — top-level key).
            let trimmed = line.trim_end();
            if trimmed == "data:" || trimmed == "stringData:" {
                in_data_section = true;
                continue;
            }
            // Section end: any non-indented line beginning with a letter.
            if in_data_section
                && !line.is_empty()
                && line.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            {
                in_data_section = false;
                continue;
            }
            if !in_data_section {
                continue;
            }
            // Line shape: `  key:` / `  key: ""` / `  key: ''` (trimmed).
            // Capture the key name and check the value is empty.
            if let Some((key, value)) = parse_indented_yaml_kv(line) {
                let v = value.trim();
                if v.is_empty() || v == "\"\"" || v == "''" {
                    findings.push(YamlFinding {
                        resource: format!("Secret/{name} ({source_label})"),
                        detail: format!("empty value for key '{key}'"),
                    });
                }
            }
        }
    }
    findings
}

/// Split a multi-document YAML stream on `---` document separators.
/// Returns each document's text (without the separator).
pub fn split_yaml_docs(rendered: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in rendered.lines() {
        if line.trim() == "---" {
            if !current.trim().is_empty() {
                out.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            continue;
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

/// Find the first occurrence of a single-line YAML field (`<field>: <value>`)
/// anywhere in `doc`. Returns the value with surrounding whitespace trimmed.
/// Matches bash's `grep -m1 '^kind:' | awk '{print $2}'` shape.
fn field_one_line(doc: &str, field: &str) -> Option<String> {
    let prefix = format!("{field}:");
    for line in doc.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Loose match for `capabilities.drop: [ALL]` — bash today does
/// `grep -qE '^\s*-\s*"?ALL"?\s*$'` so any `- ALL` / `- "ALL"` line counts.
fn contains_all_capability_line(doc: &str) -> bool {
    for line in doc.lines() {
        let trimmed = line.trim();
        if matches!(trimmed, "- ALL" | "- \"ALL\"" | "- 'ALL'") {
            return true;
        }
    }
    false
}

/// Parse an indented YAML `<key>: <value>` shape. Returns `Some((key, value))`
/// when the line matches; `None` otherwise (skip blank lines, list items,
/// nested structures).
fn parse_indented_yaml_kv(line: &str) -> Option<(String, String)> {
    if !line.starts_with(char::is_whitespace) {
        return None;
    }
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') || trimmed.starts_with('-') {
        return None;
    }
    let (key, rest) = trimmed.split_once(':')?;
    let key = key.trim().to_string();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    Some((key, rest.to_string()))
}

/// Invoke `kubeconform -strict -summary -` with `rendered` on stdin.
/// Returns `Ok(true)` on schema-clean output, `Ok(false)` with diagnostic
/// stderr otherwise. Caller is responsible for the `detect_kubeconform`
/// gate.
pub fn run_kubeconform(rendered: &str) -> Result<KubeconformResult> {
    use std::io::Write;
    let mut child = Command::new("kubeconform")
        .args(["-strict", "-summary", "-ignore-missing-schemas", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawning kubeconform: {e}"))?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("kubeconform stdin pipe missing"))?;
        stdin
            .write_all(rendered.as_bytes())
            .map_err(|e| anyhow::anyhow!("writing rendered yaml to kubeconform: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| anyhow::anyhow!("waiting kubeconform: {e}"))?;
    let stderr_full = String::from_utf8_lossy(&output.stderr);
    let stderr_head: String = stderr_full
        .lines()
        .take(10)
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(KubeconformResult {
        stderr_head,
        success: output.status.success(),
    })
}

/// Outcome of a single `kubeconform` invocation.
pub struct KubeconformResult {
    pub stderr_head: String,
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_yaml_docs_handles_separator_lines() {
        let s = "kind: Foo\nname: a\n---\nkind: Bar\nname: b\n";
        let docs = split_yaml_docs(s);
        assert_eq!(docs.len(), 2);
        assert!(docs[0].contains("kind: Foo"));
        assert!(docs[1].contains("kind: Bar"));
    }

    #[test]
    fn split_yaml_docs_skips_empty_segments() {
        let s = "---\nkind: Foo\n---\n---\nkind: Bar\n";
        let docs = split_yaml_docs(s);
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn field_one_line_extracts_kind() {
        let doc = "apiVersion: v1\nkind: Deployment\nmetadata:\n  name: foo\n";
        assert_eq!(field_one_line(doc, "kind"), Some("Deployment".to_string()));
    }

    #[test]
    fn field_one_line_finds_first_name() {
        // Two `name:` occurrences — match bash's `grep -m1 | head -1` shape.
        let doc = "kind: Deployment\nmetadata:\n  name: outer\nspec:\n  name: inner\n";
        assert_eq!(field_one_line(doc, "name"), Some("outer".to_string()));
    }

    #[test]
    fn check_security_context_full_pass() {
        let doc = "kind: Deployment\nmetadata:\n  name: ac-service\nspec:\n  template:\n    spec:\n      securityContext:\n        runAsNonRoot: true\n      containers:\n        - name: ac\n          securityContext:\n            allowPrivilegeEscalation: false\n            readOnlyRootFilesystem: true\n            capabilities:\n              drop:\n                - ALL\n";
        let findings = check_security_context(doc, "infra/services/ac-service");
        assert_eq!(
            findings.len(),
            0,
            "expected zero findings, got {findings:?}"
        );
    }

    #[test]
    fn check_security_context_missing_run_as_non_root() {
        // No runAsNonRoot anywhere.
        let doc = "kind: Deployment\nmetadata:\n  name: ac-service\nspec:\n  template:\n    spec:\n      containers:\n        - name: ac\n          securityContext:\n            allowPrivilegeEscalation: false\n            readOnlyRootFilesystem: true\n            capabilities:\n              drop:\n                - ALL\n";
        let findings = check_security_context(doc, "infra/services/ac-service");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].detail.contains("runAsNonRoot"));
        assert!(findings[0].resource.contains("ac-service"));
    }

    #[test]
    fn check_security_context_postgres_exempted_from_readonly_root_fs() {
        // No readOnlyRootFilesystem; name contains `postgres` → exempt.
        let doc = "kind: StatefulSet\nmetadata:\n  name: postgres\nspec:\n  template:\n    spec:\n      securityContext:\n        runAsNonRoot: true\n      containers:\n        - name: pg\n          securityContext:\n            allowPrivilegeEscalation: false\n            capabilities:\n              drop:\n                - ALL\n";
        let findings = check_security_context(doc, "infra/services/postgres");
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn check_security_context_skips_non_workload_kinds() {
        let doc = "kind: ConfigMap\nmetadata:\n  name: nothing\ndata:\n  foo: bar\n";
        let findings = check_security_context(doc, "infra/services/x");
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn check_empty_secret_data_finds_empty_string_value() {
        let doc = "kind: Secret\nmetadata:\n  name: db-secret\ndata:\n  username: dXNlcg==\n  password: \"\"\n";
        let findings = check_empty_secret_data(doc, "infra/services/ac-service");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].detail.contains("password"));
        // Critical: never echo the value.
        assert!(!findings[0].detail.contains("dXNlcg=="));
    }

    #[test]
    fn check_empty_secret_data_finds_unset_value() {
        let doc = "kind: Secret\nmetadata:\n  name: api-secret\nstringData:\n  api_key:\n  other: present\n";
        let findings = check_empty_secret_data(doc, "infra/services/gc-service");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].detail.contains("api_key"));
    }

    #[test]
    fn check_empty_secret_data_finds_single_quoted_empty() {
        let doc = "kind: Secret\nmetadata:\n  name: s\ndata:\n  k: ''\n";
        let findings = check_empty_secret_data(doc, "x");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn check_empty_secret_data_skips_non_secret() {
        let doc = "kind: ConfigMap\nmetadata:\n  name: cm\ndata:\n  empty: \"\"\n";
        let findings = check_empty_secret_data(doc, "x");
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn contains_all_capability_line_loose_quotes() {
        assert!(contains_all_capability_line("            - ALL\n"));
        assert!(contains_all_capability_line("            - \"ALL\"\n"));
        assert!(contains_all_capability_line("            - 'ALL'\n"));
        assert!(!contains_all_capability_line("            - NET_ADMIN\n"));
    }

    #[test]
    fn parse_indented_yaml_kv_basic() {
        assert_eq!(
            parse_indented_yaml_kv("  username: foo"),
            Some(("username".to_string(), " foo".to_string()))
        );
        assert_eq!(
            parse_indented_yaml_kv("  api_key:"),
            Some(("api_key".to_string(), String::new()))
        );
    }

    #[test]
    fn parse_indented_yaml_kv_rejects_top_level_and_list_items() {
        assert_eq!(parse_indented_yaml_kv("kind: Foo"), None);
        assert_eq!(parse_indented_yaml_kv("  - listy"), None);
        assert_eq!(parse_indented_yaml_kv("  # comment"), None);
    }
}
