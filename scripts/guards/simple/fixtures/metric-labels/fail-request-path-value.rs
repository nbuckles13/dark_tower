// Raw request_path identifier in label value — unbounded cardinality source.
use metrics::counter;

pub fn record(request_path: &str) {
    counter!(
        "svc_events_total",
        "path" => request_path.to_string()
    )
    .increment(1);
}
