// Security-extended Category B tokens — verify the expanded denylist catches
// them. Each label here would be a PII leak.
use metrics::counter;

pub fn record(
    username: &str,
    credit_card: &str,
    user_agent: &str,
    fingerprint: &str,
    latitude: f64,
) {
    counter!(
        "svc_events_total",
        "username" => username.to_string(),
        "credit_card" => credit_card.to_string(),
        "user_agent" => user_agent.to_string(),
        "fingerprint" => fingerprint.to_string(),
        "latitude" => latitude.to_string()
    )
    .increment(1);
}
