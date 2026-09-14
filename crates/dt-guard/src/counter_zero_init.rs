//! `counter-zero-init` subcommand — enforces the counter-visibility fix
//! (ADR-0036 story-1): every catalogued enumerable discrete-event `*_total`
//! counter must be present-at-zero from process start, so `increase()` sees a
//! real `0→1` edge instead of a series born at 1.
//!
//! This is NOT `metric-coverage` (ADR-0032 *test* coverage) — different artifact
//! set (catalog + init site vs tests), different operator first-action, and a
//! different scope-liveness polarity (an empty scan here is FATAL, not safe).
//!
//! # The checks
//!
//! For each service (`crates/<svc>/src/observability/metrics.rs`, production code
//! only — `#[cfg(test)]` blocks excluded):
//!
//! * **Leg 2 — R \ Z (name coverage).** `R` = catalogued `### \`name\`` metrics
//!   ending `_total` that are emitted via `counter!` in that file and are NOT
//!   `Zero-init: exempt`. `Z` = counter names touched inside a
//!   `// dt-guard:zero-init-entrypoint`-marked fn body. Any `m ∈ R \ Z` FAILS.
//! * **Leg 3 — default-deny + mutual exclusion.** A counter that is BOTH
//!   exempt-marked AND touched in an entrypoint FAILS (closes the "touch one
//!   invented label instead of writing an exemption" escape). A blank/lazy
//!   exempt reason is fail-closed: `Malformed` never exempts (own token).
//! * **Witness predicate.** Each `const fn slot_*` witness (the sole drift
//!   control for a containment `VARIANTS` array once leg-2b was dropped) must be
//!   wildcard-free (no `_ =>`) AND not `#[cfg(test)]`-gated.
//! * **Vacuity / does-it-apply.** Scope-liveness (distinct root-absent vs
//!   empty-scan tokens); a service whose catalog lists `_total` counters but has
//!   NO marked entrypoint FAILS; a marked entrypoint fn never called from that
//!   service's `main.rs` FAILS (a marker on an uncalled fn registers nothing);
//!   a `counter!` with a non-literal first arg inside a marked body is reported.
//!
//! Per-label-combination completeness is the co-located render-based component
//! test's job (`zero_init_renders_counters_present_at_zero` in each service),
//! not this guard's — same guard/test division as ADR-0032.

use crate::common::explain::{print_finding, Finding};
use crate::common::metric_catalog::{parse_annotations, Marker, CATALOG_HEAD_RE};
use crate::common::services::CANONICAL_SERVICES;
use crate::common::status::{emit_ok, emit_scope};
use crate::common::test_code_filter::compute_test_block_ranges;
use crate::metric_macros::{MacroKind, MACRO_INVOCATION_RE, MACRO_INVOCATION_WITH_FIRST_ARG_RE};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

pub const MISSING_ZERO_INIT_RULE_ID: &str = "missing_zero_init";
pub const EXEMPT_AND_INIT_RULE_ID: &str = "exempt_and_zero_init";
pub const MALFORMED_EXEMPT_RULE_ID: &str = "malformed_exempt_reason";
pub const SLOT_WILDCARD_RULE_ID: &str = "slot_witness_wildcard";
pub const SLOT_CFG_TEST_RULE_ID: &str = "slot_witness_cfg_test";
pub const NO_ENTRYPOINT_RULE_ID: &str = "catalog_but_no_entrypoint";
pub const ENTRYPOINT_NOT_CALLED_RULE_ID: &str = "entrypoint_not_called_from_main";
pub const NON_LITERAL_NAME_RULE_ID: &str = "non_literal_metric_name_in_entrypoint";
pub const ROOT_ABSENT_RULE_ID: &str = "scan_root_absent";
pub const EMPTY_SCAN_RULE_ID: &str = "empty_scan_no_counters";

/// The self-declaring zero-init entrypoint marker comment.
pub const ENTRYPOINT_MARKER: &str = "dt-guard:zero-init-entrypoint";

/// The real marker: a `//` LINE comment whose first token is the marker. This
/// deliberately does NOT match `///` doc comments or backtick-wrapped mentions
/// of the marker in prose (`/// \`dt-guard:zero-init-entrypoint\``), so a fn is
/// counted once — by its actual marker, not its documentation of it.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local static-regex initializer; compiles at load-time or the binary fails — ADR-0034 §6"
)]
static MARKER_LINE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^[ \t]*//[ \t]*dt-guard:zero-init-entrypoint\b")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local static-regex initializer; compiles at load-time or the binary fails — ADR-0034 §6"
)]
static FN_AFTER_MARKER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\bfn\s+([a-z_][a-z0-9_]*)\s*\(").expect("static pattern compiles"));

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local static-regex initializer; compiles at load-time or the binary fails — ADR-0034 §6"
)]
static SLOT_FN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\bfn\s+(slot_[a-z0-9_]*)\s*\(").expect("static pattern compiles"));

/// A catch-all match arm — either `_ =>` OR a BINDING catch-all `<lowercase-ident> =>`
/// (`other => 0`), which defeats exhaustiveness identically to `_` (infra G4). The
/// arm-ident is required to follow a `{`, `,`, `|`, `(` or whitespace so a path
/// arm `Enum::foo =>` (variant reached via `::`) does NOT match. Lowercase-only
/// start distinguishes a binding from a PascalCase variant arm (`Voluntary =>`).
///
/// Documented gaps (accepted): a PascalCase binding (`Other => 0` where `Other`
/// is not a variant) is missed — pathological, and against Rust naming; and a
/// lowercase enum variant reached bare (not via a path) is a false positive,
/// which is why the SLOT witnesses this guards use PascalCase variants. Whitespace
/// variants (`_  =>`, split across lines) are not a concern: Layer 1 runs
/// `cargo fmt`, which normalizes them before this guard sees the file.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local static-regex initializer; compiles at load-time or the binary fails — ADR-0034 §6"
)]
static CATCHALL_ARM_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[\s{,|(](?:_|[a-z][a-z0-9_]*)\s*=>").expect("static pattern compiles")
});

#[derive(Debug, Clone)]
struct Finding0 {
    file: String,
    rule_id: &'static str,
    message: String,
}

impl Finding0 {
    fn print(&self, explain: bool) {
        if explain {
            let policy = format!("counter-zero-init::{}", self.rule_id);
            print_finding(&Finding {
                file: &self.file,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &self.message,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {} [{}] {}",
                self.file, self.rule_id, self.message
            );
        }
    }
}

/// Walk the balanced `{…}` body that begins at or after byte offset `from`.
/// Returns `(body_str, end_offset)` or `None` if no `{` follows / it is unbalanced.
#[expect(
    clippy::indexing_slicing,
    reason = "balanced-brace walker — `i` is bounded by `i < bytes.len()`, and the `open+1..i` \
              slice is bounded by the same walk, so neither can panic (mirrors dashboard_panels.rs)"
)]
fn balanced_body(src: &str, from: usize) -> Option<(&str, usize)> {
    let bytes = src.as_bytes();
    let open = src[from..].find('{')? + from;
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&src[open + 1..i], i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// 1-based LINE ranges of `#[cfg(test)]` blocks — a passthrough to the shared
/// line-based helper (G5: no byte-offset conversion; `in_test_block` compares
/// line numbers, and `line_of` maps an offset to a line for that comparison).
fn test_line_set(content: &str) -> Vec<(usize, usize)> {
    compute_test_block_ranges(content)
}

/// Map a byte offset to a 1-based line number.
fn line_of(content: &str, offset: usize) -> usize {
    content[..offset.min(content.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1
}

fn in_test_block(ranges: &[(usize, usize)], line: usize) -> bool {
    ranges.iter().any(|(a, b)| line >= *a && line <= *b)
}

/// Counter (`counter!`) `*_total` metric names emitted in `content`, excluding
/// `#[cfg(test)]` blocks. Returns (production names, whether any non-literal
/// first-arg counter site appears inside a marked entrypoint body).
struct Extracted {
    /// All production `*_total` counter names emitted anywhere in the file.
    emitted: BTreeSet<String>,
    /// `*_total` counter names touched inside any marked entrypoint body.
    zero_init: BTreeSet<String>,
    /// Names of the marked entrypoint fns (for the main.rs-call check).
    entrypoint_fns: Vec<String>,
    /// True if a `counter!` with a non-literal first arg sits in a marked body.
    non_literal_in_entrypoint: bool,
}

fn extract(content: &str) -> Extracted {
    let test_ranges = test_line_set(content);
    let is_test = |off: usize| in_test_block(&test_ranges, line_of(content, off));

    // All production *_total counter emissions (name -> counted).
    let mut emitted = BTreeSet::new();
    for caps in MACRO_INVOCATION_WITH_FIRST_ARG_RE.captures_iter(content) {
        let (Some(kind_m), Some(name_m)) = (caps.get(2), caps.get(3)) else {
            continue;
        };
        if is_test(name_m.start()) {
            continue;
        }
        if MacroKind::parse(kind_m.as_str()) != Some(MacroKind::Counter) {
            continue;
        }
        let name = name_m.as_str();
        if name.ends_with("_total") {
            emitted.insert(name.to_string());
        }
    }

    // Marked entrypoint bodies: for each marker, the following fn's body.
    let mut zero_init = BTreeSet::new();
    let mut entrypoint_fns = Vec::new();
    let mut non_literal_in_entrypoint = false;
    for m in MARKER_LINE_RE.find_iter(content) {
        let marker_off = m.start();
        if is_test(marker_off) {
            continue;
        }
        // The fn following the marker.
        let Some(fn_caps) = FN_AFTER_MARKER_RE.captures(&content[marker_off..]) else {
            continue;
        };
        if let Some(nm) = fn_caps.get(1) {
            entrypoint_fns.push(nm.as_str().to_string());
        }
        let fn_open = marker_off + fn_caps.get(0).map_or(0, |m| m.start());
        let Some((body, _end)) = balanced_body(content, fn_open) else {
            continue;
        };
        // Counter names + non-literal detection inside this body.
        for caps in MACRO_INVOCATION_WITH_FIRST_ARG_RE.captures_iter(body) {
            let Some(kind_m) = caps.get(2) else { continue };
            if MacroKind::parse(kind_m.as_str()) != Some(MacroKind::Counter) {
                continue;
            }
            match caps.get(3) {
                Some(name_m) if name_m.as_str().ends_with("_total") => {
                    zero_init.insert(name_m.as_str().to_string());
                }
                _ => {}
            }
        }
        // Non-literal first arg: a `counter!(` opener in the body whose first arg
        // is not a string literal. Count openers precisely through the SHARED
        // opener (which captures the kind) filtered to `Counter`, NOT a
        // `matches("counter!")` substring — the latter also counts
        // `describe_counter!` (which CONTAINS `counter!`), a false positive on the
        // very `describe_counter!` + `counter!(…).increment(0)` shape FIX 1
        // prescribes (infra G1). The difference vs the literal-arg count is the
        // number of non-literal `counter!` sites.
        let openers = MACRO_INVOCATION_RE
            .captures_iter(body)
            .filter(|c| {
                c.get(2).and_then(|k| MacroKind::parse(k.as_str())) == Some(MacroKind::Counter)
            })
            .count();
        let literal_counters = MACRO_INVOCATION_WITH_FIRST_ARG_RE
            .captures_iter(body)
            .filter(|c| {
                c.get(2).and_then(|k| MacroKind::parse(k.as_str())) == Some(MacroKind::Counter)
            })
            .count();
        if openers > literal_counters {
            non_literal_in_entrypoint = true;
        }
    }

    Extracted {
        emitted,
        zero_init,
        entrypoint_fns,
        non_literal_in_entrypoint,
    }
}

/// Catalogued `_total` metric names for a service catalog source.
fn catalog_total_names(catalog_src: &str) -> BTreeSet<String> {
    CATALOG_HEAD_RE
        .captures_iter(catalog_src)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .filter(|n| n.ends_with("_total"))
        .collect()
}

/// Slot-witness predicate: each `const fn slot_*` (or `fn slot_*`) must be
/// wildcard-free and not inside a `#[cfg(test)]` block. Returns findings.
///
/// SCOPE: this only inspects each service's `observability/metrics.rs` (the
/// caller's scan set), so a `slot_*` witness placed in any other file would be
/// silently unprotected — the witnesses must live in metrics.rs alongside the
/// `VARIANTS` arrays they guard. (infra review residual, 2026-09-11.)
fn check_slot_witnesses(content: &str, rel: &str, findings: &mut Vec<Finding0>) {
    let test_ranges = test_line_set(content);
    for caps in SLOT_FN_RE.captures_iter(content) {
        let Some(whole) = caps.get(0) else { continue };
        let name = caps.get(1).map_or("", |m| m.as_str());
        let start = whole.start();
        if in_test_block(&test_ranges, line_of(content, start)) {
            findings.push(Finding0 {
                file: rel.to_string(),
                rule_id: SLOT_CFG_TEST_RULE_ID,
                message: format!(
                    "witness `{name}` is inside a #[cfg(test)] block — a release build never \
                     compiles it, so the sole drift control for its VARIANTS array is inert"
                ),
            });
            continue;
        }
        if let Some((body, _)) = balanced_body(content, start) {
            if CATCHALL_ARM_RE.is_match(body) {
                findings.push(Finding0 {
                    file: rel.to_string(),
                    rule_id: SLOT_WILDCARD_RULE_ID,
                    message: format!(
                        "witness `{name}` has a catch-all arm (`_ =>` or a binding like \
                         `other =>`) — a new enum variant no longer reds the build, so the \
                         containment array can silently go short"
                    ),
                });
            }
        }
    }
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let mut findings: Vec<Finding0> = Vec::new();
    let mut services_scanned = 0usize;
    let mut counters_required = 0usize;
    let mut exemptions_honored = 0usize;
    let mut entrypoints_found = 0usize;

    for (prefix, dir) in CANONICAL_SERVICES {
        let metrics_path = repo_root
            .join("crates")
            .join(dir)
            .join("src/observability/metrics.rs");
        let catalog_path = repo_root
            .join("docs/observability/metrics")
            .join(format!("{prefix}-service.md"));
        // G2: distinguish two absences.
        //   (a) The crate exists but its metrics.rs does not — a rename/move of
        //       the scan target *within* a live service. This is the silent-
        //       quarter-drop hole infra flagged: the other three services keep
        //       `services_scanned` non-zero, so the weak `== 0` check never
        //       fires and a quarter of scope vanishes unnoticed. HARD ERROR
        //       with its own token.
        //   (b) The whole crate dir is absent. In the real repo this is
        //       impossible — every `crates/*-service` is a workspace member, so
        //       cargo fails to build and `release_build_profile`'s
        //       `canonical_services_roster_drift` reds long before layer3. Its
        //       only real occurrence is a synthetic partial root (the self-test
        //       exercises one service at a time). `continue` here is therefore
        //       safe: a genuinely-deleted service is caught louder elsewhere,
        //       and the all-absent case still reds via the `services_scanned ==
        //       0` EMPTY_SCAN check below (self-test case 8).
        let crate_dir = repo_root.join("crates").join(dir);
        if !crate_dir.is_dir() {
            continue;
        }
        if !metrics_path.is_file() {
            anyhow::bail!(
                "{ROOT_ABSENT_RULE_ID}: canonical service {prefix} has a crate at {} but no \
                 metrics.rs at {} — the scan target was renamed or moved out of a live service; \
                 the guard would otherwise silently stop covering a quarter of its scope",
                crate_dir.display(),
                metrics_path.display()
            );
        }
        services_scanned += 1;

        let content = std::fs::read_to_string(&metrics_path)
            .with_context(|| format!("read {}", metrics_path.display()))?;
        // G3: a listed service's catalog is a PRECONDITION, not an empty result —
        // `unwrap_or_default()` here would reintroduce the F5 fail-open (empty
        // catalog ⇒ empty required ⇒ no `catalog_but_no_entrypoint` and the
        // service silently exits coverage while the others keep the counters
        // non-zero so `EMPTY_SCAN` never fires either). Propagate.
        let catalog_src = std::fs::read_to_string(&catalog_path).with_context(|| {
            format!(
                "read catalog {} — a canonical service's catalog is a precondition (G3)",
                catalog_path.display()
            )
        })?;

        let extracted = extract(&content);
        let catalog_totals = catalog_total_names(&catalog_src);
        let annotations = parse_annotations(&catalog_src);
        let rel = format!("crates/{dir}/src/observability/metrics.rs");

        check_slot_witnesses(&content, &rel, &mut findings);

        // Required set R = catalogued _total ∩ emitted, minus exempt.
        let exempt: HashSet<&String> = annotations
            .iter()
            .filter(|(_, a)| a.zero_init_exempt.is_present())
            .map(|(n, _)| n)
            .collect();

        // Malformed exempt markers fail closed (own finding + still required).
        for (name, a) in &annotations {
            if let Marker::Malformed(reason) = &a.zero_init_exempt {
                findings.push(Finding0 {
                    file: format!("docs/observability/metrics/{prefix}-service.md"),
                    rule_id: MALFORMED_EXEMPT_RULE_ID,
                    message: format!(
                        "metric `{name}` has a Zero-init exempt marker with a blank/lazy reason \
                         ({reason:?}) — fail-closed: it does NOT exempt, the counter stays required"
                    ),
                });
            }
        }

        let required: BTreeSet<&String> = catalog_totals
            .iter()
            .filter(|n| extracted.emitted.contains(*n))
            .filter(|n| !exempt.contains(*n))
            .collect();
        counters_required += required.len();
        exemptions_honored += exempt.len();
        entrypoints_found += extracted.entrypoint_fns.len();

        // Vacuity: catalog lists _total counters but there is no marked entrypoint.
        if !catalog_totals.is_empty() && extracted.entrypoint_fns.is_empty() && !required.is_empty()
        {
            findings.push(Finding0 {
                file: rel.clone(),
                rule_id: NO_ENTRYPOINT_RULE_ID,
                message: format!(
                    "{prefix}-service catalogues {} `*_total` counters but metrics.rs has NO \
                     `// {ENTRYPOINT_MARKER}`-marked fn — nothing is zero-initialized",
                    catalog_totals.len()
                ),
            });
        }

        // Leg 3: mutual exclusion (exempt AND zero-init'd).
        for name in &extracted.zero_init {
            if exempt.contains(name) {
                findings.push(Finding0 {
                    file: rel.clone(),
                    rule_id: EXEMPT_AND_INIT_RULE_ID,
                    message: format!(
                        "`{name}` is BOTH catalog-exempt AND touched in a zero-init entrypoint — \
                         pick one; exempt means 'not zero-init'd for a stated reason'"
                    ),
                });
            }
        }

        // Non-literal metric name inside a marked body.
        if extracted.non_literal_in_entrypoint {
            findings.push(Finding0 {
                file: rel.clone(),
                rule_id: NON_LITERAL_NAME_RULE_ID,
                message:
                    "a `counter!` with a non-literal first arg sits inside a zero-init entrypoint — \
                     the guard cannot verify which metric it touches; use a string literal"
                        .to_string(),
            });
        }

        // Leg 2: R \ Z.
        for name in &required {
            if !extracted.zero_init.contains(*name) {
                findings.push(Finding0 {
                    file: rel.clone(),
                    rule_id: MISSING_ZERO_INIT_RULE_ID,
                    message: format!(
                        "`{name}` is catalogued + emitted + not exempt, but is NOT touched in any \
                         zero-init entrypoint — it will be lazily created at 1 (the defect)"
                    ),
                });
            }
        }

        // Entrypoint-called-from-main.rs.
        // Asymmetry with the two reads above (G3): `unwrap_or_default()` is
        // intentionally fail-CLOSED *here*. An empty `main_src` contains no
        // `fn_name(` call, so every entrypoint reds `entrypoint_not_called` —
        // a missing/unreadable main.rs can only over-report, never mask. The
        // metrics.rs and catalog reads are the opposite: an empty string there
        // yields zero required counters and zero findings, silently shrinking
        // scope, so those must propagate.
        let main_path = repo_root.join("crates").join(dir).join("src/main.rs");
        let main_src = std::fs::read_to_string(&main_path).unwrap_or_default();
        for fn_name in &extracted.entrypoint_fns {
            if !main_src.contains(&format!("{fn_name}(")) {
                findings.push(Finding0 {
                    file: format!("crates/{dir}/src/main.rs"),
                    rule_id: ENTRYPOINT_NOT_CALLED_RULE_ID,
                    message: format!(
                        "zero-init entrypoint `{fn_name}` is never called from {dir}/src/main.rs — \
                         a marker on an uncalled fn registers nothing at runtime (the guard would lie)"
                    ),
                });
            }
        }
    }

    // Scope-liveness (does-it-apply positive control): distinct tokens.
    if services_scanned == 0 {
        for f in &findings {
            f.print(explain);
        }
        anyhow::bail!(ROOT_ABSENT_RULE_ID);
    }
    // Vacuity: the scan is empty only if it found NO catalogued+emitted `_total`
    // counters AT ALL (required + exempt). An all-exempt service (required=0,
    // exempt>0) is legitimately covered, not vacuous.
    if counters_required == 0 && exemptions_honored == 0 && findings.is_empty() {
        anyhow::bail!(EMPTY_SCAN_RULE_ID);
    }

    emit_scope(format!(
        "counter-zero-init services={services_scanned} required={counters_required} \
         exempt={exemptions_honored} entrypoints={entrypoints_found}"
    ));

    if findings.is_empty() {
        emit_ok(format!(
            "counter-zero-init-clean-{services_scanned}-services"
        ));
        return Ok(());
    }
    for f in &findings {
        f.print(explain);
    }
    anyhow::bail!("counter-zero-init: {} violation(s)", findings.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- extract() ----

    #[test]
    fn extracts_emitted_and_zero_init_names() {
        let src = r#"
            pub fn record_x() { counter!("svc_x_total", "k" => "v").increment(1); }
            // dt-guard:zero-init-entrypoint
            pub fn zero_initialize_counters() {
                counter!("svc_x_total", "k" => "v").increment(0);
                counter!("svc_y_total").increment(0);
            }
        "#;
        let e = extract(src);
        assert!(e.emitted.contains("svc_x_total"));
        assert!(e.zero_init.contains("svc_x_total"));
        assert!(e.zero_init.contains("svc_y_total"));
        assert_eq!(
            e.entrypoint_fns,
            vec!["zero_initialize_counters".to_string()]
        );
        assert!(!e.non_literal_in_entrypoint);
    }

    #[test]
    fn cfg_test_emissions_excluded() {
        let src = "pub fn p() {\n    counter!(\"a_total\").increment(1);\n}\n#[cfg(test)]\nmod tests {\n    fn t() {\n        counter!(\"b_total\").increment(1);\n    }\n}\n";
        let e = extract(src);
        assert!(e.emitted.contains("a_total"));
        assert!(
            !e.emitted.contains("b_total"),
            "test-block emission excluded"
        );
    }

    fn body_findings(content: &str) -> Vec<Finding0> {
        let mut f = Vec::new();
        check_slot_witnesses(content, "x.rs", &mut f);
        f
    }

    #[test]
    fn slot_witness_wildcard_fires() {
        let src = "const fn slot_x(v: T) -> usize { match v { A => 0, _ => 1 } }";
        let f = body_findings(src);
        assert!(f.iter().any(|f| f.rule_id == SLOT_WILDCARD_RULE_ID));
    }

    #[test]
    fn slot_witness_clean_passes() {
        let src = "const fn slot_x(v: T) -> usize { match v { A => 0, B => 1 } }";
        assert!(body_findings(src).is_empty());
    }

    // ---- balanced_body ----
    #[test]
    fn balanced_body_walks_nested() {
        let src = "fn f() { if x { a } else { b } }";
        let (body, _) = balanced_body(src, 0).unwrap();
        assert!(body.contains("if x"));
        assert!(body.contains("else"));
    }

    /// Derivation oracle (ADR-0034 §M4): this guard filters emissions to
    /// `MacroKind::Counter` extracted through the shared
    /// `MACRO_INVOCATION_WITH_FIRST_ARG_RE` opener. Prove `counter!` still
    /// arrives through that path, so a narrowing of `MacroKind::ALL` that dropped
    /// `Counter` would red HERE (complementing the frozen-literal pin at the SoT).
    #[test]
    fn counter_kind_is_discovered_through_the_shared_opener() {
        let caps = MACRO_INVOCATION_WITH_FIRST_ARG_RE
            .captures(r#"counter!("svc_x_total", "k" => "v")"#)
            .expect("counter! is matched by the shared opener");
        assert_eq!(
            caps.get(2).and_then(|k| MacroKind::parse(k.as_str())),
            Some(MacroKind::Counter),
            "counter! must resolve to MacroKind::Counter through the shared opener"
        );
        assert_eq!(caps.get(3).map(|m| m.as_str()), Some("svc_x_total"));
    }
}
