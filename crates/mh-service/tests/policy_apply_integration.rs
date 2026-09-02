//! Component coverage for `mh_media_policy_applies_total` (ADR-0032, ADR-0036 §8).
//!
//! The ADR-0036 §10 Tier-1b behavioural gates — apply-failure does not advance
//! the acknowledgement, generation monotonicity, re-assert idempotency, the
//! two-meeting cross-tenant pin — live next to the code they constrain, in
//! `src/grpc/mh_service.rs` and `src/routing/mod.rs`. This file covers the
//! *metric*: `dt-guard metric-coverage` searches `crates/mh-service/tests/**`
//! only, so a metric exercised solely by `#[cfg(test)]` code inside `src/`
//! counts as uncovered.
//!
//! Driving the recorder directly mirrors the `errors_grpc_metrics_integration`
//! and `token_refresh_integration` direct-wrapper pattern: the recorder is a
//! label-stable function with no surrounding policy, and the decision of
//! *which* outcome to record is asserted by the handler's own tests.
//!
//! Bounded label values per `docs/observability/metrics/mh-service.md`:
//! `outcome` ∈ {applied, rejected_stale, no_generation, rejected_invalid,
//! apply_failed}, `key_custody` = `operator` (single value).

use common::observability::testing::MetricAssertion;
use mh_service::observability::metrics::{record_media_policy_apply, PolicyApplyOutcome};

/// Every `outcome` value lands in its own series, and adjacency holds.
///
/// The adjacency half is the label-swap catcher: recording `Applied` must not
/// increment any of the other four. Without it, an emitter that ignored its
/// argument and always wrote one value would pass a per-value existence check.
#[test]
fn each_outcome_emits_its_own_series() {
    let snap = MetricAssertion::snapshot();
    record_media_policy_apply(PolicyApplyOutcome::Applied);

    snap.counter("mh_media_policy_applies_total")
        .with_labels(&[("outcome", "applied"), ("key_custody", "operator")])
        .assert_delta(1);

    // Derived from `ALL` rather than hand-listed, so a sixth variant is
    // covered by this assertion the moment it exists instead of silently
    // dropping out of it.
    for other in PolicyApplyOutcome::ALL
        .into_iter()
        .filter(|o| *o != PolicyApplyOutcome::Applied)
    {
        snap.counter("mh_media_policy_applies_total")
            .with_labels(&[("outcome", other.as_label()), ("key_custody", "operator")])
            .assert_delta(0);
    }
}

/// All five values are emittable and distinct.
#[test]
fn every_bounded_outcome_value_is_reachable() {
    let snap = MetricAssertion::snapshot();
    for outcome in PolicyApplyOutcome::ALL {
        record_media_policy_apply(outcome);
    }
    for outcome in PolicyApplyOutcome::ALL {
        snap.counter("mh_media_policy_applies_total")
            .with_labels(&[("outcome", outcome.as_label()), ("key_custody", "operator")])
            .assert_delta(1);
    }
}

/// `key_custody` is on the METRIC, not only in logs.
///
/// ADR-0036 §4 accepts operator custody of media keys as the user's recorded
/// risk decision, and `label-taxonomy.md` requires the label on logs *and*
/// metrics. It exists **in place of** an end-to-end or zero-trust boolean,
/// which no metric, log, dashboard or document may carry: the label states a
/// fact an operator can act on, a boolean would state a product claim they
/// might repeat to a customer.
///
/// A single permitted value, so cardinality is 5 (`outcome`) x 1.
#[test]
fn key_custody_is_present_and_single_valued() {
    let snap = MetricAssertion::snapshot();
    record_media_policy_apply(PolicyApplyOutcome::Applied);

    // Present with the one permitted value...
    snap.counter("mh_media_policy_applies_total")
        .with_labels(&[("outcome", "applied"), ("key_custody", "operator")])
        .assert_delta(1);

    // ...and no second custody value exists. Adding one requires an ADR-0036
    // §4 amendment, not a code change.
    snap.counter("mh_media_policy_applies_total")
        .with_labels(&[("outcome", "applied"), ("key_custody", "none")])
        .assert_delta(0);
}

/// The counter carries no unbounded label.
///
/// ADR-0036 §11 bars generation values (one new series per policy change),
/// `process_start_epoch_ms` (one per restart) and every stream or meeting
/// identity — raw *or hashed* — from metric labels. `RegisterMeetingRequest` is
/// the obvious new back door for the per-stream identity telemetry that §11
/// bars, now that `StreamTelemetry` is deleted; generations belong in the log
/// line and in this metric's *value*.
///
/// Asserted by absence: a series carrying any of these labels must not exist.
#[test]
fn no_unbounded_or_identity_label_is_emitted() {
    let snap = MetricAssertion::snapshot();
    record_media_policy_apply(PolicyApplyOutcome::Applied);

    for forbidden in [
        ("generation", "1"),
        ("applied_generation", "1"),
        ("policy_generation", "1"),
        ("meeting_id", "meeting-1"),
        ("meeting_id_hash", "abc123"),
        ("sender_id", "5"),
        ("slot_id", "0"),
        ("egress_stream_id", "1"),
        ("handler_id", "mh-test-handler"),
    ] {
        snap.counter("mh_media_policy_applies_total")
            .with_labels(&[("outcome", "applied"), forbidden])
            .assert_delta(0);
    }
}

// ---------------------------------------------------------------------------
// The counting boundary, driven through the real handler
// ---------------------------------------------------------------------------

mod counting_boundary {
    use super::*;
    use mh_service::config::PolicyLimits;
    use mh_service::grpc::MhMediaService;
    use mh_service::session::SessionManagerHandle;
    use proto_gen::dark_tower::internal::v1::media_handler_service_server::MediaHandlerService as _;
    // One fixture home for all three test sites that build these messages —
    // see `mh_test_utils::media_policy` for why.
    use mh_test_utils::media_policy::{loopback_egress as egress, register_request};
    use proto_gen::dark_tower::internal::v1::{EgressStream, RegisterMeetingRequest};
    use tonic::Request;

    fn service() -> MhMediaService {
        MhMediaService::new(
            SessionManagerHandle::new(),
            "mh-boundary-test".to_string(),
            1_700_000_000_000,
            PolicyLimits::default(),
        )
    }

    /// The pre-boundary negative case needs a MALFORMED `mc_grpc_endpoint`, so
    /// `endpoint` is overridden on the shared fixture here rather than being a
    /// parameter of the fixture builder itself. The departure from well-formed
    /// stays visible at the call site.
    fn request(
        meeting: &str,
        endpoint: &str,
        generation: u64,
        streams: Vec<EgressStream>,
    ) -> Request<RegisterMeetingRequest> {
        let mut req = register_request(meeting, generation, streams);
        req.mc_grpc_endpoint = endpoint.to_string();
        Request::new(req)
    }

    /// **The denominator invariant, both directions.**
    ///
    /// `sum(mh_media_policy_applies_total)` must equal the number of
    /// `RegisterMeeting` calls whose policy MH actually considered — every
    /// terminal path from the first read of a policy-bearing field
    /// (`egress_streams`, `selection_rules`, `policy_generation`) onward,
    /// exactly once. That makes each outcome a share of a whole rather than a
    /// floating count nothing normalises.
    ///
    /// The negative half matters as much as the positive one. Without it, an
    /// implementer making a `sum == all calls` assertion green would either
    /// fold the pre-boundary scalar rejects in or delete the malformed case —
    /// deciding the boundary by whatever happened to pass rather than by
    /// design. A malformed `mc_grpc_endpoint` and a duplicate
    /// `egress_stream_id` have different owners and different 3am remedies, so
    /// they must not share a series.
    #[tokio::test]
    async fn policy_counter_covers_post_boundary_paths_and_only_those() {
        let svc = service();
        let snap = MetricAssertion::snapshot();

        // --- four calls that reach the policy boundary --------------------
        // applied
        svc.register_meeting(request("m-1", "http://mc:50052", 5, vec![egress(1, 5, 0)]))
            .await
            .expect("valid policy is accepted");
        // rejected_stale
        svc.register_meeting(request("m-1", "http://mc:50052", 2, vec![egress(1, 5, 0)]))
            .await
            .expect("a stale generation is ignored, not an error");
        // no_generation
        svc.register_meeting(request("m-2", "http://mc:50052", 0, vec![]))
            .await
            .expect("generation 0 must not be rejected before story task 13");
        // rejected_invalid — duplicate egress_stream_id
        svc.register_meeting(request(
            "m-3",
            "http://mc:50052",
            1,
            vec![egress(1, 5, 0), egress(1, 6, 1)],
        ))
        .await
        .expect_err("a duplicate egress_stream_id rejects the whole registration");

        // Exactly one outcome per call, and each on its OWN series. Summed
        // across the five values that is 4 = the number of calls that reached
        // the policy boundary. Asserting each value at 1 rather than only the
        // sum is what makes it a label-swap catcher as well as a denominator
        // check: four calls all landing on `applied` would also sum to 4.
        for outcome in [
            "applied",
            "rejected_stale",
            "no_generation",
            "rejected_invalid",
        ] {
            snap.counter("mh_media_policy_applies_total")
                .with_labels(&[("outcome", outcome), ("key_custody", "operator")])
                .assert_delta(1);
        }
        snap.counter("mh_media_policy_applies_total")
            .with_labels(&[("outcome", "apply_failed"), ("key_custody", "operator")])
            .assert_delta(0);

        // --- three PRE-boundary calls: policy counter must not move -------
        let pre = MetricAssertion::snapshot();
        svc.register_meeting(request("", "http://mc:50052", 1, vec![]))
            .await
            .expect_err("empty meeting_id");
        svc.register_meeting(request("m-4", "ftp://mc:50052", 1, vec![]))
            .await
            .expect_err("bad endpoint scheme");
        svc.register_meeting(request("m-5", "", 1, vec![]))
            .await
            .expect_err("empty endpoint");

        // Checks reading only caller-identity and reachability scalars are
        // PRE-boundary. Folding them into the policy counter would put "MC
        // produced a bad policy" and "this caller's endpoint is malformed" in
        // one series, and make the sum unusable as a denominator.
        for outcome in [
            "applied",
            "rejected_stale",
            "no_generation",
            "rejected_invalid",
            "apply_failed",
        ] {
            pre.counter("mh_media_policy_applies_total")
                .with_labels(&[("outcome", outcome), ("key_custody", "operator")])
                .assert_delta(0);
        }
        pre.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "register_meeting"), ("status", "error")])
            .assert_delta(3);
    }
}
