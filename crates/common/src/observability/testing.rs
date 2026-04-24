//! `MetricAssertion`: point-in-time metric-value assertions for component tests.
//!
//! This helper backs ADR-0032 §Tier B — a shared utility that lets component
//! tests capture a snapshot of all metrics, run the code under test, and then
//! assert that specific counters incremented, gauges reached an expected value,
//! or histograms received an expected number of observations.
//!
//! # Usage
//!
//! ```ignore
//! use common::observability::testing::MetricAssertion;
//!
//! let snap = MetricAssertion::snapshot();
//! run_code_under_test().await;
//!
//! // Counter delta with a label filter.
//! snap.counter("ac_errors_total")
//!     .with_labels(&[("operation", "create"), ("outcome", "error")])
//!     .assert_delta(1);
//!
//! // Unpredictable values (durations, sizes): assert observation count.
//! snap.histogram("mh_webtransport_handshake_duration_seconds")
//!     .assert_observation_count_at_least(1);
//!
//! // Gauges: point-in-time value or a range.
//! snap.gauge("mh_active_connections").assert_value_in_range(0.0..=10.0);
//! ```
//!
//! # Isolation model
//!
//! [`MetricAssertion::snapshot`] instantiates a fresh
//! [`metrics_util::debugging::DebuggingRecorder`] and binds it to the current
//! thread via [`metrics::set_default_local_recorder`]. The returned
//! [`MetricSnapshot`] owns the resulting `LocalRecorderGuard` and a
//! `Snapshotter` attached to that recorder. When the snapshot drops, the
//! guard releases the thread-local binding (restoring whatever recorder was
//! previously in scope).
//!
//! Every `#[test]` function gets a private recorder. Cargo runs tests on
//! separate threads by default, so two tests asserting on the same
//! `(metric name, labels)` tuple never observe each other's emissions. Name
//! collisions, label-tuple collisions, and ordering races between tests all
//! disappear.
//!
//! The recorder itself is intentionally leaked on the heap (`Box::leak`).
//! Allocation is bounded by the number of `MetricAssertion::snapshot()`
//! calls in a given test-process lifetime (a few KB across a CI run), and
//! the process exits at `cargo test` end so the OS reclaims everything.
//! This trade avoids a self-referential struct with a manual
//! `Box::from_raw` drop, which would require `unsafe` and careful
//! drop-order auditing for a purely test-side helper.
//!
//! # Parallel tests
//!
//! Snapshots are per-thread. **Do not move a `MetricSnapshot` across
//! threads.** `LocalRecorderGuard` is `!Send`, so `MetricSnapshot` is
//! `!Send` by derivation (the compiler enforces this). Hold the snapshot in
//! the test function's stack frame.
//!
//! Emissions from async or spawned work are captured only if that work runs
//! on the same OS thread as the snapshot. Concretely:
//!
//! - **Captured:** plain synchronous code on the test thread.
//!   `#[tokio::test]` (which defaults to the current-thread runtime) and
//!   `#[tokio::test(flavor = "current_thread")]` — `.await`-ed futures stay
//!   on the test thread, so emissions made from them are captured.
//! - **NOT captured:** `std::thread::spawn`, `tokio::spawn` on a
//!   multi-thread runtime, and `tokio::task::spawn_blocking` (which runs
//!   on tokio's blocking pool — a different OS thread). Metrics emitted
//!   from those contexts are invisible to a `MetricSnapshot` on the test
//!   thread; assert on them from the spawning context only, or restructure
//!   the code path to run on the test thread for the duration of the check.
//!
//! `#[serial_test::serial]` is not needed for parallel-safety under this
//! model and is not documented as a fallback.
//!
//! # Invariants — nested snapshots
//!
//! **Do not hold two `MetricSnapshot`s simultaneously on one thread.**
//! `metrics` 0.24 nested-guard semantics: each
//! `metrics::set_default_local_recorder` call swaps the thread-local
//! recorder pointer to the new recorder for the lifetime of the returned
//! guard, restoring the previous recorder when the guard drops. If you take
//! an inner snapshot while an outer one is still live:
//!
//! - Emissions during the overlap route to the **inner** recorder (the
//!   most recently bound one), not the outer.
//! - The **outer** snapshot's `Snapshotter` records nothing for the
//!   overlap window, so its post-assert values under-report by the amount
//!   emitted during the overlap.
//!
//! Take one snapshot per test (or per discrete check) and let it drop
//! before taking another.
//!
//! # Delta semantics
//!
//! `assert_delta(N)` reads the counter value from a fresh snapshot of the
//! per-test `Snapshotter` and compares it to `N`. Because the recorder is
//! brand-new when the snapshot is taken, its pre-state is empty, so the
//! observed post-value *is* the delta:
//!
//! - **Counter absent from post-snapshot** → delta 0 (never emitted).
//!   `assert_delta(0)` passes; any other expectation panics.
//! - **Counter present with value V** → delta V; `assert_delta(V)` passes.
//!
//! There is no "present-pre / absent-post" invariant to police — the pre
//! state is always empty by construction, so the case cannot arise.
//!
//! **Counters and gauges are idempotent under repeat reads.**
//! `Snapshotter::snapshot()` re-reads the underlying atomic value each
//! time, so `snap.counter(n).assert_delta(N)` and
//! `snap.gauge(n).assert_value(V)` can be called repeatedly on the same
//! snapshot and will see the same result until new emissions land.
//!
//! **Histograms DRAIN on snapshot.** `DebuggingRecorder` stores histogram
//! observations in a buffer that is drained every time `Snapshotter::snapshot()`
//! runs. Every `assert_observation_count*` call takes a fresh snapshot, so
//! two successive `assert_observation_count*` calls on the same histogram
//! name+labels within one test will see the emitted observations on the
//! first call and zero on the second. Assert each histogram name+labels
//! combination at most once per snapshot; emit more observations and/or
//! take a new `MetricAssertion::snapshot()` if you need to check a
//! subsequent window.
//!
//! # Security
//!
//! Do not record real tokens, PII, or other secrets into the recorder, even
//! in tests — the whole snapshot is held in memory and included verbatim in
//! panic messages on assertion failure.
//!
//! # `#[must_use]`
//!
//! [`MetricSnapshot`] and the [`CounterQuery`] / [`GaugeQuery`] /
//! [`HistogramQuery`] return types carry `#[must_use]`. This is an
//! ergonomic hint only: it catches "started a snapshot but never asserted."
//! It is NOT a load-bearing enforcement mechanism per ADR-0032
//! §Explicitly-rejected — the coverage guarantee comes from
//! `scripts/guards/simple/validate-metric-coverage.sh`, not this attribute.

// ADR-0002 disallows panic/assert/unwrap in production code. This module is a
// test-time assertion helper whose contract IS to panic on mismatch, and the
// whole module is gated behind `cfg(test)` / `feature = "test-utils"`, so the
// policy doesn't apply here. Using #[expect] (over #[allow]) per ADR-0002 so
// that if a future refactor ever removes the panics, the unfulfilled
// expectation surfaces at build time and someone revisits the suppression.
#![expect(
    clippy::panic,
    clippy::missing_panics_doc,
    reason = "test-only assertion helper; intentional panics on mismatch are the contract"
)]

use metrics::LocalRecorderGuard;
use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use metrics_util::{CompositeKey, MetricKind};
use std::ops::RangeInclusive;

/// Namespace for constructing [`MetricSnapshot`] instances.
pub struct MetricAssertion;

impl MetricAssertion {
    /// Bind a fresh `DebuggingRecorder` to the current thread and return a
    /// [`MetricSnapshot`] that captures every metric emission on this thread
    /// until the snapshot is dropped.
    ///
    /// Must be called **before** the code under test. Values queried via
    /// [`MetricSnapshot::counter`] / [`MetricSnapshot::gauge`] /
    /// [`MetricSnapshot::histogram`] read from this thread's per-snapshot
    /// recorder, not any process-global one.
    pub fn snapshot() -> MetricSnapshot {
        // Leak the recorder so `LocalRecorderGuard<'static>` can hold a
        // stable reference for the life of the snapshot. The allocation is
        // bounded by the number of `snapshot()` calls per test process
        // (kilobytes over a full CI run) and is reclaimed by the OS at
        // process exit. See module docs §"Isolation model" for the
        // rationale behind preferring the leak over a self-referential
        // struct with `unsafe { Box::from_raw }` in `Drop`.
        let recorder: &'static DebuggingRecorder = Box::leak(Box::new(DebuggingRecorder::new()));
        let snapshotter = recorder.snapshotter();
        let guard = metrics::set_default_local_recorder(recorder);
        MetricSnapshot {
            _guard: guard,
            snapshotter,
        }
    }
}

/// A per-thread captured recorder binding. While this value is alive, all
/// metric emissions on the current thread route through the owned
/// `DebuggingRecorder`; on drop, the thread-local binding is released.
///
/// **Not `Send`.** `LocalRecorderGuard` is `!Send`, so `MetricSnapshot` is
/// `!Send` by derivation. Hold it in the test function's stack frame and
/// assert on it from the same thread that took the snapshot.
#[must_use]
pub struct MetricSnapshot {
    // `_guard` is never read — it exists purely for its `Drop` side effect
    // of clearing the thread-local recorder slot. The underscore silences
    // the dead-code lint.
    _guard: LocalRecorderGuard<'static>,
    snapshotter: Snapshotter,
}

impl MetricSnapshot {
    /// Begin a counter assertion for `name`.
    pub fn counter<'a>(&'a self, name: &'static str) -> CounterQuery<'a> {
        CounterQuery {
            snapshot: self,
            name,
            labels: Vec::new(),
        }
    }

    /// Begin a gauge assertion for `name`.
    pub fn gauge<'a>(&'a self, name: &'static str) -> GaugeQuery<'a> {
        GaugeQuery {
            snapshot: self,
            name,
            labels: Vec::new(),
        }
    }

    /// Begin a histogram assertion for `name`.
    pub fn histogram<'a>(&'a self, name: &'static str) -> HistogramQuery<'a> {
        HistogramQuery {
            snapshot: self,
            name,
            labels: Vec::new(),
        }
    }

    /// Flatten a fresh snapshot from the per-test `Snapshotter` into an owned
    /// `Vec` of (key, value) pairs. Called by each assertion just before it
    /// checks its expectation.
    fn take_entries(&self) -> Vec<(CompositeKey, DebugValue)> {
        self.snapshotter
            .snapshot()
            .into_vec()
            .into_iter()
            .map(|(ck, _unit, _desc, value)| (ck, value))
            .collect()
    }
}

// =============================================================================
// Label-filtering machinery
// =============================================================================

#[derive(Debug, Clone)]
struct LabelFilter {
    key: String,
    value: String,
}

impl LabelFilter {
    fn from_pair((k, v): &(&str, &str)) -> Self {
        Self {
            key: (*k).to_string(),
            value: (*v).to_string(),
        }
    }
}

/// Does `key`'s labels contain every (k, v) in `filter` (subset match)?
fn labels_match(key: &metrics::Key, filter: &[LabelFilter]) -> bool {
    filter.iter().all(|lf| {
        key.labels()
            .any(|label| label.key() == lf.key && label.value() == lf.value)
    })
}

fn find_of_kind<'a>(
    entries: &'a [(CompositeKey, DebugValue)],
    kind: MetricKind,
    name: &str,
    filter: &[LabelFilter],
) -> Option<&'a DebugValue> {
    entries
        .iter()
        .find(|(ck, _)| {
            ck.kind() == kind && ck.key().name() == name && labels_match(ck.key(), filter)
        })
        .map(|(_, v)| v)
}

fn kind_name(kind: MetricKind) -> &'static str {
    match kind {
        MetricKind::Counter => "counter",
        MetricKind::Gauge => "gauge",
        MetricKind::Histogram => "histogram",
    }
}

/// Panic with a clear message if `name` is observed in `entries` under a
/// metric kind other than `expected_kind` AND `expected_kind` is not also
/// observed. Prefers an actionable "metric X is a histogram, not a counter"
/// diagnostic over a silent "value 0" failure.
fn ensure_no_kind_mismatch(
    entries: &[(CompositeKey, DebugValue)],
    name: &str,
    expected_kind: MetricKind,
) {
    let expected_present = entries
        .iter()
        .any(|(ck, _)| ck.key().name() == name && ck.kind() == expected_kind);
    if expected_present {
        return;
    }
    let wrong_kind = entries
        .iter()
        .find(|(ck, _)| ck.key().name() == name && ck.kind() != expected_kind)
        .map(|(ck, _)| ck.kind());
    if let Some(actual) = wrong_kind {
        panic!(
            "metric '{name}' was recorded as {}, not {} — use .{}(\"{name}\") instead",
            kind_name(actual),
            kind_name(expected_kind),
            kind_name(actual),
        );
    }
}

// =============================================================================
// CounterQuery
// =============================================================================

/// Builder for a counter assertion.
#[must_use]
pub struct CounterQuery<'a> {
    snapshot: &'a MetricSnapshot,
    name: &'static str,
    labels: Vec<LabelFilter>,
}

impl CounterQuery<'_> {
    /// Filter the counter lookup to entries whose labels are a superset
    /// of `pairs`.
    pub fn with_labels(mut self, pairs: &[(&str, &str)]) -> Self {
        self.labels = pairs.iter().map(LabelFilter::from_pair).collect();
        self
    }

    /// Assert that the counter's value equals `expected`.
    ///
    /// Because the per-test recorder starts with an empty state, the
    /// observed value *is* the delta accrued during this test. Two cases:
    ///
    /// - The counter was never emitted this test → treated as 0;
    ///   `assert_delta(0)` passes, any other expectation panics.
    /// - The counter was emitted with value V → `assert_delta(V)` passes.
    pub fn assert_delta(self, expected: u64) {
        let Self {
            snapshot,
            name,
            labels,
        } = self;
        let entries = snapshot.take_entries();
        ensure_no_kind_mismatch(&entries, name, MetricKind::Counter);

        let actual = counter_value(&entries, name, &labels).unwrap_or(0);
        assert_eq!(
            actual, expected,
            "counter '{name}' with labels {labels:?}: expected delta {expected}, got {actual}"
        );
    }
}

fn counter_value(
    entries: &[(CompositeKey, DebugValue)],
    name: &str,
    filter: &[LabelFilter],
) -> Option<u64> {
    match find_of_kind(entries, MetricKind::Counter, name, filter)? {
        DebugValue::Counter(n) => Some(*n),
        _ => None,
    }
}

// =============================================================================
// GaugeQuery
// =============================================================================

/// Builder for a gauge assertion.
#[must_use]
pub struct GaugeQuery<'a> {
    snapshot: &'a MetricSnapshot,
    name: &'static str,
    labels: Vec<LabelFilter>,
}

impl GaugeQuery<'_> {
    /// Filter by label subset.
    pub fn with_labels(mut self, pairs: &[(&str, &str)]) -> Self {
        self.labels = pairs.iter().map(LabelFilter::from_pair).collect();
        self
    }

    /// Assert the gauge's current value equals `expected` (within `f64::EPSILON`).
    pub fn assert_value(self, expected: f64) {
        let Self {
            snapshot,
            name,
            labels,
        } = self;
        let entries = snapshot.take_entries();
        ensure_no_kind_mismatch(&entries, name, MetricKind::Gauge);
        match gauge_value(&entries, name, &labels) {
            Some(val) => assert!(
                (val - expected).abs() < f64::EPSILON,
                "gauge '{name}' with labels {labels:?}: expected value {expected}, got {val}"
            ),
            None => panic!("gauge '{name}' with labels {labels:?}: not observed"),
        }
    }

    /// Assert the gauge's current value falls within `range` (inclusive).
    pub fn assert_value_in_range(self, range: RangeInclusive<f64>) {
        let Self {
            snapshot,
            name,
            labels,
        } = self;
        let entries = snapshot.take_entries();
        ensure_no_kind_mismatch(&entries, name, MetricKind::Gauge);
        match gauge_value(&entries, name, &labels) {
            Some(val) => assert!(
                range.contains(&val),
                "gauge '{name}' with labels {labels:?}: value {val} outside expected range {range:?}"
            ),
            None => panic!("gauge '{name}' with labels {labels:?}: not observed"),
        }
    }
}

fn gauge_value(
    entries: &[(CompositeKey, DebugValue)],
    name: &str,
    filter: &[LabelFilter],
) -> Option<f64> {
    match find_of_kind(entries, MetricKind::Gauge, name, filter)? {
        DebugValue::Gauge(ord) => Some((*ord).into_inner()),
        _ => None,
    }
}

// =============================================================================
// HistogramQuery
// =============================================================================

/// Builder for a histogram assertion.
#[must_use]
pub struct HistogramQuery<'a> {
    snapshot: &'a MetricSnapshot,
    name: &'static str,
    labels: Vec<LabelFilter>,
}

impl HistogramQuery<'_> {
    /// Filter by label subset.
    pub fn with_labels(mut self, pairs: &[(&str, &str)]) -> Self {
        self.labels = pairs.iter().map(LabelFilter::from_pair).collect();
        self
    }

    /// Assert that exactly `expected` observations are present in the
    /// snapshot.
    pub fn assert_observation_count(self, expected: usize) {
        let Self {
            snapshot,
            name,
            labels,
        } = self;
        let entries = snapshot.take_entries();
        ensure_no_kind_mismatch(&entries, name, MetricKind::Histogram);
        let count = histogram_count(&entries, name, &labels);
        assert_eq!(
            count, expected,
            "histogram '{name}' with labels {labels:?}: expected {expected} observations, got {count}"
        );
    }

    /// Assert that at least `expected` observations are present.
    ///
    /// Prefer this over `assert_observation_count` when the exact number is
    /// not deterministic (retries, concurrent paths).
    pub fn assert_observation_count_at_least(self, expected: usize) {
        let Self {
            snapshot,
            name,
            labels,
        } = self;
        let entries = snapshot.take_entries();
        ensure_no_kind_mismatch(&entries, name, MetricKind::Histogram);
        let count = histogram_count(&entries, name, &labels);
        assert!(
            count >= expected,
            "histogram '{name}' with labels {labels:?}: expected at least {expected} observations, got {count}"
        );
    }
}

fn histogram_count(
    entries: &[(CompositeKey, DebugValue)],
    name: &str,
    filter: &[LabelFilter],
) -> usize {
    entries
        .iter()
        .filter(|(ck, _)| {
            ck.kind() == MetricKind::Histogram
                && ck.key().name() == name
                && labels_match(ck.key(), filter)
        })
        .map(|(_, v)| match v {
            DebugValue::Histogram(obs) => obs.len(),
            _ => 0,
        })
        .sum()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use metrics::{counter, gauge, histogram};

    // Each test takes its own MetricSnapshot, which binds a fresh
    // DebuggingRecorder to this thread for the snapshot's lifetime. Cargo
    // runs #[test] fns on separate threads, so tests are parallel-safe
    // without #[serial] attributes and without name-uniqueness helpers.

    #[test]
    fn counter_delta_simple_increment() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_counter_simple").increment(3);
        snap.counter("testing_counter_simple").assert_delta(3);
    }

    #[test]
    fn counter_delta_zero_when_never_observed() {
        let snap = MetricAssertion::snapshot();
        snap.counter("testing_counter_never").assert_delta(0);
    }

    #[test]
    fn counter_delta_when_metric_first_observed_during_run() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_counter_first").increment(7);
        snap.counter("testing_counter_first").assert_delta(7);
    }

    #[test]
    fn counter_with_matching_labels() {
        let snap = MetricAssertion::snapshot();
        counter!(
            "testing_counter_labels",
            "operation" => "create",
            "outcome" => "error",
        )
        .increment(1);
        snap.counter("testing_counter_labels")
            .with_labels(&[("operation", "create"), ("outcome", "error")])
            .assert_delta(1);
    }

    #[test]
    #[should_panic(expected = "expected delta 1")]
    fn counter_with_non_matching_label_filter_reports_delta_zero() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_counter_label_miss", "operation" => "create").increment(1);
        snap.counter("testing_counter_label_miss")
            .with_labels(&[("operation", "delete")])
            .assert_delta(1);
    }

    #[test]
    fn gauge_assert_value_exact() {
        let snap = MetricAssertion::snapshot();
        gauge!("testing_gauge_exact").set(42.0);
        snap.gauge("testing_gauge_exact").assert_value(42.0);
    }

    #[test]
    fn gauge_assert_value_in_range() {
        let snap = MetricAssertion::snapshot();
        gauge!("testing_gauge_range").set(5.0);
        snap.gauge("testing_gauge_range")
            .assert_value_in_range(0.0..=10.0);
    }

    #[test]
    fn histogram_observation_count_exact() {
        let snap = MetricAssertion::snapshot();
        histogram!("testing_histogram_exact").record(1.0);
        histogram!("testing_histogram_exact").record(2.0);
        histogram!("testing_histogram_exact").record(3.0);
        snap.histogram("testing_histogram_exact")
            .assert_observation_count(3);
    }

    #[test]
    fn histogram_observation_count_at_least() {
        let snap = MetricAssertion::snapshot();
        histogram!("testing_histogram_at_least").record(1.0);
        histogram!("testing_histogram_at_least").record(2.0);
        snap.histogram("testing_histogram_at_least")
            .assert_observation_count_at_least(1);
    }

    #[test]
    fn labels_disambiguate_same_name_metrics_in_one_snapshot() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_counter_tuple_iso", "tenant" => "a").increment(1);
        counter!("testing_counter_tuple_iso", "tenant" => "b").increment(5);
        snap.counter("testing_counter_tuple_iso")
            .with_labels(&[("tenant", "a")])
            .assert_delta(1);
        snap.counter("testing_counter_tuple_iso")
            .with_labels(&[("tenant", "b")])
            .assert_delta(5);
    }

    #[test]
    #[should_panic(expected = "was recorded as histogram, not counter")]
    fn mismatched_metric_kind_histogram_vs_counter_panics_clearly() {
        let snap = MetricAssertion::snapshot();
        histogram!("testing_kind_mismatch_hist").record(1.0);
        snap.counter("testing_kind_mismatch_hist").assert_delta(1);
    }

    #[test]
    #[should_panic(expected = "was recorded as gauge, not counter")]
    fn mismatched_metric_kind_gauge_vs_counter_panics_clearly() {
        let snap = MetricAssertion::snapshot();
        gauge!("testing_kind_mismatch_gauge").set(1.0);
        snap.counter("testing_kind_mismatch_gauge").assert_delta(1);
    }

    // Proves same-(metric, labels) tuples on two threads do not collide.
    // The two threads increment by 3 and 7 respectively; if isolation were
    // broken, at least one `assert_delta` would observe 10 and panic. The
    // `join().unwrap()` propagates any spawned-thread panic into the test
    // thread so the test actually fails on bleed.
    #[test]
    fn parallel_snapshots_on_different_threads_do_not_collide() {
        use std::thread;

        let t1 = thread::spawn(|| {
            let snap = MetricAssertion::snapshot();
            counter!(
                "testing_parallel_collision_check",
                "tenant" => "shared",
            )
            .increment(3);
            snap.counter("testing_parallel_collision_check")
                .with_labels(&[("tenant", "shared")])
                .assert_delta(3);
        });

        let t2 = thread::spawn(|| {
            let snap = MetricAssertion::snapshot();
            counter!(
                "testing_parallel_collision_check",
                "tenant" => "shared",
            )
            .increment(7);
            snap.counter("testing_parallel_collision_check")
                .with_labels(&[("tenant", "shared")])
                .assert_delta(7);
        });

        t1.join().unwrap();
        t2.join().unwrap();
    }
}
