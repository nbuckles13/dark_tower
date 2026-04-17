// Category A (secret) token in label key — non-bypassable per ADR-0011.
// `# pii-safe` cannot whitelist a credential-in-label.
use metrics::counter;

pub fn record(password: &str, api_key: &str, bearer_token: &str) {
    counter!(
        "svc_auth_events_total",
        "password" => password.to_string(),
        "api_key" => api_key.to_string(),
        "bearer_token" => bearer_token.to_string()
    )
    .increment(1);
}
