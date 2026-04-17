// pii-safe with a too-short reason (< 10 chars) — must be rejected.
use metrics::counter;

pub fn record(phone: &str) {
    // pii-safe: ok
    counter!("svc_events_total", "phone" => phone.to_string()).increment(1);
}
