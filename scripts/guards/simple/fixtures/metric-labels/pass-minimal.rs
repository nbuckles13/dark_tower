// Minimal passing fixture: bounded labels, snake_case, no PII.
use metrics::{counter, gauge, histogram};

pub fn record_basic(status: &str) {
    counter!("svc_requests_total", "status" => status.to_string()).increment(1);
    histogram!("svc_duration_seconds", "status" => status.to_string()).record(0.1);
    gauge!("svc_active").set(5.0);
}
