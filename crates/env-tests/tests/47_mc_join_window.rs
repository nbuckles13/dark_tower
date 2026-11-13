//! Env-test: MC ends a meeting nobody joins, at its join window (join-window
//! devloop, 2026-11-12; RULING: an MC meeting exists only while it has
//! participants).
//!
//! ONE meeting, created by this test and GC-joined once (GC assigns it, which
//! creates MC's meeting actor) but NEVER connected to MC. Every piece of
//! evidence is scoped to THIS meeting's id in Loki — no global counter, no
//! waiting for global state to settle:
//!
//! 1. Positive control: MC logged "Meeting actor created" for this id.
//! 2. MC's meeting actor logged its join-window end for this id, carrying the
//!    meeting's age on MC's MONOTONIC clock (`age_ms`, from the same `Instant`
//!    the deadline is anchored to) and MC's `lifetime_tick_ms`; the test
//!    asserts `window <= age <= window + one tick` — the task's bound — on
//!    THAT field (option 1, operator direction 2026-11-13). Wall-clock Loki
//!    timestamps are diagnostics in the failure message only: on the WSL2 host
//!    they can disagree with MC's monotonic clock by seconds. The controller's
//!    end line then carries `cause="empty"`, `reason="join_window_expired"`.
//! 3. GC logged "Ended meeting assignment(s)" for this id at or after MC's end
//!    (GC processed `NotifyMeetingEnded`) — the gate for the re-join, so the
//!    re-join cannot race the teardown onto the old assignment.
//! 4. A fresh GC join + MC join is admitted, and MC logged a SECOND
//!    "Meeting actor created" for this id after the end: a new incarnation.
//!
//! The window is read from MC's `mc_meeting_join_window_seconds` config echo,
//! never a literal: the Kind overlay is its one home.
//!
//! Loki unavailable is a FAILURE (PRECONDITION), never a skip.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "flows")]

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use env_tests::cluster::ClusterConnection;
use env_tests::eventual::ConsistencyCategory;
use env_tests::fixtures::gc_client::GcClient;
use env_tests::fixtures::kube::settled_serving_pods;
use env_tests::fixtures::loki::{query_range, LokiEntry};
use env_tests::fixtures::metrics::{gauge_by_pods, PrometheusClient, PINNED_METRIC_BOUND};
use env_tests::fixtures::participant::{mc_join, Invited};
use env_tests::fixtures::AuthClient;

// ---------------------------------------------------------------------------
// THE log lines this test matches: target + message, each citing its emitter.
// A reworded or retargeted line fails as "created line never seen" (or its
// sibling token) with this hint, never as a vacuous pass.
// ---------------------------------------------------------------------------

/// `crates/mc-service/src/actors/controller.rs` (`create_meeting`, the
/// "Meeting actor created" INFO, ~:868).
const MC_CREATED: LogLine = LogLine {
    app: "mc-service",
    target: "mc.actor.controller",
    message: "Meeting actor created",
};

/// `crates/mc-service/src/actors/meeting.rs` (`on_join_window_deadline`):
/// the actor's own join-window end, carrying `window_seconds`, `age_ms` and
/// `lifetime_tick_ms`.
const MC_WINDOW_FIRED: LogLine = LogLine {
    app: "mc-service",
    target: "mc.actor.meeting",
    message: "No participant joined within the join window; ending the meeting",
};

/// `crates/mc-service/src/actors/controller.rs` (`reap_ended`, ~:1268): one
/// per meeting end, carrying `cause`, `reason` and `age_ms`.
const MC_ENDED: LogLine = LogLine {
    app: "mc-service",
    target: "mc.actor.controller",
    message: "Meeting ended; releasing it on its media handlers",
};

/// `crates/gc-service/src/services/mc_assignment.rs` (`end_assignment`,
/// ~:119): emitted only when GC actually ended a live row for this meeting.
const GC_ENDED: LogLine = LogLine {
    app: "gc-service",
    target: "gc.service.assignment",
    message: "Ended meeting assignment(s)",
};

/// The largest join window this test will WAIT for. The Kind overlay
/// (`infra/kubernetes/overlays/kind/services/mc-service/configmap-join-window-patch.yaml`)
/// sets 10 s precisely so this test fits the Layer-7 budget (it adds about one
/// window to every run). It sits BELOW the production base value (30 s,
/// `infra/services/mc-service/config.env`), so a window above it means the
/// Kind override did not reach the pods and the test fails at once, naming
/// that, instead of eating the suite timeout (the Gate-2 L7 failure of the
/// join-window devloop).
const LAYER7_WINDOW_BUDGET: Duration = Duration::from_secs(20);

#[derive(Clone, Copy)]
struct LogLine {
    app: &'static str,
    target: &'static str,
    message: &'static str,
}

/// One matched entry: its Loki timestamp (ns) and parsed JSON line.
struct Entry {
    ts_ns: u128,
    json: serde_json::Value,
}

impl Entry {
    fn field_u64(&self, name: &str) -> Option<u64> {
        self.json
            .get("fields")
            .and_then(|f| f.get(name))
            .and_then(serde_json::Value::as_u64)
    }

    fn field(&self, name: &str) -> Option<&str> {
        self.json
            .get("fields")
            .and_then(|f| f.get(name))
            .and_then(serde_json::Value::as_str)
    }
}

fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos()
}

/// What every Loki read in this test is scoped to: THIS meeting's id, since
/// just before its GC join.
struct Scope<'a> {
    cluster: &'a ClusterConnection,
    loki: String,
    meeting_id: String,
    since_ns: u128,
}

impl Scope<'_> {
    /// Every entry of `line` for this meeting, oldest first. Matching is
    /// structural (JSON `target`, `fields.message`, `fields.meeting_id`); the
    /// LogQL line filters only narrow the scan.
    async fn entries(&self, line: LogLine) -> Vec<Entry> {
        let meeting_id = self.meeting_id.as_str();
        let query = format!(
            "{{app=\"{}\"}} |= \"{meeting_id}\" |= \"{}\"",
            line.app, line.message
        );
        // A query FAILURE fails at once with its own token — never read as
        // "no line yet" (which would surface as a content failure after the
        // whole poll ceiling).
        let raw = query_range(
            self.cluster,
            &self.loki,
            &query,
            self.since_ns,
            now_ns(),
            100,
        )
        .await
        .unwrap_or_else(|e| panic!("loki-query-failed: `{query}` for meeting {meeting_id}: {e}"));
        raw.into_iter()
            // A line that is not JSON is not one of the matched (JSON) lines.
            .filter_map(|LokiEntry { ts_ns, line }| {
                serde_json::from_str(&line)
                    .ok()
                    .map(|json| Entry { ts_ns, json })
            })
            .filter(|e| {
                e.json.get("target").and_then(serde_json::Value::as_str) == Some(line.target)
                    && e.field("message") == Some(line.message)
                    && e.field("meeting_id") == Some(meeting_id)
            })
            .collect()
    }

    /// Poll until `pick` finds an entry, or fail with `token` after `ceiling`
    /// (a FAILURE bound only — never the correctness bound).
    async fn await_entry(
        &self,
        line: LogLine,
        ceiling: Duration,
        token: &str,
        pick: impl Fn(&[Entry]) -> Option<usize>,
    ) -> Entry {
        let deadline = Instant::now() + ceiling;
        loop {
            let mut found = self.entries(line).await;
            if let Some(i) = pick(&found) {
                return found.swap_remove(i);
            }
            assert!(
                Instant::now() < deadline,
                "{token}: no `{}` line (target `{}`, app `{}`) for meeting {} within \
                 {ceiling:?}. If the line was reworded or retargeted, update the const block at \
                 the top of 47_mc_join_window.rs; otherwise see the token.",
                line.message,
                line.target,
                line.app,
                self.meeting_id
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

fn delta(later_ns: u128, earlier_ns: u128) -> Duration {
    Duration::from_nanos(u64::try_from(later_ns.saturating_sub(earlier_ns)).unwrap_or(u64::MAX))
}

#[tokio::test]
async fn a_never_joined_meeting_ends_at_its_join_window_and_a_rejoin_gets_a_fresh_one() {
    let started = Instant::now();
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    assert!(
        cluster.is_loki_available().await,
        "PRECONDITION-loki-unavailable: this test's evidence is per-meeting Loki lines; \
         Loki must be up (an unavailable Loki FAILS this test, never skips it)"
    );
    let loki = cluster.loki_base_url.clone().expect("Loki URL is set");

    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    // The window every MC pod enforces, read from its config echo — and the
    // SAME on every pod: the bound is applied to the meeting on ONE pod, so a
    // per-pod disagreement (mid-rollout, drift) would check the wrong window.
    let (_, mc_pods) = settled_serving_pods(
        "app=mc-service",
        Duration::from_secs(60),
        "mc-pods-unserving",
    )
    .await;
    let windows = gauge_by_pods(
        &prom,
        "mc_meeting_join_window_seconds",
        &mc_pods,
        PINNED_METRIC_BOUND,
    )
    .await;
    let first = windows.values().next().copied().unwrap_or(0.0);
    assert!(
        first > 0.0 && windows.values().all(|v| (v - first).abs() < f64::EPSILON),
        "PRECONDITION-window-disagrees: every MC pod must publish one positive \
         mc_meeting_join_window_seconds; got {windows:?}"
    );
    let window = Duration::from_secs_f64(first);
    assert!(
        window <= LAYER7_WINDOW_BUDGET,
        "PRECONDITION-window-exceeds-budget: MC enforces a {window:?} join window, above this \
         test's Layer-7 budget {LAYER7_WINDOW_BUDGET:?}. The Kind override \
         (configmap-join-window-patch.yaml) is not reaching the pods: check the \
         MC_MEETING_JOIN_WINDOW_SECONDS configMapKeyRef in BOTH mc-N-deployment.yaml and the \
         rendered ConfigMap (`kubectl kustomize infra/kubernetes/overlays/kind`)."
    );
    let ingest = ConsistencyCategory::LogAggregation.timeout();

    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let invited = Invited::new(&auth, &gc, "Join window", 1).await;
    // Slack before the GC join so the creation line is inside the range.
    let since_ns = now_ns().saturating_sub(Duration::from_secs(5).as_nanos());

    // GC assigns the meeting (MC creates its actor) — and nobody connects.
    // The TEST's own monotonic clock, started before GC assigns the meeting:
    // an end can never be OBSERVED before it happened, so "observed at least
    // one window after this" is a lower bound MC's clock cannot satisfy for
    // us (independent of `age_ms` and of wall-clock skew).
    let gc_join_started = Instant::now();
    let first = invited.gc_join(0).await;
    let scope = Scope {
        cluster: &cluster,
        loki,
        meeting_id: first.meeting_id.to_string(),
        since_ns,
    };
    let meeting_id = scope.meeting_id.clone();

    // 1. Positive control: the actor was created (and ingested).
    let created = scope
        .await_entry(MC_CREATED, ingest, "created-line-never-seen", |es| {
            (!es.is_empty()).then_some(0)
        })
        .await;

    // 2. The actor's join-window end, bounded on MC's MONOTONIC age (option 1):
    //    window <= age <= window + one LIFETIME_TICK (the task's bound). MC
    //    reports both the age and its tick on the same line.
    let fired = scope
        .await_entry(
            MC_WINDOW_FIRED,
            window + LAYER7_WINDOW_BUDGET + ingest,
            "created-but-not-ended",
            |es| (!es.is_empty()).then_some(0),
        )
        .await;
    let observed_after = gc_join_started.elapsed();
    assert!(
        observed_after >= window,
        "ended-before-window-test-clock: MC's join-window end for meeting {meeting_id} was \
         observed {observed_after:?} after the test's GC join, less than the window {window:?}"
    );
    let (Some(age_ms), Some(tick_ms), Some(window_s)) = (
        fired.field_u64("age_ms"),
        fired.field_u64("lifetime_tick_ms"),
        fired.field_u64("window_seconds"),
    ) else {
        panic!(
            "end-line-missing-age: MC's join-window end line for meeting {meeting_id} lacks \
             age_ms / lifetime_tick_ms / window_seconds: {}",
            fired.json
        )
    };
    assert_eq!(
        Duration::from_secs(window_s),
        window,
        "PRECONDITION-window-disagrees: the end line's window_seconds differs from the gauge"
    );
    let age = Duration::from_millis(age_ms);
    let tick = Duration::from_millis(tick_ms);
    // Wall-clock view, DIAGNOSTIC ONLY (never the bound).
    let lived_wall = delta(fired.ts_ns, created.ts_ns);
    assert!(
        window <= age && age <= window + tick,
        "ended-outside-bound: meeting {meeting_id} ended at monotonic age {age:?}; the bound is \
         window {window:?} <= age <= window + LIFETIME_TICK {tick:?}. (Diagnostic only — \
         wall-clock Loki delta created→end = {lived_wall:?}, created ts {}, end ts {}.)",
        created.ts_ns,
        fired.ts_ns
    );

    // The controller's end line: cause and reason (and its own age field).
    let end = scope
        .await_entry(MC_ENDED, ingest, "ended-line-never-seen", |es| {
            (!es.is_empty()).then_some(0)
        })
        .await;
    assert_eq!(
        (end.field("cause"), end.field("reason")),
        (Some("empty"), Some("join_window_expired")),
        "ended-with-other-reason: meeting {meeting_id} ended, but not by its join window \
         (nobody connected, so nothing else should have ended it): {}",
        end.json
    );

    // 3. Gate: GC processed NotifyMeetingEnded for THIS meeting (presence only,
    //    at or after MC's end), so the re-join below takes GC's new-assignment
    //    path instead of reusing the ended row.
    scope
        .await_entry(GC_ENDED, ingest, "gc-never-ended-the-assignment", |es| {
            es.iter().position(|e| e.ts_ns >= end.ts_ns)
        })
        .await;

    // 4. A join after the end gets a FRESH meeting: GC re-assigns, MC admits,
    //    and MC logged a second creation for this id after the end.
    let again = invited.gc_join(0).await;
    assert_eq!(again.meeting_id, first.meeting_id);
    let participant = mc_join("join-window-rejoin", &again, invited.display(0)).await;
    scope
        .await_entry(MC_CREATED, ingest, "rejoin-not-a-fresh-incarnation", |es| {
            es.iter().position(|e| e.ts_ns > end.ts_ns)
        })
        .await;
    drop(participant);

    eprintln!(
        "[47_mc_join_window] window {window:?}; monotonic age at end {age:?} (tick {tick:?}); \
         wall-clock Loki delta {lived_wall:?}; test runtime {:?}",
        started.elapsed()
    );
}
