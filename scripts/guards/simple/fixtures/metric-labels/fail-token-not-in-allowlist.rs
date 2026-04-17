// `token_value` is a `token_*` compound that is NOT in the Category A
// allowlist — it describes the token value itself, which is catastrophic.
// Must fire label_secret.
use metrics::counter;

pub fn record(token_value: &str) {
    counter!(
        "svc_events_total",
        "token_value" => token_value.to_string()
    )
    .increment(1);
}
