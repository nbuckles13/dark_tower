//! `kustomize` subcommand — port of
//! `scripts/guards/simple/validate-kustomize.sh`.
//!
//! Validates Kustomize infrastructure for the Dark Tower project. Six
//! checks (R-15..R-20), with tool-availability gating:
//!
//! * **R-15** (build-dependent) — `kustomize build` for all bases + overlays.
//! * **R-16** (local) — orphan manifest detection (per-service `*.yaml` in
//!   `kustomization.yaml::resources`).
//! * **R-17** (build- + tool-dependent) — `kubeconform` schema validation
//!   on rendered output.
//! * **R-18** (build-dependent) — security-context invariants on rendered
//!   Deployment/StatefulSet (runAsNonRoot, allowPrivilegeEscalation,
//!   capabilities.drop ALL, readOnlyRootFilesystem with bash-parity
//!   exemptions for postgres/prometheus/loki/grafana).
//! * **R-19** (build-dependent) — empty-value detection on rendered Secret
//!   `data:`/`stringData:` keys. Reports key names ONLY; never echoes
//!   values.
//! * **R-20** (local) — dashboard JSON coverage in Grafana
//!   `configMapGenerator` (bidirectional).
//!
//! Per @operations F1 + @team-lead fix-in-loop 2026-05-22: when
//! `kustomize`/`kubectl kustomize`/`kubeconform` are absent, each affected
//! check degrades to WARN (devloop containers may lack kubeconform). Never
//! FAIL on missing tools. Local-only checks always run.

use crate::common::explain::{print_finding, Finding};
use crate::common::scan::warn_skip;
use crate::common::status::emit_ok;
use crate::kustomize_tools::{
    check_empty_secret_data, check_security_context, detect_kubeconform, detect_kustomize_tool,
    run_kubeconform, run_kustomize_build, KustomizeTool,
};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const BUILD_FAILED_RULE_ID: &str = "build_failed";
pub const KUBECONFORM_FAILED_RULE_ID: &str = "kubeconform_failed";
pub const SECURITY_CONTEXT_RULE_ID: &str = "security_context";
pub const EMPTY_SECRET_RULE_ID: &str = "empty_secret_value";
pub const ORPHAN_MANIFEST_RULE_ID: &str = "orphan_manifest";
pub const DASHBOARD_ORPHAN_RULE_ID: &str = "dashboard_orphan";

const SERVICE_BASES: &[&str] = &[
    "ac-service",
    "gc-service",
    "mc-service",
    "mh-service",
    "postgres",
    "redis",
];

const ORPHAN_EXCLUSIONS: &[&str] = &["kustomization.yaml", "service-monitor.yaml"];

/// Return the content of `line` with any inline `#` comment removed, trimmed.
///
/// # This is the whole mitigation for a class, not a helper for one caller
///
/// Both list parsers below are line-oriented, not YAML parses, and both decide
/// membership with a suffix test on the tail of the line. A trailing comment —
/// `- secret.yaml   # secrets ONLY` — leaves that tail ending in the comment
/// text rather than in `.yaml`/`.json`, so the entry is never recorded as
/// declared and the coverage walk then reports **a listed file as unlisted**.
/// That message is true-shaped and points at the wrong artifact: it names the
/// manifest, not the comment that actually broke the parse, so a responder goes
/// and edits an innocent file.
///
/// Found 2026-09-03 by @operations when an inline comment added during review
/// turned Layer 3 red. It is the third instance in that devloop of one class —
/// a comment or a reformat silently defeating a guard's line-oriented parser
/// (the others being `env_config`'s `MissingEnvVar` regex versus a rustfmt wrap,
/// and a verification step that string-matched a since-reflowed line). Because
/// the class is what recurs, the repair is shared by every parser that can trip
/// on it rather than applied at the reported site. Tracked in `docs/TODO.md`
/// §Infrastructure Validation in Devloops.
fn strip_inline_comment(line: &str) -> &str {
    line.split_once('#')
        .map_or(line, |(before, _)| before)
        .trim()
}

/// Collect the `*.yaml` filenames a kustomization declares under `resources:`.
///
/// A *standalone* comment line is harmless here because it never starts with
/// `- ` — which is why the long-standing `# service-monitor.yaml NOT listed …`
/// note in several kustomizations has always been safe. Same character,
/// different position, and only one position was safe. Nothing in those files
/// signalled the distinction, so the parser is the right place to remove it.
/// (That asymmetry does **not** carry over to
/// [`extract_declared_dashboards`] — see its own note.)
fn extract_declared_resources(kustomization_content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in kustomization_content.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("- ") else {
            continue;
        };
        // Strip an inline comment BEFORE the suffix test — see the rustdoc.
        let rest = strip_inline_comment(rest);
        if rest.ends_with(".yaml") {
            out.push(rest.to_string());
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

/// R-16: orphan-manifest check. For each `infra/services/<svc>/`, every
/// `*.yaml` (excluding `kustomization.yaml` + `service-monitor.yaml`) must
/// be listed in `kustomization.yaml::resources`.
fn check_orphan_manifests(repo_root: &Path) -> Result<Vec<Hit>> {
    let mut hits: Vec<Hit> = Vec::new();
    let services_dir = repo_root.join("infra/services");
    for svc in SERVICE_BASES {
        let svc_dir = services_dir.join(svc);
        let kustomization = svc_dir.join("kustomization.yaml");
        if !kustomization.is_file() {
            // Bash today flags this as a violation; preserve.
            hits.push(Hit {
                rule_id: ORPHAN_MANIFEST_RULE_ID,
                detail: format!("infra/services/{svc}/ missing kustomization.yaml"),
                file: svc_dir
                    .strip_prefix(repo_root)
                    .unwrap_or(&svc_dir)
                    .to_path_buf(),
            });
            continue;
        }
        let kust_content = std::fs::read_to_string(&kustomization)
            .with_context(|| format!("reading {}", kustomization.display()))?;
        let declared = extract_declared_resources(&kust_content);

        // Walk one level of `*.yaml` under svc_dir.
        let entries = match std::fs::read_dir(&svc_dir) {
            Ok(e) => e,
            Err(e) => {
                warn_skip("read svc dir", &svc_dir, &e);
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.ends_with(".yaml") {
                continue;
            }
            if ORPHAN_EXCLUSIONS.contains(&name) {
                continue;
            }
            if !declared.contains(&name.to_string()) {
                hits.push(Hit {
                    rule_id: ORPHAN_MANIFEST_RULE_ID,
                    detail: format!("{name} not listed in {svc}/kustomization.yaml"),
                    file: path.strip_prefix(repo_root).unwrap_or(&path).to_path_buf(),
                });
            }
        }
    }
    Ok(hits)
}

/// Collect the dashboard basenames the Grafana kustomization declares under a
/// `configMapGenerator` entry's `files:`.
///
/// # Two repairs, because this parser failed in both directions
///
/// It takes the shared [`strip_inline_comment`] for the same reason
/// [`extract_declared_resources`] does. It additionally anchors on the `- `
/// bullet, which that sibling got for free and this one did not: the original
/// admitted **any** line containing a `/` whose tail ended in `.json`, so a
/// standalone comment naming a dashboard path *declared* it. That direction is
/// **fail-open** — a genuinely-unlisted dashboard mentioned in a nearby
/// comment passes R-20 silently — and its mirror invents a direction-2 finding
/// against a path that only ever existed in prose.
///
/// So the "standalone comments were always safe" reasoning recorded on the
/// sibling was a property of *that* parser's `- ` anchor, not of comments. Two
/// parsers, one mechanism, two different exposures; fixing only the reported
/// one would have left the fail-open half in place.
fn extract_declared_dashboards(kustomization_content: &str) -> Vec<String> {
    extract_declared_generator_files(kustomization_content, ".json")
}

/// The mechanism behind [`extract_declared_dashboards`], parameterised on the
/// suffix so the alert-rules loading check can share it instead of growing a
/// third parser over the same file shape.
///
/// **Generalised on the extension only.** Both repairs above are properties of
/// the *line shape*, not of the extension, so they carry over unchanged — which
/// is the whole reason to share rather than copy. The alert-rules ConfigMap
/// entries are `- rules/gc-alerts.yaml`, the dashboards' are
/// `- foo.json=../../../grafana/dashboards/foo.json`; both are `- ` bullets
/// whose tail after the last `/` is the basename, and both sit in files that
/// carry explanatory comments.
///
/// The `/` requirement is inherited deliberately: it is what keeps a bare
/// `- some.yaml` under `resources:` from being read as a generator entry.
pub(crate) fn extract_declared_generator_files(
    kustomization_content: &str,
    suffix: &str,
) -> Vec<String> {
    let mut declared: Vec<String> = Vec::new();
    for line in kustomization_content.lines() {
        let Some(rest) = line.trim().strip_prefix("- ") else {
            continue;
        };
        let rest = strip_inline_comment(rest);
        let Some(idx) = rest.rfind('/') else {
            continue;
        };
        let candidate = &rest[idx + 1..];
        if candidate.ends_with(suffix) {
            declared.push(candidate.to_string());
        }
    }
    declared
}

/// R-20: dashboard coverage. Every `*.json` in `infra/grafana/dashboards/`
/// (excluding `_template-*.json`) must appear in the Grafana kustomization;
/// every kustomization reference must exist on disk.
fn check_dashboard_coverage(repo_root: &Path) -> Result<Vec<Hit>> {
    let mut hits: Vec<Hit> = Vec::new();
    let dashboards = repo_root.join("infra/grafana/dashboards");
    let kustomization = repo_root.join("infra/grafana/kustomization.yaml");
    if !dashboards.is_dir() || !kustomization.is_file() {
        return Ok(hits);
    }
    let kust_content = std::fs::read_to_string(&kustomization)
        .with_context(|| format!("reading {}", kustomization.display()))?;
    let mut declared = extract_declared_dashboards(&kust_content);
    declared.sort();
    declared.dedup();

    // Walk actual dashboards.
    let mut actual: Vec<String> = Vec::new();
    let entries = match std::fs::read_dir(&dashboards) {
        Ok(e) => e,
        Err(e) => {
            warn_skip("dashboards dir", &dashboards, &e);
            return Ok(hits);
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.ends_with(".json") || name.starts_with("_template-") {
            continue;
        }
        actual.push(name.to_string());
    }
    actual.sort();
    actual.dedup();

    // Direction 1: actual NOT in declared.
    for name in &actual {
        if !declared.contains(name) {
            hits.push(Hit {
                rule_id: DASHBOARD_ORPHAN_RULE_ID,
                detail: format!(
                    "{name} exists in infra/grafana/dashboards/ but not in configMapGenerator"
                ),
                file: PathBuf::from(format!("infra/grafana/dashboards/{name}")),
            });
        }
    }
    // Direction 2: declared NOT in actual.
    for name in &declared {
        if !actual.contains(name) {
            hits.push(Hit {
                rule_id: DASHBOARD_ORPHAN_RULE_ID,
                detail: format!("configMapGenerator references {name} but file does not exist"),
                file: PathBuf::from("infra/grafana/kustomization.yaml"),
            });
        }
    }
    Ok(hits)
}

/// Enumerate the kustomization directories bash today builds (per
/// `validate-kustomize.sh:108-145`): per-service bases under
/// `infra/services/<svc>/`, the observability base, per-service overlays
/// under `infra/kubernetes/overlays/kind/services/<svc>/`, and the
/// observability overlay.
fn build_targets(repo_root: &Path) -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    let services_dir = repo_root.join("infra/services");
    let overlays_dir = repo_root.join("infra/kubernetes/overlays/kind");
    let obs_base = repo_root.join("infra/kubernetes/observability");
    let obs_overlay = overlays_dir.join("observability");
    for svc in SERVICE_BASES {
        let base = services_dir.join(svc);
        if base.is_dir() {
            out.push((base, format!("base: infra/services/{svc}")));
        }
        let overlay = overlays_dir.join("services").join(svc);
        if overlay.is_dir() {
            out.push((overlay, format!("overlay: overlays/kind/services/{svc}")));
        }
    }
    if obs_base.is_dir() {
        out.push((obs_base, "base: infra/kubernetes/observability".to_string()));
    }
    if obs_overlay.is_dir() {
        out.push((
            obs_overlay,
            "overlay: overlays/kind/observability".to_string(),
        ));
    }
    out
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let infra_root = repo_root.join("infra");
    if !infra_root.is_dir() {
        emit_ok("kustomize-no-infra-dir");
        return Ok(());
    }

    let tool = detect_kustomize_tool();
    let has_kubeconform = detect_kubeconform();

    let mut all_hits: Vec<Hit> = Vec::new();

    // R-16 + R-20 — local checks, always run.
    all_hits.extend(check_orphan_manifests(repo_root)?);
    all_hits.extend(check_dashboard_coverage(repo_root)?);

    // R-15/R-17/R-18/R-19 — build-dependent. Gate on `tool`.
    if let Some(tool) = tool {
        run_build_dependent_checks(repo_root, tool, has_kubeconform, &mut all_hits)?;
    } else {
        warn_skip(
            "kustomize tool absent",
            &infra_root,
            &std::io::Error::other(
                "neither `kustomize` nor `kubectl kustomize` available; R-15/R-17/R-18/R-19 skipped",
            ),
        );
    }
    if tool.is_some() && !has_kubeconform {
        warn_skip(
            "kubeconform absent",
            &infra_root,
            &std::io::Error::other("kubeconform not installed; R-17 schema validation skipped"),
        );
    }

    if all_hits.is_empty() {
        match (tool, has_kubeconform) {
            (Some(_), true) => emit_ok("kustomize-clean"),
            (Some(_), false) => emit_ok("kustomize-clean-kubeconform-skipped"),
            (None, _) => emit_ok("kustomize-tool-absent-skipped"),
        }
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("kustomize::{}", hit.rule_id);
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

    // REASON token grouped by failure-shape class: report the largest
    // class (deterministic ordering — R-15 build > R-17 schema > R-18
    // securityContext > R-19 empty-secret > R-16 orphan > R-20 dashboard).
    let by_kind = |id: &str| all_hits.iter().filter(|h| h.rule_id == id).count();
    let build = by_kind(BUILD_FAILED_RULE_ID);
    let schema = by_kind(KUBECONFORM_FAILED_RULE_ID);
    let sec = by_kind(SECURITY_CONTEXT_RULE_ID);
    let secrets = by_kind(EMPTY_SECRET_RULE_ID);
    let orphan = by_kind(ORPHAN_MANIFEST_RULE_ID);
    let dash = by_kind(DASHBOARD_ORPHAN_RULE_ID);
    let max_class = [
        ("kustomize-build-failed", build),
        ("kustomize-kubeconform-failed", schema),
        ("kustomize-security-context", sec),
        ("kustomize-empty-secret-value", secrets),
        ("kustomize-orphan-manifest", orphan),
        ("kustomize-dashboard-orphan", dash),
    ]
    .into_iter()
    .max_by_key(|(_, n)| *n)
    .unwrap_or(("kustomize-violations", 0));
    anyhow::bail!("{}-{}", max_class.0, max_class.1)
}

/// Execute R-15 → R-17 → R-18 → R-19 in sequence per build target. R-15
/// failures are recorded but do NOT short-circuit subsequent targets;
/// downstream checks (R-17/R-18/R-19) silently skip the affected target
/// (their input is the missing rendered stdout). Bash today: same shape.
fn run_build_dependent_checks(
    repo_root: &Path,
    tool: KustomizeTool,
    has_kubeconform: bool,
    all_hits: &mut Vec<Hit>,
) -> Result<()> {
    for (dir, label) in build_targets(repo_root) {
        // R-15: kustomize build.
        let build = run_kustomize_build(tool, &dir)?;
        let rel = dir.strip_prefix(repo_root).unwrap_or(&dir).to_path_buf();
        if !build.success {
            let detail = if build.stderr_head.is_empty() {
                format!("{label} — kustomize build failed")
            } else {
                format!("{label} — kustomize build failed\n{}", build.stderr_head)
            };
            all_hits.push(Hit {
                rule_id: BUILD_FAILED_RULE_ID,
                detail,
                file: rel,
            });
            continue;
        }
        let rendered = &build.stdout;

        // R-17: kubeconform — only if available.
        if has_kubeconform {
            let result = run_kubeconform(rendered)?;
            if !result.success {
                let detail = if result.stderr_head.is_empty() {
                    format!("{label} — kubeconform schema validation failed")
                } else {
                    format!(
                        "{label} — kubeconform schema validation failed\n{}",
                        result.stderr_head
                    )
                };
                all_hits.push(Hit {
                    rule_id: KUBECONFORM_FAILED_RULE_ID,
                    detail,
                    file: dir.strip_prefix(repo_root).unwrap_or(&dir).to_path_buf(),
                });
            }
        }

        // R-18: security-context — only on base builds (bash today's
        // `check_security_contexts $TMPDIR_BUILD/base-*.yaml` shape). The
        // overlays inherit container specs from the bases they reference;
        // checking the overlay-rendered output would double-flag every
        // base finding without surfacing new ones.
        let is_base = !label.starts_with("overlay:");
        if is_base {
            for f in check_security_context(rendered, &label) {
                all_hits.push(Hit {
                    rule_id: SECURITY_CONTEXT_RULE_ID,
                    detail: format!("{} — {}", f.resource, f.detail),
                    file: dir.strip_prefix(repo_root).unwrap_or(&dir).to_path_buf(),
                });
            }

            // R-19: empty-secret-data — bash today scans base builds only,
            // same rationale (overlays reference the same Secrets).
            for f in check_empty_secret_data(rendered, &label) {
                all_hits.push(Hit {
                    rule_id: EMPTY_SECRET_RULE_ID,
                    detail: format!("{} — {}", f.resource, f.detail),
                    file: dir.strip_prefix(repo_root).unwrap_or(&dir).to_path_buf(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_declared_resources_finds_yaml_bullets() {
        let kust = r#"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
resources:
  - deployment.yaml
  - service.yaml
  - configmap.yaml
"#;
        let out = extract_declared_resources(kust);
        assert_eq!(out.len(), 3);
        assert!(out.contains(&"deployment.yaml".to_string()));
    }

    /// A resource entry carrying a trailing `#` comment is still declared.
    ///
    /// Regression pin for the 2026-09-03 defect: without the `#` strip this
    /// returns 1, and the orphan walk then reports `secret.yaml` and
    /// `service.yaml` as unlisted when both are plainly listed — a true-shaped
    /// finding naming the wrong file. Asserted as an EXACT set rather than a
    /// count so that stripping too much (e.g. eating the whole line) fails here
    /// too, not just stripping too little.
    #[test]
    fn extract_declared_resources_strips_inline_comments() {
        let kust = r#"resources:
  - deployment.yaml
  - service.yaml   # per-instance NodePorts
  - secret.yaml# no space before the hash
  # standalone comments were always safe: they never start with "- "
  # service-monitor.yaml NOT listed - requires Prometheus Operator CRD
"#;
        let mut out = extract_declared_resources(kust);
        out.sort();
        assert_eq!(
            out,
            vec![
                "deployment.yaml".to_string(),
                "secret.yaml".to_string(),
                "service.yaml".to_string(),
            ]
        );
    }

    /// R-20's parser is the SAME line-oriented mechanism as R-16's, so it takes
    /// the same two repairs — the `- ` anchor and the inline-comment strip.
    ///
    /// It was the more dangerous of the two before this pin, because it failed
    /// in BOTH directions where R-16 failed in one:
    ///
    /// * **Fail-loud, wrong artifact** — an inline comment on a `files:` entry
    ///   leaves the tail after the last `/` ending in the comment text, so the
    ///   dashboard is not recorded as declared and direction 1 reports a listed
    ///   dashboard as an orphan.
    /// * **FAIL-OPEN** — the old parser anchored on nothing but "a line
    ///   containing `/` whose tail ends in `.json`", so a *standalone comment*
    ///   naming a dashboard path declared it. A genuinely-unlisted dashboard
    ///   mentioned in a nearby comment therefore passed R-20 silently, and a
    ///   comment naming a path that does not exist invented a direction-2
    ///   finding against a file nobody had touched.
    ///
    /// The standalone-comment half has no R-16 counterpart: there, a comment
    /// never starts with `- `. That asymmetry is exactly why fixing the
    /// reported instance alone was insufficient.
    #[test]
    fn extract_declared_dashboards_anchors_on_bullets_and_strips_comments() {
        let kust = r#"configMapGenerator:
  - name: grafana-dashboards-mc
    files:
      - mc-logs.json=dashboards/mc-logs.json
      - mc-overview.json=dashboards/mc-overview.json  # new media row, task 14
      # mc-legacy.json is deliberately unlisted: see dashboards/mc-legacy.json
"#;
        let mut out = extract_declared_dashboards(kust);
        out.sort();
        assert_eq!(
            out,
            vec!["mc-logs.json".to_string(), "mc-overview.json".to_string()],
            "inline comment must not un-declare a listed dashboard, and a \
             standalone comment must not declare an unlisted one"
        );
    }

    /// The generalisation must not change R-20's behaviour — it changes a
    /// function whose two repairs were earned from live failures, and "a shared
    /// helper that quietly changes its original caller" is the classic cost of a
    /// good extraction. Pinned mechanically rather than by reading the diff.
    ///
    /// Asserts the two properties that make the parameterisation a no-op for
    /// the dashboards path: the wrapper is exactly the helper at `".json"`, and
    /// the suffix is genuinely discriminating rather than incidentally
    /// satisfied by every entry.
    #[test]
    fn generalisation_is_behaviour_preserving_for_the_dashboards_path() {
        let kust = r#"configMapGenerator:
  - name: grafana-dashboards-mc
    files:
      - mc-logs.json=dashboards/mc-logs.json
      - mc-overview.json=dashboards/mc-overview.json  # new media row, task 14
      # mc-legacy.json is deliberately unlisted: see dashboards/mc-legacy.json
resources:
  - rules/mc-alerts.yaml
"#;
        assert_eq!(
            extract_declared_dashboards(kust),
            extract_declared_generator_files(kust, ".json"),
            "the wrapper must be exactly the helper at \".json\""
        );
        // The suffix discriminates: the `.yaml` bullet above is invisible to the
        // dashboards call and visible to a `.yaml` one. Without this, the
        // parameterisation could be vacuous and the equality above trivial.
        assert_eq!(
            extract_declared_dashboards(kust),
            vec!["mc-logs.json".to_string(), "mc-overview.json".to_string()]
        );
        assert_eq!(
            extract_declared_generator_files(kust, "-alerts.yaml"),
            vec!["mc-alerts.yaml".to_string()]
        );
    }

    #[test]
    fn orphan_exclusions_skipped() {
        assert!(ORPHAN_EXCLUSIONS.contains(&"kustomization.yaml"));
        assert!(ORPHAN_EXCLUSIONS.contains(&"service-monitor.yaml"));
    }
}
