// Unbounded value source: Uuid::new_v4() emits a new value every call.
use metrics::counter;
use uuid::Uuid;

pub fn record_request() {
    counter!(
        "svc_requests_total",
        "request_id" => Uuid::new_v4().to_string()
    )
    .increment(1);
}
