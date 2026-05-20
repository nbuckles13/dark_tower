# Lazy-ignore vocab-branch negative fixture

This fixture pins the FIRST branch of `is_lazy_reason`: a reason in the
vocab denylist ("test", "tmp", "todo", "fix me", "wip") is REJECTED — the
cite is NOT ignored and a `lazy_ignore_reason` violation should fire
(rule_id emitted in the `--explain` output).

See crates/mc-service/src/foo.rs:11 in this fixture. <!-- guard:ignore(test) -->

Invariant: `extract_cites` returns one `Cite` with `is_ignored=false`,
because the same-line `guard:ignore(test)` reason matches LAZY_REASON_RE
`^(test|tmp|todo|fix ?me|wip)\b`. The bare-line cite still counts as a
violation; downstream emits a `lazy_ignore_reason` policy finding through
`common::explain::print_finding`.
