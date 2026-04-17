// Label key with uppercase — must be snake_case lowercase.
use metrics::counter;

pub fn record_event(status: &str) {
    counter!("svc_events_total", "Status" => status.to_string()).increment(1);
}
