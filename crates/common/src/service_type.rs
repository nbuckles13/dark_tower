//! Canonical service-type vocabulary for the JWT `service_type` claim, and the
//! metric-label clamp derived from it.
//!
//! `ServiceClaims::service_type` (see [`crate::jwt`]) is an `Option<String>`:
//! only the token *signature* is validated, the value itself is unconstrained
//! and peer-controlled. Any code path that forwards that value into a Prometheus
//! label RAW admits **unbounded label cardinality** — a signature-valid peer can
//! drive one new series per arbitrary `service_type` string, an exporter-memory
//! DoS. The bounded value-set below and [`service_type_metric_label`] are the
//! single source of truth for clamping the claim to a metric-safe label
//! *before* it becomes a label value.
//!
//! This is the ADR-0011 clause-(c) "bounded output of a documented normalization
//! function" discipline applied to `service_type` (see `docs/TODO.md`).
//!
//! # Single source of truth
//!
//! [`SERVICE_TYPE_IDENTITIES`] is authoritative for the recognized identities.
//! `ac-service`'s `ServiceType` enum currently mirrors these same three strings;
//! that duplication is DRY-tracked (route `ServiceType` through this const) in
//! `docs/TODO.md`. The clamp is bounded *by construction* regardless of that
//! drift: an identity `ac-service` adds but this list lacks would clamp to
//! [`SERVICE_TYPE_LABEL_OTHER`] — a diagnostic loss, never a cardinality
//! regression.

/// The recognized service-type identities carried by a JWT `service_type` claim
/// (ADR-0003). Single source of truth for the bounded value-set.
///
/// Kept in sync with `ac-service`'s `ServiceType::as_str()` (DRY follow-up in
/// `docs/TODO.md`); a mismatch is safe (unknown identities clamp to
/// [`SERVICE_TYPE_LABEL_OTHER`]) but loses a real value's diagnostic granularity.
pub const SERVICE_TYPE_IDENTITIES: [&str; 3] =
    ["global-controller", "meeting-controller", "media-handler"];

/// Metric-label bucket for a `service_type` claim that is **absent** (the JWT
/// carried no `service_type`). Distinct from [`SERVICE_TYPE_LABEL_OTHER`] so an
/// omitted claim (an issuance/config bug) is triageable apart from a present but
/// off-spec one (a stronger tampering signal).
pub const SERVICE_TYPE_LABEL_UNKNOWN: &str = "unknown";

/// Metric-label bucket for a `service_type` claim that is **present but not a
/// recognized identity** (a forged, off-spec, or newly-introduced value). All
/// such values collapse to this single bucket, which is what bounds the label
/// cardinality.
pub const SERVICE_TYPE_LABEL_OTHER: &str = "other";

/// Clamp a peer-supplied `service_type` claim to a bounded, `'static`
/// metric-label value.
///
/// - a recognized identity (see [`SERVICE_TYPE_IDENTITIES`]) is preserved as-is,
///   so a *valid* service calling the *wrong* endpoint keeps its real value and
///   service-confusion stays diagnosable;
/// - a value that is present but unrecognized collapses to
///   [`SERVICE_TYPE_LABEL_OTHER`];
/// - an absent claim (`None`) maps to [`SERVICE_TYPE_LABEL_UNKNOWN`].
///
/// The return is always one of the constants in this module — never the caller's
/// bytes — so the label domain is `SERVICE_TYPE_IDENTITIES.len() + 2` at most,
/// independent of what the peer sends. Callers that also want the RAW claim for
/// a log line must read it separately; this function is for the METRIC label
/// only.
#[must_use]
pub fn service_type_metric_label(claim: Option<&str>) -> &'static str {
    match claim {
        None => SERVICE_TYPE_LABEL_UNKNOWN,
        Some(value) => SERVICE_TYPE_IDENTITIES
            .into_iter()
            .find(|identity| *identity == value)
            .unwrap_or(SERVICE_TYPE_LABEL_OTHER),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognized_identities_are_preserved() {
        for identity in SERVICE_TYPE_IDENTITIES {
            assert_eq!(service_type_metric_label(Some(identity)), identity);
        }
    }

    #[test]
    fn absent_claim_maps_to_unknown() {
        assert_eq!(service_type_metric_label(None), SERVICE_TYPE_LABEL_UNKNOWN);
    }

    #[test]
    fn present_but_unrecognized_maps_to_other() {
        // A forged/off-spec claim — the exact DoS vector — collapses to `other`.
        assert_eq!(
            service_type_metric_label(Some("auth-controller")),
            SERVICE_TYPE_LABEL_OTHER
        );
        assert_eq!(
            service_type_metric_label(Some("../../etc/passwd")),
            SERVICE_TYPE_LABEL_OTHER
        );
        assert_eq!(
            service_type_metric_label(Some("")),
            SERVICE_TYPE_LABEL_OTHER
        );
    }

    #[test]
    fn output_domain_is_bounded_and_static() {
        // The return is always one of exactly N+2 constants regardless of input.
        let clamp_targets = [
            "global-controller",
            "meeting-controller",
            "media-handler",
            SERVICE_TYPE_LABEL_UNKNOWN,
            SERVICE_TYPE_LABEL_OTHER,
        ];
        for probe in [
            "x",
            "GLOBAL-CONTROLLER",
            "media-handler ",
            "unknown",
            "other",
        ] {
            let label = service_type_metric_label(Some(probe));
            assert!(
                clamp_targets.contains(&label),
                "clamp produced out-of-domain label {label:?} for {probe:?}"
            );
        }
    }
}
