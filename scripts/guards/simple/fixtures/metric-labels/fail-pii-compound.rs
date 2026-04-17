// Compound PII labels: `user_email` (contains `email` component),
// `user_id` (raw identifier), `client_ip` (contains `ip`).
use metrics::counter;

pub fn record_login(user_email: &str, user_id: &str, client_ip: &str) {
    counter!(
        "svc_logins_total",
        "user_email" => user_email.to_string(),
        "user_id" => user_id.to_string(),
        "client_ip" => client_ip.to_string()
    )
    .increment(1);
}
