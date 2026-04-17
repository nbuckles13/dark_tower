// `token_type` describes which flavor of token (meeting, guest, service) —
// it's a bounded-enum label, NOT a token value. Must pass. This covers the
// false-positive that the `token` bare-word denylist would have triggered.
use metrics::counter;

pub fn record(token_type: &str) {
    counter!(
        "svc_jwt_validations_total",
        "token_type" => token_type.to_string()
    )
    .increment(1);
}
