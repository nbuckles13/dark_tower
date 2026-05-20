# Path-escape negative fixture

A cite whose path traverses up out of the repo root via embedded `..`
segments should be rejected by `resolve_cited_path` per @security
commitment #19. At the `extract_cites` level the cite IS still produced
(extraction is path-agnostic), but the resolver vetoes it.

See crates/../../../escape/target.rs::root for the cite that should be blocked.

Invariant: `extract_cites` returns one `Cite` with `kind="symbol"`,
`path="crates/../../../escape/target.rs"`, `extra="root"`, `is_ignored=false`.
Downstream `resolve_cited_path` returns None because the resolved
canonical path is not contained within repo_root.

Note: the PATH_PREFIX boundary regex requires the path to start with
`[A-Za-z_]` AND end with `\.[a-z]{1,5}`. Using a leading `crates/` segment
+ `.rs` suffix satisfies both, while embedded `..` segments exercise the
resolver-layer path-containment check.
