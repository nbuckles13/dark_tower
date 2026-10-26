//! Metric and log label vocabulary shared across Dark Tower services.
//!
//! Labels that more than one service emits live here so the *spelling* of a
//! label key and the *set* of its permitted values have exactly one home. Two
//! copies of a `&'static str` drift loudly — a mismatched series is visible the
//! first time someone writes a `PromQL` query. Two copies of the reasoning
//! attached to it drift silently, and it is the reasoning that keeps a
//! constraint from being relaxed by someone who only ever reads one copy.
//!
//! This module holds label vocabulary only. It is deliberately not a home for
//! recorder functions: those belong to the service that owns the metric, and
//! `crates/common` has no `metrics` facade dependency on the production path.

/// Label KEY for media key custody.
///
/// Fleet-wide rollout of this label to every service's metrics is R-26
/// (observability task 22); the constant living here does not itself perform
/// that rollout.
pub const KEY_CUSTODY_LABEL: &str = "key_custody";

/// Label VALUE for media key custody. **One permitted value.**
///
/// ADR-0036 §4: media is encrypted between clients; MH, transport and storage
/// cannot read it; **MC can**. That is accepted operator custody, recorded as
/// the user's risk decision.
///
/// A `&'static str` const, never derived from config, deployment mode, a
/// feature flag, or whether a KEK happens to be provisioned: this is a
/// *constraint*, not a snapshot of today's deployment. Adding a second value
/// requires an ADR-0036 §4 amendment.
///
/// This exists **in place of** an end-to-end or zero-trust boolean, which no
/// metric, log, dashboard or document may carry — a stat panel reading
/// `E2E: true` is a product claim rendered to an operator, who may repeat it to
/// a customer. The label carries the truth; a boolean would carry a claim.
pub const KEY_CUSTODY_OPERATOR: &str = "operator";

/// Label KEY for the media kind a series' subject has.
///
/// Closed value set `{audio, video}`, always co-carried with
/// [`CONTENT_KIND_LABEL`] (all or none). The single source of truth for the
/// keys, their values and the projection from the wire `MediaKind` is
/// `docs/observability/label-taxonomy.md` (the `media_kind` / `content_kind`
/// rows and the projection table). The ONE projection per language is MC's
/// `KindLabels::from_proto` (`crates/mc-service/src/media_kind.rs`) and the
/// SDK's `projectMediaKind` (`packages/sdk-core/src/media/setup/mediaMetrics.ts`);
/// value literals live nowhere else (`dt-guard metric-labels` /
/// `ts-name-guard-dt-client`, rule `kind_label_literal`).
///
/// Carried by MC and the client SDK ONLY. **MH must not import this
/// constant**: ADR-0036 §7 makes MH type-blind, so no `mh_*` series may carry a
/// media kind. Enforced at source by `dt-guard metric-labels`
/// (`kind_label_on_mh`, any kind key in an `mh-service` metric macro) and on
/// stored series by the live env-test hygiene rule
/// (`crates/env-tests/src/fixtures/metric_hygiene.rs`).
pub const MEDIA_KIND_LABEL: &str = "media_kind";

/// Label KEY for the content a media series carries: `main` (the camera or
/// microphone) or `screen` (content share). Closed value set `{main, screen}`,
/// always co-carried with [`MEDIA_KIND_LABEL`]; everything on that constant
/// (single source of truth, projection homes, MH-never) applies here unchanged.
pub const CONTENT_KIND_LABEL: &str = "content_kind";

#[cfg(test)]
mod tests {
    use super::*;

    /// The label key and value are wire-visible: a rename is a silent metric
    /// break for every dashboard and alert selecting on them, so the literals
    /// are pinned rather than left to review.
    #[test]
    fn key_custody_literals_are_stable() {
        assert_eq!(KEY_CUSTODY_LABEL, "key_custody");
        assert_eq!(KEY_CUSTODY_OPERATOR, "operator");
    }

    /// Same reason: `media_kind` is selected on by dashboards, alerts and the
    /// browser read-back, and GC's datapoint allowlist holds this constant.
    #[test]
    fn media_kind_literal_is_stable() {
        assert_eq!(MEDIA_KIND_LABEL, "media_kind");
        assert_eq!(CONTENT_KIND_LABEL, "content_kind");
    }
}
