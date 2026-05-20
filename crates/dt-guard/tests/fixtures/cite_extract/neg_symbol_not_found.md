# Symbol-not-found negative fixture

A symbol cite where the path exists but the symbol does not. At the
`extract_cites` level, the cite is still produced; the resolver returns
"symbol-not-found" after reading the file and running the per-language
resolver.

See crates/dt-guard/src/lib.rs::definitely_not_a_real_symbol_xyz123 for the cite.

Invariant: `extract_cites` returns one `Cite` with `kind="symbol"`,
`path="crates/dt-guard/src/lib.rs"`, `extra="definitely_not_a_real_symbol_xyz123"`,
`is_ignored=false`. The path resolves (file exists in this repo at the
referenced location) but the resolver's RS_FN_RESOLVER will not find
the symbol — emitting `reason=symbol-not-found`.
