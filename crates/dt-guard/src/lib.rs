//! dt-guard library — pure logic re-exported for binary + integration tests.
//!
//! Per ADR-0034 §1: `lib.rs` re-exports policy modules; `main.rs` is pure clap
//! dispatch + STATUS emission with no business logic. Modules organize by
//! policy + shared kernel:
//!
//! * [`common`] — STATUS emission, error helpers, duration parsing, path-safety
//!   gate (canonical home for `resolve_cited_path`; consumed by `cite_extract`
//!   AND `alert_rules` per ADR §5/§8).
//! * [`ignore`] — canonical `LAZY_REASON_RE` + `IGNORE_MARKER_RE` (hash + html
//!   flavors). Per ADR §6: single home for the lazy-reason vocabulary across
//!   cite-extract, alert-rules, metric-labels.
//! * [`secret_patterns`] — canonical `HYGIENE_PATTERNS` (Wave 2 reuse hook).
//! * [`metric_macros`] — canonical home for the `counter!`/`gauge!`/`histogram!`
//!   family per ADR §1 (Wave 2 reuse hook — `application_metrics`,
//!   `metric_labels`, `infrastructure_metrics`).
//! * [`cite_extract`] — doc-citation extraction + resolution (ADR §4-5).

pub mod alert_rules;
pub mod application_metrics;
pub mod cite_extract;
pub mod common;
pub mod dashboard_panels;
pub mod grafana_datasources;
pub mod ignore;
pub mod infrastructure_metrics;
pub mod metric_labels;
pub mod metric_macros;
pub mod secret_patterns;
