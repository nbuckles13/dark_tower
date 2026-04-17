// Bare `token` is Category A per Lead ruling 2026-04-17 — a bare `token`
// label key almost always means "the actual token value" which is
// catastrophic. Must fire label_secret (non-bypassable), even with a valid
// `# pii-safe` reason.
use metrics::counter;

pub fn record(token: &str) {
    // pii-safe: we need this for debugging, review approved 2026-04-17
    counter!("svc_events_total", "token" => token.to_string()).increment(1);
}
