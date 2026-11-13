// Every `#[sqlx::test]` is current_thread; `MetricAssertion` and the event
// capture are thread-bound, so that pinning is LOAD-BEARING.
//
//! `AcquireMediaHandler` (story 4 GC-3, R-23; ADR-0039 §3, §5, §9), driven
//! through the real `McService::acquire_media_handler` so validation, the
//! ownership check, selection and the single recording site all run.
//!
//! Status behaviour, the eligibility predicate vs its SQL, seeded-RNG
//! equivalence, the cap, per-outcome metric cells and the
//! `gc.mh.replacement_report` event. The exposed label SHAPE (render-based) is
//! in `mh_replacement_metrics_integration.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use gc_service::auth::Claims;
use gc_service::grpc::McService;
use gc_service::observability::metrics::{
    is_emitted_replacement_combination, GcView, ReplacementOutcome, ReplacementTrigger,
};
use gc_service::repositories::{
    HealthStatus, MediaHandlersRepository, LOAD_BALANCING_CANDIDATE_COUNT,
};
use gc_service::services::mh_selection::is_fresh;
use gc_service::services::weighted_pick::{SelectionRng, SystemSelectionRng};
use gc_service::services::MhSelectionService;
use proto_gen::dark_tower::internal::v1::global_controller_service_server::GlobalControllerService;
use proto_gen::dark_tower::internal::v1::{
    AcquireMediaHandlerRequest, AcquireMediaHandlerResponse, RegisterMcRequest, ReleaseCause,
};
use sqlx::PgPool;
use test_common::event_capture::EventCapture;
use test_common::mc_grpc::{mc_app_state, seed_assignment, seed_mc, set_load};
use test_common::mh_registry::{ms, seed};
use tonic::{Code, Request, Response, Status};

const REGION: &str = "us-east-1";
const OTHER_REGION: &str = "eu-west-1";
const MC: &str = "mc-1";
const MEETING: &str = "meeting-1";
const EVENT_TARGET: &str = "gc.mh.replacement_report";
const HB: i32 = ReleaseCause::HeartbeatTimeout as i32;

/// The registry facts one row contributes to the eligibility predicate.
struct CandidateFacts<'a> {
    handler_id: &'a str,
    region: &'a str,
    health_status: HealthStatus,
    heartbeat_age: Duration,
    current_streams: i32,
    max_streams: i32,
}

/// The eligibility predicate stated in Rust: the ORACLE the SQL in
/// `MediaHandlersRepository::get_candidate_mhs` (the one production copy) is
/// proven against. Freshness uses the production rule (`is_fresh`).
fn is_selectable(
    facts: &CandidateFacts<'_>,
    region: &str,
    staleness_bound: Duration,
    exclude: &[String],
) -> bool {
    facts.health_status == HealthStatus::Healthy
        && is_fresh(facts.heartbeat_age, staleness_bound)
        && facts.current_streams < facts.max_streams
        && facts.region == region
        && !exclude.iter().any(|id| id == facts.handler_id)
}

/// Deterministic draw source (splitmix64): the same seed gives the same
/// draws, and small seeds still spread over the whole u64 range.
struct SeededRng(Mutex<u64>);

impl SeededRng {
    fn new(seed: u64) -> Arc<Self> {
        Arc::new(Self(Mutex::new(seed)))
    }
}

impl SelectionRng for SeededRng {
    fn next_u64(&self) -> Option<u64> {
        let mut s = self.0.lock().unwrap();
        *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Some(z ^ (z >> 31))
    }
}

fn svc(pool: &PgPool) -> McService {
    McService::new(mc_app_state(pool.clone(), None))
}

fn svc_with_cap(pool: &PgPool, cap: u32) -> McService {
    McService::new(mc_app_state(pool.clone(), Some(cap)))
}

fn req(
    meeting: &str,
    controller: &str,
    exclude: &[&str],
    trigger: i32,
    replaced: Option<&str>,
) -> Request<AcquireMediaHandlerRequest> {
    Request::new(AcquireMediaHandlerRequest {
        meeting_id: meeting.to_string(),
        controller_id: controller.to_string(),
        exclude_handler_ids: exclude.iter().map(|s| (*s).to_string()).collect(),
        trigger,
        replaced_handler_id: replaced.map(str::to_string),
    })
}

/// The meeting is active on `MC` in `REGION`.
async fn own_meeting(pool: &PgPool) {
    seed_mc(pool, MC, REGION).await;
    seed_assignment(pool, MEETING, MC, REGION, false).await;
}

/// Fresh healthy MHs in `REGION`, each at a distinct load (permille).
async fn seed_healthy(pool: &PgPool, handlers: &[(&str, i32)]) {
    for (id, load) in handlers {
        seed(pool, id, REGION, "healthy", 0).await;
        set_load(pool, id, *load, 1000).await;
    }
}

fn granted_id(r: Result<Response<AcquireMediaHandlerResponse>, Status>) -> String {
    let assignment = r
        .expect("granted")
        .into_inner()
        .assignment
        .expect("OK always carries an assignment");
    assert!(!assignment.mh_id.is_empty());
    assert!(!assignment.webtransport_endpoint.is_empty());
    assert!(!assignment.grpc_endpoint.is_empty());
    assignment.mh_id
}

fn code(r: Result<Response<AcquireMediaHandlerResponse>, Status>) -> Code {
    r.expect_err("expected a non-OK status").code()
}

/// `expected` +1 (and one duration observation for its outcome); EVERY other
/// emitted cell unobserved.
///
/// Order is load-bearing: a histogram query drains its own name+labels, so
/// the expected count is asserted before `assert_unobserved` on the same cell.
fn assert_cell(
    snap: &MetricSnapshot,
    trigger: ReplacementTrigger,
    outcome: ReplacementOutcome,
    gc_view: GcView,
) {
    snap.histogram("gc_mh_replacement_duration_seconds")
        .with_labels(&[("outcome", outcome.label())])
        .assert_observation_count(1);
    for o in ReplacementOutcome::ALL {
        if o != outcome {
            snap.histogram("gc_mh_replacement_duration_seconds")
                .with_labels(&[("outcome", o.label())])
                .assert_unobserved();
        }
    }
    for t in ReplacementTrigger::ALL {
        for o in ReplacementOutcome::ALL {
            for v in GcView::ALL {
                if !is_emitted_replacement_combination(t, o, v) {
                    continue;
                }
                let q = snap
                    .counter("gc_mh_replacement_requests_total")
                    .with_labels(&[
                        ("trigger", t.label()),
                        ("outcome", o.label()),
                        ("gc_view", v.label()),
                    ]);
                if (t, o, v) == (trigger, outcome, gc_view) {
                    q.assert_delta(1);
                } else {
                    q.assert_unobserved();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Selection table
// ---------------------------------------------------------------------------

/// {A,B,C} excluding {A,B} → C; excluding all → RESOURCE_EXHAUSTED.
#[sqlx::test(migrations = "../../migrations")]
async fn exclusion_table(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-a", 100), ("mh-b", 200), ("mh-c", 300)]).await;
    let svc = svc(&pool);

    let snap = MetricAssertion::snapshot();
    let got = granted_id(
        svc.acquire_media_handler(req(MEETING, MC, &["mh-a", "mh-b"], HB, None))
            .await,
    );
    assert_eq!(got, "mh-c");
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::Granted,
        GcView::Unregistered,
    );
    // A replacement is NOT a new-assignment selection.
    snap.counter("gc_mh_selections_total").assert_unobserved();
    snap.histogram("gc_mh_selection_duration_seconds")
        .assert_unobserved();
    drop(snap);

    let snap = MetricAssertion::snapshot();
    let c = code(
        svc.acquire_media_handler(req(MEETING, MC, &["mh-a", "mh-b", "mh-c"], HB, None))
            .await,
    );
    assert_eq!(c, Code::ResourceExhausted);
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::NoCapacity,
        GcView::Unregistered,
    );
}

/// Unknown excluded ids are ignored.
#[sqlx::test(migrations = "../../migrations")]
async fn unknown_excluded_ids_are_ignored(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-a", 100), ("mh-b", 200), ("mh-c", 300)]).await;
    let svc = svc(&pool);
    let got = granted_id(
        svc.acquire_media_handler(req(MEETING, MC, &["mh-a", "mh-b", "mh-unknown"], HB, None))
            .await,
    );
    assert_eq!(got, "mh-c");

    sqlx::query("DELETE FROM media_handlers WHERE handler_id IN ('mh-b', 'mh-c')")
        .execute(&pool)
        .await
        .unwrap();
    let got = granted_id(
        svc.acquire_media_handler(req(MEETING, MC, &["mh-unknown"], HB, None))
            .await,
    );
    assert_eq!(got, "mh-a");
}

/// A draining MH is never granted, even when it is the least loaded.
#[sqlx::test(migrations = "../../migrations")]
async fn draining_handler_is_never_granted(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-a", 500)]).await;
    seed(&pool, "mh-d", REGION, "draining", 0).await;
    set_load(&pool, "mh-d", 0, 1000).await;

    // Positive arm, fixed seed: the healthy one.
    let svc_seeded =
        McService::with_selection_rng(mc_app_state(pool.clone(), None), SeededRng::new(7));
    let got = granted_id(
        svc_seeded
            .acquire_media_handler(req(MEETING, MC, &[], HB, None))
            .await,
    );
    assert_eq!(got, "mh-a");
    // Excluding the healthy one leaves only the draining one: no grant.
    let c = code(
        svc(&pool)
            .acquire_media_handler(req(MEETING, MC, &["mh-a"], HB, None))
            .await,
    );
    assert_eq!(c, Code::ResourceExhausted);
}

/// The region comes from the ownership row: a healthy MH in another region is
/// never granted.
#[sqlx::test(migrations = "../../migrations")]
async fn region_comes_from_the_ownership_row(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-a", 100)]).await;
    seed(&pool, "mh-z", OTHER_REGION, "healthy", 0).await;
    let svc = svc(&pool);
    let c = code(
        svc.acquire_media_handler(req(MEETING, MC, &["mh-a"], HB, None))
            .await,
    );
    assert_eq!(c, Code::ResourceExhausted, "never the other region's mh-z");
}

/// Exclusion runs BEFORE the least-loaded cut: COUNT + 1 healthy rows with
/// strictly increasing load, the COUNT least loaded excluded → the most
/// loaded is still granted (a post-LIMIT filter would return nothing).
#[sqlx::test(migrations = "../../migrations")]
async fn exclusion_is_applied_before_the_limit(pool: PgPool) {
    own_meeting(&pool).await;
    let count = usize::try_from(LOAD_BALANCING_CANDIDATE_COUNT).unwrap();
    let ids: Vec<String> = (0..=count).map(|i| format!("mh-{i}")).collect();
    for (i, id) in ids.iter().enumerate() {
        seed(&pool, id, REGION, "healthy", 0).await;
        set_load(&pool, id, i32::try_from(i * 100).unwrap(), 1000).await;
    }
    let excluded: Vec<&str> = ids[..count].iter().map(String::as_str).collect();
    let got = granted_id(
        svc(&pool)
            .acquire_media_handler(req(MEETING, MC, &excluded, HB, None))
            .await,
    );
    assert_eq!(got, ids[count], "the (COUNT+1)-th, most loaded row");
}

// ---------------------------------------------------------------------------
// Ownership
// ---------------------------------------------------------------------------

/// not_owner: an ended assignment, another controller's assignment, no row.
/// Positive control: identical seeding with the right controller is granted.
#[sqlx::test(migrations = "../../migrations")]
async fn not_owner_arms_and_positive_control(pool: PgPool) {
    seed_mc(&pool, MC, REGION).await;
    seed_mc(&pool, "mc-2", REGION).await;
    seed_healthy(&pool, &[("mh-a", 100)]).await;
    seed_assignment(&pool, "m-ended", MC, REGION, true).await;
    seed_assignment(&pool, "m-other", "mc-2", REGION, false).await;
    let svc = svc(&pool);

    let mut messages = BTreeSet::new();
    for meeting in ["m-ended", "m-other", "m-none"] {
        let snap = MetricAssertion::snapshot();
        let err = svc
            .acquire_media_handler(req(meeting, MC, &[], HB, Some("mh-a")))
            .await
            .expect_err("not owner");
        assert_eq!(err.code(), Code::FailedPrecondition, "{meeting}");
        messages.insert(err.message().to_string());
        assert_cell(
            &snap,
            ReplacementTrigger::HeartbeatTimeout,
            ReplacementOutcome::NotOwner,
            GcView::Unevaluated,
        );
    }
    assert_eq!(
        messages.len(),
        1,
        "no existence oracle: one message for all three"
    );

    // Positive control: mc-2 owns m-other.
    let got = granted_id(
        svc.acquire_media_handler(req("m-other", "mc-2", &[], HB, None))
            .await,
    );
    assert_eq!(got, "mh-a");
}

/// The same controller active in two regions for one meeting is a broken
/// invariant: INTERNAL, outcome error, no grant.
#[sqlx::test(migrations = "../../migrations")]
async fn two_active_regions_is_an_error(pool: PgPool) {
    own_meeting(&pool).await;
    seed_assignment(&pool, MEETING, MC, OTHER_REGION, false).await;
    seed_healthy(&pool, &[("mh-a", 100)]).await;
    let snap = MetricAssertion::snapshot();
    let c = code(
        svc(&pool)
            .acquire_media_handler(req(MEETING, MC, &[], HB, None))
            .await,
    );
    assert_eq!(c, Code::Internal);
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::Error,
        GcView::Unevaluated,
    );
}

// ---------------------------------------------------------------------------
// Validation and the cap
// ---------------------------------------------------------------------------

/// Cap boundary with a NON-default cap (7): exactly 7 ids (2 real + 5 unknown)
/// is granted; 8 is INVALID_ARGUMENT. Duplicates count toward the cap.
#[sqlx::test(migrations = "../../migrations")]
async fn exclude_cap_boundary_and_duplicates(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-a", 100), ("mh-b", 200), ("mh-c", 300)]).await;
    let svc = svc_with_cap(&pool, 7);

    let at_cap = ["mh-a", "mh-b", "u1", "u2", "u3", "u4", "u5"];
    assert_eq!(
        granted_id(
            svc.acquire_media_handler(req(MEETING, MC, &at_cap, HB, None))
                .await
        ),
        "mh-c"
    );
    let mut over = at_cap.to_vec();
    over.push("u6");
    let snap = MetricAssertion::snapshot();
    assert_eq!(
        code(
            svc.acquire_media_handler(req(MEETING, MC, &over, HB, None))
                .await
        ),
        Code::InvalidArgument
    );
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::InvalidRequest,
        GcView::Unevaluated,
    );
    drop(snap);

    // Duplicates: 7 copies of one real id → granted (another one); 8 → invalid.
    let dup7 = ["mh-a"; 7];
    let got = granted_id(
        svc.acquire_media_handler(req(MEETING, MC, &dup7, HB, None))
            .await,
    );
    assert_ne!(got, "mh-a");
    let dup8 = ["mh-a"; 8];
    assert_eq!(
        code(
            svc.acquire_media_handler(req(MEETING, MC, &dup8, HB, None))
                .await
        ),
        Code::InvalidArgument
    );
}

/// Every invalid-request arm: INVALID_ARGUMENT, outcome invalid_request,
/// gc_view unevaluated, and the trigger label pinned: a bad trigger →
/// `invalid` (also when another field is bad too); a valid trigger with a bad
/// field keeps its projected value.
#[sqlx::test(migrations = "../../migrations")]
async fn invalid_request_arms_and_trigger_labels(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-a", 100)]).await;
    let svc = svc(&pool);
    let long = "m".repeat(256);
    let cases: Vec<(
        &str,
        Request<AcquireMediaHandlerRequest>,
        ReplacementTrigger,
    )> = vec![
        (
            "empty meeting",
            req("", MC, &[], HB, None),
            ReplacementTrigger::HeartbeatTimeout,
        ),
        (
            "malformed meeting",
            req("m/x", MC, &[], HB, None),
            ReplacementTrigger::HeartbeatTimeout,
        ),
        (
            "long meeting",
            req(&long, MC, &[], HB, None),
            ReplacementTrigger::HeartbeatTimeout,
        ),
        (
            "malformed controller",
            req(MEETING, "mc 1", &[], HB, None),
            ReplacementTrigger::HeartbeatTimeout,
        ),
        (
            "empty exclude id",
            req(MEETING, MC, &[""], HB, None),
            ReplacementTrigger::HeartbeatTimeout,
        ),
        (
            "malformed exclude id",
            req(MEETING, MC, &["mh/a"], HB, None),
            ReplacementTrigger::HeartbeatTimeout,
        ),
        (
            "malformed replaced id",
            req(MEETING, MC, &[], HB, Some("mh a")),
            ReplacementTrigger::HeartbeatTimeout,
        ),
        (
            "empty replaced id",
            req(MEETING, MC, &[], HB, Some("")),
            ReplacementTrigger::HeartbeatTimeout,
        ),
        (
            "unspecified",
            req(MEETING, MC, &[], ReleaseCause::Unspecified as i32, None),
            ReplacementTrigger::Invalid,
        ),
        (
            "meeting ended",
            req(MEETING, MC, &[], ReleaseCause::MeetingEnded as i32, None),
            ReplacementTrigger::Invalid,
        ),
        (
            "unknown number",
            req(MEETING, MC, &[], 99, None),
            ReplacementTrigger::Invalid,
        ),
        (
            "bad trigger and bad meeting",
            req("m/x", MC, &[], 99, None),
            ReplacementTrigger::Invalid,
        ),
        (
            "valid planned_transfer, bad meeting",
            req("m/x", MC, &[], ReleaseCause::PlannedTransfer as i32, None),
            ReplacementTrigger::PlannedTransfer,
        ),
    ];
    for (name, request, trigger) in cases {
        let snap = MetricAssertion::snapshot();
        let err = svc.acquire_media_handler(request).await.expect_err(name);
        assert_eq!(err.code(), Code::InvalidArgument, "{name}");
        assert!(
            !err.message().contains("m/x") && !err.message().contains("mh/a"),
            "{name}: status must not echo raw input: {}",
            err.message()
        );
        assert_cell(
            &snap,
            trigger,
            ReplacementOutcome::InvalidRequest,
            GcView::Unevaluated,
        );
    }
}

// ---------------------------------------------------------------------------
// gc_view and the event
// ---------------------------------------------------------------------------

/// MC declares X failed while GC sees X fresh and healthy: the grant is still
/// issued (GC's view is never a veto), counted with gc_view=healthy. The event
/// names the REPLACED handler, not the granted one.
#[sqlx::test(migrations = "../../migrations")]
async fn mc_says_dead_gc_says_healthy_is_granted(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-x", 100), ("mh-y", 200)]).await;
    let capture = EventCapture::install(EVENT_TARGET);
    let snap = MetricAssertion::snapshot();
    // The auth layer injects the validated service token's claims; a
    // distinctive `sub` proves the event logs IT (and not controller_id).
    let mut request = req(MEETING, MC, &["mh-x"], HB, Some("mh-x"));
    request.extensions_mut().insert(Claims {
        sub: "mc-svc-sub-1".to_string(),
        exp: 0,
        iat: 0,
        scope: "service.write.gc".to_string(),
        service_type: Some("meeting-controller".to_string()),
    });
    let got = granted_id(svc(&pool).acquire_media_handler(request).await);
    assert_eq!(got, "mh-y");
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::Granted,
        GcView::Healthy,
    );
    let events = capture.take();
    assert_eq!(events.len(), 1, "exactly one event: {events:?}");
    let e = &events[0];
    assert_eq!(e.level, tracing::Level::INFO);
    assert_eq!(e.field("handler_id"), Some("mh-x"));
    assert_ne!(
        e.field("handler_id"),
        Some(got.as_str()),
        "replaced, never granted"
    );
    assert_eq!(e.field("webtransport_endpoint"), Some("https://mh-x:443"));
    assert_eq!(e.field("controller_id"), Some(MC));
    assert_eq!(e.field("trigger"), Some("heartbeat_timeout"));
    assert_eq!(e.field("gc_view"), Some("healthy"));
    assert_eq!(e.field("outcome"), Some("granted"));
    assert_eq!(e.field("caller_sub"), Some("mc-svc-sub-1"));
    assert_ne!(e.field("caller_sub"), e.field("controller_id"));
    assert!(
        e.field("meeting_id").is_none(),
        "meeting id is not an event field"
    );
}

/// gc_view per replaced-row state: draining, stale (even when draining),
/// unregistered (no row → endpoint omitted), and no replaced id → no event.
#[sqlx::test(migrations = "../../migrations")]
async fn gc_view_states_and_event_presence(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-y", 200)]).await;
    let bound = gc_test_utils::reference_mh_registry().staleness_bound();
    seed(&pool, "mh-drain", REGION, "draining", 0).await;
    seed(&pool, "mh-stale", REGION, "draining", ms(bound) + 1000).await;
    seed(&pool, "mh-pend", REGION, "pending", 0).await;
    let svc = svc(&pool);
    let capture = EventCapture::install(EVENT_TARGET);

    for (replaced, view) in [
        ("mh-drain", GcView::Draining),
        ("mh-stale", GcView::Stale),
        ("mh-pend", GcView::Pending),
        ("mh-gone", GcView::Unregistered),
    ] {
        let snap = MetricAssertion::snapshot();
        granted_id(
            svc.acquire_media_handler(req(
                MEETING,
                MC,
                &[replaced],
                ReleaseCause::PlannedTransfer as i32,
                Some(replaced),
            ))
            .await,
        );
        assert_cell(
            &snap,
            ReplacementTrigger::PlannedTransfer,
            ReplacementOutcome::Granted,
            view,
        );
        let events = capture.take();
        assert_eq!(events.len(), 1, "{replaced}");
        assert_eq!(events[0].field("gc_view"), Some(view.label()));
        assert_eq!(
            events[0].field("caller_sub"),
            None,
            "no claims injected → omitted"
        );
        assert_eq!(
            events[0].field("webtransport_endpoint").is_some(),
            view != GcView::Unregistered,
            "{replaced}: endpoint present iff the row exists"
        );
    }

    // No replaced id: counted (unregistered), but no event.
    let snap = MetricAssertion::snapshot();
    granted_id(
        svc.acquire_media_handler(req(MEETING, MC, &[], HB, None))
            .await,
    );
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::Granted,
        GcView::Unregistered,
    );
    assert!(
        capture.take().is_empty(),
        "no event without replaced_handler_id"
    );
}

/// The event fires on rejected outcomes too (denominators), carrying only
/// validated values; a MALFORMED replaced id produces no event at all.
#[sqlx::test(migrations = "../../migrations")]
async fn event_on_rejections_and_never_for_malformed_ids(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-x", 100)]).await;
    let svc = svc(&pool);
    let capture = EventCapture::install(EVENT_TARGET);

    // not_owner: event fires, gc_view unevaluated, endpoint never read.
    svc.acquire_media_handler(req("m-none", MC, &[], HB, Some("mh-x")))
        .await
        .unwrap_err();
    let events = capture.take();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].field("outcome"), Some("not_owner"));
    assert_eq!(events[0].field("gc_view"), Some("unevaluated"));
    assert_eq!(events[0].field("webtransport_endpoint"), None);

    // invalid_request with a bad controller id: event fires WITHOUT it.
    svc.acquire_media_handler(req(MEETING, "mc 1", &[], HB, Some("mh-x")))
        .await
        .unwrap_err();
    let events = capture.take();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].field("outcome"), Some("invalid_request"));
    assert_eq!(
        events[0].field("controller_id"),
        None,
        "raw input never logged"
    );

    // Malformed replaced id: INVALID_ARGUMENT and zero events.
    let snap = MetricAssertion::snapshot();
    let c = code(
        svc.acquire_media_handler(req(MEETING, MC, &[], HB, Some("mh/x\n")))
            .await,
    );
    assert_eq!(c, Code::InvalidArgument);
    assert!(
        capture.take().is_empty(),
        "no event for a malformed replaced id"
    );
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::InvalidRequest,
        GcView::Unevaluated,
    );
}

// ---------------------------------------------------------------------------
// Database errors
// ---------------------------------------------------------------------------

/// A failure reading the replaced handler's row fails the request
/// (outcome=error, gc_view unevaluated), never a silent `unregistered`; a
/// failure in selection (no replaced id) is error with the evaluated view.
/// The registry table is renamed so the ownership query still succeeds.
#[sqlx::test(migrations = "../../migrations")]
async fn db_errors_at_lookup_and_at_selection(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-a", 100)]).await;
    sqlx::query("ALTER TABLE media_handlers RENAME TO media_handlers_gone")
        .execute(&pool)
        .await
        .unwrap();
    let svc = svc(&pool);

    let snap = MetricAssertion::snapshot();
    let err = svc
        .acquire_media_handler(req(MEETING, MC, &[], HB, Some("mh-a")))
        .await
        .unwrap_err();
    assert_eq!(err.code(), Code::Internal);
    assert!(
        !err.message().contains("media_handlers"),
        "no sqlx text in the status"
    );
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::Error,
        GcView::Unevaluated,
    );
    drop(snap);

    let snap = MetricAssertion::snapshot();
    let c = code(
        svc.acquire_media_handler(req(MEETING, MC, &[], HB, None))
            .await,
    );
    assert_eq!(c, Code::Internal);
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::Error,
        GcView::Unregistered,
    );
}

/// A failure of the ownership query itself: error, unevaluated.
#[sqlx::test(migrations = "../../migrations")]
async fn db_error_at_ownership(pool: PgPool) {
    let svc = svc(&pool);
    pool.close().await;
    let snap = MetricAssertion::snapshot();
    let c = code(
        svc.acquire_media_handler(req(MEETING, MC, &[], HB, None))
            .await,
    );
    assert_eq!(c, Code::Internal);
    assert_cell(
        &snap,
        ReplacementTrigger::HeartbeatTimeout,
        ReplacementOutcome::Error,
        GcView::Unevaluated,
    );
}

// ---------------------------------------------------------------------------
// ADR-0039 §9: selection is identical with and without replacement evidence
// ---------------------------------------------------------------------------

/// Same DB state, a fresh source with the same seed per call: the request
/// WITHOUT evidence (no replaced id, heartbeat_timeout) and WITH it (replaced
/// id, planned_transfer) get the same handler. Control: different seeds
/// pick different handlers, so the source really drives the choice.
#[sqlx::test(migrations = "../../migrations")]
async fn selection_is_identical_with_and_without_evidence(pool: PgPool) {
    own_meeting(&pool).await;
    seed_healthy(&pool, &[("mh-a", 100), ("mh-b", 400), ("mh-c", 700)]).await;
    seed(&pool, "mh-x", REGION, "healthy", 0).await;
    let state = mc_app_state(pool.clone(), None);

    let mut picks = BTreeSet::new();
    for seed_value in 1..=24_u64 {
        let plain = McService::with_selection_rng(state.clone(), SeededRng::new(seed_value));
        let with_evidence =
            McService::with_selection_rng(state.clone(), SeededRng::new(seed_value));
        let a = granted_id(
            plain
                .acquire_media_handler(req(MEETING, MC, &["mh-x"], HB, None))
                .await,
        );
        let b = granted_id(
            with_evidence
                .acquire_media_handler(req(
                    MEETING,
                    MC,
                    &["mh-x"],
                    ReleaseCause::PlannedTransfer as i32,
                    Some("mh-x"),
                ))
                .await,
        );
        assert_eq!(a, b, "seed {seed_value}: evidence changed the selection");
        picks.insert(a);
    }
    assert!(
        picks.len() > 1,
        "control: the seeded source must drive the pick ({picks:?})"
    );
}

// ---------------------------------------------------------------------------
// The eligibility predicate vs its SQL
// ---------------------------------------------------------------------------

/// `is_selectable` (Rust) and `get_candidate_mhs` (SQL) agree on a matrix of
/// status × age × capacity × region × excluded, plus extra eligible rows so
/// the expected set has several members (and stays within the LIMIT). Also:
/// initial-assignment selection over the same rows picks only rows the
/// predicate accepts.
#[sqlx::test(migrations = "../../migrations")]
async fn eligibility_predicate_matches_sql(pool: PgPool) {
    let bound = gc_test_utils::reference_mh_registry().staleness_bound();
    // Fresh with a 1 s margin (NOW() advances between seed and query);
    // stale 1 s past the bound. The ms boundary is pinned in the unit table.
    let fresh = Duration::from_millis(u64::try_from(ms(bound) - 1000).unwrap());
    let stale = bound + Duration::from_secs(1);
    let statuses = [
        HealthStatus::Healthy,
        HealthStatus::Draining,
        HealthStatus::Unhealthy,
        HealthStatus::Degraded,
        HealthStatus::Pending,
    ];

    struct Row {
        id: String,
        region: &'static str,
        status: HealthStatus,
        age: Duration,
        current: i32,
        max: i32,
        excluded: bool,
    }
    let mut rows = Vec::new();
    for (si, status) in statuses.iter().enumerate() {
        for (ai, age) in [fresh, stale].iter().enumerate() {
            for (ci, current) in [10, 100].iter().enumerate() {
                for (ri, region) in [REGION, OTHER_REGION].iter().enumerate() {
                    for excluded in [false, true] {
                        rows.push(Row {
                            id: format!("m-{si}{ai}{ci}{ri}{}", u8::from(excluded)),
                            region,
                            status: *status,
                            age: *age,
                            current: *current,
                            max: 100,
                            excluded,
                        });
                    }
                }
            }
        }
    }
    for i in 0..3 {
        rows.push(Row {
            id: format!("extra-{i}"),
            region: REGION,
            status: HealthStatus::Healthy,
            age: Duration::ZERO,
            current: 20 + i,
            max: 100,
            excluded: false,
        });
    }
    for r in &rows {
        seed(&pool, &r.id, r.region, r.status.as_db_str(), ms(r.age)).await;
        set_load(&pool, &r.id, r.current, r.max).await;
    }
    // Re-stamp EVERY row's heartbeat age in ONE statement, right before the
    // query, so all ages share one NOW(). Seeding row by row lets NOW()
    // advance across the loop, and under load the first "fresh" row (1 s
    // inside the bound) aged past it before the query ran — a time-dependent
    // flake, not a predicate disagreement.
    let (ids, ages): (Vec<String>, Vec<i64>) =
        rows.iter().map(|r| (r.id.clone(), ms(r.age))).unzip();
    sqlx::query(
        "UPDATE media_handlers AS m \
         SET last_heartbeat_at = NOW() - v.age_ms * INTERVAL '1 millisecond' \
         FROM UNNEST($1::text[], $2::bigint[]) AS v(handler_id, age_ms) \
         WHERE m.handler_id = v.handler_id",
    )
    .bind(&ids)
    .bind(&ages)
    .execute(&pool)
    .await
    .expect("re-stamp heartbeat ages");
    let exclude: Vec<String> = rows
        .iter()
        .filter(|r| r.excluded)
        .map(|r| r.id.clone())
        .collect();
    let expected_for = |exclude: &[String]| -> BTreeSet<String> {
        rows.iter()
            .filter(|r| {
                is_selectable(
                    &CandidateFacts {
                        handler_id: &r.id,
                        region: r.region,
                        health_status: r.status,
                        heartbeat_age: r.age,
                        current_streams: r.current,
                        max_streams: r.max,
                    },
                    REGION,
                    bound,
                    exclude,
                )
            })
            .map(|r| r.id.clone())
            .collect()
    };

    let expected = expected_for(&exclude);
    let limit = usize::try_from(LOAD_BALANCING_CANDIDATE_COUNT).unwrap();
    assert!(
        expected.len() > 1 && expected.len() <= limit,
        "vacuity guard: {expected:?}"
    );
    let got: BTreeSet<String> =
        MediaHandlersRepository::get_candidate_mhs(&pool, REGION, bound, &exclude)
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.handler_id)
            .collect();
    assert_eq!(got, expected, "SQL and is_selectable disagree");

    // Initial assignment (empty exclude) over the same rows.
    let initial_ok = expected_for(&[]);
    assert!(initial_ok.len() <= limit, "vacuity guard: {initial_ok:?}");
    let picked = MhSelectionService::select(
        &pool,
        REGION,
        bound,
        initial_ok.len(),
        &[],
        &SystemSelectionRng,
    )
    .await
    .unwrap();
    assert!(initial_ok.len() > 1, "vacuity guard: {initial_ok:?}");
    // count == |eligible| and picks are without replacement, so the picked
    // SET must equal the eligible set (a silently dropped row fails).
    let picked: BTreeSet<String> = picked.into_iter().map(|h| h.mh_id).collect();
    assert_eq!(picked, initial_ok);
}

// ---------------------------------------------------------------------------
// The cap push
// ---------------------------------------------------------------------------

/// `RegisterMCResponse.max_exclude_handler_ids` is the configured value (a
/// non-default 23, so a hardcoded constant fails).
#[sqlx::test(migrations = "../../migrations")]
async fn register_mc_returns_the_configured_cap(pool: PgPool) {
    let resp = svc_with_cap(&pool, 23)
        .register_mc(Request::new(RegisterMcRequest {
            id: "mc-cap".to_string(),
            region: REGION.to_string(),
            grpc_endpoint: "http://mc-cap:50051".to_string(),
            webtransport_endpoint: String::new(),
            max_meetings: 10,
            max_participants: 100,
        }))
        .await
        .expect("register")
        .into_inner();
    assert_eq!(resp.max_exclude_handler_ids, 23);
    assert!(resp.accepted);
}
