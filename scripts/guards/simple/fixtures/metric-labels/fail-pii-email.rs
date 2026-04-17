// PII label: `email` is a raw identifier in the denylist.
use metrics::counter;

pub fn record_signup(email: &str) {
    counter!("svc_signups_total", "email" => email.to_string()).increment(1);
}
