//! Ending a test's own MC meeting, PROVED with evidence scoped to that
//! meeting's id (never a fleet counter).
//!
//! # The rule (narrowed on purpose)
//!
//! A suite that releases MH state behind MC's back (the injection suites:
//! `fixtures::injection`), or a meeting that `tests/40_resilience.rs`'s legs
//! depend on (its legs and their placement misses), MUST end its meeting
//! through this module before it returns. Such a meeting left alive is the
//! orphan class: MC holds it for its disconnect grace, declares the handlers
//! the test released Dead, and asks GC for a replacement — which GC may grant
//! from a later pool leg's run-owned MH.
//!
//! Every OTHER meeting-creating suite is outside the rule: it never releases a
//! meeting on MH behind MC, so MC's handlers keep heartbeating it and MC ends
//! it after its grace. That is the operator's "a meeting ends when its last
//! participant leaves" rule working, not a leak; nothing couples to it.
//!
//! # The proof, per meeting, in order
//!
//! The scope of every read is the meeting's id from just before its earliest
//! departed participant JOINED (a scan bound only; ordering is decided on MC's
//! own Loki timestamps).
//!
//! 1. **Positive control** — one [`MC_CLEAN_CLOSE`] line per departed
//!    participant id ([`CLEAN_CLOSE_NEVER_SEEN`]). It proves every session
//!    the caller passed closed CLEANLY (so MC removed it at once). It runs
//!    first, so a Loki ingest gap fails here, never as an MC defect.
//! 2. **Ended** — exactly one [`MC_MEETING_ENDED`] in the scope, NOT before
//!    the newest of those clean-close lines (an earlier end means MC ended the
//!    meeting with a participant still in it), with `cause=empty`,
//!    `reason=last_participant_left` ([`MEETING_NOT_ENDED`]). A participant
//!    the caller FORGOT and left connected keeps the meeting alive: this step
//!    then fails, and its message names that possibility beside the MC one.
//! 3. **Released** — exactly one [`MC_MEETING_RELEASED`] at or after the end,
//!    with `failed == 0`, `quiesce_timed_out == false`, `released == handlers`
//!    and, whenever a departing participant last saw a non-empty handler set,
//!    `handlers >= 1` (an empty plan would make `failed == 0` vacuous)
//!    ([`MEETING_NOT_RELEASED`]).
//! 4. **No grace path** — NO [`MC_GRACE_EXPIRED`] for this meeting in the
//!    scope, for any participant ([`MEETING_ENDED_BY_GRACE`]): a participant
//!    the caller forgot and that was DROPPED (not left) ends the meeting only
//!    at grace expiry, which steps 2-3 would accept, ~one grace slower. Checked
//!    after (2) matched in the same scope (its positive control); the line
//!    itself is pinned to its emitter by `loki.rs`'s hermetic test. MC's
//!    "grace period started" line is deliberately NOT checked: MC documents a
//!    legitimate race in which a `ConnectionLost` is processed before the
//!    queued `ClientClosed` (`crates/mc-service/src/actors/meeting.rs`, the
//!    `ClientClosed` arm).
//!
//! # Bounds (failure ceilings only)
//!
//! `ceiling` = MC's published disconnect grace ([`mc_disconnect_grace`], a
//! config echo) + the log-ingest bound ([`ConsistencyCategory::LogAggregation`]).
//! ONE deadline, `ceiling` from the call, is shared by steps 1-2 of every
//! meeting (they end concurrently on MC); ONE release deadline, `ceiling`
//! after that, by every meeting's step 3 (each still gets at least `ceiling`
//! from its own end line). The whole call is bounded by 2 x `ceiling` (the TS
//! twin, `packages/web-app/e2e/mcLoki.ts`, has the same shape). The release
//! window must cover MC's teardown worst case, which MC logs at startup as
//! `teardown_fence_hold_max_seconds` — read that field, not a number here. A
//! clean close ends the meeting at once, so the expected cost is one ingest
//! latency.
//!
//! Loki unavailable is a FAILURE
//! ([`crate::fixtures::loki::PRECONDITION_LOKI_UNAVAILABLE`]), never a skip;
//! callers check it up front with [`crate::fixtures::loki::require_loki`].

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::cluster::ClusterConnection;
use crate::eventual::ConsistencyCategory;
use crate::fixtures::loki::{
    meeting_logql, require_loki, Entry, MeetingScope, MC_CLEAN_CLOSE, MC_GRACE_EXPIRED,
    MC_MEETING_ENDED, MC_MEETING_RELEASED, SCOPE_SKEW_SLACK,
};
use crate::fixtures::metrics::{mc_disconnect_grace, PrometheusClient};
use crate::fixtures::participant::Participant;

/// Reason token: a departed participant's clean-close line never appeared.
pub const CLEAN_CLOSE_NEVER_SEEN: &str = "clean-close-never-seen";
/// Reason token: MC did not end the emptied meeting (product defect, or a
/// participant was not passed to the end path).
pub const MEETING_NOT_ENDED: &str = "meeting-not-ended";
/// Reason token: MC did not finish releasing the ended meeting on its handlers.
pub const MEETING_NOT_RELEASED: &str = "meeting-not-released";
/// Reason token: a participant of the meeting was removed by grace expiry.
pub const MEETING_ENDED_BY_GRACE: &str = "meeting-ended-by-grace";

/// How often the proof re-reads Loki.
const POLL: Duration = Duration::from_secs(1);

/// One participant that LEFT gracefully ([`Participant::leave`]): what the
/// proof needs. The participant id is never printed (labels are).
#[derive(Clone)]
pub struct Departure {
    pub meeting_id: Uuid,
    pub participant_id: String,
    pub label: &'static str,
    /// The participant's last-seen handler set was non-empty, so MC's
    /// teardown must have planned at least one release.
    pub had_handlers: bool,
    /// Host wall clock just before the participant joined (scan bound only).
    pub joined_ns: u128,
}

/// Leave every participant in `ps` gracefully, then [`await_ended`] for every
/// meeting among them and `also` (departures that already left, e.g. 40's
/// placement misses).
///
/// # Panics
/// Nothing to end; any token of [`await_ended`].
pub async fn leave_and_await_ended(
    phase: &str,
    cluster: &ClusterConnection,
    prom: &PrometheusClient,
    ps: Vec<Participant>,
    also: Vec<Departure>,
) {
    assert!(
        !ps.is_empty() || !also.is_empty(),
        "{phase}: leave_and_await_ended called with no participant"
    );
    let mut departures: Vec<Departure> = ps.into_iter().map(Participant::leave).collect();
    departures.extend(also);
    await_ended(phase, cluster, prom, departures).await;
}

/// The proof's budget (module doc §Bounds).
struct Budget {
    grace: Duration,
    ingest: Duration,
    /// Steps 1-2 of every meeting.
    deadline: Instant,
    /// Step 3 of every meeting.
    release_deadline: Instant,
}

impl Budget {
    fn ceiling(&self) -> Duration {
        self.grace + self.ingest
    }
}

/// Prove, per meeting among `departures`, that MC removed every departed
/// participant cleanly, ended the meeting and released it on its handlers
/// (module doc).
///
/// # Panics
/// [`CLEAN_CLOSE_NEVER_SEEN`], [`MEETING_NOT_ENDED`], [`MEETING_NOT_RELEASED`],
/// [`MEETING_ENDED_BY_GRACE`], `PRECONDITION-loki-unavailable` or
/// `loki-query-failed`.
pub async fn await_ended(
    phase: &str,
    cluster: &ClusterConnection,
    prom: &PrometheusClient,
    departures: Vec<Departure>,
) {
    let loki = require_loki(cluster).await;
    let grace = mc_disconnect_grace(prom).await;
    let ingest = ConsistencyCategory::LogAggregation.timeout();
    let deadline = Instant::now() + grace + ingest;
    let budget = Budget {
        grace,
        ingest,
        deadline,
        release_deadline: deadline + grace + ingest,
    };
    let mut by_meeting: BTreeMap<Uuid, Vec<Departure>> = BTreeMap::new();
    for d in departures {
        by_meeting.entry(d.meeting_id).or_default().push(d);
    }
    for (meeting_id, departed) in by_meeting {
        let since_ns = departed
            .iter()
            .map(|d| d.joined_ns)
            .min()
            .unwrap_or(0)
            .saturating_sub(SCOPE_SKEW_SLACK.as_nanos());
        let scope = MeetingScope {
            cluster,
            loki: loki.clone(),
            meeting_id,
            since_ns,
        };
        prove_meeting_ended(phase, &scope, &departed, &budget).await;
    }
}

/// Steps 1-4 of the module doc for ONE meeting.
async fn prove_meeting_ended(
    phase: &str,
    scope: &MeetingScope<'_>,
    departed: &[Departure],
    budget: &Budget,
) {
    let started = Instant::now();
    let meeting_id = scope.meeting_id;
    let (grace, ingest, ceiling) = (budget.grace, budget.ingest, budget.ceiling());

    // 1. Positive control: one clean-close line per departed participant.
    let closes = scope
        .poll_until(MC_CLEAN_CLOSE, budget.deadline, POLL, |es| {
            missing_departures(departed, es)
                .is_empty()
                .then(|| es.to_vec())
        })
        .await
        .unwrap_or_else(|last| {
            panic!(
                "{phase} {CLEAN_CLOSE_NEVER_SEEN}: meeting {meeting_id}: no clean-close line for \
                 participant(s) {:?} within {ceiling:?} (LogQL `{}`). MC removes a participant \
                 at once ONLY on a clean close: was every session left with mc_session::leave \
                 (never dropped)? If Loki is not ingesting mc-service, that is the \
                 environment, not MC.",
                missing_departures(departed, &last),
                meeting_logql(MC_CLEAN_CLOSE, meeting_id)
            )
        });
    let newest_close = newest_close_ns(departed, &closes)
        .unwrap_or_else(|e| panic!("{phase} {CLEAN_CLOSE_NEVER_SEEN}: {e}"));

    // 2. Exactly one end in the scope, not before the newest clean close.
    let end = scope
        .poll_until(MC_MEETING_ENDED, budget.deadline, POLL, |es| {
            pick_end(es, scope.since_ns, newest_close)
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "{phase} {MEETING_NOT_ENDED}: MC did not end an emptied meeting: product defect, \
                 not env (see 195b7059) — or a participant of this meeting was not passed to \
                 the end path and is still connected. Meeting {meeting_id}: every passed \
                 participant closed cleanly (clean-close lines seen), yet no `{}` line within \
                 {ceiling:?} (grace {grace:?} + ingest {ingest:?}); LogQL `{}`",
                MC_MEETING_ENDED.message,
                meeting_logql(MC_MEETING_ENDED, meeting_id)
            )
        })
        .unwrap_or_else(|e| panic!("{phase} {MEETING_NOT_ENDED}: meeting {meeting_id}: {e}"));

    // 3. Exactly one completed release, after the end.
    let expect_handlers = departed.iter().any(|d| d.had_handlers);
    let released = scope
        .poll_until(MC_MEETING_RELEASED, budget.release_deadline, POLL, |es| {
            released_verdict(es, end.ts_ns, expect_handlers)
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "{phase} {MEETING_NOT_RELEASED}: meeting {meeting_id} ended but MC logged no `{}` \
                 after it before the release deadline (one {ceiling:?} window after the end \
                 deadline); LogQL `{}`",
                MC_MEETING_RELEASED.message,
                meeting_logql(MC_MEETING_RELEASED, meeting_id)
            )
        })
        .unwrap_or_else(|e| panic!("{phase} {MEETING_NOT_RELEASED}: meeting {meeting_id}: {e}"));

    // 4. No grace path, for ANY participant of the meeting (after (2) matched).
    let (named, unlisted) = grace_removed(departed, &scope.entries(MC_GRACE_EXPIRED).await);
    assert!(
        named.is_empty() && unlisted == 0,
        "{phase} {MEETING_ENDED_BY_GRACE}: meeting {meeting_id}: participant(s) {named:?} and \
         {unlisted} participant(s) NOT passed to the end path were removed by disconnect-grace \
         expiry, not by a clean close (a session dropped instead of left); LogQL `{}`",
        meeting_logql(MC_GRACE_EXPIRED, meeting_id)
    );

    eprintln!(
        "[meeting_end] {phase}: meeting {meeting_id} ended (cleanly, {} participant(s)) and \
         released on {} handler(s), proved in {:?}",
        departed.len(),
        released.field_u64("handlers").unwrap_or(0),
        started.elapsed()
    );
}

/// PURE: the participant ids `entries` name.
fn named_ids(entries: &[Entry]) -> BTreeSet<&str> {
    entries
        .iter()
        .filter_map(|e| e.field("participant_id"))
        .collect()
}

/// PURE: the labels of `departed` with no entry naming their participant id.
#[must_use]
pub fn missing_departures(departed: &[Departure], entries: &[Entry]) -> Vec<&'static str> {
    let seen = named_ids(entries);
    departed
        .iter()
        .filter(|d| !seen.contains(d.participant_id.as_str()))
        .map(|d| d.label)
        .collect()
}

/// PURE: the departed participants' grace-expiry removals: their labels, and
/// the COUNT of grace-expired participants not in `departed` (ids never
/// printed).
#[must_use]
pub fn grace_removed(departed: &[Departure], grace_lines: &[Entry]) -> (Vec<&'static str>, usize) {
    let named = named_ids(grace_lines);
    let ours: BTreeSet<&str> = departed.iter().map(|d| d.participant_id.as_str()).collect();
    let labels = departed
        .iter()
        .filter(|d| named.contains(d.participant_id.as_str()))
        .map(|d| d.label)
        .collect();
    (labels, named.difference(&ours).count())
}

/// PURE: the newest clean-close line of a DEPARTED participant. None is an
/// error, never "accept any".
///
/// # Errors
/// No clean-close line for any departed participant.
pub fn newest_close_ns(departed: &[Departure], closes: &[Entry]) -> Result<u128, String> {
    let ours: BTreeSet<&str> = departed.iter().map(|d| d.participant_id.as_str()).collect();
    closes
        .iter()
        .filter(|e| e.field("participant_id").is_some_and(|p| ours.contains(p)))
        .map(|e| e.ts_ns)
        .max()
        .ok_or_else(|| "no clean-close line for any departed participant to anchor on".to_string())
}

/// PURE: the end of THIS meeting. `None` while no end line is in the scope
/// (at/after `since_ns`); exactly one, not before `newest_close`, with
/// `cause=empty`, `reason=last_participant_left` is `Ok(it)`; anything else
/// is an error — two ends, an end BEFORE the newest clean close (MC ended the
/// meeting with a participant still in it), or another cause.
#[must_use]
pub fn pick_end(
    entries: &[Entry],
    since_ns: u128,
    newest_close: u128,
) -> Option<Result<Entry, String>> {
    let in_scope: Vec<&Entry> = entries.iter().filter(|e| e.ts_ns >= since_ns).collect();
    let one = match in_scope.as_slice() {
        [] => return None,
        [one] => *one,
        many => {
            return Some(Err(format!(
                "{} end lines since the participants joined (exactly one expected)",
                many.len()
            )))
        }
    };
    Some(if one.ts_ns < newest_close {
        Err(format!(
            "the meeting ended BEFORE participant(s) departed (end at {}, newest clean close at \
             {newest_close}, MC's Loki clock)",
            one.ts_ns
        ))
    } else if (one.field("cause"), one.field("reason"))
        == (Some("empty"), Some("last_participant_left"))
    {
        Ok(one.clone())
    } else {
        Err(format!(
            "the meeting ended, but not because its last participant left: cause {:?}, reason \
             {:?}",
            one.field("cause"),
            one.field("reason")
        ))
    })
}

/// PURE: the completed release after `end_ts`. `None` while absent; exactly
/// one with `failed == 0`, `!quiesce_timed_out`, `released == handlers` and
/// (when `expect_handlers`) `handlers >= 1` is `Ok`; anything else an error.
#[must_use]
pub fn released_verdict(
    entries: &[Entry],
    end_ts: u128,
    expect_handlers: bool,
) -> Option<Result<Entry, String>> {
    let after: Vec<&Entry> = entries.iter().filter(|e| e.ts_ns >= end_ts).collect();
    let one = match after.as_slice() {
        [] => return None,
        [one] => *one,
        many => {
            return Some(Err(format!(
                "{} release lines after the end (exactly one expected)",
                many.len()
            )))
        }
    };
    let (handlers, released, failed, timed_out) = (
        one.field_u64("handlers"),
        one.field_u64("released"),
        one.field_u64("failed"),
        one.field_bool("quiesce_timed_out"),
    );
    let ok = failed == Some(0)
        && timed_out == Some(false)
        && handlers.is_some()
        && released == handlers
        && (!expect_handlers || handlers.unwrap_or(0) >= 1);
    Some(if ok {
        Ok(one.clone())
    } else {
        Err(format!(
            "release incomplete: handlers {handlers:?}, released {released:?}, failed \
             {failed:?}, quiesce_timed_out {timed_out:?} (expected failed 0, no quiesce \
             timeout, released == handlers{})",
            if expect_handlers {
                ", handlers >= 1: a departing participant last saw a non-empty handler set"
            } else {
                ""
            }
        ))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn dep(label: &'static str, pid: &str, had_handlers: bool) -> Departure {
        Departure {
            meeting_id: Uuid::nil(),
            participant_id: pid.to_string(),
            label,
            had_handlers,
            joined_ns: 100,
        }
    }

    fn entry(ts: u128, fields: serde_json::Value) -> Entry {
        Entry {
            ts_ns: ts,
            json: serde_json::json!({ "fields": fields }),
        }
    }

    fn close(ts: u128, pid: &str) -> Entry {
        entry(ts, serde_json::json!({ "participant_id": pid }))
    }

    fn end(ts: u128, cause: &str, reason: &str) -> Entry {
        entry(ts, serde_json::json!({ "cause": cause, "reason": reason }))
    }

    fn rel(ts: u128, handlers: u64, released: u64, failed: u64, timed_out: bool) -> Entry {
        entry(
            ts,
            serde_json::json!({
                "handlers": handlers, "released": released,
                "failed": failed, "quiesce_timed_out": timed_out
            }),
        )
    }

    #[test]
    fn a_participant_without_a_clean_close_is_named() {
        let d = [dep("A", "pa", true), dep("B", "pb", true)];
        assert_eq!(missing_departures(&d, &[close(1, "pa")]), vec!["B"]);
        assert!(missing_departures(&d, &[close(1, "pa"), close(2, "pb")]).is_empty());
        // Another participant's close does not count for B.
        assert_eq!(
            missing_departures(&d, &[close(1, "pa"), close(2, "px")]),
            vec!["B"]
        );
    }

    #[test]
    fn the_anchor_is_the_newest_departed_close_and_never_absent() {
        let d = [dep("A", "pa", true), dep("B", "pb", true)];
        // (a) No anchor line: an error, never "accept any".
        assert!(newest_close_ns(&d, &[]).is_err());
        assert!(newest_close_ns(&d, &[close(50, "px")]).is_err());
        // (c) Newest of OUR closes, ignoring a foreign one.
        assert_eq!(
            newest_close_ns(&d, &[close(10, "pa"), close(30, "pb"), close(90, "px")]),
            Ok(30)
        );
    }

    #[test]
    fn an_end_before_the_scope_is_not_this_meeting() {
        // (b) An end older than the scope start (the participants joined at 20).
        assert!(pick_end(&[end(10, "empty", "last_participant_left")], 20, 30).is_none());
        let got = pick_end(
            &[
                end(10, "empty", "last_participant_left"),
                end(35, "empty", "last_participant_left"),
            ],
            20,
            30,
        );
        assert_eq!(got.unwrap().unwrap().ts_ns, 35);
    }

    #[test]
    fn an_end_before_the_last_clean_close_fails() {
        // In scope (>= 20) but before the newest clean close (30): MC ended the
        // meeting with a participant still in it.
        let err = pick_end(&[end(25, "empty", "last_participant_left")], 20, 30)
            .unwrap()
            .unwrap_err();
        assert!(err.contains("BEFORE"), "{err}");
    }

    #[test]
    fn exactly_one_end_with_the_last_leave_cause() {
        assert!(pick_end(&[end(31, "empty", "join_window_expired")], 20, 30)
            .unwrap()
            .is_err());
        assert!(pick_end(&[end(31, "closed", "closed")], 20, 30)
            .unwrap()
            .is_err());
        assert!(pick_end(
            &[
                end(31, "empty", "last_participant_left"),
                end(32, "empty", "last_participant_left")
            ],
            20,
            30
        )
        .unwrap()
        .is_err());
    }

    #[test]
    fn the_release_must_be_complete_and_non_vacuous() {
        assert!(released_verdict(&[], 10, true).is_none());
        // Before the end: not this release.
        assert!(released_verdict(&[rel(5, 2, 2, 0, false)], 10, true).is_none());
        assert!(released_verdict(&[rel(11, 2, 2, 0, false)], 10, true)
            .unwrap()
            .is_ok());
        // failed > 0.
        assert!(released_verdict(&[rel(11, 2, 1, 1, false)], 10, true)
            .unwrap()
            .is_err());
        // released < handlers with failed == 0 (isolated).
        assert!(released_verdict(&[rel(11, 2, 1, 0, false)], 10, true)
            .unwrap()
            .is_err());
        // Quiesce timed out.
        assert!(released_verdict(&[rel(11, 2, 2, 0, true)], 10, true)
            .unwrap()
            .is_err());
        // Empty plan while a participant saw handlers: vacuous, so an error.
        assert!(released_verdict(&[rel(11, 0, 0, 0, false)], 10, true)
            .unwrap()
            .is_err());
        // Empty plan when nobody saw a handler (zombie with no replacement).
        assert!(released_verdict(&[rel(11, 0, 0, 0, false)], 10, false)
            .unwrap()
            .is_ok());
        // Two releases.
        assert!(released_verdict(
            &[rel(11, 1, 1, 0, false), rel(12, 1, 1, 0, false)],
            10,
            true
        )
        .unwrap()
        .is_err());
        // Missing fields are not a pass.
        assert!(
            released_verdict(&[entry(11, serde_json::json!({}))], 10, false)
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn any_grace_removal_is_reported_departed_by_label_others_by_count() {
        let d = [dep("A", "pa", true), dep("B", "pb", true)];
        assert_eq!(grace_removed(&d, &[close(1, "pb")]), (vec!["B"], 0));
        // A participant the caller never passed (forgotten, dropped): counted.
        assert_eq!(grace_removed(&d, &[close(1, "px")]), (vec![], 1));
        assert_eq!(grace_removed(&d, &[]), (vec![], 0));
    }
}
