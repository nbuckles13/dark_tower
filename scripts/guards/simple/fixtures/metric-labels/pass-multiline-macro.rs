// Multiline macro call — must be parsed as a single balanced-paren span.
use metrics::counter;

pub fn record_http(method: &str, endpoint: &str, status_code: u16) {
    counter!(
        "svc_http_requests_total",
        "method" => method.to_string(),
        "endpoint" => endpoint.to_string(),
        "status_code" => status_code.to_string()
    )
    .increment(1);
}
