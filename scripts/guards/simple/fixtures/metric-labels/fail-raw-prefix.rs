// `raw_*` prefix denylist — catches raw_email, raw_user_id,
// and raw_request_id even when the suffix isn't itself in the token list.
use metrics::counter;

pub fn record(raw_email: &str, raw_request_id: &str) {
    counter!(
        "svc_events_total",
        "raw_email" => raw_email.to_string(),
        "raw_request_id" => raw_request_id.to_string()
    )
    .increment(1);
}
