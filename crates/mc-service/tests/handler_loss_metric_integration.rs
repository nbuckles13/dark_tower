//! The required loss metric, the client-loss disagreement and the handler
//! census (ADR-0039 §8; story 4 MC-E: R-33, R-34; test-gates "MC loss
//! metric").
//!
//! Tier 1b on the MC-C rig (`common/replacement_rig.rs`): the real gRPC
//! coordination service and meeting actors under a controller, paused time, a
//! LIVE fake MH (`heartbeat_via`: `silence` is a crash) and a scripted GC.
//! "Believed connected" is MH-reported connectivity (`connect`); client
//! reports go through the meeting actor's real intake
//! (`report_client_handler_statuses`), stamped at MC receipt.
//!
//! Every negative has its positive arm in the same test; "never" uses the
//! ADR-0032 unobserved assertion. A histogram query drains its own
//! name+labels, so each histogram is read once per snapshot.

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::collections::BTreeMap;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use mc_service::actors::MeetingActorHandle;
use mc_service::grpc::gc_client::AcquireResult;
use mc_service::media_dial::status::ReportedState;
use mc_service::media_routing::{HandlerEndpoint, HandlerId};
use proto_gen::dark_tower::internal::v1::ApplyRefusal;
use proto_gen::dark_tower::signaling::v1::server_message;
use test_common::media_dial::{report, Token};
use test_common::replacement_rig::*;
use test_common::{advance_live, disagreement_threshold, drain_frames, mh_handler, run_ready};

const HT: &str = "heartbeat_timeout";
const AL: &str = "apply_lag";
const PT: &str = "planned_transfer";

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/// A set's version and `handler id -> ref` (the id is the token audience, so
/// two incarnations at one URL stay apart).
fn refs_of(
    set: &proto_gen::dark_tower::signaling::v1::MediaHandlerSet,
) -> (u64, BTreeMap<String, u32>) {
    let refs = set
        .handlers
        .iter()
        .map(|h| {
            (
                Token::parse(&h.media_dial_token).str("aud").to_string(),
                h.handler_ref,
            )
        })
        .collect();
    (set.handler_set_version, refs)
}

/// The latest `MediaHandlerSet` pushed on `q` since the last read.
fn latest_refs(q: &mut tokio::sync::mpsc::Receiver<bytes::Bytes>) -> (u64, BTreeMap<String, u32>) {
    let set = drain_frames(q)
        .into_iter()
        .filter_map(|f| match f {
            server_message::Message::MediaHandlerSet(s) => Some(s),
            _ => None,
        })
        .next_back()
        .expect("a MediaHandlerSet pushed on the queue");
    refs_of(&set)
}

async fn say(
    meeting: &MeetingActorHandle,
    p: &str,
    handler_ref: u32,
    version: u64,
    state: ReportedState,
) {
    meeting
        .report_client_handler_statuses(p.to_string(), vec![report(handler_ref, version, state)])
        .await
        .unwrap();
    run_ready().await;
}

fn loss(snap: &MetricSnapshot, reported: &str) -> u64 {
    snap.counter("mc_media_handler_loss_clients_total")
        .with_labels(&[("reported", reported), KC])
        .delta()
}

fn counter(snap: &MetricSnapshot, name: &'static str, trigger: &'static str) -> u64 {
    snap.counter(name)
        .with_labels(&[("trigger", trigger), KC])
        .delta()
}

fn disagreements(snap: &MetricSnapshot) -> u64 {
    snap.counter("mc_media_handler_client_loss_disagreements_total")
        .with_labels(&[KC])
        .delta()
}

/// Every loss series unobserved (hard form).
fn assert_no_loss_observation(snap: &MetricSnapshot) {
    for h in [
        "mc_media_handler_loss_prior_report_ratio",
        "mc_media_handler_loss_client_lead_seconds",
    ] {
        snap.histogram(h).with_labels(&[KC]).assert_unobserved();
    }
    for reported in ["before_declaration", "not_reported"] {
        snap.counter("mc_media_handler_loss_clients_total")
            .with_labels(&[("reported", reported), KC])
            .assert_unobserved();
    }
}

// ----------------------------------------------------------------------------
// The required loss metric
// ----------------------------------------------------------------------------

/// k of n at a Dead declaration, from the ONE snapshot at the transition.
/// n = 5 MH-reported connected to H1 (a..e); k = 2:
/// - a: an unsuperseded DISCONNECTED → counted (the positive arm);
/// - b: FAILED with a STALE set version, ref still in the set → counted
///   (version is correlation-only);
/// - c: DISCONNECTED then CONNECTED after a redial → not counted (latest
///   status per ref);
/// - d, e: no report;
/// - f: reports DISCONNECTED but MH never reported it connected → in neither
///   count (numerator ⊆ denominator).
///
/// So `before_declaration` +2, `not_reported` +3 (not +4: f excluded), one
/// ratio observation (0.4, pinned in the pure table) and one lead observation.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn k_of_n_loss_reports_are_counted_at_a_dead_declaration() {
    let r = rig();
    let meeting = r.create("m", &[H1, H2]).await;
    let (_a, joined) = r.join_with_set("m", "a").await;
    for p in ["b", "c", "d", "e", "f"] {
        let _ = r.join("m", p).await;
    }
    for p in ["a", "b", "c", "d", "e"] {
        assert_ne!(r.connect("m", p, H1, &format!("{p}1")).await, 0);
    }
    advance_live(settle()).await;
    let (version, refs) = refs_of(&joined);
    let r1 = refs[H1];

    say(&meeting, "a", r1, version, ReportedState::Disconnected).await;
    say(&meeting, "b", r1, version - 1, ReportedState::Failed).await;
    say(&meeting, "c", r1, version, ReportedState::Disconnected).await;
    say(&meeting, "c", r1, version, ReportedState::Connected).await;
    say(&meeting, "f", r1, version, ReportedState::Disconnected).await;
    advance_live(interval()).await;
    let snap = MetricAssertion::snapshot();

    r.mh.silence(H1);
    advance_live(past_declaration()).await;
    assert_eq!(health(&meeting, H1).await, None, "declared and removed");

    snap.histogram("mc_media_handler_loss_prior_report_ratio")
        .with_labels(&[KC])
        .assert_observation_count(1);
    snap.histogram("mc_media_handler_loss_client_lead_seconds")
        .with_labels(&[KC])
        .assert_observation_count(1);
    assert_eq!(loss(&snap, "before_declaration"), 2);
    assert_eq!(loss(&snap, "not_reported"), 3);
    assert_eq!(counter(&snap, "mc_media_handler_declarations_total", HT), 1);
    r.c.cancel();
}

/// A report received after its ref was REMOVED is ignored, and two
/// incarnations sharing ONE URL are attributed by `handler_ref` — the case URL
/// attribution would get wrong. (Two CONCURRENT members can never share a URL:
/// the set refuses it, `HandlerSetError::DuplicateUrl`; sharing happens only
/// across incarnations.) H1 (ref R1, URL U) dies; a client reports losing R1
/// AFTER the removal; GC grants mh-1b at the SAME URL U (an in-place restart's
/// new incarnation, ref R2). When mh-1b is declared, the R1 reports do not
/// count; b's report naming R2 does (the positive arm).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_report_after_removal_is_ignored_even_at_the_same_url() {
    const H1B: &str = "mh-1b";
    let r = rig();
    r.gc.push(AcquireResult::Granted(HandlerEndpoint {
        id: HandlerId::new(H1B),
        webtransport_url: url(H1),
        grpc_endpoint: mh_handler(H1B).grpc_endpoint,
    }));
    let meeting = r.create("m", &[H1, H2]).await;
    let (mut a, joined) = r.join_with_set("m", "a").await;
    let _b = r.join("m", "b").await;
    for p in ["a", "b"] {
        assert_ne!(r.connect("m", p, H1, &format!("{p}1")).await, 0);
    }
    advance_live(settle()).await;
    let (version0, refs) = refs_of(&joined);
    let r1 = refs[H1];

    r.mh.silence(H1);
    advance_live(past_declaration()).await;
    assert_eq!(health(&meeting, H1).await, None);
    // After the removal: a report naming the removed ref.
    say(&meeting, "a", r1, version0, ReportedState::Disconnected).await;

    // The replacement at the same URL becomes Active and everyone redials it.
    advance_live(interval()).await;
    let (version, refs) = latest_refs(&mut a);
    assert!(!refs.contains_key(H1));
    let r2 = refs[H1B];
    assert_ne!(r1, r2, "a new incarnation is a new ref");
    for p in ["a", "b"] {
        assert_ne!(r.connect("m", p, H1B, &format!("{p}1b")).await, 0);
    }
    advance_live(settle()).await;
    say(&meeting, "a", r1, version, ReportedState::Disconnected).await;
    say(&meeting, "b", r2, version, ReportedState::Disconnected).await;
    let snap = MetricAssertion::snapshot();

    r.mh.silence(H1B);
    advance_live(past_declaration()).await;
    assert_eq!(health(&meeting, H1B).await, None);
    assert_eq!(loss(&snap, "before_declaration"), 1, "b's R2 report only");
    assert_eq!(loss(&snap, "not_reported"), 1, "a's R1 reports never count");
    r.c.cancel();
}

/// An `apply_lag` declaration (a capacity refusal of MC's push) observes the
/// loss metric too: a reported, b did not; c joined but never connected. One
/// lead observation (>= 1 client reported).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_apply_lag_declaration_observes_the_loss_metric() {
    let r = rig();
    let meeting = r.create("m", &[H1]).await;
    let (_a, joined) = r.join_with_set("m", "a").await;
    let _b = r.join("m", "b").await;
    for p in ["a", "b"] {
        assert_ne!(r.connect("m", p, H1, &format!("{p}1")).await, 0);
    }
    advance_live(settle()).await;
    let (version, refs) = refs_of(&joined);
    say(&meeting, "a", refs[H1], version, ReportedState::Failed).await;
    let snap = MetricAssertion::snapshot();

    r.mh.refuse(H1, ApplyRefusal::StreamCeiling);
    let _c = r.join("m", "c").await; // MC pushes a new generation; refused
    for _ in 0..4 {
        advance_live(interval()).await;
        if health(&meeting, H1).await.is_none() {
            break;
        }
    }
    assert_eq!(health(&meeting, H1).await, None, "declared Impaired");
    snap.histogram("mc_media_handler_loss_prior_report_ratio")
        .with_labels(&[KC])
        .assert_observation_count(1);
    snap.histogram("mc_media_handler_loss_client_lead_seconds")
        .with_labels(&[KC])
        .assert_observation_count(1);
    assert_eq!(counter(&snap, "mc_media_handler_declarations_total", AL), 1);
    assert_eq!(loss(&snap, "before_declaration"), 1);
    assert_eq!(loss(&snap, "not_reported"), 1);
    r.c.cancel();
}

/// A PLANNED_TRANSFER produces NO loss observation (unobserved), even with a
/// client reporting loss on the draining handler; its first declaration is
/// counted under `planned_transfer`. The positive control in the SAME rig: a
/// crash of the target afterwards observes.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_planned_transfer_never_observes_while_a_crash_in_the_same_rig_does() {
    let r = rig();
    r.gc.push(grant(H2));
    let meeting = r.create("m", &[H1]).await;
    let (_a, joined) = r.join_with_set("m", "a").await;
    let _b = r.join("m", "b").await;
    for p in ["a", "b"] {
        assert_ne!(r.connect("m", p, H1, &format!("{p}1")).await, 0);
    }
    advance_live(settle()).await;
    let (version, refs) = refs_of(&joined);
    say(
        &meeting,
        "a",
        refs[H1],
        version,
        ReportedState::Disconnected,
    )
    .await;
    let snap = MetricAssertion::snapshot();

    r.mh.drain(H1);
    advance_live(interval() * 2).await;
    run_ready().await;
    for p in ["a", "b"] {
        assert_ne!(r.connect("m", p, H2, &format!("{p}2")).await, 0);
    }
    advance_live(interval() * 2).await;
    assert_eq!(health(&meeting, H1).await, None, "transferred off H1");
    assert_no_loss_observation(&snap);
    assert_eq!(counter(&snap, "mc_media_handler_declarations_total", PT), 1);
    assert_eq!(
        counter(&snap, "mc_media_handler_first_declarations_total", PT),
        1
    );

    // Positive control: the same rig observes a crash.
    let snap = MetricAssertion::snapshot();
    r.mh.silence(H2);
    advance_live(past_declaration()).await;
    assert_eq!(health(&meeting, H2).await, None);
    snap.histogram("mc_media_handler_loss_prior_report_ratio")
        .with_labels(&[KC])
        .assert_observation_count(1);
    snap.histogram("mc_media_handler_loss_client_lead_seconds")
        .with_labels(&[KC])
        .assert_unobserved();
    assert_eq!(counter(&snap, "mc_media_handler_declarations_total", HT), 1);
    assert_eq!(loss(&snap, "before_declaration"), 0, "nobody reported H2");
    assert_eq!(loss(&snap, "not_reported"), 2);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// Client-loss disagreement (NON-default threshold: fraction 0.6, min 2)
// ----------------------------------------------------------------------------

/// 3 of 5 believed-connected clients report losing H1 (fraction exactly 0.6,
/// min met) while its heartbeats stay healthy: counted ONCE after the quorum
/// has held longer than N, never a removal. One reporter redials (2 of 5):
/// the episode ends; the quorum returning is a new episode, counted again.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_held_client_loss_quorum_with_healthy_heartbeats_is_one_disagreement_per_episode() {
    let mut r = rig();
    r.disagreement = disagreement_threshold(0.6, 2);
    let meeting = r.create("m", &[H1, H2]).await;
    let (_a, joined) = r.join_with_set("m", "a").await;
    for p in ["b", "c", "d", "e"] {
        let _ = r.join("m", p).await;
    }
    for p in ["a", "b", "c", "d", "e"] {
        assert_ne!(r.connect("m", p, H1, &format!("{p}1")).await, 0);
    }
    advance_live(settle()).await;
    let (version, refs) = refs_of(&joined);
    let r1 = refs[H1];
    let snap = MetricAssertion::snapshot();

    for p in ["a", "b", "c"] {
        say(&meeting, p, r1, version, ReportedState::Disconnected).await;
    }
    advance_live(n() / 2).await;
    assert_eq!(disagreements(&snap), 0, "held for less than N");
    advance_live(n() / 2 + interval() * 2).await;
    assert_eq!(disagreements(&snap), 1, "held longer than N");
    advance_live(n()).await;
    assert_eq!(disagreements(&snap), 1, "once per episode");
    assert_eq!(
        health(&meeting, H1).await,
        Some("healthy"),
        "never a removal"
    );
    for t in [HT, AL] {
        snap.counter("mc_media_handler_declarations_total")
            .with_labels(&[("trigger", t), KC])
            .assert_unobserved();
    }

    // c redials: 2 of 5 (0.4) — the episode ends.
    say(&meeting, "c", r1, version, ReportedState::Connected).await;
    advance_live(interval() * 2).await;
    // The quorum returns: a NEW episode, counted after its own N.
    say(&meeting, "c", r1, version, ReportedState::Disconnected).await;
    advance_live(n() / 2).await;
    assert_eq!(disagreements(&snap), 1);
    advance_live(n() / 2 + interval() * 2).await;
    assert_eq!(
        disagreements(&snap),
        2,
        "a new episode after the quorum was lost"
    );
    assert_eq!(health(&meeting, H1).await, Some("healthy"));
    r.c.cancel();
}

/// Below either threshold → never counted, however long it holds:
/// - one connected client reporting (1/1: fraction met, min − 1);
/// - 2 of 5 reporting (0.4: min met, fraction just below).
///
/// Positive arm: a third reporter in the second meeting reaches 3/5 and is
/// counted.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn below_either_threshold_is_never_a_disagreement() {
    let mut r = rig();
    r.disagreement = disagreement_threshold(0.6, 2);
    let solo = r.create("solo", &[H1]).await;
    let (_s, solo_set) = r.join_with_set("solo", "a").await;
    assert_ne!(r.connect("solo", "a", H1, "solo-a1").await, 0);
    let five = r.create("five", &[H2]).await;
    let (_q, five_set) = r.join_with_set("five", "a").await;
    for p in ["b", "c", "d", "e"] {
        let _ = r.join("five", p).await;
    }
    for p in ["a", "b", "c", "d", "e"] {
        assert_ne!(r.connect("five", p, H2, &format!("five-{p}2")).await, 0);
    }
    advance_live(settle()).await;
    let (sv, srefs) = refs_of(&solo_set);
    let (fv, frefs) = refs_of(&five_set);
    let snap = MetricAssertion::snapshot();

    say(&solo, "a", srefs[H1], sv, ReportedState::Disconnected).await;
    for p in ["a", "b"] {
        say(&five, p, frefs[H2], fv, ReportedState::Disconnected).await;
    }
    advance_live(n() * 3).await;
    assert_eq!(disagreements(&snap), 0, "min - 1, and fraction just below");

    say(&five, "c", frefs[H2], fv, ReportedState::Failed).await;
    advance_live(n() + interval() * 2).await;
    assert_eq!(disagreements(&snap), 1, "positive arm: 3 of 5");
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// HandlerCensus
// ----------------------------------------------------------------------------

/// One handler declared by THREE meetings: three declarations (the positive
/// control that each meeting fired) and ONE first declaration; the other
/// trigger cells unobserved. `mc_media_handlers_in_use` counts distinct
/// handlers (H1 in three meetings + H2 = 2), drops when H1 left every set,
/// and is recomputed when a meeting ends.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn one_handler_declared_by_three_meetings_is_one_first_declaration() {
    let r = rig();
    let snap = MetricAssertion::snapshot();
    for m in ["m1", "m2", "m3"] {
        let _ = r.create(m, &[H1]).await;
        let _ = r.join(m, "a").await;
    }
    let other = r.create("m4", &[H2]).await;
    let _ = r.join("m4", "a").await;
    run_ready().await;
    snap.gauge("mc_media_handlers_in_use")
        .with_labels(&[KC])
        .assert_value(2.0);

    r.mh.silence(H1);
    advance_live(past_declaration()).await;
    assert_eq!(counter(&snap, "mc_media_handler_declarations_total", HT), 3);
    assert_eq!(
        counter(&snap, "mc_media_handler_first_declarations_total", HT),
        1
    );
    for t in [AL, PT] {
        snap.counter("mc_media_handler_first_declarations_total")
            .with_labels(&[("trigger", t), KC])
            .assert_unobserved();
    }
    snap.gauge("mc_media_handlers_in_use")
        .with_labels(&[KC])
        .assert_value(1.0);

    other.close_meeting("done".into()).await.unwrap();
    advance_live(interval()).await;
    snap.gauge("mc_media_handlers_in_use")
        .with_labels(&[KC])
        .assert_value(0.0);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// The published threshold (config echo)
// ----------------------------------------------------------------------------

/// The disagreement threshold echo is published when the server is built,
/// from the SAME `DisagreementThreshold` every meeting's `assess` reads (the
/// shipped ConfigMap values), with `key_custody`.
#[tokio::test(flavor = "current_thread")]
async fn the_disagreement_threshold_is_published_at_boot() {
    let shipped = test_common::client_loss_disagreement();
    let boot = MetricAssertion::snapshot();
    let stack = test_common::build_test_stack("mc-e-threshold").await;
    let _rig = test_common::accept_loop_rig::AcceptLoopRig::start(
        std::sync::Arc::clone(&stack.controller_handle),
        std::sync::Arc::clone(&stack.jwt_validator),
        std::sync::Arc::clone(&stack.mh_reg_client)
            as std::sync::Arc<dyn mc_service::grpc::MhRegistrationClient>,
    )
    .await;
    boot.gauge("mc_media_handler_client_loss_disagreement_ratio")
        .with_labels(&[KC])
        .assert_value(shipped.fraction().get());
    boot.gauge("mc_media_handler_client_loss_disagreement_min_clients")
        .with_labels(&[KC])
        .assert_value(f64::from(shipped.min_clients().get()));
}
