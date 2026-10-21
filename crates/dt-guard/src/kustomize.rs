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
//! * **R-21** (build-dependent, FAIL-closed) — every pod-consumed ConfigMap is
//!   content-addressed (ADR-0038 §2); see `kustomize_content_addressing`.
//!
//! Plus the **downward-API resource rule** (build-dependent; story 3, policy
//! content operations): every `resourceFieldRef` names a resource its target
//! container declares — see [`crate::kustomize_resource_field_refs`]. It runs
//! over the rendered bases and the environment root.
//!
//! Service bases are DERIVED (`common::services::infra_service_bases`, every
//! directory under `infra/services/`), never listed here, so a new base —
//! including a generator-only one such as `media-codec-profile/` — is built and
//! orphan-checked with no special case.
//!
//! Plus one local check over generated ConfigMaps, in
//! [`crate::kustomize_configmaps`] (added at story 2 task 12; no story
//! R-number): **`configmap_annotation_size`** (every generator under `infra/`
//! stays under 80% of Kubernetes' 262144-byte annotation cap, measured as
//! client-side apply actually serialises it). It always runs and needs no
//! kustomize binary.
//!
//! Per @operations F1 + @team-lead fix-in-loop 2026-05-22: when
//! `kustomize`/`kubectl kustomize`/`kubeconform` are absent, each affected
//! check degrades to WARN (devloop containers may lack kubeconform). Local-only
//! checks always run. **Exception: R-21 FAILs when no kustomize tool exists**
//! (`content_addressing_unverifiable`) — it is the only enforcement of the
//! ADR-0038 content-addressing invariant, and a WARN would be a skipped gate.
//! Layer 3 always has one: the devloop image installs kubectl
//! (`infra/devloop/Dockerfile`) and the GitHub ubuntu runners ship it.

use crate::common::explain::{print_finding, Finding};
use crate::common::kustomize_generators::parse_config_map_generators;
use crate::common::kustomize_generators::shared_generator_index;
use crate::common::scan::warn_skip;
use crate::common::services::{infra_service_bases, INFRA_SERVICES_DIR};
use crate::common::status::emit_ok;
use crate::kustomize_content_addressing::{
    check_rendered, declared_generator_names, RenderContext, NOT_CONTENT_ADDRESSED_RULE_ID,
    UNREFERENCED_CONFIGMAP_RULE_ID, UNRESOLVED_REFERENCE_RULE_ID, UNVERIFIABLE_RULE_ID,
    VACUOUS_RULE_ID,
};
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

// Service bases are NOT listed here: R-15/R-16 enumerate every base under
// `infra/services/` through `common::services::infra_service_bases`, the ONE
// enumeration (a hand list here once had to be remembered for every new base —
// e.g. the generator-only `media-codec-profile/` — and missed silently). That
// includes the migration Job's base, `db-migrate/` (ADR-0038 §2): applied ahead
// of the root, not part of it, so it is built, orphan-checked and
// schema-checked here.

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
///
/// Visible to the crate (not just this module) for exactly that reason: it is
/// the single home for this class, so any new line-oriented parser over the
/// same file shape imports it rather than carrying its own copy, which would be
/// a copy that can drift out of the mitigation while looking like it has it.
/// (`kustomize_configmaps` used to be such a parser; it now reads generators
/// through the YAML parser in [`crate::common::kustomize_generators`].)
pub(crate) fn strip_inline_comment(line: &str) -> &str {
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
/// be listed in `kustomization.yaml::resources` or be a `configMapGenerator`
/// source.
fn check_orphan_manifests(repo_root: &Path) -> Result<Vec<Hit>> {
    let mut hits: Vec<Hit> = Vec::new();
    let services_dir = repo_root.join(INFRA_SERVICES_DIR);
    for svc in infra_service_bases(repo_root)? {
        let svc_dir = services_dir.join(&svc);
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
        // Containment-gated read (THE one base-kustomization reader).
        let Some(kust_content) =
            crate::common::kustomize_generators::read_base_kustomization(repo_root, &svc)?
        else {
            anyhow::bail!("infra/services/{svc}/kustomization.yaml vanished while being read");
        };
        let mut declared = extract_declared_resources(&kust_content);
        // Generator sources (ADR-0038 §2: `collector.yaml`, `*.env`, …) are
        // declared too — read through the ONE structured generator parser.
        match parse_config_map_generators(&kust_content) {
            Ok(entries) => {
                for e in &entries {
                    declared.extend(e.source_paths().map(str::to_string));
                }
            }
            Err(e) => hits.push(Hit {
                rule_id: ORPHAN_MANIFEST_RULE_ID,
                detail: format!("{svc}/kustomization.yaml: configMapGenerator unreadable: {e:#}"),
                file: kustomization
                    .strip_prefix(repo_root)
                    .unwrap_or(&kustomization)
                    .to_path_buf(),
            }),
        }

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
///
/// For `configMapGenerator` name/envs/files/literals resolution use
/// [`crate::common::kustomize_generators`], the structured YAML parser R-16,
/// R-21 and env-config moved to. This line scanner is retained only for R-20
/// dashboards + alert-rules, whose `- name=/path` (`/`-bullet basename)
/// property it documents above.
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
fn build_targets(repo_root: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    let services_dir = repo_root.join(INFRA_SERVICES_DIR);
    let overlays_dir = repo_root.join("infra/kubernetes/overlays/kind");
    let obs_base = repo_root.join("infra/kubernetes/observability");
    let obs_overlay = overlays_dir.join("observability");
    for svc in infra_service_bases(repo_root)? {
        let base = services_dir.join(&svc);
        if base.is_dir() {
            out.push((base, format!("base: infra/services/{svc}")));
        }
        let overlay = overlays_dir.join("services").join(&svc);
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
    // The environment root itself — what deploy.sh applies (ADR-0038 §2).
    if overlays_dir.is_dir() {
        out.push((overlays_dir, ENV_ROOT_LABEL.to_string()));
    }
    Ok(out)
}

/// Label of the Kind environment-root build target; R-21's vacuity check and
/// the OK status line's consumed-ConfigMap count are taken from this target.
const ENV_ROOT_LABEL: &str = "overlay: overlays/kind (environment root)";

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let infra_root = repo_root.join("infra");
    if !infra_root.is_dir() {
        emit_ok("kustomize-no-infra-dir");
        return Ok(());
    }

    let tool = detect_kustomize_tool();
    let has_kubeconform = detect_kubeconform();

    let mut all_hits: Vec<Hit> = Vec::new();

    // R-16 + R-20 + generated-ConfigMap checks — local, always run.
    all_hits.extend(check_orphan_manifests(repo_root)?);
    all_hits.extend(check_dashboard_coverage(repo_root)?);
    all_hits.extend(
        crate::kustomize_configmaps::check(repo_root)?
            .into_iter()
            .map(|f| Hit {
                rule_id: f.rule_id,
                detail: f.detail,
                file: f.file,
            }),
    );

    // R-15/R-17/R-18/R-19/R-21 — build-dependent. Gate on `tool`.
    let mut consumed_configmaps = 0;
    let mut root_resource_field_refs = 0;
    if let Some(tool) = tool {
        (consumed_configmaps, root_resource_field_refs) =
            run_build_dependent_checks(repo_root, tool, has_kubeconform, &mut all_hits)?;
    } else {
        warn_skip(
            "kustomize tool absent",
            &infra_root,
            &std::io::Error::other(
                "neither `kustomize` nor `kubectl kustomize` available; R-15/R-17/R-18/R-19 skipped",
            ),
        );
        // R-21 does NOT degrade: see the module doc.
        all_hits.push(no_tool_hit());
    }
    if tool.is_some() && !has_kubeconform {
        warn_skip(
            "kubeconform absent",
            &infra_root,
            &std::io::Error::other("kubeconform not installed; R-17 schema validation skipped"),
        );
    }

    if all_hits.is_empty() {
        // `tool` is necessarily Some here: its absence is an R-21 hit.
        if has_kubeconform {
            emit_ok(format!(
                "kustomize-clean-{consumed_configmaps}-consumed-configmaps-{root_resource_field_refs}-resource-field-refs"
            ));
        } else {
            emit_ok(format!(
                "kustomize-clean-kubeconform-skipped-{consumed_configmaps}-consumed-configmaps-{root_resource_field_refs}-resource-field-refs"
            ));
        }
        return Ok(());
    }

    // "Could not check" lines first, as `ERROR: PRECONDITION`, so they survive
    // head-truncated re-emission (docs/runbooks/devloop-validation.md §6.3
    // vacuity convention).
    for hit in all_hits
        .iter()
        .filter(|h| h.rule_id == UNVERIFIABLE_RULE_ID || h.rule_id == VACUOUS_RULE_ID)
    {
        eprintln!(
            "ERROR: PRECONDITION [{}] {} — {}",
            hit.rule_id,
            hit.file.display(),
            hit.detail
        );
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

    // REASON token grouped by failure-shape class (see `fail_token`).
    anyhow::bail!("{}", fail_token(&all_hits))
}

/// The hit a run without any kustomize tool always carries. FATAL (it is a
/// "could not check" class in [`fail_token`]), and that fatality is what keeps
/// EVERY build-dependent rule from passing silently on a tool-less host —
/// R-21 and the downward-API resource rule
/// (`kustomize_resource_field_refs`, whose SOUNDNESS note depends on it).
fn no_tool_hit() -> Hit {
    Hit {
        rule_id: UNVERIFIABLE_RULE_ID,
        detail: "neither `kustomize` nor `kubectl kustomize` is available, so the environment root cannot be rendered: pod-consumed ConfigMaps cannot be verified content-addressed (R-21) and downward-API resource refs cannot be checked — both fail closed".to_string(),
        file: PathBuf::from("infra"),
    }
}

/// R-21 cross-base deferrals that nothing verified: when the environment root
/// did not render in this run, every deferred reference is a finding.
fn unverified_deferral_hits(root_rendered: bool, deferred: Vec<(PathBuf, String)>) -> Vec<Hit> {
    if root_rendered {
        return Vec::new();
    }
    deferred
        .into_iter()
        .map(|(file, d)| Hit {
            rule_id: UNRESOLVED_REFERENCE_RULE_ID,
            detail: format!(
                "{d} — deferred to the environment root, which did not render in this run, so the cross-base reference was never verified"
            ),
            file,
        })
        .collect()
}

/// Select the single REASON token for a failing R-15..R-21 run from the
/// accumulated hits. Extracted pure so the fail-closed precedence and the exact
/// emitted strings — which runbooks and the OK/skip enum consumers key on — are
/// unit-testable WITHOUT a kustomize tool on PATH. R-21's unverifiable branch
/// is unreachable in a normal Layer-3 run (the devloop image always ships
/// kubectl), so a test on `run()` cannot exercise it; an untested fail-closed
/// branch is an assertion, not a control (cf. `env_config::clean_outcome`,
/// extracted for exactly this reason).
///
/// Precedence: a "could not check" class (UNVERIFIABLE, then VACUOUS) outranks
/// any count of content findings — it means there is unchecked surface, so it
/// names the run whenever present. Otherwise the largest content-failure class
/// wins, in the deterministic order R-15 build > R-17 schema > R-18
/// securityContext > R-19 empty-secret > R-16 orphan > R-20 dashboard > R-21
/// content-addressing > the generated-ConfigMap size check > the downward-API
/// resource rule.
///
/// # Ties go to the FIRST class listed, which needs saying in code
///
/// The order is a ranking: a build failure means the later checks ran on
/// nothing trustworthy, so it is the reason worth reporting. `max_by_key`
/// alone returns the LAST maximum, so one build failure plus one
/// annotation-size finding reported `kustomize-configmap-annotation-size` —
/// the ordering comment was true of the list and false of the selection.
/// `Reverse(index)` as the secondary key makes the first listed maximum win.
fn fail_token(all_hits: &[Hit]) -> String {
    let by_kind = |id: &str| all_hits.iter().filter(|h| h.rule_id == id).count();
    // "Could not check" outranks any count of content findings: it means there
    // is unchecked surface, so it names the run whenever present.
    for (id, token) in [
        (
            UNVERIFIABLE_RULE_ID,
            "kustomize-content-addressing-unverifiable",
        ),
        (VACUOUS_RULE_ID, "kustomize-content-addressing-vacuous"),
    ] {
        let n = by_kind(id);
        if n > 0 {
            return format!("{token}-{n}");
        }
    }
    let content_addressing = by_kind(NOT_CONTENT_ADDRESSED_RULE_ID)
        + by_kind(UNRESOLVED_REFERENCE_RULE_ID)
        + by_kind(UNREFERENCED_CONFIGMAP_RULE_ID);
    let (class, count) = [
        ("kustomize-build-failed", by_kind(BUILD_FAILED_RULE_ID)),
        (
            "kustomize-kubeconform-failed",
            by_kind(KUBECONFORM_FAILED_RULE_ID),
        ),
        (
            "kustomize-security-context",
            by_kind(SECURITY_CONTEXT_RULE_ID),
        ),
        (
            "kustomize-empty-secret-value",
            by_kind(EMPTY_SECRET_RULE_ID),
        ),
        (
            "kustomize-orphan-manifest",
            by_kind(ORPHAN_MANIFEST_RULE_ID),
        ),
        (
            "kustomize-dashboard-orphan",
            by_kind(DASHBOARD_ORPHAN_RULE_ID),
        ),
        (
            "kustomize-configmap-not-content-addressed",
            content_addressing,
        ),
        (
            "kustomize-configmap-annotation-size",
            by_kind(crate::kustomize_configmaps::CONFIGMAP_ANNOTATION_SIZE_RULE_ID),
        ),
        (
            "kustomize-resource-field-ref",
            crate::kustomize_resource_field_refs::RULE_IDS
                .iter()
                .map(|id| by_kind(id))
                .sum(),
        ),
    ]
    .into_iter()
    .enumerate()
    .max_by_key(|(i, (_, n))| (*n, std::cmp::Reverse(*i)))
    .map_or(("kustomize-violations", 0), |(_, class)| class);
    format!("{class}-{count}")
}

/// Execute R-15 → R-17 → R-18 → R-19 in sequence per build target. R-15
/// failures are recorded but do NOT short-circuit subsequent targets;
/// downstream checks (R-17/R-18/R-19) silently skip the affected target
/// (their input is the missing rendered stdout). Bash today: same shape.
/// Returns, for the environment-root render, the number of distinct ConfigMaps
/// pod templates consume (R-21's status-line count) and the number of
/// downward-API `resourceFieldRef`s the resource rule examined (its positive
/// control: a blind walk shows as 0 on the OK line).
fn run_build_dependent_checks(
    repo_root: &Path,
    tool: KustomizeTool,
    has_kubeconform: bool,
    all_hits: &mut Vec<Hit>,
) -> Result<(usize, usize)> {
    let targets = build_targets(repo_root)?;
    if targets.is_empty() {
        all_hits.push(Hit {
            rule_id: VACUOUS_RULE_ID,
            detail:
                "no kustomize build targets found under infra/ — nothing rendered, nothing checked"
                    .to_string(),
            file: PathBuf::from("infra"),
        });
    }
    let declared_generators = declared_generator_names(repo_root)?;
    let shared = shared_generator_index(repo_root)?;
    let mut root_consumed: Option<usize> = None;
    let mut root_resource_field_refs = 0;
    // R-21 cross-base deferrals from non-root targets. Sound ONLY if the
    // environment root rendered in THIS run (checked after the loop).
    let mut deferred: Vec<(PathBuf, String)> = Vec::new();
    for (dir, label) in targets {
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

        // R-21: content-addressed ConfigMaps — every target, bases and
        // overlays alike (a base with a plain ConfigMap is the defect, and an
        // overlay can introduce a namespace mismatch).
        let is_env_root = label == ENV_ROOT_LABEL;
        let r21 = check_rendered(
            rendered,
            &label,
            &RenderContext {
                declared_generators: &declared_generators,
                shared: &shared,
                is_env_root,
            },
        );
        deferred.extend(r21.deferred.into_iter().map(|d| (rel.clone(), d)));
        for f in r21.findings {
            all_hits.push(Hit {
                rule_id: f.rule_id,
                detail: f.detail,
                file: rel.clone(),
            });
        }
        if label == ENV_ROOT_LABEL {
            root_consumed = Some(r21.consumed);
            if r21.consumed == 0 {
                all_hits.push(Hit {
                    rule_id: VACUOUS_RULE_ID,
                    detail: format!(
                        "{label} — rendered with zero pod-template ConfigMap references; the environment root always consumes configuration, so the reference walk saw nothing"
                    ),
                    file: rel.clone(),
                });
            }
        }

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

        // Downward-API resource rule — bases (attribution) and the
        // environment root (catches an overlay stripping a limit). See
        // `kustomize_resource_field_refs` for the SOUNDNESS dependency on
        // R-21's fail-closed unverifiable branch.
        if is_base || is_env_root {
            let rf = crate::kustomize_resource_field_refs::check_rendered(rendered, &label);
            if is_env_root {
                root_resource_field_refs = rf.checked;
            }
            for f in rf.findings {
                all_hits.push(Hit {
                    rule_id: f.rule_id,
                    detail: f.detail,
                    file: rel.clone(),
                });
            }
        }

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
    // A deferral is only a promise that the root checks the reference. If the
    // root did not render in this run (absent target, or a build failure),
    // nothing kept that promise: every deferred reference fails.
    all_hits.extend(unverified_deferral_hits(root_consumed.is_some(), deferred));
    // The root target is always built when infra/ exists; if it did not render
    // (R-15 hit) the count is 0 and the run is already red.
    Ok((root_consumed.unwrap_or(0), root_resource_field_refs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `Hit` carrying only the rule id — `fail_token` reads nothing else.
    fn hit(rule_id: &'static str) -> Hit {
        Hit {
            rule_id,
            detail: String::new(),
            file: PathBuf::new(),
        }
    }

    /// R-21's fail-closed token is the ADR-0038 invariant's only enforcement,
    /// yet its unverifiable branch never fires in a normal Layer-3 run (the
    /// devloop image always ships kubectl). These pin the exact emitted strings
    /// (runbooks key on them) and the fail-closed precedence directly, without a
    /// kustomize tool — an untested fail-closed branch is an assertion, not a
    /// control (cf. `env_config::clean_outcome`).
    #[test]
    fn fail_token_unverifiable_alone_names_the_run() {
        assert_eq!(
            fail_token(&[hit(UNVERIFIABLE_RULE_ID)]),
            "kustomize-content-addressing-unverifiable-1"
        );
    }

    #[test]
    fn fail_token_unverifiable_outranks_content_findings() {
        // "Could not check" means unchecked surface: it must name the run even
        // when content findings are also present and more numerous.
        let hits = [
            hit(UNVERIFIABLE_RULE_ID),
            hit(NOT_CONTENT_ADDRESSED_RULE_ID),
            hit(NOT_CONTENT_ADDRESSED_RULE_ID),
            hit(BUILD_FAILED_RULE_ID),
        ];
        assert_eq!(
            fail_token(&hits),
            "kustomize-content-addressing-unverifiable-1",
            "the unverifiable precedence must not be defeated by a larger content class"
        );
    }

    /// SOUNDNESS pin for the build-dependent rules that have no fail-closed
    /// branch of their own (the downward-API resource rule): a tool-less run
    /// carries the unverifiable hit, and it names the run over any content
    /// class. Make R-21's unverifiable branch non-fatal and this fails.
    #[test]
    fn no_tool_run_is_red_so_build_dependent_rules_cannot_pass_silently() {
        let h = no_tool_hit();
        assert_eq!(h.rule_id, UNVERIFIABLE_RULE_ID);
        assert!(h.detail.contains("downward-API"), "{}", h.detail);
        let hits = [
            no_tool_hit(),
            hit(crate::kustomize_resource_field_refs::RESOURCE_FIELD_REF_UNDECLARED_RULE_ID),
            hit(crate::kustomize_resource_field_refs::RESOURCE_FIELD_REF_UNDECLARED_RULE_ID),
        ];
        assert_eq!(
            fail_token(&hits),
            "kustomize-content-addressing-unverifiable-1"
        );
    }

    #[test]
    fn resource_field_findings_have_their_own_token_class() {
        let hits = [
            hit(crate::kustomize_resource_field_refs::RESOURCE_FIELD_REF_UNDECLARED_RULE_ID),
            hit(crate::kustomize_resource_field_refs::RESOURCE_FIELD_REF_UNKNOWN_CONTAINER_RULE_ID),
        ];
        assert_eq!(fail_token(&hits), "kustomize-resource-field-ref-2");
    }

    /// Security S2: a deferral that the root never verified is a finding.
    #[test]
    fn deferred_refs_fail_unless_the_root_rendered() {
        let deferred = vec![(
            PathBuf::from("infra/services/mc-service"),
            "mc-0 ref".to_string(),
        )];
        assert!(unverified_deferral_hits(true, deferred.clone()).is_empty());
        let hits = unverified_deferral_hits(false, deferred);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rule_id, UNRESOLVED_REFERENCE_RULE_ID);
        assert!(hits[0].detail.contains("never verified"));
    }

    /// 6c: a generator-only base is orphan-clean when its sources are
    /// declared, and still orphan-checked (a stray manifest fails).
    #[test]
    fn generator_only_base_is_orphan_checked_without_special_cases() {
        let td = tempfile::tempdir().expect("tempdir");
        let dir = td.path().join("infra/services/profile");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(
            dir.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: profile\n    envs: [profile.env]\n",
        )
        .expect("write");
        std::fs::write(dir.join("profile.env"), "K=v\n").expect("write");
        assert!(check_orphan_manifests(td.path()).expect("walk").is_empty());
        std::fs::write(dir.join("stray.yaml"), "kind: ConfigMap\n").expect("write");
        let hits = check_orphan_manifests(td.path()).expect("walk");
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].detail.contains("stray.yaml"));
    }

    /// The real tree: every directory under infra/services/ is a base build
    /// target — including the generator-only media-codec-profile — with no
    /// hand list to forget it from.
    #[test]
    fn every_real_service_base_is_a_build_target() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bases = infra_service_bases(&root).expect("bases");
        assert!(
            bases.contains(&"media-codec-profile".to_string()),
            "{bases:?}"
        );
        let targets = build_targets(&root).expect("targets");
        for b in &bases {
            let label = format!("base: infra/services/{b}");
            assert!(targets.iter().any(|(_, l)| *l == label), "{label} missing");
        }
        let orphans = check_orphan_manifests(&root).expect("orphan walk");
        assert!(orphans.is_empty(), "{orphans:#?}");
    }

    /// R-21 on the REAL tree's cross-base shape: the mc-service base render
    /// DEFERS its media-codec-profile refs (no finding), and the environment
    /// root resolves every one to the hashed generator. Skips VISIBLY without
    /// a kustomize tool, as R-15 does.
    #[test]
    fn real_cross_base_profile_refs_defer_in_base_and_resolve_at_root() {
        let Some(tool) = crate::kustomize_tools::detect_kustomize_tool() else {
            eprintln!("SKIP: real_cross_base_profile_refs_defer_in_base_and_resolve_at_root — no kustomize tool");
            return;
        };
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let declared = declared_generator_names(&root).expect("declared");
        let shared = shared_generator_index(&root).expect("index");
        assert!(shared.generators.contains_key("media-codec-profile"));
        let ctx = |is_env_root| RenderContext {
            declared_generators: &declared,
            shared: &shared,
            is_env_root,
        };
        let mc = crate::kustomize_tools::run_kustomize_build(
            tool,
            &root.join("infra/services/mc-service"),
        )
        .expect("build mc");
        assert!(mc.success, "{}", mc.stderr_head);
        let r = check_rendered(&mc.stdout, "mc base", &ctx(false));
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert!(
            !r.deferred.is_empty(),
            "positive control: the profile refs were deferred"
        );
        // The same render judged AS the root never defers: it must fail.
        let as_root = check_rendered(&mc.stdout, "mc as root", &ctx(true));
        assert!(as_root.deferred.is_empty());
        assert!(
            !as_root.findings.is_empty(),
            "a root without the profile must fail"
        );
        let env_root = crate::kustomize_tools::run_kustomize_build(
            tool,
            &root.join("infra/kubernetes/overlays/kind"),
        )
        .expect("build root");
        assert!(env_root.success, "{}", env_root.stderr_head);
        let r = check_rendered(&env_root.stdout, ENV_ROOT_LABEL, &ctx(true));
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert!(env_root.stdout.contains("name: media-codec-profile-"));
    }

    /// Positive control for the downward-API rule on the REAL tree: the mh
    /// base render yields the MH_POD_MEMORY_LIMIT_BYTES ref on BOTH mh-0 and
    /// mh-1 and is clean; stripping the declared memory limit from the render
    /// yields exactly one `resource_field_ref_undeclared` per workload. Skips
    /// VISIBLY without a kustomize tool, as R-15 does.
    #[test]
    fn real_mh_memory_limit_refs_are_seen_and_guarded() {
        use crate::common::pod_spec::{
            meta_str, parse_docs, pod_resource_field_refs, pod_spec, ResourceFieldOrigin,
        };
        use crate::kustomize_resource_field_refs::{
            check_rendered as check_rf, RESOURCE_FIELD_REF_UNDECLARED_RULE_ID,
        };
        let Some(tool) = crate::kustomize_tools::detect_kustomize_tool() else {
            eprintln!("SKIP: real_mh_memory_limit_refs_are_seen_and_guarded — no kustomize tool");
            return;
        };
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mh = crate::kustomize_tools::run_kustomize_build(
            tool,
            &root.join("infra/services/mh-service"),
        )
        .expect("build mh");
        assert!(mh.success, "{}", mh.stderr_head);
        let docs = parse_docs(&mh.stdout).expect("parse");
        for name in ["mh-0", "mh-1"] {
            let doc = docs
                .iter()
                .find(|d| {
                    d.get("kind").and_then(serde_norway::Value::as_str) == Some("Deployment")
                        && meta_str(d, "name") == Some(name)
                })
                .expect("mh Deployment rendered");
            let refs = pod_resource_field_refs(pod_spec(doc).expect("pod"));
            assert!(
                refs.iter().any(|r| r.origin
                    == ResourceFieldOrigin::Env(Some("MH_POD_MEMORY_LIMIT_BYTES"))
                    && r.resource == Some("limits.memory")),
                "{name}: walker did not see MH_POD_MEMORY_LIMIT_BYTES"
            );
        }
        let clean = check_rf(&mh.stdout, "mh base");
        assert!(clean.findings.is_empty(), "{:?}", clean.findings);
        assert!(clean.checked >= 2, "checked {}", clean.checked);
        let stripped = mh.stdout.replace(
            "          limits:\n            cpu: 1000m\n            memory: 1Gi\n",
            "          limits:\n            cpu: 1000m\n",
        );
        assert_ne!(
            stripped, mh.stdout,
            "the render's limits shape changed; update this control"
        );
        let broken = check_rf(&stripped, "mh base");
        assert_eq!(broken.findings.len(), 2, "{:?}", broken.findings);
        for (f, name) in broken
            .findings
            .iter()
            .zip(["Deployment/mh-0", "Deployment/mh-1"])
        {
            assert_eq!(f.rule_id, RESOURCE_FIELD_REF_UNDECLARED_RULE_ID);
            assert!(f.detail.contains(name), "{}", f.detail);
        }
    }

    #[test]
    fn fail_token_vacuous_alone_names_the_run() {
        assert_eq!(
            fail_token(&[hit(VACUOUS_RULE_ID), hit(VACUOUS_RULE_ID)]),
            "kustomize-content-addressing-vacuous-2"
        );
    }

    #[test]
    fn fail_token_content_only_reports_the_max_class_with_count() {
        // The three content-addressing rule ids fold into one class count.
        let hits = [
            hit(NOT_CONTENT_ADDRESSED_RULE_ID),
            hit(UNRESOLVED_REFERENCE_RULE_ID),
            hit(UNREFERENCED_CONFIGMAP_RULE_ID),
            hit(ORPHAN_MANIFEST_RULE_ID),
        ];
        assert_eq!(
            fail_token(&hits),
            "kustomize-configmap-not-content-addressed-3"
        );
    }

    #[test]
    fn fail_token_reports_the_largest_content_class_by_count() {
        // Build has the larger count, so it names the run regardless of array
        // order. (On an equal count the EARLIER class wins — pinned by
        // `a_tie_is_broken_toward_the_earlier_class_so_build_failures_win`.)
        let hits = [
            hit(BUILD_FAILED_RULE_ID),
            hit(BUILD_FAILED_RULE_ID),
            hit(DASHBOARD_ORPHAN_RULE_ID),
        ];
        assert_eq!(fail_token(&hits), "kustomize-build-failed-2");
    }

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

    /// On a TIE the earlier-listed class must win, because the list order is a
    /// ranking: a build failure means the later checks saw untrustworthy input.
    ///
    /// Regression pin for the `max_by_key`-returns-the-LAST-maximum defect: one
    /// build failure plus one annotation-size finding reported
    /// `kustomize-configmap-annotation-size`, so the newest check silently
    /// outranked R-15 and the reason token pointed a responder at a dashboard
    /// when the build was broken.
    #[test]
    fn a_tie_is_broken_toward_the_earlier_class_so_build_failures_win() {
        assert_eq!(
            fail_token(&[
                hit(BUILD_FAILED_RULE_ID),
                hit(crate::kustomize_configmaps::CONFIGMAP_ANNOTATION_SIZE_RULE_ID),
            ]),
            "kustomize-build-failed-1"
        );
        // Order of the hits themselves must not matter either.
        assert_eq!(
            fail_token(&[
                hit(crate::kustomize_configmaps::CONFIGMAP_ANNOTATION_SIZE_RULE_ID),
                hit(ORPHAN_MANIFEST_RULE_ID),
            ]),
            "kustomize-orphan-manifest-1"
        );
        // A genuine majority still wins over an earlier class.
        assert_eq!(
            fail_token(&[
                hit(BUILD_FAILED_RULE_ID),
                hit(crate::kustomize_configmaps::CONFIGMAP_ANNOTATION_SIZE_RULE_ID),
                hit(crate::kustomize_configmaps::CONFIGMAP_ANNOTATION_SIZE_RULE_ID),
            ]),
            "kustomize-configmap-annotation-size-2"
        );
    }

    #[test]
    fn orphan_exclusions_skipped() {
        assert!(ORPHAN_EXCLUSIONS.contains(&"kustomization.yaml"));
        assert!(ORPHAN_EXCLUSIONS.contains(&"service-monitor.yaml"));
    }

    /// The migration Job's base is a build target (R-15/R-17/R-18/R-19) and is
    /// orphan-checked (R-16) like every other base.
    #[test]
    fn service_bases_include_db_migrate() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(infra_service_bases(&root)
            .expect("bases")
            .contains(&"db-migrate".to_string()));
        assert!(
            build_targets(&root)
                .expect("targets")
                .iter()
                .any(|(_, label)| label == "base: infra/services/db-migrate"),
            "db-migrate must be a build target"
        );
        let orphans = check_orphan_manifests(&root).expect("orphan walk");
        assert!(
            !orphans
                .iter()
                .any(|h| h.file.to_string_lossy().contains("db-migrate")),
            "{orphans:#?}"
        );
    }

    /// R-15 + R-18 (security's Job ruling) on the REAL db-migrate base: it
    /// builds, and its Job satisfies every security-context invariant. Skips
    /// VISIBLY, exactly as R-15 does, when no kustomize tool exists.
    #[test]
    fn real_db_migrate_base_renders_security_clean() {
        let Some(tool) = crate::kustomize_tools::detect_kustomize_tool() else {
            eprintln!("SKIP: real_db_migrate_base_renders_security_clean — neither `kustomize` nor `kubectl kustomize` available");
            return;
        };
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../infra/services/db-migrate");
        let built = crate::kustomize_tools::run_kustomize_build(tool, &base).expect("build");
        assert!(
            built.success,
            "db-migrate base failed to build:\n{}",
            built.stderr_head
        );
        assert!(
            built.stdout.contains("kind: Job"),
            "positive control: the Job rendered"
        );
        let findings = crate::kustomize_tools::check_security_context(
            &built.stdout,
            "infra/services/db-migrate",
        );
        assert!(findings.is_empty(), "{findings:?}");
    }
}
