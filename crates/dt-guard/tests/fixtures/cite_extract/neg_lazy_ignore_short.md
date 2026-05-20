# Lazy-ignore length-branch negative fixture

This fixture pins the SECOND branch of `is_lazy_reason`: a reason shorter
than `_MIN_REASON_LEN = 10` is REJECTED even when not in the vocab list.

See crates/mc-service/src/bar.rs:22 in this fixture. <!-- guard:ignore(short) -->

Invariant: `extract_cites` returns one `Cite` with `is_ignored=false`,
because the same-line `guard:ignore(short)` reason is "short" (5 chars,
< 10). The bare-line cite still counts as a violation; downstream emits
`lazy_ignore_reason` via `common::explain::print_finding`.

Together with `lazy_ignore_vocab.md` and `lazy_ignore_accepted.md`, this
pins all three branches of the shared `is_lazy_reason` kernel that Wave 2
alert-rules will consume from `src/ignore.rs` per ADR-0034 §6.
