// Category A secrets cannot be bypassed with `# pii-safe` — a valid reason
// must still fail on a Category A label.
use metrics::counter;

pub fn record(jwt: &str) {
    // pii-safe: we need this for debugging, review approved 2026-04-17
    counter!("svc_events_total", "jwt" => jwt.to_string()).increment(1);
}
