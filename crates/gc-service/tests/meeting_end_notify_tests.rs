// Every `#[sqlx::test]` is current_thread; `MetricAssertion` and the event
// capture are thread-bound, so that pinning is LOAD-BEARING.
//
//! `NotifyMeetingEnded` (ADR-0010 §3), driven through the real
//! `McService::notify_meeting_ended`: required-field validation, the
//! incarnation fence (one row ended only when controller AND
//! `assignment_id` match), the wire outcome, the single
//! `gc_meeting_end_notifications_total{outcome}` recording site, and the
//! fenced WARN line.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use ::common::observability::testing::MetricAssertion;
use gc_service::auth::Claims;
use gc_service::grpc::McService;
use gc_service::repositories::{AssignmentId, EndAssignmentOutcome};
use proto_gen::dark_tower::internal::v1::global_controller_service_server::GlobalControllerService;
use proto_gen::dark_tower::internal::v1::{
    NotifyMeetingEndedOutcome, NotifyMeetingEndedRequest, NotifyMeetingEndedResponse,
};
use sqlx::PgPool;
use test_common::event_capture::EventCapture;
use test_common::mc_grpc::{mc_app_state, seed_mc};
use tonic::{Code, Request, Response, Status};

const REGION: &str = "us-east-1";
const MC: &str = "mc-1";
const MEETING: &str = "meeting-1";
const METRIC: &str = "gc_meeting_end_notifications_total";
const LOG_TARGET: &str = "gc.grpc.notify_meeting_ended";

fn svc(pool: &PgPool) -> McService {
    McService::new(mc_app_state(pool.clone(), None))
}

fn req(
    meeting: &str,
    region: &str,
    controller: &str,
    id: &str,
) -> Request<NotifyMeetingEndedRequest> {
    Request::new(NotifyMeetingEndedRequest {
        meeting_id: meeting.to_string(),
        region: region.to_string(),
        controller_id: controller.to_string(),
        assignment_id: id.to_string(),
    })
}

/// A live row of `MEETING` on `MC` carrying `id`.
async fn live_row(pool: &PgPool, id: AssignmentId) {
    seed_mc(pool, MC, REGION).await;
    sqlx::query(
        "INSERT INTO meeting_assignments \
         (meeting_id, meeting_controller_id, region, assigned_by_gc_id, assignment_id) \
         VALUES ($1, $2, $3, 'gc-test', $4)",
    )
    .bind(MEETING)
    .bind(MC)
    .bind(REGION)
    .bind(id.as_uuid())
    .execute(pool)
    .await
    .expect("seed live row");
}

async fn is_live(pool: &PgPool) -> bool {
    let (live,): (bool,) = sqlx::query_as(
        "SELECT ended_at IS NULL FROM meeting_assignments WHERE meeting_id = $1 AND region = $2",
    )
    .bind(MEETING)
    .bind(REGION)
    .fetch_one(pool)
    .await
    .expect("row");
    live
}

fn outcome(r: Result<Response<NotifyMeetingEndedResponse>, Status>) -> NotifyMeetingEndedOutcome {
    let resp = r.expect("notify must succeed").into_inner();
    NotifyMeetingEndedOutcome::try_from(resp.outcome).expect("known outcome")
}

/// Exactly one outcome cell moved, by one.
fn assert_one_cell(
    snap: &::common::observability::testing::MetricSnapshot,
    want: EndAssignmentOutcome,
) {
    for o in EndAssignmentOutcome::ALL {
        let q = snap
            .counter(METRIC)
            .with_labels(&[("outcome", o.as_label())]);
        if o == want {
            q.assert_delta(1);
        } else {
            q.assert_delta(0);
        }
    }
}

/// Every field is REQUIRED; there is no unfenced legacy mode. Each bad input
/// is INVALID_ARGUMENT, ends nothing, and is not counted as an outcome.
#[sqlx::test(migrations = "../../migrations")]
async fn invalid_requests_are_refused_and_end_nothing(pool: PgPool) {
    let id = AssignmentId::new_v4();
    live_row(&pool, id).await;
    let good = id.to_string();
    let cases: &[(&str, &str, &str, &str)] = &[
        ("", REGION, MC, &good),
        (MEETING, "", MC, &good),
        (MEETING, REGION, "", &good),
        (MEETING, REGION, "mc 1", &good),
        (MEETING, REGION, MC, ""),
        (MEETING, REGION, MC, "not-a-uuid"),
        (MEETING, REGION, MC, "00000000-0000-0000-0000-000000000000"),
    ];
    for (meeting, region, controller, aid) in cases {
        let snap = MetricAssertion::snapshot();
        let got = svc(&pool)
            .notify_meeting_ended(req(meeting, region, controller, aid))
            .await
            .expect_err("invalid request must be refused");
        assert_eq!(
            got.code(),
            Code::InvalidArgument,
            "({meeting:?}, {region:?}, {controller:?}, {aid:?})"
        );
        snap.counter(METRIC).assert_unobserved();
    }
    // Positive control: the row survived every refusal, and the valid notify
    // ends it.
    assert!(is_live(&pool).await);
    let got = svc(&pool)
        .notify_meeting_ended(req(MEETING, REGION, MC, &good))
        .await;
    assert_eq!(outcome(got), NotifyMeetingEndedOutcome::Ended);
    assert!(!is_live(&pool).await);
}

/// The matching notify ends the row (ENDED); a re-send is NOT_CURRENT and
/// counted `already_ended`.
#[sqlx::test(migrations = "../../migrations")]
async fn matching_notify_ends_and_a_resend_is_not_current(pool: PgPool) {
    let id = AssignmentId::new_v4();
    live_row(&pool, id).await;

    let snap = MetricAssertion::snapshot();
    let got = svc(&pool)
        .notify_meeting_ended(req(MEETING, REGION, MC, &id.to_string()))
        .await;
    assert_eq!(outcome(got), NotifyMeetingEndedOutcome::Ended);
    assert_one_cell(&snap, EndAssignmentOutcome::Ended);
    assert!(!is_live(&pool).await);

    let snap = MetricAssertion::snapshot();
    let got = svc(&pool)
        .notify_meeting_ended(req(MEETING, REGION, MC, &id.to_string()))
        .await;
    assert_eq!(outcome(got), NotifyMeetingEndedOutcome::NotCurrent);
    assert_one_cell(&snap, EndAssignmentOutcome::AlreadyEnded);
}

/// A notify naming another incarnation (or another controller) while a live
/// row exists ends NOTHING, answers NOT_CURRENT (MC stops retrying), is
/// counted `fenced`, and logs one WARN carrying the caller's validated
/// subject for attribution.
#[sqlx::test(migrations = "../../migrations")]
async fn notify_for_another_incarnation_is_fenced(pool: PgPool) {
    let live = AssignmentId::new_v4();
    live_row(&pool, live).await;
    let stale = AssignmentId::new_v4();

    for (controller, aid) in [(MC, stale.to_string()), ("mc-2", live.to_string())] {
        let capture = EventCapture::install(LOG_TARGET);
        let snap = MetricAssertion::snapshot();
        let mut request = req(MEETING, REGION, controller, &aid);
        request.extensions_mut().insert(Claims {
            sub: "mc-svc-sub-1".to_string(),
            exp: 0,
            iat: 0,
            scope: "service.write.gc".to_string(),
            service_type: Some("meeting-controller".to_string()),
        });
        let got = svc(&pool).notify_meeting_ended(request).await;
        assert_eq!(outcome(got), NotifyMeetingEndedOutcome::NotCurrent);
        assert_one_cell(&snap, EndAssignmentOutcome::Fenced);
        assert!(is_live(&pool).await, "the live incarnation is untouched");

        let events = capture.take();
        let warns: Vec<_> = events
            .iter()
            .filter(|e| e.level == tracing::Level::WARN)
            .collect();
        assert_eq!(warns.len(), 1, "one fenced WARN: {events:?}");
        assert_eq!(warns[0].field("caller_sub"), Some("mc-svc-sub-1"));
        assert_eq!(warns[0].field("mc_id"), Some(controller));
    }
}
