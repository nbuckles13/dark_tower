//! Component tests for `mc_system_readings_unavailable_total`, driven through
//! the public per-tick step the GC heartbeat task runs
//! (`SystemReadings::to_wire` + `SystemReadings::record_unavailable`), not
//! through the recorder directly.
//!
//! Asserts the acceptance criterion of the sysinfo 0.40 adaptation: an
//! unavailable reading reaches GC unset (never `Some(0.0)`) and is counted
//! under its exact `{reading, reason}` series, with label-swap adjacency (the
//! sibling reading's series stays at 0). `cpu_baseline_pending` is expected on
//! every start and is NOT counted.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use common::observability::testing::MetricAssertion;
use mc_service::system_info::{ReadingUnavailable, SystemReadings};

const METRIC: &str = "mc_system_readings_unavailable_total";

#[test]
fn unavailable_memory_is_sent_unset_and_counted_on_its_own_series() {
    let snap = MetricAssertion::snapshot();
    let readings = SystemReadings {
        cpu: Ok(12.5),
        memory: Err(ReadingUnavailable::NoMemoryTotal),
    };

    assert_eq!(readings.to_wire(), (Some(12.5), None));
    readings.record_unavailable();

    snap.counter(METRIC)
        .with_labels(&[("reading", "memory"), ("reason", "no_memory_total")])
        .assert_delta(1);
    snap.counter(METRIC)
        .with_labels(&[("reading", "cpu")])
        .assert_delta(0);
}

#[test]
fn init_failure_counts_both_readings_and_sends_neither() {
    let snap = MetricAssertion::snapshot();
    let readings = SystemReadings::unavailable(ReadingUnavailable::InitFailed);

    assert_eq!(readings.to_wire(), (None, None));
    readings.record_unavailable();

    for reading in ["cpu", "memory"] {
        snap.counter(METRIC)
            .with_labels(&[("reading", reading), ("reason", "init_failed")])
            .assert_delta(1);
    }
}

#[test]
fn cpu_baseline_pending_is_sent_unset_but_not_counted() {
    let snap = MetricAssertion::snapshot();
    let readings = SystemReadings {
        cpu: Err(ReadingUnavailable::CpuBaselinePending),
        memory: Ok(40.0),
    };

    assert_eq!(readings.to_wire(), (None, Some(40.0)));
    readings.record_unavailable();

    snap.counter(METRIC).assert_delta(0);
}
