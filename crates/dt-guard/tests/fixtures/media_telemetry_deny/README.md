# `media_telemetry_deny` fixtures

Fixtures for `dt-guard media-telemetry-deny` (ADR-0036 §11). Consumed by
`crates/dt-guard/tests/media_telemetry_deny_e2e.rs`.

## Convention

Flat `pos_<slug>.rs` / `neg_<slug>.rs`, mirroring `tests/fixtures/cite_extract/`.
`pos_` means **the guard fires**; `neg_` means **the guard stays green**.

Every fixture closes with an `// Invariant:` paragraph restating its expected
outcome in words. **That paragraph is the specification and the `catalog()`
rows in the harness are derived from it.** If the walker disagrees with an
Invariant block, that is a question for the fixture's owner
(`observability`, per `CLAUDE.md` §Specialists) — the expectation does not
move to match the implementation. A fixture whose expectation was quietly
relaxed to match the code is the exact failure this guard exists to prevent.

## Why `.rs` fixtures here are safe — read before adding one

These are the first `.rs` files under any `fixtures/` directory in this
repository (`cite_extract/` is `.md`, `ts_retained_credentials/` is `.ts` /
`.svelte`), so "does a file full of `counter!`, `println!` and `#[instrument]`
red some *other* guard?" is a real question. It was swept in full on
2026-09-07 and the answer is no:

* `rust_secrets`, `rust_pii`, `rust_log_secrets` and `instrument_skip_all`
  exclude via `common::test_code_filter::is_test_path`, which matches the
  `/fixtures/` path segment.
* `metric_labels`, `histogram_buckets`, `application_metrics` and
  `metric_coverage` scope to `crates/<service>/src/observability/metrics.rs`.
* `test_coverage` excludes `/tests/`; `test_rigidity` is scoped to
  `crates/env-tests/tests`.
* **Unreferenced `.rs` under `tests/` is in no cargo target**, so Layer 2
  (`cargo fmt --all --check`) and Layer 6 (clippy) never see these files.
  That is the load-bearing fact, and it is what makes a deliberately
  malformed fixture such as `pos_unparseable_use.rs` safe to commit.

The guard under test reads these files as *data*, via
`media_telemetry_deny::check_file`, so nothing here is compiled.

## One fixture whose LAYOUT is the test

`pos_handle_call_colocated_with_macro.rs` puts a handle call and a denied
macro on **one physical line**. That is not a style choice — it is the only
shape that distinguishes "the allow is satisfied by construction" from "the
allow is a line-level filter", because a fixture holding handle calls alone
passes identically under either implementation. Splitting that line to satisfy
a style preference silently turns the fixture into a duplicate of
`pos_metrics_label_macros.rs`.

`cargo fmt` cannot do this to you (these files are in no cargo target — see
below); a human can. The fixture says so at its foot, and the expectation of
exactly one hit is what reds if it happens.

## Two things not to do

1. **Do not name the harness `*_tests.rs`.** `test_registration` pairs
   `crates/*/tests/<X>_tests.rs` with a sibling `tests/<X>/` directory and
   demands `#[path = …]` registration for every `.rs` inside it. The harness
   is `media_telemetry_deny_e2e.rs` for that reason, matching the
   `cite_extract_e2e.rs` / `ts_retained_credentials_fixtures.rs` precedent.
2. **Do not plant a fixture inside `crates/mh-service/src/media/`.** The
   guard's real scope is that directory; a planted macro there reds every run
   for the whole team. The self-test plants into throwaway copies instead.
