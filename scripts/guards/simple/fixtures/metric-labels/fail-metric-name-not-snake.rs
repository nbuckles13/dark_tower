// Metric name violates snake_case convention (camelCase).
use metrics::counter;

pub fn record() {
    counter!("SvcEvents", "status" => "ok".to_string()).increment(1);
}
