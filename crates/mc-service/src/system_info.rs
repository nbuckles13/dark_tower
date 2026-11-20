//! Host CPU / memory readings for the comprehensive heartbeat (ADR-0023 Phase 6c).
//!
//! # The invariant
//!
//! A reading that was not measured is **absent**, never a placeholder `0`.
//! The comprehensive heartbeat's `cpu_usage_percent` / `memory_usage_percent`
//! are `optional float` on the wire: [`SystemReadings::to_wire`] maps every
//! unavailable reading to `None`, and every unavailable reading is loud: it
//! is counted in `mc_system_readings_unavailable_total{reading, reason}` and
//! logged (see [`SystemReadings::record_unavailable`]; `cpu_baseline_pending`
//! is logged at debug and not counted).
//!
//! # Why a long-lived sampler
//!
//! sysinfo computes CPU usage as a delta between two CPU refreshes at least
//! [`sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`] apart (and skips a refresh that
//! comes sooner). A `System` built fresh per heartbeat therefore never has a
//! baseline and reads ~0 on every call. [`SystemSampler`] keeps one `System`
//! for the life of the GC task, so each comprehensive tick reports the real
//! usage over the interval since the previous tick.
//!
//! # Failure handling
//!
//! sysinfo 0.40 made `System` construction fallible. A failed init is NOT
//! fatal: the readings are advisory telemetry and the MC serves meetings fine
//! without them, so the sampler warns, reports both readings as
//! [`ReadingUnavailable::InitFailed`], and retries init on the next sample.
//! Heartbeat health is not touched (a telemetry gap is not service unhealth).
//!
//! Classification of raw values is the pure `classify`, so every reason is
//! testable without a broken host.

use std::fmt;
use std::time::{Duration, Instant};

use sysinfo::{System, MINIMUM_CPU_UPDATE_INTERVAL};
use tracing::{debug, warn};

use crate::observability::metrics::record_system_reading_unavailable;

/// Which host reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// Global CPU usage percent.
    Cpu,
    /// Memory-in-use percent of host total.
    Memory,
}

impl Reading {
    /// Every reading, for zero-initialization.
    pub const ALL: [Self; 2] = [Self::Cpu, Self::Memory];

    /// The `reading` metric label / log field value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
        }
    }
}

/// Why a reading is not available this interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingUnavailable {
    /// `System` construction failed (the sysinfo error is logged at the
    /// attempt); retried on the next sample.
    InitFailed,
    /// No prior CPU refresh at least `MINIMUM_CPU_UPDATE_INTERVAL` old, so
    /// there is no delta to report. Expected once per process start (and once
    /// after a successful init retry): logged at debug, NOT counted.
    CpuBaselinePending,
    /// sysinfo reported no CPUs (CPU stats unreadable).
    NoCpus,
    /// sysinfo reported a total memory of 0 (memory stats unreadable).
    NoMemoryTotal,
    /// The value was non-finite or outside `[0, 100]`. Never clamped:
    /// clamping would fabricate a value. The raw value is logged at the
    /// sample.
    InvalidValue,
}

impl ReadingUnavailable {
    /// The `reason` metric label / log field value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::InitFailed => "init_failed",
            Self::CpuBaselinePending => "cpu_baseline_pending",
            Self::NoCpus => "no_cpus",
            Self::NoMemoryTotal => "no_memory_total",
            Self::InvalidValue => "invalid_value",
        }
    }

    /// Whether this reason is counted in `mc_system_readings_unavailable_total`.
    /// `CpuBaselinePending` is expected on every start, so counting it would
    /// make every rollout move the series.
    #[must_use]
    pub const fn is_counted(self) -> bool {
        !matches!(self, Self::CpuBaselinePending)
    }

    /// The counted reasons reachable for `reading` (zero-initialization and
    /// the catalog's emitted set): `no_cpus` is CPU-only, `no_memory_total`
    /// memory-only.
    #[must_use]
    pub const fn counted_for(reading: Reading) -> [Self; 3] {
        match reading {
            Reading::Cpu => [Self::InitFailed, Self::NoCpus, Self::InvalidValue],
            Reading::Memory => [Self::InitFailed, Self::NoMemoryTotal, Self::InvalidValue],
        }
    }
}

impl fmt::Display for ReadingUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One reading: a percent in `[0, 100]`, or why there is none.
pub type ReadingResult = Result<f32, ReadingUnavailable>;

/// The readings for one comprehensive heartbeat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemReadings {
    /// CPU usage percent.
    pub cpu: ReadingResult,
    /// Memory-in-use percent.
    pub memory: ReadingResult,
}

impl SystemReadings {
    /// Both readings unavailable for the same reason.
    #[must_use]
    pub const fn unavailable(reason: ReadingUnavailable) -> Self {
        Self {
            cpu: Err(reason),
            memory: Err(reason),
        }
    }

    /// The wire values `(cpu_usage_percent, memory_usage_percent)`: an
    /// unavailable reading is `None` (sent unset), never `Some(0.0)`.
    #[must_use]
    pub fn to_wire(&self) -> (Option<f32>, Option<f32>) {
        (self.cpu.ok(), self.memory.ok())
    }

    /// Make every unavailable reading loud: count it (counted reasons) and
    /// log it. `InitFailed` (sysinfo error) and `InvalidValue` (raw value) are
    /// logged where that context exists — the init attempt and
    /// [`SystemSampler::sample`] — so here they are only counted.
    pub fn record_unavailable(&self) {
        for (reading, result) in [(Reading::Cpu, self.cpu), (Reading::Memory, self.memory)] {
            let Err(reason) = result else { continue };
            if reason.is_counted() {
                record_system_reading_unavailable(reading, reason);
            }
            match reason {
                ReadingUnavailable::InitFailed | ReadingUnavailable::InvalidValue => {}
                ReadingUnavailable::CpuBaselinePending => debug!(
                    target: "mc.system_info",
                    reading = reading.label(),
                    reason = reason.label(),
                    "Host reading unavailable this interval (no CPU baseline yet)"
                ),
                ReadingUnavailable::NoCpus | ReadingUnavailable::NoMemoryTotal => warn!(
                    target: "mc.system_info",
                    reading = reading.label(),
                    reason = reason.label(),
                    "Host reading unavailable; sent unset in the comprehensive heartbeat"
                ),
            }
        }
    }
}

/// Raw sysinfo values after one refresh, the input to [`classify`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct RawSample {
    /// `System::global_cpu_usage()`.
    global_cpu_usage: f32,
    /// `System::cpus().len()`.
    cpu_count: usize,
    /// Age of the previous effective CPU refresh when this one was taken;
    /// `None` when there was none.
    since_last_cpu_refresh: Option<Duration>,
    /// `System::used_memory()`.
    used_memory: u64,
    /// `System::total_memory()`.
    total_memory: u64,
}

/// Classify a raw sample into readings. Pure.
#[must_use]
fn classify(raw: &RawSample) -> SystemReadings {
    SystemReadings {
        cpu: classify_cpu(raw),
        memory: classify_memory(raw),
    }
}

fn classify_cpu(raw: &RawSample) -> ReadingResult {
    if raw.cpu_count == 0 {
        return Err(ReadingUnavailable::NoCpus);
    }
    match raw.since_last_cpu_refresh {
        Some(age) if age >= MINIMUM_CPU_UPDATE_INTERVAL => {}
        _ => return Err(ReadingUnavailable::CpuBaselinePending),
    }
    percent(raw.global_cpu_usage)
}

fn classify_memory(raw: &RawSample) -> ReadingResult {
    if raw.total_memory == 0 {
        return Err(ReadingUnavailable::NoMemoryTotal);
    }
    // Byte counts far below 2^52 are exact in f64; the ratio is then
    // narrowed to the wire's f32.
    #[expect(
        clippy::cast_precision_loss,
        reason = "memory byte counts are exact in f64 well past any host size"
    )]
    let ratio = raw.used_memory as f64 / raw.total_memory as f64;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a percent narrowed to the wire's f32; non-finite is rejected by `percent`"
    )]
    let value = (ratio * 100.0) as f32;
    percent(value)
}

/// A measured percent: anything non-finite or outside `[0, 100]` is invalid.
/// Never clamped: clamping would turn a bad reading (NaN, or sysinfo
/// reporting used > total) into a fabricated "measured" value.
fn percent(value: f32) -> ReadingResult {
    if value.is_finite() && (0.0..=100.0).contains(&value) {
        Ok(value)
    } else {
        Err(ReadingUnavailable::InvalidValue)
    }
}

/// Log each `InvalidValue` reading with the raw value that made it invalid
/// (log fields, never labels): whether it is float noise (100.0001) or a
/// sysinfo bug (used > total) is only visible from the value.
fn warn_invalid_values(readings: &SystemReadings, raw: &RawSample) {
    if readings.cpu == Err(ReadingUnavailable::InvalidValue) {
        warn!(
            target: "mc.system_info",
            reading = Reading::Cpu.label(),
            reason = ReadingUnavailable::InvalidValue.label(),
            raw_value = f64::from(raw.global_cpu_usage),
            "Host reading invalid; sent unset in the comprehensive heartbeat"
        );
    }
    if readings.memory == Err(ReadingUnavailable::InvalidValue) {
        warn!(
            target: "mc.system_info",
            reading = Reading::Memory.label(),
            reason = ReadingUnavailable::InvalidValue.label(),
            used_memory = raw.used_memory,
            total_memory = raw.total_memory,
            "Host reading invalid; sent unset in the comprehensive heartbeat"
        );
    }
}

/// The sysinfo error's variant, for the init-failure log.
fn error_kind(e: &sysinfo::Error) -> &'static str {
    match e {
        sysinfo::Error::Unsupported => "unsupported",
        sysinfo::Error::Io(_) => "io",
        sysinfo::Error::Other(_) => "other",
    }
}

/// Long-lived CPU + memory sampler owned by the GC heartbeat task.
///
/// Built with `System::new()` (nothing loaded: no process scan, which would
/// also read every process's environment), refreshing only CPU usage and
/// memory per sample.
pub struct SystemSampler<F = fn() -> Result<System, sysinfo::Error>> {
    init: F,
    system: Option<System>,
    /// When the last *effective* CPU refresh happened (sysinfo skips one that
    /// comes sooner than `MINIMUM_CPU_UPDATE_INTERVAL`, and so do we).
    last_cpu_refresh: Option<Instant>,
}

impl SystemSampler {
    /// Production sampler (`System::new`). Attempts init and primes the CPU
    /// baseline immediately; a failure is retried on the next sample.
    #[must_use]
    pub fn new() -> Self {
        Self::with_init(System::new)
    }
}

impl Default for SystemSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl<F> SystemSampler<F>
where
    F: FnMut() -> Result<System, sysinfo::Error>,
{
    /// Sampler with an injected `System` constructor (test seam).
    fn with_init(init: F) -> Self {
        let mut sampler = Self {
            init,
            system: None,
            last_cpu_refresh: None,
        };
        sampler.system = sampler.try_init();
        sampler
    }

    /// One init attempt. On success, primes the CPU baseline (a new `System`
    /// has no baseline but this priming refresh); on failure, logs the
    /// sysinfo error and clears the baseline.
    fn try_init(&mut self) -> Option<System> {
        match (self.init)() {
            Ok(mut system) => {
                system.refresh_cpu_usage();
                self.last_cpu_refresh = Some(Instant::now());
                Some(system)
            }
            Err(e) => {
                warn!(
                    target: "mc.system_info",
                    error = %e,
                    error_kind = error_kind(&e),
                    "Failed to initialize host CPU/memory sampler; readings sent unset, retrying next interval"
                );
                self.last_cpu_refresh = None;
                None
            }
        }
    }

    /// Take one sample (retrying init first if it failed before). Logs the
    /// context-bearing reasons here (init failure with the sysinfo error,
    /// `InvalidValue` with the raw value); counting and the remaining logs are
    /// the caller's job via [`SystemReadings::record_unavailable`].
    pub fn sample(&mut self) -> SystemReadings {
        // A System created in THIS call has no baseline older than this call,
        // whatever the wall clock says: classified as no baseline by
        // construction, not by timing.
        let just_initialized = self.system.is_none();
        if just_initialized {
            self.system = self.try_init();
        }
        let Some(system) = self.system.as_mut() else {
            return SystemReadings::unavailable(ReadingUnavailable::InitFailed);
        };

        let since_last_cpu_refresh = if just_initialized {
            None
        } else {
            self.last_cpu_refresh.map(|t| t.elapsed())
        };
        system.refresh_cpu_usage();
        system.refresh_memory();
        // Mirror sysinfo's own gate: it skips a CPU refresh sooner than the
        // minimum interval, so only an effective refresh moves the baseline.
        if !just_initialized
            && since_last_cpu_refresh.is_none_or(|age| age >= MINIMUM_CPU_UPDATE_INTERVAL)
        {
            self.last_cpu_refresh = Some(Instant::now());
        }

        let raw = RawSample {
            global_cpu_usage: system.global_cpu_usage(),
            cpu_count: system.cpus().len(),
            since_last_cpu_refresh,
            used_memory: system.used_memory(),
            total_memory: system.total_memory(),
        };
        let readings = classify(&raw);
        warn_invalid_values(&readings, &raw);
        readings
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use common::observability::testing::MetricAssertion;

    const METRIC: &str = "mc_system_readings_unavailable_total";

    fn raw() -> RawSample {
        RawSample {
            global_cpu_usage: 42.5,
            cpu_count: 4,
            since_last_cpu_refresh: Some(MINIMUM_CPU_UPDATE_INTERVAL),
            used_memory: 25,
            total_memory: 100,
        }
    }

    // ---- classify: CPU ------------------------------------------------------

    #[test]
    fn cpu_without_a_baseline_is_pending() {
        let r = RawSample {
            since_last_cpu_refresh: None,
            ..raw()
        };
        assert_eq!(
            classify(&r).cpu,
            Err(ReadingUnavailable::CpuBaselinePending)
        );
    }

    #[test]
    fn cpu_baseline_one_nanosecond_short_is_pending() {
        let r = RawSample {
            since_last_cpu_refresh: Some(MINIMUM_CPU_UPDATE_INTERVAL - Duration::from_nanos(1)),
            ..raw()
        };
        assert_eq!(
            classify(&r).cpu,
            Err(ReadingUnavailable::CpuBaselinePending)
        );
    }

    #[test]
    fn cpu_baseline_exactly_the_minimum_is_a_reading() {
        assert_eq!(classify(&raw()).cpu, Ok(42.5));
    }

    #[test]
    fn cpu_with_no_cpus_is_unavailable() {
        let r = RawSample {
            cpu_count: 0,
            ..raw()
        };
        assert_eq!(classify(&r).cpu, Err(ReadingUnavailable::NoCpus));
    }

    #[test]
    fn non_finite_or_out_of_range_cpu_is_invalid_never_clamped() {
        for v in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.5,
            100.4,
            9999.0,
        ] {
            let r = RawSample {
                global_cpu_usage: v,
                ..raw()
            };
            assert_eq!(
                classify(&r).cpu,
                Err(ReadingUnavailable::InvalidValue),
                "cpu {v} must be invalid"
            );
        }
    }

    #[test]
    fn cpu_range_boundaries_are_real_readings() {
        let full = RawSample {
            global_cpu_usage: 100.0,
            ..raw()
        };
        assert_eq!(classify(&full).cpu, Ok(100.0));
        let zero = RawSample {
            global_cpu_usage: 0.0,
            ..raw()
        };
        assert_eq!(classify(&zero).cpu, Ok(0.0));
    }

    // ---- classify: memory ---------------------------------------------------

    #[test]
    fn memory_with_zero_total_is_unavailable_not_zero() {
        let r = RawSample {
            used_memory: 0,
            total_memory: 0,
            ..raw()
        };
        assert_eq!(classify(&r).memory, Err(ReadingUnavailable::NoMemoryTotal));
    }

    #[test]
    fn memory_percent_boundaries() {
        assert_eq!(classify(&raw()).memory, Ok(25.0));
        let full = RawSample {
            used_memory: 100,
            ..raw()
        };
        assert_eq!(classify(&full).memory, Ok(100.0));
        let over = RawSample {
            used_memory: 150,
            ..raw()
        };
        // sysinfo reporting used > total is a bad reading, not "100%".
        assert_eq!(
            classify(&over).memory,
            Err(ReadingUnavailable::InvalidValue)
        );
    }

    #[test]
    fn readings_are_independent() {
        let r = RawSample {
            since_last_cpu_refresh: None,
            ..raw()
        };
        let readings = classify(&r);
        assert_eq!(readings.cpu, Err(ReadingUnavailable::CpuBaselinePending));
        assert_eq!(readings.memory, Ok(25.0));
    }

    // ---- to_wire ------------------------------------------------------------

    #[test]
    fn every_unavailable_reason_goes_on_the_wire_as_none_never_zero() {
        for reason in [
            ReadingUnavailable::InitFailed,
            ReadingUnavailable::CpuBaselinePending,
            ReadingUnavailable::NoCpus,
            ReadingUnavailable::NoMemoryTotal,
            ReadingUnavailable::InvalidValue,
        ] {
            assert_eq!(
                SystemReadings::unavailable(reason).to_wire(),
                (None, None),
                "{reason} must be sent unset"
            );
        }
        let available = SystemReadings {
            cpu: Ok(0.0),
            memory: Ok(55.5),
        };
        assert_eq!(available.to_wire(), (Some(0.0), Some(55.5)));
    }

    // ---- record_unavailable -------------------------------------------------

    #[test]
    fn each_counted_reason_increments_its_series_by_one() {
        for reading in Reading::ALL {
            for reason in ReadingUnavailable::counted_for(reading) {
                let snap = MetricAssertion::snapshot();
                let readings = match reading {
                    Reading::Cpu => SystemReadings {
                        cpu: Err(reason),
                        memory: Ok(10.0),
                    },
                    Reading::Memory => SystemReadings {
                        cpu: Ok(10.0),
                        memory: Err(reason),
                    },
                };
                readings.record_unavailable();
                snap.counter(METRIC)
                    .with_labels(&[("reading", reading.label()), ("reason", reason.label())])
                    .assert_delta(1);
            }
        }
    }

    #[test]
    fn cpu_baseline_pending_is_not_counted() {
        let snap = MetricAssertion::snapshot();
        SystemReadings {
            cpu: Err(ReadingUnavailable::CpuBaselinePending),
            memory: Ok(10.0),
        }
        .record_unavailable();
        snap.counter(METRIC).assert_delta(0);
    }

    #[test]
    fn mixed_readings_send_the_good_one_and_count_only_the_bad_one() {
        let snap = MetricAssertion::snapshot();
        let readings = SystemReadings {
            cpu: Ok(37.0),
            memory: Err(ReadingUnavailable::NoMemoryTotal),
        };
        assert_eq!(readings.to_wire(), (Some(37.0), None));
        readings.record_unavailable();
        snap.counter(METRIC)
            .with_labels(&[("reading", "memory"), ("reason", "no_memory_total")])
            .assert_delta(1);
        snap.counter(METRIC).assert_delta(1);
    }

    // ---- SystemSampler ------------------------------------------------------

    #[test]
    fn init_failure_reports_both_unavailable_then_a_retry_recovers() {
        // Fails at construction and at the first sample, succeeds after.
        let mut failures_left = 2;
        let mut sampler = SystemSampler::with_init(move || {
            if failures_left > 0 {
                failures_left -= 1;
                Err(sysinfo::Error::Unsupported)
            } else {
                System::new()
            }
        });
        assert_eq!(
            sampler.sample(),
            SystemReadings::unavailable(ReadingUnavailable::InitFailed)
        );
        // The retry succeeds: a real attempt, not stuck at InitFailed ...
        let recovered = sampler.sample();
        assert_ne!(recovered.memory, Err(ReadingUnavailable::InitFailed));
        // ... and a System created in this sample has no CPU baseline, by
        // construction (not timing).
        assert_eq!(recovered.cpu, Err(ReadingUnavailable::CpuBaselinePending));
        // The recovered sampler is not stuck pending: once the baseline is old
        // enough, CPU is a real reading.
        std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
        let cpu = sampler.sample().cpu.expect("CPU reading after recovery");
        assert!(cpu.is_finite() && (0.0..=100.0).contains(&cpu), "cpu {cpu}");
    }

    #[test]
    fn persistent_init_failure_is_init_failed_for_both_readings() {
        let mut sampler = SystemSampler::with_init(|| Err(sysinfo::Error::Unsupported));
        assert_eq!(
            sampler.sample(),
            SystemReadings::unavailable(ReadingUnavailable::InitFailed)
        );
        assert_eq!(
            sampler.sample(),
            SystemReadings::unavailable(ReadingUnavailable::InitFailed)
        );
    }

    /// Regression for the always-~0 CPU of a per-call `System`: a persistent
    /// sampler yields a real CPU reading once the baseline is old enough.
    /// Never asserts > 0 (an idle CI host may read 0).
    #[test]
    fn persistent_sampler_reports_real_cpu_after_the_minimum_interval() {
        let mut sampler = SystemSampler::new();
        std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
        let readings = sampler.sample();
        let cpu = readings
            .cpu
            .expect("CPU reading after the minimum interval");
        assert!(cpu.is_finite() && (0.0..=100.0).contains(&cpu), "cpu {cpu}");
        let memory = readings.memory.expect("memory reading");
        assert!((0.0..=100.0).contains(&memory), "memory {memory}");
    }
}
