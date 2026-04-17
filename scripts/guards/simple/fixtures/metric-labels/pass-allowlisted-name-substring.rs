// `hostname` contains the `name` PII token as a substring but is allowlisted —
// it's infrastructure identity, not user identity.
use metrics::counter;

pub fn record_host_event(hostname: &str) {
    counter!("svc_host_events_total", "hostname" => hostname.to_string()).increment(1);
}
