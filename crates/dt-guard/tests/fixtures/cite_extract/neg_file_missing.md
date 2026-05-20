# File-missing negative fixture

A symbol cite whose path doesn't exist on disk should yield
`reason=file-missing` (or `path-escape-or-missing`) in the symbol-resolves
guard. At the `extract_cites` level, this still produces a `Cite` with
`kind="symbol"`, `is_ignored=false` — the file-existence check is downstream
of extraction.

See crates/nonexistent/src/lib.rs::no_such_function for the citation.

Invariant: `extract_cites` returns one `Cite` with `kind="symbol"`,
`path="crates/nonexistent/src/lib.rs"`, `extra="no_such_function"`,
`is_ignored=false`. Downstream resolver returns "file-missing" or
"path-escape-or-missing" depending on whether the path canonicalizes.
