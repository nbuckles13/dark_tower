// pii-safe with a lazy reason ("todo") — must be rejected, and the underlying
// PII violation must still fire.
use metrics::counter;

pub fn record(email: &str) {
    // pii-safe: todo
    counter!("svc_events_total", "email" => email.to_string()).increment(1);
}
