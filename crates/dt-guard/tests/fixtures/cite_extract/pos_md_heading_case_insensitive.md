# Markdown heading case-insensitive resolution (positive)

A symbol cite whose target file is markdown should resolve when the
symbol appears in a heading — Python's `re.IGNORECASE` semantics mean
`## Foo Bar` resolves a citation for `sym = "foo"`.

See docs/runbooks/sample.md::foo for the resolution target.

Invariant: `extract_cites` produces one `Cite` with `kind="symbol"`,
`path="docs/runbooks/sample.md"`, `extra="foo"`, `line_no=7`,
`is_ignored=false`. Downstream `symbol_resolves_in_file` walks the
target's headings, splits each into words (per `MD_WORD_RESOLVER`),
and matches each word against `sym` via `eq_ignore_ascii_case`. A
heading like `## Foo Bar Baz` resolves `foo`/`Foo`/`FOO`.
