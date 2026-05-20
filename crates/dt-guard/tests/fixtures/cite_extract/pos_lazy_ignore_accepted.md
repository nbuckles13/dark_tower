# Lazy-ignore reason accepted (positive)

This fixture pins the third branch of `is_lazy_reason`: a reason that is
≥10 chars AND not in the vocab denylist falls through and the cite IS ignored.

See crates/mc-service/src/old.rs:99 here. <!-- guard:ignore(citing-removed-method-from-removed-pr-12345) -->

Invariant: `extract_cites` returns one `Cite` with `is_ignored=true`,
because the same-line guard-ignore-marker reason "citing-removed-method-
from-removed-pr-12345" is 47 chars (≥10) and not in the vocab list
("test", "tmp", "todo", "fix me", "wip"). Wave 2 alert-rules will share
the same `LAZY_REASON_RE` canonical, so this fixture exercises the same
path through `is_lazy_reason` from a different caller.

`extract_cites` reads the marker on the SAME line as the cite per the
line-walker semantics in the cite-extract module.
