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
//! [`MetricAssertion::snapshot`] instantiates a fresh in-process test recorder
//! and binds it to the current thread via [`metrics::set_default_local_recorder`].
//! The returned [`MetricSnapshot`] owns the resulting `LocalRecorderGuard` and
//! a reference to the recorder. When the snapshot drops, the guard releases
//! the thread-local binding (restoring whatever recorder was previously in
//! scope).
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
//! The recorder is a hand-rolled string-keyed `HashMap` per metric kind —
//! deliberately not the upstream `metrics-util` `DebuggingRecorder`. The
//! upstream recorder shards storage by `available_parallelism().next_power_of_two()`
//! and surfaced an environment-dependent dedup loss in CI under PR #54
//! (different shard counts on the GitHub Actions runner versus local
//! WSL2). Rolling our own keeps the dedup logic on a single canonical
//! `name{k=v,...}` string with sorted labels — no shards, no fixed-key
//! AHash, no unsafe — so test outcomes are deterministic across
//! environments. Production code still flows through `metrics-rs` →
//! `metrics-exporter-prometheus`; this only owns the test path.
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
//! - The **outer** snapshot's reader records nothing for the overlap
//!   window, so its post-assert values under-report by the amount emitted
//!   during the overlap.
//!
//! Take one snapshot per test (or per discrete check) and let it drop
//! before taking another.
//!
//! # Delta semantics
//!
//! `assert_delta(N)` reads the counter value from the per-test recorder
//! and compares it to `N`. Because the recorder is brand-new when the
//! snapshot is taken, its pre-state is empty, so the observed post-value
//! *is* the delta:
//!
//! - **Counter absent from post-snapshot** → delta 0 (never emitted).
//!   `assert_delta(0)` passes; any other expectation panics.
//! - **Counter present with value V** → delta V; `assert_delta(V)` passes.
//!
//! There is no "present-pre / absent-post" invariant to police — the pre
//! state is always empty by construction, so the case cannot arise.
//!
//! **Counters and gauges are idempotent under repeat reads.** A snapshot
//! re-reads the underlying atomic value each time, so
//! `snap.counter(n).assert_delta(N)` and `snap.gauge(n).assert_value(V)`
//! can be called repeatedly on the same snapshot and will see the same
//! result until new emissions land.
//!
//! **Histograms DRAIN on snapshot.** The recorder stores histogram
//! observations in a buffer that is drained every time a snapshot is read.
//! Every `assert_observation_count*` call takes a fresh snapshot, so two
//! successive `assert_observation_count*` calls on the same histogram
//! name+labels within one test will see the emitted observations on the
//! first call and zero on the second. Assert each histogram name+labels
//! combination at most once per snapshot; emit more observations and/or
//! take a new `MetricAssertion::snapshot()` if you need to check a
//! subsequent window.
//!
//! # Unobserved semantics
//!
//! All three query types ([`CounterQuery`] / [`GaugeQuery`] /
//! [`HistogramQuery`]) ship an `assert_unobserved` method. The unifying
//! invariant across kinds is **kind-mismatch hardening** via
//! `ensure_no_kind_mismatch`: if the same metric `name` was recorded under a
//! DIFFERENT kind in this snapshot, `assert_unobserved` panics with a
//! redirect message (e.g. `"metric 'foo' was recorded as histogram, not
//! counter — use .histogram(\"foo\") instead"`). This catches the
//! load-bearing label-swap-bug class where a refactor accidentally re-emits
//! a metric under the wrong kind; the soft form
//! (`assert_delta(0)` / `assert_value(0.0)` / `assert_observation_count(0)`)
//! does NOT trip on kind-mismatch and would silently pass.
//!
//! Symmetry table — soft form (zero-value axis) vs hard form (kind+name+labels axis):
//!
//! | Kind      | Soft form              | Hard form           | Soft catches              | Hard catches                          |
//! |-----------|------------------------|---------------------|---------------------------|---------------------------------------|
//! | Counter   | `assert_delta(0)`      | `assert_unobserved` | wrong N>0 increment       | wrong call site (any) + wrong kind    |
//! | Gauge     | (no soft form¹)        | `assert_unobserved` | —                         | wrong call site + wrong kind          |
//! | Histogram | `assert_observation_count(0)` | `assert_unobserved` | wrong N>0 observations | wrong call site + wrong kind          |
//!
//! ¹ Gauge has no native "set-to-zero is indistinguishable from never-set"
//! idiom because gauges are inherently state-bearing — `set(0.0)` is an
//! explicit observation, not a "never touched" signal. This is why gauge
//! needed `assert_unobserved` first (ADR-0032 §F4 motivating use case).
//!
//! Per-kind asymmetry beyond the kind-mismatch invariant:
//!
//! - **Counter**: `assert_unobserved` is the hard form ("counter never
//!   registered"); `assert_delta(0)` is the soft form ("counter never
//!   accrued"). Both pass when the counter is genuinely absent; they diverge
//!   only on kind-mismatch detection.
//! - **Gauge**: `assert_unobserved` fills an actual gap in the API —
//!   `assert_value(0.0)` panics with "not observed" when the gauge has
//!   never been emitted, so there was no way to express "this gauge correctly
//!   received zero emissions" before. ADR-0032 §F4 motivating use case.
//! - **Histogram**: `assert_unobserved` is functionally equivalent to
//!   `assert_observation_count(0)` on the observation-count axis but adds
//!   kind-mismatch detection. Subject to the drain-on-read constraint above —
//!   a `histogram.assert_observation_count*` call earlier in the test
//!   silently empties the buffer, so `histogram.assert_unobserved` after
//!   that point falsely passes. Either call `assert_unobserved` BEFORE any
//!   `assert_observation_count*` on the same name+labels in this snapshot,
//!   or take a fresh `MetricAssertion::snapshot()` between assertions.
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

use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, LocalRecorderGuard,
    Metadata, Recorder, SharedString, Unit,
};
use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

// =============================================================================
// In-process test recorder.
// =============================================================================
//
// One `Mutex<HashMap>` per metric kind, keyed by a canonical `name{k=v,...}`
// string with sorted labels. `register_*` does get-or-insert and clones an
// `Arc` of the underlying storage into the returned handle, so two
// registrations for the same logical key share one atomic / one observation
// buffer. See module §"Isolation model" for why we don't use upstream
// `DebuggingRecorder`.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MetricKind {
    Counter,
    Gauge,
    Histogram,
}

struct CounterEntry {
    name: String,
    labels: Vec<(String, String)>,
    value: Arc<AtomicU64>,
}

struct GaugeEntry {
    name: String,
    labels: Vec<(String, String)>,
    // Stores `f64::to_bits` — atomic load/store/CAS over the bit pattern.
    bits: Arc<AtomicU64>,
}

struct HistogramEntry {
    name: String,
    labels: Vec<(String, String)>,
    observations: Arc<Mutex<Vec<f64>>>,
}

struct TestRecorder {
    counters: Mutex<HashMap<String, CounterEntry>>,
    gauges: Mutex<HashMap<String, GaugeEntry>>,
    histograms: Mutex<HashMap<String, HistogramEntry>>,
}

impl TestRecorder {
    fn new() -> Self {
        Self {
            counters: Mutex::new(HashMap::new()),
            gauges: Mutex::new(HashMap::new()),
            histograms: Mutex::new(HashMap::new()),
        }
    }
}

/// Build the canonical `name{k=v,...}` string for a `Key`. Labels are sorted
/// so two calls with the same content produce identical reprs regardless of
/// the order labels were emitted.
fn canonicalize_key(key: &Key) -> (String, Vec<(String, String)>) {
    let mut labels: Vec<(String, String)> = key
        .labels()
        .map(|l| (l.key().to_string(), l.value().to_string()))
        .collect();
    labels.sort();
    let labels_str = labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");
    (format!("{}{{{}}}", key.name(), labels_str), labels)
}

impl Recorder for TestRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        let (repr, labels) = canonicalize_key(key);
        let mut map = self
            .counters
            .lock()
            .expect("test recorder counters poisoned");
        let entry = map.entry(repr).or_insert_with(|| CounterEntry {
            name: key.name().to_string(),
            labels,
            value: Arc::new(AtomicU64::new(0)),
        });
        Counter::from_arc(Arc::new(TestCounter {
            value: entry.value.clone(),
        }))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        let (repr, labels) = canonicalize_key(key);
        let mut map = self.gauges.lock().expect("test recorder gauges poisoned");
        let entry = map.entry(repr).or_insert_with(|| GaugeEntry {
            name: key.name().to_string(),
            labels,
            bits: Arc::new(AtomicU64::new(0)),
        });
        Gauge::from_arc(Arc::new(TestGauge {
            bits: entry.bits.clone(),
        }))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        let (repr, labels) = canonicalize_key(key);
        let mut map = self
            .histograms
            .lock()
            .expect("test recorder histograms poisoned");
        let entry = map.entry(repr).or_insert_with(|| HistogramEntry {
            name: key.name().to_string(),
            labels,
            observations: Arc::new(Mutex::new(Vec::new())),
        });
        Histogram::from_arc(Arc::new(TestHistogram {
            observations: entry.observations.clone(),
        }))
    }
}

struct TestCounter {
    value: Arc<AtomicU64>,
}

impl CounterFn for TestCounter {
    fn increment(&self, value: u64) {
        self.value.fetch_add(value, Ordering::SeqCst);
    }
    fn absolute(&self, value: u64) {
        self.value.store(value, Ordering::SeqCst);
    }
}

struct TestGauge {
    bits: Arc<AtomicU64>,
}

impl GaugeFn for TestGauge {
    fn increment(&self, value: f64) {
        let mut cur = self.bits.load(Ordering::SeqCst);
        loop {
            let new = (f64::from_bits(cur) + value).to_bits();
            match self
                .bits
                .compare_exchange_weak(cur, new, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return,
                Err(observed) => cur = observed,
            }
        }
    }
    fn decrement(&self, value: f64) {
        let mut cur = self.bits.load(Ordering::SeqCst);
        loop {
            let new = (f64::from_bits(cur) - value).to_bits();
            match self
                .bits
                .compare_exchange_weak(cur, new, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return,
                Err(observed) => cur = observed,
            }
        }
    }
    fn set(&self, value: f64) {
        self.bits.store(value.to_bits(), Ordering::SeqCst);
    }
}

struct TestHistogram {
    observations: Arc<Mutex<Vec<f64>>>,
}

impl HistogramFn for TestHistogram {
    fn record(&self, value: f64) {
        self.observations
            .lock()
            .expect("test recorder histogram observations poisoned")
            .push(value);
    }
}

// =============================================================================
// Snapshot entries.
// =============================================================================

#[derive(Debug)]
enum EntryValue {
    Counter(u64),
    Gauge(f64),
    Histogram(Vec<f64>),
}

#[derive(Debug)]
struct EntryKey {
    kind: MetricKind,
    name: String,
    labels: Vec<(String, String)>,
}

// =============================================================================
// Public API
// =============================================================================

/// Namespace for constructing [`MetricSnapshot`] instances.
pub struct MetricAssertion;

impl MetricAssertion {
    /// Bind a fresh test recorder to the current thread and return a
    /// [`MetricSnapshot`] that captures every metric emission on this thread
    /// until the snapshot is dropped.
    ///
    /// Must be called **before** the code under test. Values queried via
    /// [`MetricSnapshot::counter`] / [`MetricSnapshot::gauge`] /
    /// [`MetricSnapshot::histogram`] read from this thread's per-snapshot
    /// recorder, not any process-global one.
    pub fn snapshot() -> MetricSnapshot {
        let recorder: &'static TestRecorder = Box::leak(Box::new(TestRecorder::new()));
        let guard = metrics::set_default_local_recorder(recorder);
        MetricSnapshot {
            _guard: guard,
            recorder,
        }
    }
}

/// A per-thread captured recorder binding. While this value is alive, all
/// metric emissions on the current thread route through the owned test
/// recorder; on drop, the thread-local binding is released.
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
    recorder: &'static TestRecorder,
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

    /// Read every entry from the per-test recorder. Counters and gauges are
    /// loaded non-destructively; histograms are drained (see module doc
    /// §"Histograms DRAIN on snapshot").
    fn take_entries(&self) -> Vec<(EntryKey, EntryValue)> {
        let mut out = Vec::new();

        {
            let map = self
                .recorder
                .counters
                .lock()
                .expect("test recorder counters poisoned");
            for entry in map.values() {
                let v = entry.value.load(Ordering::SeqCst);
                out.push((
                    EntryKey {
                        kind: MetricKind::Counter,
                        name: entry.name.clone(),
                        labels: entry.labels.clone(),
                    },
                    EntryValue::Counter(v),
                ));
            }
        }

        {
            let map = self
                .recorder
                .gauges
                .lock()
                .expect("test recorder gauges poisoned");
            for entry in map.values() {
                let v = f64::from_bits(entry.bits.load(Ordering::SeqCst));
                out.push((
                    EntryKey {
                        kind: MetricKind::Gauge,
                        name: entry.name.clone(),
                        labels: entry.labels.clone(),
                    },
                    EntryValue::Gauge(v),
                ));
            }
        }

        {
            let map = self
                .recorder
                .histograms
                .lock()
                .expect("test recorder histograms poisoned");
            for entry in map.values() {
                let mut obs = entry
                    .observations
                    .lock()
                    .expect("test recorder histogram observations poisoned");
                let drained: Vec<f64> = obs.drain(..).collect();
                out.push((
                    EntryKey {
                        kind: MetricKind::Histogram,
                        name: entry.name.clone(),
                        labels: entry.labels.clone(),
                    },
                    EntryValue::Histogram(drained),
                ));
            }
        }

        out
    }
}

/// Print all entries with `name` to stderr — used by assertion methods on
/// failure to give richer diagnostic context than the bare `expected/got`.
fn dump_failure_context(
    entries: &[(EntryKey, EntryValue)],
    name: &str,
    filter_labels: &[LabelFilter],
) {
    eprintln!("=== metric assertion failure on '{name}' (filter: {filter_labels:?}) ===");
    let mut matched = 0usize;
    for (ek, value) in entries {
        if ek.name == name {
            eprintln!(
                "  {:?} {} labels={:?} = {:?}",
                ek.kind, ek.name, ek.labels, value
            );
            matched += 1;
        }
    }
    eprintln!(
        "=== ({} entries with this name; {} entries total in snapshot) ===",
        matched,
        entries.len()
    );
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

/// Does `entry_labels` contain every (k, v) in `filter` (subset match)?
fn labels_match(entry_labels: &[(String, String)], filter: &[LabelFilter]) -> bool {
    filter.iter().all(|lf| {
        entry_labels
            .iter()
            .any(|(k, v)| k == &lf.key && v == &lf.value)
    })
}

fn find_of_kind<'a>(
    entries: &'a [(EntryKey, EntryValue)],
    kind: MetricKind,
    name: &str,
    filter: &[LabelFilter],
) -> Option<&'a EntryValue> {
    entries
        .iter()
        .find(|(ek, _)| ek.kind == kind && ek.name == name && labels_match(&ek.labels, filter))
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
    entries: &[(EntryKey, EntryValue)],
    name: &str,
    expected_kind: MetricKind,
) {
    let expected_present = entries
        .iter()
        .any(|(ek, _)| ek.name == name && ek.kind == expected_kind);
    if expected_present {
        return;
    }
    let wrong_kind = entries
        .iter()
        .find(|(ek, _)| ek.name == name && ek.kind != expected_kind)
        .map(|(ek, _)| ek.kind);
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
        if actual != expected {
            dump_failure_context(&entries, name, &labels);
        }
        assert_eq!(
            actual, expected,
            "counter '{name}' with labels {labels:?}: expected delta {expected}, got {actual}"
        );
    }

    /// Assert the counter was never registered for this (name, label-filter)
    /// tuple in this snapshot window.
    ///
    /// `assert_delta(0)` is the soft form ("counter never accrued" — passes
    /// whether the counter is absent OR present-with-value-zero).
    /// `assert_unobserved` is the hard form ("counter never registered" —
    /// passes only when absent from the snapshot entirely).
    ///
    /// Use the soft form for "this label combo accumulated zero increments"
    /// under partial-label adjacency. Use the hard form for "this code path
    /// did not touch this metric at all" — the load-bearing per-failure-class
    /// adjacency case for code paths that should be silent on a metric (see
    /// ADR-0032).
    ///
    /// Surfaces kind mismatches loudly: if the metric was recorded as a
    /// gauge or histogram under the same name, panics with a redirect to the
    /// correct query type.
    pub fn assert_unobserved(self) {
        let Self {
            snapshot,
            name,
            labels,
        } = self;
        let entries = snapshot.take_entries();
        ensure_no_kind_mismatch(&entries, name, MetricKind::Counter);
        if let Some(val) = counter_value(&entries, name, &labels) {
            panic!("counter '{name}' with labels {labels:?}: expected unobserved, got value {val}");
        }
    }
}

fn counter_value(
    entries: &[(EntryKey, EntryValue)],
    name: &str,
    filter: &[LabelFilter],
) -> Option<u64> {
    match find_of_kind(entries, MetricKind::Counter, name, filter)? {
        EntryValue::Counter(n) => Some(*n),
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

    /// Assert the gauge was never set for this (name, label-filter) tuple
    /// in this snapshot window.
    ///
    /// Fills the gap that `assert_value` / `assert_value_in_range` cannot
    /// express: those panic with "not observed" when the gauge is absent,
    /// which is the *opposite* of what failure-path adjacency tests need.
    /// Use `assert_unobserved` to prove a code path did NOT touch a gauge
    /// that other code paths set — the load-bearing per-failure-class
    /// adjacency case for gauges (ADR-0032).
    ///
    /// Surfaces kind mismatches loudly: if the metric was recorded as a
    /// counter or histogram under the same name, panics with a redirect.
    pub fn assert_unobserved(self) {
        let Self {
            snapshot,
            name,
            labels,
        } = self;
        let entries = snapshot.take_entries();
        ensure_no_kind_mismatch(&entries, name, MetricKind::Gauge);
        if let Some(val) = gauge_value(&entries, name, &labels) {
            panic!("gauge '{name}' with labels {labels:?}: expected unobserved, got value {val}");
        }
    }
}

fn gauge_value(
    entries: &[(EntryKey, EntryValue)],
    name: &str,
    filter: &[LabelFilter],
) -> Option<f64> {
    match find_of_kind(entries, MetricKind::Gauge, name, filter)? {
        EntryValue::Gauge(v) => Some(*v),
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
        if count != expected {
            dump_failure_context(&entries, name, &labels);
        }
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
        if count < expected {
            dump_failure_context(&entries, name, &labels);
        }
        assert!(
            count >= expected,
            "histogram '{name}' with labels {labels:?}: expected at least {expected} observations, got {count}"
        );
    }

    /// Assert the histogram was never recorded for this (name, label-filter)
    /// tuple in this snapshot window.
    ///
    /// `assert_observation_count(0)` is the soft form (passes whether the
    /// histogram is absent OR present-with-zero-observations).
    /// `assert_unobserved` is the hard form (passes only when absent from
    /// the snapshot entirely).
    ///
    /// Use the soft form for "this label combo accumulated zero observations"
    /// under partial-label adjacency. Use the hard form for "this code path
    /// did not touch this metric at all" — the load-bearing per-failure-class
    /// adjacency case (ADR-0032).
    ///
    /// **Drain-on-read caveat (load-bearing):** each `assert_observation_count*`
    /// call drains the histogram entries; a subsequent `assert_unobserved` on
    /// the same name+labels would falsely pass. Either: (a) call
    /// `assert_unobserved` BEFORE any `assert_observation_count*` on the same
    /// histogram name+labels in this snapshot, or (b) take a fresh
    /// [`MetricAssertion::snapshot`] between assertions. The `CounterQuery` and
    /// `GaugeQuery` `assert_unobserved` methods are not affected because
    /// counter/gauge values are idempotent on re-read.
    ///
    /// Surfaces kind mismatches loudly: if the metric was recorded as a
    /// counter or gauge under the same name, panics with a redirect.
    pub fn assert_unobserved(self) {
        let Self {
            snapshot,
            name,
            labels,
        } = self;
        let entries = snapshot.take_entries();
        ensure_no_kind_mismatch(&entries, name, MetricKind::Histogram);
        let count = histogram_count(&entries, name, &labels);
        assert!(
            count == 0,
            "histogram '{name}' with labels {labels:?}: expected unobserved, got {count} observations"
        );
    }
}

fn histogram_count(
    entries: &[(EntryKey, EntryValue)],
    name: &str,
    filter: &[LabelFilter],
) -> usize {
    entries
        .iter()
        .filter(|(ek, _)| {
            ek.kind == MetricKind::Histogram && ek.name == name && labels_match(&ek.labels, filter)
        })
        .map(|(_, v)| match v {
            EntryValue::Histogram(obs) => obs.len(),
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
    // TestRecorder to this thread for the snapshot's lifetime. Cargo
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

    // Two `register_counter` calls for the same key (e.g., two `counter!()`
    // macro invocations) must dedupe to a single underlying atomic, so two
    // `.increment(1)` calls accumulate to 2. This is the exact failure mode
    // that motivated rolling our own recorder under PR #54: upstream
    // `metrics-util 0.18` `DebuggingRecorder` lost one increment in CI.
    #[test]
    fn same_key_registered_twice_accumulates_increments() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_counter_repeat_register", "action" => "go").increment(1);
        counter!("testing_counter_repeat_register", "action" => "go").increment(1);
        snap.counter("testing_counter_repeat_register")
            .with_labels(&[("action", "go")])
            .assert_delta(2);
    }

    // -------------------------------------------------------------------------
    // assert_unobserved — counter / gauge / histogram (ADR-0032 Step 4)
    // -------------------------------------------------------------------------

    #[test]
    fn counter_assert_unobserved_passes_when_never_set() {
        let snap = MetricAssertion::snapshot();
        snap.counter("testing_counter_unobserved_never")
            .assert_unobserved();
    }

    #[test]
    #[should_panic(expected = "expected unobserved, got value 5")]
    fn counter_assert_unobserved_panics_when_set() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_counter_unobserved_set").increment(5);
        snap.counter("testing_counter_unobserved_set")
            .assert_unobserved();
    }

    #[test]
    fn counter_assert_unobserved_with_label_filter_ignores_other_labels() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_counter_unobserved_labels", "tenant" => "a").increment(1);
        // Filter targets a label that was never set — must pass even though
        // the metric was emitted under different labels.
        snap.counter("testing_counter_unobserved_labels")
            .with_labels(&[("tenant", "b")])
            .assert_unobserved();
    }

    #[test]
    fn counter_assert_unobserved_passes_when_other_label_set() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_counter_unobserved_other_label", "tenant" => "a").increment(7);
        snap.counter("testing_counter_unobserved_other_label")
            .with_labels(&[("tenant", "b")])
            .assert_unobserved();
    }

    #[test]
    fn gauge_assert_unobserved_passes_when_never_set() {
        let snap = MetricAssertion::snapshot();
        snap.gauge("testing_gauge_unobserved_never")
            .assert_unobserved();
    }

    #[test]
    #[should_panic(expected = "expected unobserved, got value 42")]
    fn gauge_assert_unobserved_panics_when_set() {
        let snap = MetricAssertion::snapshot();
        gauge!("testing_gauge_unobserved_set").set(42.0);
        snap.gauge("testing_gauge_unobserved_set")
            .assert_unobserved();
    }

    #[test]
    fn gauge_assert_unobserved_with_label_filter_ignores_other_labels() {
        let snap = MetricAssertion::snapshot();
        gauge!("testing_gauge_unobserved_labels", "tenant" => "a").set(1.0);
        snap.gauge("testing_gauge_unobserved_labels")
            .with_labels(&[("tenant", "b")])
            .assert_unobserved();
    }

    #[test]
    fn gauge_assert_unobserved_passes_when_other_label_set() {
        let snap = MetricAssertion::snapshot();
        gauge!("testing_gauge_unobserved_other_label", "tenant" => "a").set(99.0);
        snap.gauge("testing_gauge_unobserved_other_label")
            .with_labels(&[("tenant", "b")])
            .assert_unobserved();
    }

    #[test]
    fn histogram_assert_unobserved_passes_when_never_recorded() {
        let snap = MetricAssertion::snapshot();
        snap.histogram("testing_histogram_unobserved_never")
            .assert_unobserved();
    }

    #[test]
    #[should_panic(expected = "expected unobserved, got 1 observations")]
    fn histogram_assert_unobserved_panics_when_recorded() {
        let snap = MetricAssertion::snapshot();
        histogram!("testing_histogram_unobserved_set").record(1.0);
        snap.histogram("testing_histogram_unobserved_set")
            .assert_unobserved();
    }

    #[test]
    fn histogram_assert_unobserved_with_label_filter_ignores_other_labels() {
        let snap = MetricAssertion::snapshot();
        histogram!("testing_histogram_unobserved_labels", "tenant" => "a").record(1.0);
        snap.histogram("testing_histogram_unobserved_labels")
            .with_labels(&[("tenant", "b")])
            .assert_unobserved();
    }

    // Widened to also exercise drain-on-read interaction across labels:
    // draining label-A's observations must not falsely make label-B "appear"
    // observed (label scope is independent from drain).
    #[test]
    fn histogram_assert_unobserved_passes_when_other_label_set() {
        let snap = MetricAssertion::snapshot();
        histogram!("testing_histogram_unobserved_other_label", "tenant" => "a").record(1.0);
        snap.histogram("testing_histogram_unobserved_other_label")
            .with_labels(&[("tenant", "a")])
            .assert_observation_count(1);
        snap.histogram("testing_histogram_unobserved_other_label")
            .with_labels(&[("tenant", "b")])
            .assert_unobserved();
    }

    // PROOF OF TRAP: documents why histogram-first ordering matters for
    // assert_unobserved. After assert_observation_count(1) drains the
    // histogram, the entries are gone — assert_unobserved on the same
    // (name, labels) tuple FALSELY PASSES even though the histogram WAS
    // observed. This is intentional (lighter alternative to a runtime
    // tracking flag); the module doc-comment §"Unobserved semantics" and
    // §"Histograms DRAIN on snapshot" document the constraint and the
    // developer is expected to take a fresh MetricAssertion::snapshot()
    // between assertions. This test is non-panicking: it PASSES,
    // demonstrating the trap exists. A future refactor that makes
    // `assert_observation_count` idempotent would silently break this test
    // and surface the contract change at build time — that's the
    // executable-doc value of keeping this in code rather than doc-only.
    #[test]
    fn histogram_assert_unobserved_after_assert_observation_count_falsely_passes() {
        let snap = MetricAssertion::snapshot();
        histogram!("testing_histogram_drain_trap").record(1.0);
        snap.histogram("testing_histogram_drain_trap")
            .assert_observation_count(1);
        // PROOF OF TRAP: drain-on-read makes a subsequent assert_unobserved
        // see an empty buffer, so this PASSES even though the histogram WAS
        // observed. See module §"Unobserved semantics" for the contract.
        snap.histogram("testing_histogram_drain_trap")
            .assert_unobserved();
    }

    // Kind-mismatch hardening on the negative-assertion path. Parallel to the
    // existing `mismatched_metric_kind_*_panics_clearly` tests which prove
    // `ensure_no_kind_mismatch` works for `assert_delta` / `assert_value`.
    // These three prove the same hardening applies for `assert_unobserved` —
    // the load-bearing distinction over `assert_delta(0)` /
    // `assert_observation_count(0)` per @test reviewer. Without these, a
    // `counter!("foo")` accidentally emitted under a metric catalogued as a
    // histogram would silently pass `histogram("foo").
    // assert_observation_count(0)` and the regression would ship.

    #[test]
    #[should_panic(expected = "was recorded as histogram, not counter")]
    fn counter_assert_unobserved_panics_when_recorded_as_histogram() {
        let snap = MetricAssertion::snapshot();
        histogram!("testing_unobs_kind_mismatch_hist").record(1.0);
        snap.counter("testing_unobs_kind_mismatch_hist")
            .assert_unobserved();
    }

    #[test]
    #[should_panic(expected = "was recorded as counter, not gauge")]
    fn gauge_assert_unobserved_panics_when_recorded_as_counter() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_unobs_kind_mismatch_counter").increment(1);
        snap.gauge("testing_unobs_kind_mismatch_counter")
            .assert_unobserved();
    }

    #[test]
    #[should_panic(expected = "was recorded as counter, not histogram")]
    fn histogram_assert_unobserved_panics_when_recorded_as_counter() {
        let snap = MetricAssertion::snapshot();
        counter!("testing_unobs_kind_mismatch_for_hist").increment(1);
        snap.histogram("testing_unobs_kind_mismatch_for_hist")
            .assert_unobserved();
    }
}
