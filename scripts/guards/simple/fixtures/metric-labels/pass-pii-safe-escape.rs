// pii-safe escape hatch with a sufficiently specific reason.
use metrics::counter;

pub fn record_admin_action(display_name: &str) {
    // pii-safe: admin usernames are public/organizational; review approved 2026-04-17
    counter!(
        "svc_admin_actions_total",
        "display_name" => display_name.to_string()
    )
    .increment(1);
}
