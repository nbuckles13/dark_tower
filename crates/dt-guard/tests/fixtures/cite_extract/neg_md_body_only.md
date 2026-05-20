# Markdown body-only-not-heading (negative)

A symbol cite whose target file contains the symbol ONLY in body text
(not in any heading) must NOT resolve. Python's `_build_md_pattern`
constrains matches to heading content via `re.MULTILINE` over heading
lines; body prose is intentionally outside scope.

See docs/runbooks/body_only_sample.md::specific_function_name in this case.

Invariant: `extract_cites` produces one `Cite` with `kind="symbol"`,
`path="docs/runbooks/body_only_sample.md"`, `extra="specific_function_name"`,
`line_no=8`, `is_ignored=false`. Downstream `md_symbol_resolves` walks
the target's headings (per `MD_HEADING_RESOLVER`), tokenizes each into
words, and finds NO match — because `specific_function_name` appears
only in body prose, not in any `#`/`##`/`###` heading. Resolver returns
`false`; cite-symbol-resolves guard emits `symbol-not-found`.
