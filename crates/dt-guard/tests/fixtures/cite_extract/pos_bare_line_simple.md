# Bare-line simple positive fixture

A simple bare-line cite that should produce one un-ignored `bare-line` `Cite`.

See crates/mc-service/src/lib.rs:42 for the canonical example.

This fixture's invariant: `extract_cites` returns exactly one `Cite` with
`kind="bare-line"`, `path="crates/mc-service/src/lib.rs"`, `extra="42"`,
`line_no=5` (this is the 5th line of the fixture, 1-based), `is_ignored=false`.
