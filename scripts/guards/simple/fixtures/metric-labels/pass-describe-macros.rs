// describe_* macros take (name, description) — description can be long English prose
// and is NOT subject to label-value length checks.
use metrics::{describe_counter, describe_gauge, describe_histogram};

pub fn register_metrics() {
    describe_counter!(
        "svc_requests_total",
        "Total number of requests received by this service since it started, including successful and failed responses"
    );
    describe_gauge!("svc_active", "Currently active request count");
    describe_histogram!("svc_duration_seconds", "Request duration in seconds");
}
