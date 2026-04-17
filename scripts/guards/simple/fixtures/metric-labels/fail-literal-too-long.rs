// Literal label value exceeds 64-character cardinality budget.
use metrics::counter;

pub fn record_long() {
    counter!(
        "svc_events_total",
        "reason" => "this_is_a_very_long_literal_reason_string_that_exceeds_sixty_four_chars_threshold".to_string()
    )
    .increment(1);
}
