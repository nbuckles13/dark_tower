//! The media kind labels `media_kind` / `content_kind`: their two projection
//! homes, the key set, key-constant resolution, and the PromQL co-occurrence
//! helper. Shared by the kind-label rules of `metric_labels` (Rust),
//! `ts_metric_naming` (TypeScript), `alert_rules` and `dashboard_panels`.
//!
//! # Ownership (CLAUDE.md guard-crate split)
//!
//! MEMBERSHIP is policy content and belongs to `observability`: which keys are
//! kind labels ([`KIND_LABEL_KEYS`]) and which files are the projection homes
//! ([`RUST_PROJECTION_HOME`], [`TS_PROJECTION_HOME`]). The construct shapes and
//! their evaluation are machinery (`infrastructure`).
//!
//! # The invariant these serve
//!
//! `docs/observability/label-taxonomy.md` §"Carried: `media_kind` and
//! `content_kind`": the two labels are ONE projection of the wire `MediaKind`,
//! with exactly one projection per language. Their value literals live only in
//! the two homes; every other emission site gets its values from the
//! projection. Drift between the homes, the taxonomy and the collector is
//! `client_metrics_export`'s `kind_projection_*` rules; this module serves the
//! per-site rules.

use crate::common::ident::{is_ident_byte, names_identifier};
use crate::common::rust_consts::rust_str_consts;
use std::collections::HashMap;

/// The kind label keys, always co-carried (all or none).
pub const KIND_LABEL_KEYS: [&str; 2] = ["media_kind", "content_kind"];

/// The Rust projection home: `KindLabels::from_proto` and the two `label()`
/// value matches. The ONLY Rust file whose kind-label values may be literals.
pub const RUST_PROJECTION_HOME: &str = "crates/mc-service/src/media_kind.rs";

/// The TypeScript projection home: `MEDIA_KIND_LABEL_PROJECTION` /
/// `projectMediaKind`. The ONLY TypeScript file whose kind-label values may be
/// literals.
pub const TS_PROJECTION_HOME: &str = "packages/sdk-core/src/media/setup/mediaMetrics.ts";

/// Where the Rust key constants (`MEDIA_KIND_LABEL`, `CONTENT_KIND_LABEL`)
/// are declared. A missing file is vacuity, never "no keys to check".
pub const LABELS_RS: &str = "crates/common/src/observability/labels.rs";

/// MH is type-blind (ADR-0036 §7): no kind key in any metric under this crate.
pub const MH_CRATE_PREFIX: &str = "crates/mh-service/";

/// The Rust spellings of the kind keys: the string literal itself and every
/// `pub const` in [`LABELS_RS`] whose value is a kind key.
#[derive(Debug, Clone, Default)]
pub struct KindKeys {
    /// Const ident -> the kind key it spells.
    idents: HashMap<String, &'static str>,
    /// The key set in force (narrowed only by the neutered-matcher tests).
    keys: Vec<&'static str>,
}

impl KindKeys {
    /// Resolve the key constants from [`LABELS_RS`]'s source.
    ///
    /// # Errors
    ///
    /// A kind key with no `pub const` spelling it: the guard could not
    /// recognise constant-keyed sites, so it must not report clean.
    pub fn load(labels_src: &str) -> Result<Self, String> {
        Self::with_keys(labels_src, &KIND_LABEL_KEYS)
    }

    /// [`Self::load`] over an explicit key set. The seam for the
    /// neutered-matcher proofs (an empty set must turn every red fixture
    /// green).
    ///
    /// # Errors
    ///
    /// As [`Self::load`].
    pub fn with_keys(labels_src: &str, keys: &[&'static str]) -> Result<Self, String> {
        let consts = rust_str_consts(labels_src);
        let mut idents = HashMap::new();
        for key in keys {
            let named: Vec<&String> = consts
                .iter()
                .filter(|(_, v)| v.as_str() == *key)
                .map(|(k, _)| k)
                .collect();
            if named.is_empty() {
                return Err(format!(
                    "no `pub const …: &str = \"{key}\";` in {LABELS_RS}: constant-keyed \
                     `{key}` sites cannot be recognised"
                ));
            }
            for ident in named {
                idents.insert(ident.clone(), *key);
            }
        }
        Ok(Self {
            idents,
            keys: keys.to_vec(),
        })
    }

    /// The kind key a Rust label-KEY expression spells, if any: a string
    /// literal equal to a key, or an identifier (last `::` segment) resolved
    /// through [`LABELS_RS`].
    #[must_use]
    pub fn key_of(&self, key_expr: &str) -> Option<&'static str> {
        let t = key_expr.trim();
        if let Some(lit) = t.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
            return self.keys.iter().copied().find(|k| *k == lit);
        }
        let ident = t.rsplit("::").next().unwrap_or(t).trim();
        self.idents.get(ident).copied()
    }

    /// The key set in force.
    #[must_use]
    pub fn keys(&self) -> &[&'static str] {
        &self.keys
    }
}

/// Is `value_expr` a projection value: a `….label()` method call (on a
/// `KindLabels` field, by the recorders' signatures)?
///
/// BOUND, stated honestly: this is a shape check. It cannot prove the
/// receiver's type, so `x.label()` on an unrelated enum passes. The type-taking
/// recorders (`KindLabels`) and the drift rules close that gap; this rule
/// closes the literal and the bare-variable forms.
#[must_use]
pub fn is_projection_value(value_expr: &str) -> bool {
    let t = value_expr.trim();
    t.strip_suffix(".label()")
        .is_some_and(|recv| !recv.trim().is_empty() && !recv.trim_end().ends_with('"'))
}

/// The rule id of the co-occurrence rule, shared by every host that runs it
/// (`alert_rules` over rule exprs and docs PromQL, `dashboard_panels`).
pub const KIND_LABEL_CO_OCCURRENCE_RULE_ID: &str = "kind_label_co_occurrence";

/// The docs whose PromQL operators copy and paste: the co-occurrence rule's
/// docs scope. Dated records (`docs/devloop-outputs/`, `docs/debates/`,
/// `docs/decisions/`, `docs/user-stories/`) are deliberately outside it —
/// rewriting history there is wrong. Membership is observability's.
pub const DOC_PROMQL_SCOPE: &[&str] = &["docs/observability/", "docs/runbooks/"];

/// The clause keywords whose parenthesised label list is a co-occurrence unit.
const CLAUSE_KEYWORDS: [&str; 6] = [
    "by",
    "without",
    "on",
    "ignoring",
    "group_left",
    "group_right",
];

/// The byte offset of the bracket closing the one at `open` (same bracket
/// kind, nesting-aware), or the end of `text`.
fn closing(text: &str, open: usize, open_b: u8, close_b: u8) -> usize {
    let mut depth = 0usize;
    for (i, b) in text.bytes().enumerate().skip(open) {
        if b == open_b {
            depth += 1;
        } else if b == close_b {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return i;
            }
        }
    }
    text.len()
}

/// The bracketed groups of a PromQL fragment: `{…}` inners (not legends),
/// `{{…}}` legend placeholder inners, and clause `(…)` inners.
#[derive(Debug, Default)]
struct Groups<'a> {
    braces: Vec<&'a str>,
    legends: Vec<&'a str>,
    clauses: Vec<&'a str>,
}

fn groups(expr: &str) -> Groups<'_> {
    let bytes = expr.as_bytes();
    let mut g = Groups::default();
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i) == Some(&b'{') {
            let end = closing(expr, i, b'{', b'}');
            let inner = expr.get(i + 1..end).unwrap_or("");
            match inner.strip_prefix('{').and_then(|x| x.strip_suffix('}')) {
                Some(legend) => g.legends.push(legend),
                None => g.braces.push(inner),
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    for kw in CLAUSE_KEYWORDS {
        for (at, _) in expr.match_indices(kw) {
            let before = at.checked_sub(1).and_then(|j| bytes.get(j)).copied();
            let after = at + kw.len();
            if before.is_some_and(is_ident_byte)
                || bytes.get(after).copied().is_some_and(is_ident_byte)
            {
                continue;
            }
            let rest = expr.get(after..).unwrap_or("");
            let open = after + (rest.len() - rest.trim_start().len());
            if bytes.get(open) == Some(&b'(') {
                let end = closing(expr, open, b'(', b')');
                g.clauses.push(expr.get(open + 1..end).unwrap_or(""));
            }
        }
    }
    g
}

/// The co-occurrence UNITS of a PromQL fragment: each `{…}` selector (or
/// key-list) and each `by|without|on|ignoring|group_left|group_right (…)`
/// clause. Grafana legend placeholders `{{…}}` are NOT units here: together
/// they form one unit (a legend `{{media_kind}}/{{content_kind}}` names both),
/// which [`co_occurrence_gaps`] builds.
#[must_use]
pub fn co_occurrence_units(expr: &str) -> Vec<&str> {
    let g = groups(expr);
    g.braces.into_iter().chain(g.clauses).collect()
}

/// Whether a docs code span is PromQL-shaped enough to check (observability's
/// ruling, F3): it has (a) a label matcher (`=`, `=~`, `!=`, `!~` inside a
/// `{…}`), or (b) a grouping / vector-matching clause followed by `(`, or (c)
/// a `{…}` or `{{…}}` group naming a kind key (key lists, legends). Bare prose
/// such as `` `media_kind` `` is not scanned.
#[must_use]
pub fn is_promql_shaped(text: &str, keys: &[&str]) -> bool {
    let g = groups(text);
    g.braces.iter().any(|b| b.contains('='))
        || !g.clauses.is_empty()
        || g.braces
            .iter()
            .chain(&g.legends)
            .any(|b| keys.iter().any(|k| names_identifier(b, k)))
}

/// The co-occurrence GAPS of a PromQL fragment, per unit
/// ([`co_occurrence_units`]): `(named, missing)` for every unit naming one
/// kind key and not the other (taxonomy §query rule). An expression naming
/// both keys only across DIFFERENT units still fails, because a selector that
/// names `media_kind` alone folds screen share into the camera however the
/// rest of the expression groups.
#[must_use]
pub fn co_occurrence_gaps<'k>(expr: &str, keys: &[&'k str]) -> Vec<(&'k str, &'k str)> {
    let check = |unit: &str| -> Option<(&'k str, &'k str)> {
        let named: Vec<&'k str> = keys
            .iter()
            .copied()
            .filter(|k| names_identifier(unit, k))
            .collect();
        let first = *named.first()?;
        let missing = keys.iter().copied().find(|k| !named.contains(k))?;
        Some((first, missing))
    };
    let g = groups(expr);
    let legend = g.legends.join(" ");
    g.braces
        .iter()
        .chain(&g.clauses)
        .copied()
        .chain((!legend.is_empty()).then_some(legend.as_str()))
        .filter_map(check)
        .collect()
}

/// The first co-occurrence gap, if any (see [`co_occurrence_gaps`]).
#[must_use]
pub fn co_occurrence_gap<'k>(expr: &str, keys: &[&'k str]) -> Option<(&'k str, &'k str)> {
    co_occurrence_gaps(expr, keys).into_iter().next()
}

/// THE finding message of the co-occurrence rule, for every host.
#[must_use]
pub fn co_occurrence_message(named: &str, missing: &str) -> String {
    format!(
        "a selector or clause names `{named}` but not `{missing}`; the two kind labels are one \
         projection — every `{{…}}` selector and every by/without/on/ignoring clause names both \
         or neither (label-taxonomy.md §\"Carried: `media_kind` and `content_kind`\", query rule)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LABELS: &str = r#"
pub const MEDIA_KIND_LABEL: &str = "media_kind";
pub const CONTENT_KIND_LABEL: &str = "content_kind";
pub const KEY_CUSTODY_LABEL: &str = "key_custody";
"#;

    #[test]
    fn keys_resolve_by_literal_const_and_path() {
        let k = KindKeys::load(LABELS).unwrap();
        assert_eq!(k.key_of("\"media_kind\""), Some("media_kind"));
        assert_eq!(k.key_of("CONTENT_KIND_LABEL"), Some("content_kind"));
        assert_eq!(
            k.key_of("common::observability::labels::MEDIA_KIND_LABEL"),
            Some("media_kind")
        );
        assert_eq!(k.key_of("KEY_CUSTODY_LABEL"), None);
        assert_eq!(k.key_of("\"outcome\""), None);
    }

    #[test]
    fn an_unresolvable_key_is_an_error_not_an_empty_set() {
        let err = KindKeys::load("pub const MEDIA_KIND_LABEL: &str = \"media_kind\";").unwrap_err();
        assert!(err.contains("content_kind"), "{err}");
    }

    #[test]
    fn the_real_labels_file_resolves_both_keys() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join(LABELS_RS),
        )
        .unwrap();
        let k = KindKeys::load(&src).unwrap();
        assert_eq!(k.key_of("MEDIA_KIND_LABEL"), Some("media_kind"));
        assert_eq!(k.key_of("CONTENT_KIND_LABEL"), Some("content_kind"));
    }

    #[test]
    fn projection_values_are_label_calls_only() {
        assert!(is_projection_value("labels.media_kind.label()"));
        assert!(is_projection_value(" l.content_kind.label() "));
        assert!(!is_projection_value("\"audio\""));
        assert!(!is_projection_value("kind"));
        assert!(!is_projection_value("\"audio\".label()"));
        assert!(!is_projection_value(".label()"));
    }

    #[test]
    fn co_occurrence_needs_both_or_neither_per_unit() {
        let keys = KIND_LABEL_KEYS;
        let gap = |e: &str| co_occurrence_gap(e, &keys);
        assert_eq!(
            gap("sum by (media_kind) (rate(x[5m]))"),
            Some(("media_kind", "content_kind"))
        );
        assert_eq!(
            gap("x{content_kind=\"main\"}"),
            Some(("content_kind", "media_kind"))
        );
        assert_eq!(gap("sum by (media_kind, content_kind) (x)"), None);
        assert_eq!(gap("sum(x)"), None);
        // Whole identifiers only.
        assert_eq!(gap("x{outcome=\"media_kind_unsupported\"}"), None);
        // PER UNIT: both keys in the expr, split across units, still fails.
        assert_eq!(
            gap("sum(rate(a{media_kind=\"video\", content_kind=\"main\"}[5m])) / sum(rate(b{media_kind=\"video\"}[5m]))"),
            Some(("media_kind", "content_kind"))
        );
        assert_eq!(
            co_occurrence_gaps(
                "sum by (media_kind) (rate(x{content_kind=\"main\"}[5m]))",
                &keys
            ),
            vec![
                ("content_kind", "media_kind"),
                ("media_kind", "content_kind")
            ]
        );
        assert_eq!(
            gap("a / ignoring(media_kind) group_left() b{media_kind=\"v\", content_kind=\"main\"}"),
            Some(("media_kind", "content_kind"))
        );
        assert_eq!(
            gap("a / ignoring(media_kind, content_kind) group_left() b"),
            None
        );
        // Key lists and legends.
        assert_eq!(
            gap("mc_media_receive_slot_cap{media_kind}"),
            Some(("media_kind", "content_kind"))
        );
        assert_eq!(gap("{{media_kind}}/{{content_kind}}"), None);
        assert_eq!(
            gap("{{instance}} {{media_kind}}"),
            Some(("media_kind", "content_kind"))
        );
        // The keyword must be a whole identifier: `bonus (media_kind)` is no clause.
        assert!(co_occurrence_units("rate(x[5m]) on(instance)").contains(&"instance"));
        assert!(co_occurrence_units("bonus (media_kind)").is_empty());
    }

    #[test]
    fn promql_shape_needs_a_matcher_a_clause_or_a_kind_brace() {
        let keys = KIND_LABEL_KEYS;
        assert!(is_promql_shaped("x{reason=\"a\"}", &keys));
        assert!(is_promql_shaped("sum by (instance) (x)", &keys));
        assert!(is_promql_shaped(
            "mc_media_receive_slot_cap{media_kind}",
            &keys
        ));
        assert!(is_promql_shaped("{{media_kind}}", &keys));
        assert!(!is_promql_shaped("media_kind", &keys));
        assert!(!is_promql_shaped("sum(rate(x[5m]))", &keys));
        assert!(!is_promql_shaped("{instance}", &keys));
    }

    #[test]
    fn the_message_names_both_keys() {
        let m = co_occurrence_message("media_kind", "content_kind");
        assert!(
            m.contains("`media_kind`") && m.contains("`content_kind`"),
            "{m}"
        );
    }
}
