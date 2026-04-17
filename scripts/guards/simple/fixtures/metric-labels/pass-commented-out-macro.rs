// A commented-out metric macro must not trip the parser. Surfaced by
// security's probe — the opener regex matches `counter!(` inside a line
// comment; without paren-matching awareness the walker may see an unbalanced
// span when the closing paren is broken across lines or absent from the
// commented-out form.
use metrics::counter;

pub fn record(status: &str) {
    // TODO: remove. counter!("old_svc_events_total", "status" => status.to_string()
    // ).increment(1);
    counter!("svc_events_total", "status" => status.to_string()).increment(1);
}
