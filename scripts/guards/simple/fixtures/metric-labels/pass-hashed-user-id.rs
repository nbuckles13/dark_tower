// Hashed/opaque suffix is allowed for PII-adjacent labels.
use metrics::counter;

pub fn record_user_action(user_id_hash: &str, action: &str) {
    counter!(
        "svc_user_actions_total",
        "user_id_hash" => user_id_hash.to_string(),
        "action" => action.to_string()
    )
    .increment(1);
}
