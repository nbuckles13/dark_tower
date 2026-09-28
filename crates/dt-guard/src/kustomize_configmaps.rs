//! Two local checks over kustomize-GENERATED ConfigMaps, run by the
//! `kustomize` subcommand alongside R-16 and R-20.
//!
//! * **`configmap_annotation_size`** — every `configMapGenerator` entry under
//!   `infra/` must stay under [`ANNOTATION_HEADROOM_PERCENT`] of Kubernetes'
//!   annotation cap once client-side apply serialises it.
//! * **`dashboard_configmap_label`** — every Grafana generator that ships
//!   dashboard JSON must carry the label the Grafana sidecar selects on.
//!
//! # SCOPE: GENERATED ConfigMaps ONLY — not every ConfigMap that is applied
//!
//! The size rule covers `configMapGenerator` entries. It does **NOT** cover a
//! literal `kind: ConfigMap` manifest listed under `resources:`, which is
//! applied the same way and is capped by the same annotation. Do not read a
//! green result as "every applied ConfigMap fits".
//!
//! That gap is deliberate and measured. Covering literal manifests needs a real
//! YAML parse of `data:` block scalars, which is out of keeping with this
//! subcommand's line-oriented parsers, or a new dependency. The one
//! non-trivial literal ConfigMap in the tree when this was written is
//! `otel-collector-config` (`infra/services/otel-collector/configmap.yaml`):
//! **30,084 bytes, 11.5% of the cap** (kubectl-measured), about 7x headroom.
//! Tracked in `docs/TODO.md`, "The ConfigMap-size guard covers generated
//! ConfigMaps only".
//!
//! Added at story 2 task 12, whose Layer 7 cluster setup failed with
//! `The ConfigMap "grafana-dashboards-mc" is invalid: metadata.annotations:
//! Too long: must have at most 262144 bytes`, after a dashboard grew. Every
//! Layer-3 check was green, and the failure surfaced only at cluster setup.
//!
//! # Why the size is MODELLED, not read off the file
//!
//! `kubectl apply` (client-side) stores the whole object, JSON-encoded, in the
//! `kubectl.kubernetes.io/last-applied-configuration` annotation. The dashboard
//! file is therefore embedded as an escaped JSON **string**. The escaping is not
//! small: `mc-overview.json` was 236,747 bytes on disk and ~261 KB escaped, a
//! ~10% inflation. So a check comparing file bytes to the cap would have PASSED
//! the exact file that broke cluster setup, with apparent headroom to spare —
//! worse than no check, because it reads as coverage.
//!
//! The model is Go's `encoding/json` with HTML escaping ON (kubectl's default):
//! standard JSON escapes, plus `<`, `>` and `&` as six-byte `<` sequences
//! (PromQL is full of `>` and `&`), plus U+2028/U+2029. Non-ASCII stays raw
//! UTF-8, and map keys are sorted. It was pinned byte-for-byte against
//! `kubectl create configmap --save-config --dry-run=client` on three real
//! ConfigMaps and on a fixture exercising every escape class (the unit test
//! [`tests::the_model_matches_kubectl_byte_for_byte_on_every_escape_class`]
//! keeps that pin).
//!
//! # What is NOT modelled, and why that is the safe direction
//!
//! Only the name and the data are measured exactly. Namespace, generator
//! labels and anything an OVERLAY adds (for example the observability overlay's
//! `managed-by` label) are covered by a fixed [`METADATA_ALLOWANCE_BYTES`]
//! rather than resolved, because one kustomization cannot see the overlays
//! that include it. The model also keeps `"creationTimestamp":null`, which
//! the apply path may omit. Both err toward OVER-estimating, which is the
//! safe direction for a size guard.
//!
//! # The cap is an artifact of CLIENT-SIDE apply, not a law of Kubernetes
//!
//! 262144 bytes is the `metadata.annotations` total-size cap. It is NOT the
//! ConfigMap data limit (~1 MiB, etcd-side). The only reason a large ConfigMap
//! trips it is that client-side `kubectl apply` writes the whole submitted
//! object into `kubectl.kubernetes.io/last-applied-configuration`. **Server-side
//! apply (`kubectl apply --server-side`) writes no such annotation**, so it
//! would remove this failure mode entirely from
//! `infra/kind/scripts/setup.sh::deploy_observability` and
//! `scripts/layer7.sh::__apply_observability_overlay`. It was deliberately NOT
//! adopted with this guard: switching carries field-manager and ownership
//! migration consequences for every existing object, which makes it a task of
//! its own. If it lands, this size rule can be retired — do not keep treating
//! 262144 as a constraint after the apply mode that creates it is gone.
//!
//! # Names are measured as written, hash suffixes included by allowance
//!
//! Generators are read from source, so a hash-suffixed name
//! (`prometheus-rules-<hash>`) is measured without its suffix. The ~11 bytes the
//! suffix adds are inside [`METADATA_ALLOWANCE_BYTES`].
//!
//! # Ownership
//!
//! Machinery (`infrastructure`), per CLAUDE.md's guard-ownership split. The
//! label rule's CONTENT — which label, which value — is not restated here: it is
//! read from the sidecar's own `LABEL` / `LABEL_VALUE` in the Grafana
//! deployment, the single source of truth for what the sidecar selects.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::common::path_safety::resolve_cited_path;
use crate::common::scan::warn_skip;
// The single home for the inline-comment strip, by its own rustdoc: a second
// copy here would be a copy that can drift out of the mitigation while looking
// like it has it.
use crate::kustomize::strip_inline_comment;

pub const CONFIGMAP_ANNOTATION_SIZE_RULE_ID: &str = "configmap_annotation_size";
pub const DASHBOARD_CONFIGMAP_LABEL_RULE_ID: &str = "dashboard_configmap_label";

/// Kubernetes' cap on the total size of an object's annotations
/// (`TotalAnnotationSizeLimitB` in `k8s.io/apimachinery`'s object-meta
/// validation). This is the number in the error message
/// ("must have at most 262144 bytes"), not a guess.
pub const ANNOTATION_SIZE_LIMIT_BYTES: usize = 262_144;

/// Fail at this share of the cap, not at the cap itself.
///
/// **Derived from the measured failure, not chosen for roundness.** The
/// threshold must leave room for at least one ordinary task's growth, or the
/// guard is green on a ConfigMap that the very next task breaks — the ratchet
/// this guard was added in, where whoever breaks cluster setup is not whoever
/// filled the budget. Measured with the kubectl-pinned model when the guard
/// was written (story 2 task 12):
///
/// * before that task, `grafana-dashboards-mc` was ALREADY at **95.2%** of the
///   cap (249,492 bytes: the kubectl-exact annotation). The guard itself
///   REPORTS that state as 95.5% (250,516 bytes), because it adds
///   [`METADATA_ALLOWANCE_BYTES`]. Two numbers, two quantities: 95.2% is the
///   measurement, 95.5% is the guard's conservative view of it. The trap was
///   armed a task earlier either way;
/// * that one task (nine panels plus description edits) added 29,697 bytes,
///   **11.3% of the cap**, taking it to 106.5% and failing cluster setup.
///
/// So the threshold must sit at or below `100% − 11.3% ≈ 88.7%`. A 90% threshold
/// FAILS that test: a ConfigMap at 89.9% passes, and one such task takes it to
/// 101%. **80% leaves ~1.7 tasks of runway**, and every generated ConfigMap in
/// the tree passed it when written (the largest, `grafana-dashboards-mh`, was at
/// 67%). Proposed by @infrastructure from rendered sizes; the 90% first shipped
/// was superseded on this evidence. If a single task is ever observed to add
/// more than 20% of the cap, lower this.
pub const ANNOTATION_HEADROOM_PERCENT: usize = 80;

/// Fixed allowance for metadata this guard cannot resolve from one
/// kustomization: namespace, generator labels, and labels or annotations added
/// by an overlay that includes it. See the module doc: over-estimating is
/// the safe direction.
pub const METADATA_ALLOWANCE_BYTES: usize = 1_024;

/// The byte count at or below which a ConfigMap passes.
pub const fn headroom_threshold_bytes() -> usize {
    ANNOTATION_SIZE_LIMIT_BYTES * ANNOTATION_HEADROOM_PERCENT / 100
}

const GRAFANA_KUSTOMIZATION: &str = "infra/grafana/kustomization.yaml";
const GRAFANA_DEPLOYMENT: &str = "infra/grafana/deployment.yaml";

/// One finding, in the shape `kustomize::run` reports.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ConfigMapFinding {
    pub rule_id: &'static str,
    pub detail: String,
    pub file: PathBuf,
}

/// Where a generated key's value comes from.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// `files:` — `key=path`, or a bare `path` whose key is its basename.
    File { key: String, path: String },
    /// `literals:` — `KEY=VALUE`.
    Literal { key: String, value: String },
    /// `envs:` — a file of `KEY=VALUE` lines, each its own key.
    Env { path: String },
}

/// One `configMapGenerator` entry.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Generator {
    pub name: String,
    pub labels: Vec<(String, String)>,
    pub sources: Vec<Source>,
}

/// Where in an entry the parser is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    None,
    Files,
    Literals,
    Envs,
    Labels,
}

/// Parse the `configMapGenerator:` section of one kustomization.
///
/// Line-oriented, like every other parser in the `kustomize` subcommand, and
/// with the same two hard-won protections: inline comments are stripped before
/// a line is interpreted, and standalone comment lines are skipped (see
/// [`strip_inline_comment`] for why both matter — this parser imports it rather
/// than repeating it). The section ends at the next top-level key.
///
/// # The entry boundary is the BULLET, not `name:`
///
/// A generator entry is a YAML **map**, so its keys may appear in any order:
/// `- files:` … `name:` is as valid as `- name:` … `files:`, and kustomize
/// accepts both. An earlier version of this parser treated `- name:` as the
/// only entry boundary, which lost or mis-attributed every reordered entry —
/// silently, and in the fail-OPEN direction: a reordered entry's `- files:`
/// bullet was swallowed as a *file* of the preceding entry (so a 250 KB
/// dashboard went unmeasured, keyed as `files:`), or, when the reordered entry
/// came first in the section, dropped along with everything in it.
///
/// So the boundary is the indent of the section's first `- ` bullet: any `- `
/// at that indent starts a new entry. Sub-list items (`- foo.json`) are
/// necessarily indented deeper. `name:`, `files:`, `literals:`, `envs:` and
/// `labels:` are then recognised with or without the leading dash, wherever
/// they fall in the entry. An entry that ends with no name at all is reported
/// by [`check`] as a finding: kustomize REQUIRES `name`, so a nameless entry
/// means this parser lost track, and staying quiet about it would put the
/// fail-open case straight back.
///
/// # Indentation of a block sequence is a STYLE, and the entry indent may be 0
///
/// YAML lets a block sequence sit at the same indent as the key that owns it,
/// so all four of these are the same document, and kustomize accepts all four:
///
/// ```text
/// configMapGenerator:          configMapGenerator:
///   - name: big                - name: big
///     files:                     files:
///       - big.json                 - big.json
/// ```
///
/// …and the two with the sub-list at its own key's indent. The zero-indent form
/// is what `yq` emits by DEFAULT, so it arrives via any reformat. A column-0
/// line therefore ends the section only when it is a `key:` — reading a column-0
/// `- ` bullet as a new top-level key skipped the entire section, and skipped it
/// fail-OPEN: an oversized dashboard went unmeasured while `measured` stayed
/// non-zero, so the vacuity guard in [`check`] stayed quiet too. Since one
/// reformat could silence a whole file that way, [`check`] additionally reports
/// a kustomization that DECLARES `configMapGenerator:` and yields no entries.
///
/// A sub-list at its own key's indent needs no extra rule: the entry bullet is
/// strictly shallower than the entry's own keys (YAML requires a sequence
/// item's map keys to be indented past the `- `), so a sub-item bullet is always
/// deeper than the entry bullet, and a bullet AT the entry indent is always the
/// next entry — which is also the kustomize-correct reading, because such a
/// bullet is an item of the outer `configMapGenerator` sequence.
pub(crate) fn parse_generators(content: &str) -> Vec<Generator> {
    let mut out: Vec<Generator> = Vec::new();
    let mut in_section = false;
    let mut entry_indent: Option<usize> = None;
    let mut mode = Mode::None;
    // Indent of the key that opened `mode`. A field line at or left of it has
    // DEDENTED out of that block, which is what lets a `name:` after an
    // `options: labels:` block be the generator's name while a `name:` key
    // *inside* that block stays a label.
    let mut mode_indent = 0usize;
    for raw in content.lines() {
        let indent = raw.len() - raw.trim_start().len();
        let line = strip_inline_comment(raw);
        if line.is_empty() {
            continue;
        }
        // A column-0 line ends the section only when it is a KEY. A column-0
        // `- ` bullet is a block sequence at its parent key's indent (see the
        // rustdoc) and belongs to the OPEN section. A document marker is not a
        // key, but it does end the section: what follows is another document.
        let is_document_marker = line == "---" || line == "...";
        if indent == 0 && (is_document_marker || !line.starts_with('-')) {
            in_section = line == "configMapGenerator:";
            entry_indent = None;
            mode = Mode::None;
            continue;
        }
        if !in_section {
            continue;
        }
        // Classify the line: a bullet at the entry indent OPENS an entry (and
        // its remainder is still a field of that entry); a deeper bullet is a
        // sub-list item; anything else is a field of the open entry.
        let (field, is_item) = match line.strip_prefix("- ") {
            Some(rest) => {
                if entry_indent.is_none_or(|e| indent <= e) {
                    entry_indent = Some(indent);
                    out.push(Generator::default());
                    mode = Mode::None;
                    (rest.trim(), false)
                } else {
                    (rest.trim(), true)
                }
            }
            None => (line, false),
        };
        let Some(current) = out.last_mut() else {
            continue;
        };
        if is_item {
            push_item(current, mode, unquote(field));
            continue;
        }
        // A field at or left of the opening key's indent has left that block.
        if mode != Mode::None && indent <= mode_indent {
            mode = Mode::None;
        }
        match field {
            "files:" | "literals:" | "envs:" | "labels:" => {
                mode = match field {
                    "files:" => Mode::Files,
                    "literals:" => Mode::Literals,
                    "envs:" => Mode::Envs,
                    _ => Mode::Labels,
                };
                mode_indent = indent;
            }
            _ => {
                // `name:` inside a `labels:` block is a LABEL called `name`, not
                // the generator's name.
                if let Some(name) = field.strip_prefix("name:").filter(|_| mode != Mode::Labels) {
                    current.name = unquote(name.trim()).to_string();
                    mode = Mode::None;
                } else if let Some((key, value)) = field.split_once(':') {
                    if mode == Mode::Labels && !value.trim().is_empty() {
                        current
                            .labels
                            .push((key.trim().to_string(), unquote(value.trim()).to_string()));
                    } else {
                        // Any other key (`options:`, `namespace:`, `behavior:`)
                        // closes the current list.
                        mode = Mode::None;
                    }
                }
            }
        }
    }
    out
}

/// Does this kustomization DECLARE a `configMapGenerator:` section?
///
/// Backs a PER-FILE vacuity guard in [`check`]. The whole-tree "found ZERO
/// generators" guard is not enough on its own: with several kustomizations in
/// the tree, one of them losing its entire section leaves `measured` non-zero,
/// so the run reports clean while saying nothing about that file — exactly how
/// the zero-indent block-sequence style went unnoticed. A file that declares
/// the section must yield at least one entry.
///
/// Anchored on the bare key at column 0, so the flow spelling
/// (`configMapGenerator: []`, which legitimately has no entries) is not matched,
/// and neither is a commented-out key.
pub(crate) fn declares_generator_section(content: &str) -> bool {
    content.lines().any(|raw| {
        !raw.starts_with([' ', '\t']) && strip_inline_comment(raw) == "configMapGenerator:"
    })
}

/// Record one sub-list item against the list the parser is currently inside.
fn push_item(current: &mut Generator, mode: Mode, item: &str) {
    match mode {
        Mode::Files => current.sources.push(match item.split_once('=') {
            Some((key, path)) => Source::File {
                key: key.to_string(),
                path: path.to_string(),
            },
            None => Source::File {
                key: basename(item).to_string(),
                path: item.to_string(),
            },
        }),
        Mode::Literals => {
            if let Some((key, value)) = item.split_once('=') {
                current.sources.push(Source::Literal {
                    key: key.to_string(),
                    value: unquote(value).to_string(),
                });
            }
        }
        Mode::Envs => current.sources.push(Source::Env {
            path: item.to_string(),
        }),
        Mode::Labels | Mode::None => {}
    }
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    for q in ['"', '\''] {
        if let Some(inner) = s.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
            return inner;
        }
    }
    s
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Bytes Go's `encoding/json` (HTML escaping on) writes for `s` as a string,
/// quotes included.
pub(crate) fn go_json_string_len(s: &str) -> usize {
    2 + s
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\n' | '\r' | '\t' => 2,
            '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => 6,
            c if (c as u32) < 0x20 => 6,
            c => c.len_utf8(),
        })
        .sum::<usize>()
}

/// The length of the `last-applied-configuration` annotation client-side
/// apply would write for a ConfigMap with this name and data, EXCLUDING the
/// metadata allowance (see [`METADATA_ALLOWANCE_BYTES`]).
///
/// Shape, byte for byte as kubectl writes it:
/// `{"kind":"ConfigMap","apiVersion":"v1","metadata":{"name":N,"creationTimestamp":null},"data":{K:V,...}}\n`
pub(crate) fn last_applied_len(name: &str, data: &BTreeMap<String, String>) -> usize {
    const HEAD: &str = r#"{"kind":"ConfigMap","apiVersion":"v1","metadata":{"name":"#;
    const MID: &str = r#","creationTimestamp":null},"data":{"#;
    const TAIL: &str = "}}\n";
    let entries: usize = data
        .iter()
        .map(|(k, v)| go_json_string_len(k) + 1 + go_json_string_len(v))
        .sum();
    let commas = data.len().saturating_sub(1);
    HEAD.len() + go_json_string_len(name) + MID.len() + entries + commas + TAIL.len()
}

/// Resolve one YAML-supplied source path, relative to the kustomization's own
/// directory, through the repository's containment gate.
///
/// # The path comes from a file, so it faces the gate every other cited path does
///
/// `files:` / `envs:` entries are attacker-shaped input in the same sense the
/// doc-cite paths are: `dir.join(path)` alone follows `../` out of the tree, and
/// an ABSOLUTE entry replaces the base outright (`join("/w/infra", "/etc/shadow")`
/// is `/etc/shadow`). Containment is NOT re-implemented here — that is a reject
/// by [`crate::common::path_safety`]'s own module doc, which is the single SoT —
/// so the joined path is handed to [`resolve_cited_path`].
///
/// Fail CLOSED: an escaping path is a FINDING, never a skip. A skip would mean a
/// generator whose size was never measured reporting as measured, which is the
/// direction this whole guard exists to prevent.
///
/// The two outcomes are reported DIFFERENTLY because the responses differ — fix
/// a path typo versus stop citing outside the repo — and `resolve_cited_path`
/// returns `None` for both. They are told apart by asking whether the joined
/// path canonicalizes at all (i.e. exists): if it does, the `None` was
/// containment; if it does not, the file is missing or unreadable. Only the
/// question is asked here; the containment DECISION stays in the gate.
///
/// `what` is a message qualifier ending in a space (`""` for a `files:` entry,
/// `"env file "` for an `envs:` one), so the two source kinds stay
/// distinguishable in a finding.
fn resolve_source_path(
    gen_name: &str,
    what: &str,
    path: &str,
    dir: &Path,
    repo_root: &Path,
) -> Result<PathBuf> {
    let joined = dir.join(path);
    if let Some(resolved) = resolve_cited_path(repo_root, &joined.to_string_lossy()) {
        return Ok(resolved);
    }
    if std::fs::canonicalize(&joined).is_ok() {
        // It resolves, just not inside the repository. Report the path AS
        // WRITTEN: never echo the canonicalized out-of-tree absolute path.
        anyhow::bail!(
            "configMapGenerator `{gen_name}` references {what}`{path}`, which resolves outside \
             the repository, so its size cannot be measured. Generator sources must stay inside \
             the repo (kustomize itself refuses paths outside its root)."
        );
    }
    anyhow::bail!(
        "configMapGenerator `{gen_name}` references {what}`{path}` but it cannot be read, so its \
         size cannot be measured"
    )
}

/// Resolve a generator's sources into the data map it produces.
///
/// A referenced file that cannot be read — or that escapes the repository — is
/// an error naming the generator and the path: its size cannot be measured, and
/// silently skipping it would under-count, the fail-open direction. [`check`]
/// reports it as a finding.
fn resolve_data(gen: &Generator, dir: &Path, repo_root: &Path) -> Result<BTreeMap<String, String>> {
    let mut data = BTreeMap::new();
    for source in &gen.sources {
        match source {
            Source::File { key, path } => {
                let full = resolve_source_path(&gen.name, "", path, dir, repo_root)?;
                let value = std::fs::read_to_string(&full).with_context(|| {
                    format!(
                        "configMapGenerator `{}` references `{path}` but it cannot be read, so \
                         its size cannot be measured",
                        gen.name
                    )
                })?;
                data.insert(key.clone(), value);
            }
            Source::Literal { key, value } => {
                data.insert(key.clone(), value.clone());
            }
            Source::Env { path } => {
                let full = resolve_source_path(&gen.name, "env file ", path, dir, repo_root)?;
                let text = std::fs::read_to_string(&full).with_context(|| {
                    format!(
                        "configMapGenerator `{}` references env file `{path}` but it cannot be \
                         read",
                        gen.name
                    )
                })?;
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        data.insert(k.trim().to_string(), v.to_string());
                    }
                }
            }
        }
    }
    Ok(data)
}

/// Judge one generator's measured size. Pure, so the threshold arithmetic is
/// tested without a filesystem.
pub(crate) fn judge_size(name: &str, measured: usize, file: &Path) -> Option<ConfigMapFinding> {
    let with_allowance = measured + METADATA_ALLOWANCE_BYTES;
    if with_allowance <= headroom_threshold_bytes() {
        return None;
    }
    // Tenths of a percent in integer arithmetic (no float casts).
    let permille = with_allowance * 1000 / ANNOTATION_SIZE_LIMIT_BYTES;
    Some(ConfigMapFinding {
        rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
        detail: format!(
            "generated ConfigMap `{name}` would carry an estimated {with_allowance}-byte \
             last-applied-configuration annotation — {measured} bytes measured from its name \
             and data, plus a {METADATA_ALLOWANCE_BYTES}-byte allowance for metadata this \
             guard cannot resolve from one kustomization (namespace, generator labels, and \
             anything an overlay adds) — ({}.{}% of Kubernetes' {} byte cap; \
             this guard fails above {}%). Client-side apply stores the WHOLE ConfigMap, \
             JSON-escaped, in that annotation, so past the cap cluster setup fails with \
             `metadata.annotations: Too long`. Remedy: split a file out into its own \
             configMapGenerator entry (as mc-media.json was split from mc-overview.json), \
             and carry the Grafana sidecar label on it if it is a dashboard.",
            permille / 10,
            permille % 10,
            ANNOTATION_SIZE_LIMIT_BYTES,
            ANNOTATION_HEADROOM_PERCENT,
        ),
        file: file.to_path_buf(),
    })
}

/// Read the sidecar's `LABEL` / `LABEL_VALUE` env from the Grafana deployment.
pub(crate) fn sidecar_label(deployment: &str) -> Option<(String, String)> {
    let mut label = None;
    let mut value = None;
    let mut pending: Option<&str> = None;
    for raw in deployment.lines() {
        let line = strip_inline_comment(raw);
        if let Some(name) = line.strip_prefix("- name:") {
            pending = match unquote(name.trim()) {
                "LABEL" => Some("LABEL"),
                "LABEL_VALUE" => Some("LABEL_VALUE"),
                _ => None,
            };
            continue;
        }
        if let (Some(which), Some(v)) = (pending, line.strip_prefix("value:")) {
            let v = unquote(v.trim()).to_string();
            if which == "LABEL" {
                label = Some(v);
            } else {
                value = Some(v);
            }
            pending = None;
        }
    }
    Some((label?, value?))
}

/// Judge the label on one Grafana generator. Only generators that ship
/// dashboard JSON are in scope: `grafana-dashboards-config` ships the
/// provisioning YAML and must NOT be picked up by the sidecar as a dashboard.
pub(crate) fn judge_label(
    gen: &Generator,
    expected: &(String, String),
    file: &Path,
) -> Option<ConfigMapFinding> {
    let ships_json = gen.sources.iter().any(|s| match s {
        Source::File { key, .. } => key.ends_with(".json"),
        _ => false,
    });
    if !ships_json {
        return None;
    }
    let carries = gen
        .labels
        .iter()
        .any(|(k, v)| k == &expected.0 && v == &expected.1);
    if carries {
        return None;
    }
    Some(ConfigMapFinding {
        rule_id: DASHBOARD_CONFIGMAP_LABEL_RULE_ID,
        detail: format!(
            "configMapGenerator `{}` ships dashboard JSON but does not carry the label \
             `{}: \"{}\"` that the Grafana sidecar selects on (its LABEL / LABEL_VALUE in \
             {GRAFANA_DEPLOYMENT}). Without it the dashboard exists, is registered, passes \
             the dashboard coverage check, and is NEVER loaded by Grafana.",
            gen.name, expected.0, expected.1
        ),
        file: file.to_path_buf(),
    })
}

/// Find every `kustomization.yaml` under `infra/`.
fn kustomizations(infra: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![infra.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                warn_skip("kustomization walk", &dir, &e);
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()) == Some("kustomization.yaml") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn rel(repo_root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(repo_root).unwrap_or(path).to_path_buf()
}

/// Run both checks over the repository.
pub(crate) fn check(repo_root: &Path) -> Result<Vec<ConfigMapFinding>> {
    let mut findings = Vec::new();
    let infra = repo_root.join("infra");
    if !infra.is_dir() {
        return Ok(findings);
    }

    // Size: every generator, in every kustomization.
    //
    // EVERY read failure on this path is a FINDING, not an early `Err`. An `Err`
    // aborts the whole `kustomize` run and takes the other checks' findings with
    // it, so an unreadable kustomization would hide real R-16/R-18/R-19
    // violations behind one I/O error — the same masking the per-file branch
    // below was written to avoid.
    let mut measured = 0usize;
    for kust in kustomizations(&infra) {
        let content = match std::fs::read_to_string(&kust) {
            Ok(c) => c,
            Err(e) => {
                findings.push(ConfigMapFinding {
                    rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
                    detail: format!(
                        "cannot read {}: {e}. Its configMapGenerator entries were NOT measured, \
                         so this run says nothing about their annotation size.",
                        rel(repo_root, &kust).display()
                    ),
                    file: rel(repo_root, &kust),
                });
                continue;
            }
        };
        let dir = kust.parent().unwrap_or(repo_root);
        let generators = parse_generators(&content);
        // PER-FILE vacuity: one file losing its whole section leaves the
        // whole-tree `measured` counter non-zero, so that guard cannot see it.
        if generators.is_empty() && declares_generator_section(&content) {
            findings.push(ConfigMapFinding {
                rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
                detail: format!(
                    "{} declares `configMapGenerator:` but this guard parsed ZERO entries from \
                     it, so NOTHING in that section was measured and a clean result for this \
                     file would be vacuous. Either the section is empty (remove the key) or \
                     this guard's parser does not understand how it is written.",
                    rel(repo_root, &kust).display()
                ),
                file: rel(repo_root, &kust),
            });
        }
        for gen in generators {
            measured += 1;
            if gen.name.is_empty() {
                // kustomize REQUIRES `name`, so this is the parser having lost
                // track of an entry boundary, not a real nameless generator.
                findings.push(ConfigMapFinding {
                    rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
                    detail: format!(
                        "a configMapGenerator entry has no `name:` ({} sources parsed). \
                         kustomize requires `name`, so either this kustomization is invalid or \
                         this guard's entry parser lost track of an entry boundary — either way \
                         the entry's annotation size was NOT measured.",
                        gen.sources.len()
                    ),
                    file: rel(repo_root, &kust),
                });
                continue;
            }
            match resolve_data(&gen, dir, repo_root) {
                Ok(data) => {
                    if let Some(f) = judge_size(
                        &gen.name,
                        last_applied_len(&gen.name, &data),
                        &rel(repo_root, &kust),
                    ) {
                        findings.push(f);
                    }
                }
                Err(e) => findings.push(ConfigMapFinding {
                    rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
                    detail: format!("{e:#}"),
                    file: rel(repo_root, &kust),
                }),
            }
        }
    }

    // Vacuity guard: a run that collected NOTHING and reported clean would be
    // indistinguishable from a real pass. The tree has generators (Grafana's,
    // Prometheus's), so finding none means the parser or the walk broke — for
    // example a reformatted `configMapGenerator:` key — not that all is well.
    if measured == 0 {
        findings.push(ConfigMapFinding {
            rule_id: CONFIGMAP_ANNOTATION_SIZE_RULE_ID,
            detail: "found ZERO configMapGenerator entries under infra/ — the size check \
                     measured nothing, so a clean result would be vacuous. The parser or the \
                     kustomization walk is broken, or every generator was removed."
                .to_string(),
            file: PathBuf::from("infra"),
        });
    }

    // Label: Grafana dashboard generators, against the sidecar's own selector.
    let grafana_kust = repo_root.join(GRAFANA_KUSTOMIZATION);
    if grafana_kust.is_file() {
        let deployment_path = repo_root.join(GRAFANA_DEPLOYMENT);
        // Same rule as above: an unreadable deployment is a finding, not an
        // abort. Without the selector the check cannot be performed, and a
        // silent skip would read as a pass.
        let deployment = match std::fs::read_to_string(&deployment_path) {
            Ok(d) => d,
            Err(e) => {
                findings.push(ConfigMapFinding {
                    rule_id: DASHBOARD_CONFIGMAP_LABEL_RULE_ID,
                    detail: format!(
                        "cannot read {GRAFANA_DEPLOYMENT} ({e}), which is the single source of \
                         truth for the label the Grafana sidecar selects on, so dashboard \
                         ConfigMap labels cannot be checked"
                    ),
                    file: PathBuf::from(GRAFANA_DEPLOYMENT),
                });
                return Ok(findings);
            }
        };
        let Some(expected) = sidecar_label(&deployment) else {
            // Fail LOUD: without the selector, "every generator is labelled" can
            // be neither confirmed nor denied, and skipping would read as a pass.
            findings.push(ConfigMapFinding {
                rule_id: DASHBOARD_CONFIGMAP_LABEL_RULE_ID,
                detail: format!(
                    "cannot find the Grafana sidecar's LABEL and LABEL_VALUE env in \
                     {GRAFANA_DEPLOYMENT}, so dashboard ConfigMap labels cannot be checked"
                ),
                file: PathBuf::from(GRAFANA_DEPLOYMENT),
            });
            return Ok(findings);
        };
        let content = match std::fs::read_to_string(&grafana_kust) {
            Ok(c) => c,
            Err(e) => {
                findings.push(ConfigMapFinding {
                    rule_id: DASHBOARD_CONFIGMAP_LABEL_RULE_ID,
                    detail: format!(
                        "cannot read {GRAFANA_KUSTOMIZATION} ({e}), so dashboard ConfigMap labels \
                         cannot be checked"
                    ),
                    file: PathBuf::from(GRAFANA_KUSTOMIZATION),
                });
                return Ok(findings);
            }
        };
        for gen in parse_generators(&content) {
            if let Some(f) = judge_label(&gen, &expected, Path::new(GRAFANA_KUSTOMIZATION)) {
                findings.push(f);
            }
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the model to kubectl's real output, byte for byte, on a fixture
    /// exercising every escape class: `<`, `>`, `&`, `"`, `\`, tab, CR, LF, a
    /// C0 control, U+2028, and non-ASCII (which stays raw). The expected 209 is
    /// what `kubectl create configmap fixture-cm --from-file=f.json=<fixture>
    /// --save-config --dry-run=client` wrote (kubectl v1.32.3), measured when
    /// this guard was written — not derived from this code.
    #[test]
    fn the_model_matches_kubectl_byte_for_byte_on_every_escape_class() {
        let fixture =
            "a > b && c < d\n\"quoted\" back\\slash\ttab\u{2028}ls caf\u{e9} \u{1}ctl\r\n";
        let mut data = BTreeMap::new();
        data.insert("f.json".to_string(), fixture.to_string());
        assert_eq!(last_applied_len("fixture-cm", &data), 209);
    }

    #[test]
    fn html_sensitive_characters_cost_six_bytes_not_one() {
        // The whole reason file size is the wrong measurement.
        assert_eq!(go_json_string_len(">"), 2 + 6);
        assert_eq!(go_json_string_len("&"), 2 + 6);
        assert_eq!(go_json_string_len("<"), 2 + 6);
        assert_eq!(go_json_string_len("a"), 2 + 1);
        assert_eq!(
            go_json_string_len("\u{e9}"),
            2 + 2,
            "non-ASCII stays raw UTF-8"
        );
    }

    #[test]
    fn map_keys_are_joined_with_commas_in_sorted_order() {
        let mut data = BTreeMap::new();
        data.insert("b".to_string(), String::new());
        data.insert("a".to_string(), String::new());
        // {"a":"","b":""} inside the envelope: 2 keys, one comma.
        let one = {
            let mut d = BTreeMap::new();
            d.insert("a".to_string(), String::new());
            last_applied_len("n", &d)
        };
        assert_eq!(
            last_applied_len("n", &data),
            one + 1 + go_json_string_len("b") + 1 + 2
        );
    }

    #[test]
    fn the_threshold_is_eighty_percent_of_the_kubernetes_cap() {
        assert_eq!(ANNOTATION_SIZE_LIMIT_BYTES, 262_144);
        assert_eq!(headroom_threshold_bytes(), 209_715);
    }

    #[test]
    fn a_configmap_over_the_headroom_fails_naming_it_and_the_remedy() {
        let at = headroom_threshold_bytes() - METADATA_ALLOWANCE_BYTES;
        assert_eq!(
            judge_size("cm", at, Path::new("k.yaml")),
            None,
            "exactly at the threshold passes"
        );
        let f = judge_size("grafana-dashboards-mc", at + 1, Path::new("k.yaml")).unwrap();
        assert_eq!(f.rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(f.detail.contains("grafana-dashboards-mc"));
        assert!(
            f.detail.contains("split a file out"),
            "the remedy is non-obvious; say it"
        );
        // The reported number is an ESTIMATE, and the message must say so plus
        // where the difference comes from — a responder who measures the real
        // annotation with kubectl gets a smaller number and must not read that
        // as the guard being wrong.
        assert!(
            f.detail.contains("estimated")
                && f.detail
                    .contains(&format!("{METADATA_ALLOWANCE_BYTES}-byte allowance"))
                && f.detail.contains(&format!("{} bytes measured", at + 1)),
            "{}",
            f.detail
        );
    }

    /// The live failure, as a test: the pre-split MC ConfigMap measured
    /// 279,189 bytes by kubectl. The guard must reject it.
    #[test]
    fn the_configmap_that_broke_cluster_setup_is_rejected() {
        assert!(judge_size("grafana-dashboards-mc", 279_189, Path::new("k.yaml")).is_some());
    }

    const GRAFANA: &str = r#"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: dark-tower-observability
configMapGenerator:
  - name: grafana-dashboards-config
    files:
      - dashboards.yaml=provisioning/dashboards/dashboards.yaml
  - name: grafana-dashboards-mc   # the overview
    options:
      labels:
        grafana_dashboard: "1"
    files:
      - mc-overview.json=dashboards/mc-overview.json  # inline comment
      # standalone comment between items
      - dashboards/mc-slos.json
  # a standalone comment between entries
  - name: grafana-dashboards-unlabelled
    files:
      - x.json=dashboards/x.json
  - name: lits
    literals:
      - KEY="quoted value"
generatorOptions:
  disableNameSuffixHash: true
"#;

    #[test]
    fn the_parser_reads_entries_labels_files_and_literals_through_comments() {
        let g = parse_generators(GRAFANA);
        let names: Vec<&str> = g.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "grafana-dashboards-config",
                "grafana-dashboards-mc",
                "grafana-dashboards-unlabelled",
                "lits"
            ]
        );
        assert_eq!(
            g[1].labels,
            vec![("grafana_dashboard".to_string(), "1".to_string())]
        );
        assert_eq!(
            g[1].sources,
            vec![
                Source::File {
                    key: "mc-overview.json".into(),
                    path: "dashboards/mc-overview.json".into()
                },
                Source::File {
                    key: "mc-slos.json".into(),
                    path: "dashboards/mc-slos.json".into()
                },
            ],
            "an inline comment must not corrupt a path, and a bare path keys on its basename"
        );
        assert_eq!(
            g[3].sources,
            vec![Source::Literal {
                key: "KEY".into(),
                value: "quoted value".into()
            }]
        );
    }

    #[test]
    fn a_top_level_key_ends_the_section() {
        // `generatorOptions:` follows; nothing after it may become an entry.
        assert_eq!(parse_generators(GRAFANA).len(), 4);
    }

    #[test]
    fn a_dashboard_generator_without_the_sidecar_label_is_a_finding() {
        let g = parse_generators(GRAFANA);
        let expected = ("grafana_dashboard".to_string(), "1".to_string());
        let p = Path::new(GRAFANA_KUSTOMIZATION);
        assert_eq!(
            judge_label(&g[0], &expected, p),
            None,
            "provisioning YAML is not a dashboard"
        );
        assert_eq!(judge_label(&g[1], &expected, p), None, "labelled");
        let f = judge_label(&g[2], &expected, p).unwrap();
        assert_eq!(f.rule_id, DASHBOARD_CONFIGMAP_LABEL_RULE_ID);
        assert!(f.detail.contains("grafana-dashboards-unlabelled"));
        let wrong = ("grafana_dashboard".to_string(), "2".to_string());
        assert!(
            judge_label(&g[1], &wrong, p).is_some(),
            "a wrong VALUE is not the label"
        );
    }

    #[test]
    fn the_expected_label_is_read_from_the_sidecar_not_restated() {
        let deployment = r#"      containers:
        - name: k8s-sidecar
          env:
            - name: LABEL
              value: grafana_dashboard
            - name: LABEL_VALUE
              value: "1"
            - name: FOLDER
              value: /tmp/dashboards
"#;
        assert_eq!(
            sidecar_label(deployment),
            Some(("grafana_dashboard".to_string(), "1".to_string()))
        );
        assert_eq!(sidecar_label("containers: []\n"), None);
    }

    #[test]
    fn check_runs_end_to_end_on_a_temp_tree() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let g = root.join("infra/grafana");
        std::fs::create_dir_all(g.join("dashboards")).unwrap();
        std::fs::write(
            g.join("deployment.yaml"),
            "- name: LABEL\n  value: grafana_dashboard\n- name: LABEL_VALUE\n  value: \"1\"\n",
        )
        .unwrap();
        std::fs::write(
            g.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: big\n    options:\n      labels:\n        grafana_dashboard: \"1\"\n    files:\n      - big.json=dashboards/big.json\n  - name: small\n    files:\n      - small.json=dashboards/small.json\n",
        )
        .unwrap();
        // `>` costs six bytes escaped: 40,000 of them is ~240 KB, over 80%,
        // while the file itself is only 40 KB — the case file size would miss.
        std::fs::write(g.join("dashboards/big.json"), ">".repeat(40_000)).unwrap();
        std::fs::write(g.join("dashboards/small.json"), "{}").unwrap();
        let findings = check(root).unwrap();
        let rules: Vec<(&str, bool)> = findings
            .iter()
            .map(|f| {
                (
                    f.rule_id,
                    f.detail.contains("`big`") || f.detail.contains("`small`"),
                )
            })
            .collect();
        assert_eq!(
            rules,
            vec![
                (CONFIGMAP_ANNOTATION_SIZE_RULE_ID, true),
                (DASHBOARD_CONFIGMAP_LABEL_RULE_ID, true)
            ],
            "big is too large though its file is 40 KB; small ships JSON unlabelled"
        );
    }

    /// Mechanism 5 (review-protocol vacuity): a tree with `infra/` but no
    /// generators must NOT report clean.
    #[test]
    fn collecting_zero_generators_is_a_finding_not_a_pass() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(k.join("kustomization.yaml"), "resources:\n  - a.yaml\n").unwrap();
        let findings = check(dir.path()).unwrap();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].detail.contains("ZERO"));
    }

    /// The ratchet the headroom exists for: the measured single-task growth
    /// (11.3% of the cap) must fit above the threshold.
    #[test]
    fn one_measured_task_of_growth_fits_between_the_threshold_and_the_cap() {
        const OBSERVED_SINGLE_TASK_GROWTH_BYTES: usize = 29_697;
        assert!(
            headroom_threshold_bytes() + OBSERVED_SINGLE_TASK_GROWTH_BYTES
                <= ANNOTATION_SIZE_LIMIT_BYTES,
            "a ConfigMap just under the threshold must survive one more task like story 2 \
             task 12; if this fails, the threshold is too close to the cap"
        );
    }

    /// A kustomize entry is a YAML MAP, so its keys may come in any order. All
    /// three orderings below were silently lost or mis-attributed when `- name:`
    /// was treated as the entry boundary.
    const REORDERED: &str = r#"configMapGenerator:
  - files:
      - a.json=dashboards/a.json
    options:
      labels:
        grafana_dashboard: "1"
    name: reordered-first
  - name: after-files
    files:
      - b.json=dashboards/b.json
  - literals:
      - K=V
    name: entry-after-a-files-list
  - name: entry-after-a-literals-list
    files:
      - c.json=dashboards/c.json
generatorOptions:
  disableNameSuffixHash: true
"#;

    /// The three reordered shapes, with their names, sources AND labels — not
    /// just a count, because the old parser's failure was mis-ATTRIBUTION as
    /// much as loss: a following entry's `- files:` bullet became a *file* of
    /// the preceding entry.
    #[test]
    fn entry_keys_may_come_in_any_order_because_an_entry_is_a_yaml_map() {
        let g = parse_generators(REORDERED);
        let names: Vec<&str> = g.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            [
                // Shape 1: a reordered entry FIRST in the section (the whole
                // entry used to vanish, taking its dashboard with it).
                "reordered-first",
                "after-files",
                // Shape 2: an entry opening on `- literals:` right after one
                // that ended in a `files:` list.
                "entry-after-a-files-list",
                // Shape 3: an entry right after one that ended in a
                // `literals:` list.
                "entry-after-a-literals-list",
            ]
        );
        assert_eq!(
            g[0].sources,
            vec![Source::File {
                key: "a.json".into(),
                path: "dashboards/a.json".into()
            }]
        );
        assert_eq!(
            g[0].labels,
            vec![("grafana_dashboard".to_string(), "1".to_string())],
            "a label block before `name:` still belongs to this entry"
        );
        assert_eq!(
            g[1].sources,
            vec![Source::File {
                key: "b.json".into(),
                path: "dashboards/b.json".into()
            }],
            "the NEXT entry's `- literals:` bullet must not be read as a file of this one"
        );
        assert_eq!(
            g[2].sources,
            vec![Source::Literal {
                key: "K".into(),
                value: "V".into()
            }]
        );
        assert_eq!(
            g[3].sources,
            vec![Source::File {
                key: "c.json".into(),
                path: "dashboards/c.json".into()
            }]
        );
        assert!(
            g.iter().all(|g| !g.name.is_empty()),
            "every entry must be named, else `check` reports a lost boundary"
        );
    }

    /// The consequence that matters: a large dashboard in a reordered entry is
    /// MEASURED. Swallowed as a file of the preceding entry it was never
    /// attributed, and the guard reported on a ConfigMap that does not exist
    /// while saying nothing about the one that would break cluster setup.
    #[test]
    fn a_dashboard_in_a_reordered_entry_is_measured_and_can_fail_the_size_rule() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let k = root.join("infra/x");
        std::fs::create_dir_all(k.join("dashboards")).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: first\n    files:\n      - small.json=dashboards/small.json\n  - files:\n      - big.json=dashboards/big.json\n    name: reordered\n",
        )
        .unwrap();
        std::fs::write(k.join("dashboards/small.json"), "{}").unwrap();
        // 40,000 `>` escape to six bytes each: ~240 KB, over the 80% threshold.
        std::fs::write(k.join("dashboards/big.json"), ">".repeat(40_000)).unwrap();
        let findings = check(root).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(
            findings[0].detail.contains("`reordered`"),
            "the finding must name the reordered generator, not its neighbour: {}",
            findings[0].detail
        );
    }

    #[test]
    fn a_nameless_entry_is_a_finding_because_kustomize_requires_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - literals:\n      - K=V\n",
        )
        .unwrap();
        let findings = check(dir.path()).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(findings[0].detail.contains("no `name:`"));
    }

    /// A YAML-supplied source path must face the repo's containment gate, and
    /// an escape must be a FINDING — never a skip, which would report a
    /// generator as measured when it was not.
    ///
    /// Both escape shapes: `../` traversal, and an ABSOLUTE path (which
    /// `Path::join` lets replace the base outright).
    #[test]
    fn a_source_path_resolving_outside_the_repository_is_a_finding_not_a_measurement() {
        let outer = tempfile::tempdir().unwrap();
        // A real, readable file OUTSIDE the repository root.
        let outside = outer.path().join("outside.json");
        std::fs::write(&outside, ">".repeat(40_000)).unwrap();
        let root = outer.path().join("repo");
        let k = root.join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            format!(
                "configMapGenerator:\n  - name: traverser\n    files:\n      \
                 - t.json=../../../outside.json\n  - name: absoluter\n    files:\n      \
                 - a.json={}\n",
                outside.display()
            ),
        )
        .unwrap();
        let findings = check(&root).unwrap();
        assert_eq!(findings.len(), 2, "{findings:#?}");
        for f in &findings {
            assert_eq!(f.rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
            assert!(
                f.detail.contains("resolves outside the repository"),
                "an escaping path must be reported as an escape, not measured or \
                 mislabelled unreadable: {}",
                f.detail
            );
        }
        assert!(
            findings.iter().any(|f| f.detail.contains("`traverser`"))
                && findings.iter().any(|f| f.detail.contains("`absoluter`")),
            "both escape shapes must be reported: {findings:#?}"
        );
    }

    /// A block sequence at its parent key's indent is valid YAML, accepted by
    /// kustomize, and `yq`'s DEFAULT output — including the sub-list sitting at
    /// its own `files:` key's indent. Every entry must still be found.
    const ZERO_INDENT: &str = r#"apiVersion: kustomize.config.k8s.io/v1beta1
configMapGenerator:
- name: zero-indent-first
  options:
    labels:
      grafana_dashboard: "1"
  files:
  - a.json=dashboards/a.json
  - dashboards/b.json
- literals:
  - K=V
  name: zero-indent-reordered
generatorOptions:
  disableNameSuffixHash: true
- name: after-the-top-level-key
"#;

    #[test]
    fn a_block_sequence_at_its_parent_keys_indent_is_still_the_section() {
        let g = parse_generators(ZERO_INDENT);
        let names: Vec<&str> = g.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(
            names,
            ["zero-indent-first", "zero-indent-reordered"],
            "a column-0 `- ` bullet is an ENTRY, not a new top-level key; and \
             `generatorOptions:` still closes the section, so the trailing bullet \
             after it must NOT be read as an entry"
        );
        assert_eq!(
            g[0].sources,
            vec![
                Source::File {
                    key: "a.json".into(),
                    path: "dashboards/a.json".into()
                },
                Source::File {
                    key: "b.json".into(),
                    path: "dashboards/b.json".into()
                },
            ],
            "a sub-list at its own `files:` key's indent is still a sub-list: its \
             bullets are deeper than the ENTRY bullet, which is what decides"
        );
        assert_eq!(
            g[0].labels,
            vec![("grafana_dashboard".to_string(), "1".to_string())]
        );
        assert_eq!(
            g[1].sources,
            vec![Source::Literal {
                key: "K".into(),
                value: "V".into()
            }],
            "the reordered zero-indent entry keeps its own literals"
        );
    }

    /// The consequence, measured: the whole section used to be skipped, so an
    /// oversized dashboard passed while `measured` stayed non-zero — fail-open
    /// with the vacuity guard none the wiser.
    #[test]
    fn a_zero_indent_section_is_measured_and_can_fail_the_size_rule() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let k = root.join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "apiVersion: kustomize.config.k8s.io/v1beta1\nconfigMapGenerator:\n- name: big\n  files:\n  - big.json=big.json\n",
        )
        .unwrap();
        std::fs::write(k.join("big.json"), ">".repeat(40_000)).unwrap();
        let findings = check(root).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(
            findings[0].detail.contains("`big`"),
            "{}",
            findings[0].detail
        );
    }

    /// The guard that makes the class self-reporting rather than silent: one
    /// file losing its whole section is invisible to the whole-tree `measured`
    /// counter, because the OTHER files keep it non-zero.
    #[test]
    fn a_declared_but_unparsed_section_is_a_finding_even_when_other_files_parse() {
        assert!(declares_generator_section(
            "configMapGenerator:\n- name: x\n"
        ));
        assert!(
            !declares_generator_section("configMapGenerator: []\n"),
            "the flow spelling legitimately has no entries"
        );
        assert!(
            !declares_generator_section("# configMapGenerator:\n"),
            "a commented-out key declares nothing"
        );

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("infra/good")).unwrap();
        std::fs::create_dir_all(root.join("infra/lost")).unwrap();
        std::fs::write(
            root.join("infra/good/kustomization.yaml"),
            "configMapGenerator:\n  - name: ok\n    literals:\n      - K=V\n",
        )
        .unwrap();
        // Declares the section, but every entry is written in a shape the parser
        // does not recognise (flow maps).
        std::fs::write(
            root.join("infra/lost/kustomization.yaml"),
            "configMapGenerator:\n  [{name: flow, literals: [K=V]}]\n",
        )
        .unwrap();
        let findings = check(root).unwrap();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(
            findings[0].detail.contains("parsed ZERO entries")
                && findings[0].detail.contains("infra/lost/kustomization.yaml"),
            "{}",
            findings[0].detail
        );
    }

    /// Containment is fail-CLOSED, so a root spelled non-canonically (`--root .`
    /// or a trailing `/.`, both of which Layer 3 could pass) must NOT make every
    /// in-repo source look like an escape — that would turn a whole-repo guard
    /// red on an artifact of how the root was spelled.
    #[test]
    fn a_non_canonical_repo_root_does_not_make_every_source_look_like_an_escape() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: cm\n    files:\n      - small.json\n",
        )
        .unwrap();
        std::fs::write(k.join("small.json"), "{}").unwrap();
        let dotted = dir.path().join(".");
        assert_eq!(check(&dotted).unwrap(), Vec::new());
    }

    /// The label rule's own source of truth being unreadable must not abort the
    /// whole `kustomize` run — that would take R-16/R-18/R-19 findings down
    /// with it — and must not pass quietly either.
    #[test]
    fn an_unreadable_grafana_deployment_is_a_label_finding_not_an_aborted_run() {
        let dir = tempfile::tempdir().unwrap();
        let g = dir.path().join("infra/grafana");
        std::fs::create_dir_all(&g).unwrap();
        std::fs::write(
            g.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: d\n    literals:\n      - K=V\n",
        )
        .unwrap();
        // No deployment.yaml at all.
        let findings = check(dir.path()).expect("a read failure must be a finding, not an Err");
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].rule_id, DASHBOARD_CONFIGMAP_LABEL_RULE_ID);
        assert!(
            findings[0]
                .detail
                .contains("cannot read infra/grafana/deployment.yaml"),
            "{}",
            findings[0].detail
        );
    }

    #[test]
    fn an_unreadable_referenced_file_is_a_finding_not_a_silent_undercount() {
        let dir = tempfile::tempdir().unwrap();
        let k = dir.path().join("infra/x");
        std::fs::create_dir_all(&k).unwrap();
        std::fs::write(
            k.join("kustomization.yaml"),
            "configMapGenerator:\n  - name: cm\n    files:\n      - missing.yaml\n",
        )
        .unwrap();
        let findings = check(dir.path()).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, CONFIGMAP_ANNOTATION_SIZE_RULE_ID);
        assert!(findings[0].detail.contains("cannot be read"));
    }
}
