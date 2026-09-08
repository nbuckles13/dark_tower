# Devloop Output: Media-Path Telemetry Deny — Policy Manifest & Fixtures (ADR-0036 §11)

**Date**: 2026-09-08
**Task**: Author the policy manifest and fixtures for the media-path telemetry deny guard (story task #18)
**Specialist**: observability
**Mode**: Agent Teams (v2), full
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c0538b56f295b9bd3b36138be7e227243afd66a6` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (observability) |
| Implementing Specialist | `observability` |
| Iteration | `1` (resumed after session loss; review round ran 4 iterations) |
| Security | `RESOLVED-DEFERRED` |
| Test | `RESOLVED-DEFERRED` |
| Observability | `RESOLVED-DEFERRED` |
| Code Quality | `RESOLVED-FIXED` |
| DRY | `RESOLVED-FIXED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `CLEAR` |
| Infrastructure (cross-boundary owner) | `RESOLVED-DEFERRED` |
| Media Handler (cross-boundary owner) | `CLEAR` |

---

## Task Overview

### Objective

Supply the **policy content** and the **coverage-demonstration tests** for the
media-path telemetry deny guard required by ADR-0036 §11. The guard *mechanism*,
its Layer-3 wiring and its wrapper script were the infrastructure task's scope
(landed in `c0538b56`, devloop `2026-09-07-media-telemetry-deny-guard`).

### Setup-time finding — the sibling infrastructure devloop over-delivered

`c0538b56` already landed the YAML manifest, the fixture directory and the shell
self-test that this task was written to author. This devloop's work is therefore
an **audit against task #18's explicit deliverable list**, closing whatever is
genuinely absent, rather than authoring from scratch. Anything already correct is
confirmed and cited, not rewritten — re-authoring identical policy content would
be churn with a drift risk and no gain.

### Scope
- **Service(s)**: `crates/dt-guard/` (guard-crate policy content), `scripts/guards/`
- **Schema**: No
- **Cross-cutting**: Yes — policy content owned by observability, machinery owned by infrastructure (CLAUDE.md §Specialists, Guard-crate ownership)

### Debate Decision
NOT NEEDED — ADR-0036 §11 already settles the policy; this is its expression.

---

## Cross-Boundary Classification

`crates/dt-guard/**` is **NOT** a Guarded Shared Area: no module under it meets an
ADR-0024 §6.4 criterion, it has no key in
`scripts/guards/simple/cross-boundary-ownership.yaml`, and **this loop adds none** —
that file is a GSA mirror and `dt-guard gsa-sync` rejects stray keys.

Split applied per CLAUDE.md §Specialists "Guard-crate ownership (interim)": an edit
that *changes* a machinery construct is infrastructure's; an edit that merely
*satisfies* an existing construct is content.

| File | Change | Content / Machinery | Owner | Trailer needed |
|---|---|---|---|---|
| `crates/dt-guard/src/media_telemetry_deny.rs` | (a) doc comment: why SPAN + `#[instrument]` are §11's own invariant, not scope creep (G1), plus the amended "Everything below follows" sentence and the seam-adapter residual pointer (G8); (b) `DENIED_MACRO_RE` **and** `OPEN_SPAN_RE` anchors `!` → `\s*!` (G4); (c) unit tests for both | (a) Content — policy rationale, no construct touched. (b) **changes the matcher construct's shape — Machinery**, and per CLAUDE.md "an edit that changes a machinery construct is **both**", it also carries a policy half: whether `info !(` *ought* to be denied is observability's call on what the deny covers | **infrastructure** (construct shape) + observability (policy half + all prose, satisfied in-loop) | **Yes — `Approved-Cross-Boundary: infrastructure`** |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/pos_span_bare.rs` | extend `// Invariant:` with the same argument at the point of use | Content (fixture) | observability | No |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/pos_instrument_bare.rs` | same | Content (fixture) | observability | No |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/pos_macro_space_before_bang.rs` | **new** — fixture proving `info !(`, `tracing::warn  !(`, `counter !(`, `custom_span !(` are denied, plus the brace/bracket forms that were code-covered but fixture-uncovered | Content (fixture) | observability | No |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/pos_handle_call_colocated_with_macro.rs` | **new** — one physical line with a handle call AND a real `counter!`; the only shape that distinguishes by-construction from a line filter (G5) | Content (fixture) | observability | No |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/neg_cached_handles.rs` | correct the "This fixture is the proof" over-claim down to what the body shows; cross-reference the co-location fixture (G5) | Content (fixture) | observability | No |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/README.md` | record that `pos_handle_call_colocated_with_macro.rs`'s single-line layout IS the test and must not be reflowed | Content (fixture-suite convention) | observability | No |
| `crates/dt-guard/tests/media_telemetry_deny_e2e.rs` | new catalog rows; module-doc cites the env-config precedent on the *applies* half | Content (expectation rows are transcribed fixture invariants) | observability | No |
| `scripts/guards/media-telemetry-deny.test.sh` | new case (d2c) truly-empty directory; §D header cites the env-config precedent | Content (coverage demonstration; reuses the existing tree-copy rig and assert helpers) | observability | No |
| `scripts/guards/simple/media-telemetry-deny.yaml` | §INCOMPLETE BY DESIGN gains the seam-adapter complement (G8); header gains the env-config precedent | **Machinery** — @code-reviewer ruling 2026-09-08, see below | **infrastructure** | **Yes** |
| `crates/mh-service/tests/media_metrics_integration.rs` | G8 **DO NOT RETIRE** banner naming the walker as the dt-guard scope's complement; **and at review, the matcher itself** — literal `name!(` substrings replaced with a shape matcher (`contains_macro_invocation`), plus the level, `print`/`eprint` and `*_span` names that were missing | Content (policy: what the complement must cover) | observability, **@media-handler co-signed** | **Yes — `Approved-Cross-Boundary: media-handler`** |
| `crates/dt-guard/src/metric_labels.rs` | re-point `MACRO_OPENER_RE` at `MACRO_NAME_ALTERNATION` (G6) **+ BOTH anchor halves** (G10 — whitespace, then the delimiter half after the deferral was re-priced and withdrawn in review); `find_macro_invocations` derives `close` from `open`; unknown-delimiter arm routed to `PARSE_ERROR_RULE_ID` instead of silently dropping | **changes the regex construct and the body scanner** — **Machinery** | **infrastructure** | **Yes** |
| `docs/runbooks/devloop-validation.md` | **My hunks**: re-measured self-test figure; hand-copied assertion count deleted; OPS-1 wording, OPS-3 `unparseable-use` carve-out at both prose sites, OPS-4 sibling literal. **@operations' own hunks (authored by the owner, not by me)**: the §6.5 `--all-targets` row, the new §4 "never triage a tool by grepping its stdout" subsection, and the §2.1 restructuring re-keyed from scenario to operation, plus the §8 symptom row | Content in an **operations-owned file** | operations | **Yes — `Approved-Cross-Boundary: operations`, for MY hunks only.** The owner's own hunks need no trailer: a trailer records a non-owner editing an owned surface, and @operations authoring in their own file is not that |
| `crates/dt-guard/src/telemetry_macros.rs` | `LOG_MACRO_RE` + `TRACING_NAMED_RE` anchors widened (G9); pinned literals move with them | **Machinery** (anchor construct) + policy half | **infrastructure** (construct shape) + **security** (detection surface) + observability (policy half, satisfied in-loop) | **Yes — all three** |
| `crates/dt-guard/src/metric_macros.rs` | **review, @infrastructure F3 + @dry-reviewer F5**: canonical-home doc now enumerates all four `MACRO_NAME_ALTERNATION` consumers and states which are still `(`-only; the anti-narrowing pin `macro_name_alternation_is_frozen_against_narrowing` MOVED here from `metric_labels`' test module, because the person deleting a variant edits this file and would never have seen it there | Doc + test on the canonical home; **@infrastructure claimed it explicitly** ("F3 is the one adjacent file, and it is mine") | **infrastructure** | **Yes — covered by the existing `Approved-Cross-Boundary: infrastructure`** |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/neg_bare_identifiers.rs` | **review, @dry-reviewer F4**: the two negatives BOUNDING the whitespace widening (`my_info !(v)`, `frame_counter !(v)`, `error != (0)`) added as fixture data rather than living only in a unit test; expectation stays `&[]`, so no catalog row moves | Content (fixture) | observability | No |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/neg_comments_only.rs` | **review, @operations OPS-8(d)**: stale `!\s*\(` anchor spelling replaced with the property the argument depends on | Content (fixture) | observability | No |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/pos_metrics_label_macros.rs` | **review, @operations OPS-8(c) + @dry-reviewer F3**: stale anchor spelling replaced by naming `DENIED_MACRO_RE`; the `\b` property it depends on kept | Content (fixture) | observability | No |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/pos_span_open_shape.rs` | **review, @test + @dry-reviewer F3**: stale `\b\w+_span!\s*\(` quote replaced by naming `OPEN_SPAN_RE` and its requires-a-literal-`_` property | Content (fixture) | observability | No |
| `docs/TODO.md` | two new §Observability Debt entries (the vacuity class; the remaining matcher anchors), and a RESOLVED-with-both-remedies-ruled-out update to the pre-existing seam-adapter entry | Content | observability | No |

**Not touched, deliberately** (superseded in part by G9/G10 below — `telemetry_macros.rs` and `metric_labels.rs` MOVED INTO scope after the sibling-anchor probe; this list is kept as written so the scope growth is visible rather than retconned) — `telemetry_macros.rs`, `metric_macros.rs`,
`common/pii_vocabulary.rs`, `rust_pii.rs`, `rust_log_secrets.rs`,
`instrument_skip_all.rs`, `common/scope.rs`, `scripts/guards/simple/*`. @security's
c0538b56 trailer covers the baseline, not this diff; no membership set moves, so no
fresh security-owned cross-boundary edit arises. *(**This inference is exactly what
G9 disproved, and it is left standing rather than deleted so the paragraph documents
the error instead of restating it.** The co-signed freeze is on group MEMBERSHIP; the
gap was in the ANCHOR, which the freeze never covered. Security IS a fresh
cross-boundary owner on G9, with a trailer three rows up in this same table.)* Notably the metrics family stays
derived from `metric_macros::MacroKind::ALL` and the telemetry family from
`telemetry_macros` group selection — **no third list is introduced.**

---

## Planning

### Method

The task was written before the sibling infrastructure loop landed. Every clause was
therefore audited against the tree at `c0538b56` and recorded MET (with the file and
line that meets it) or GAP. Nothing already correct is rewritten.

### Restating the problem in mechanism language — and the wider class it exposes

*Instance language*: "deny telemetry macros inside MH's media directory."

*Mechanism language*: **"a guard whose scope is a path the guard does not own, which
reports clean when that path stops resolving."** The directory list is not the
interesting part; the self-failing scope assertion is.

Restated that way the class is wider than the task names, and it has a **live
same-owner sibling today**:

- `crates/dt-guard/src/metric_coverage.rs:113-133` warn-skips a service whose
  `crates/<svc>/src/observability/metrics.rs` is absent, and again when that file
  yields no emissions, then falls through to
  `emit_ok("metric-coverage-all-covered")` at `:147`. That is *precisely* the
  env-config failure — `warn_skip` writes to stderr and `run-guards.sh` reports
  `STATUS=OK`. An observability-owned guard, reporting clean over services it did
  not check. `common::scope::assert_scope_live` now exists and is the fix shape.
- `instrument_skip_all.rs:170` is **not** in this class: it is diff-scoped, so an
  empty changed-file set is a legitimate no-op rather than a lost premise.

**Not taken in this loop** — deferring with a reason, per CLAUDE.md: converting
`metric_coverage`'s vacuity from warn-skip to FAIL changes a production guard's exit
semantics across four services and needs @operations sign-off on the lane and a
runbook row. That is task-sized, not a trivial in-tree edit. Recorded in
`docs/TODO.md` §Observability Debt and surfaced to reviewers.

### Audit result — clause by clause

| # | Task clause | Verdict | Citation |
|---|---|---|---|
| 1 | YAML manifest beside the guard, `cross-boundary-ownership.yaml` precedent, config-driven list | **MET** | `scripts/guards/simple/media-telemetry-deny.yaml` — `denied_directories:` at the tail; the header's OWNERSHIP block names the precedent and states why a `.yaml` at that path is inert |
| 2 | Seeded with `crates/mh-service/src/media/`; sibling-layout premise | **MET, pending @media-handler confirmation** | manifest tail; directory holds `caps.rs forward.rs forwarder.rs ingress.rs mod.rs queue.rs sampler.rs`; the §11 sibling premise is written at `crates/mh-service/src/media/mod.rs:9,33-37` ("If setup or teardown drifts in here at review, pull it out") |
| 3 | Header records manifest-path/real-directory as two encodings that cannot drift silently; scope-missing IS the drift guard | **MET** | manifest header §"WHY THIS PATH AND THE ON-DISK DIRECTORY ARE NOT DRIFT-PRONE DUPLICATION": names CLAUDE.md §SSoT, explains there is nothing to *derive* a policy scope from, then "THE SCOPE-MISSING FAILURE IS THE DRIFT GUARD" |
| 4 | Denied families complete, path-prefix-insensitive, metrics driven off `MacroKind::ALL` | **MET** | `media_telemetry_deny.rs:DENIED_MACRO_ALTERNATION` = `MACRO_NAME_ALTERNATION` (itself `MacroKind::ALL.join("|")`, `metric_macros.rs:105-111`) + `alternation_for(&[Level, Event, LogMacro, Span, Print])`. Path-prefix insensitivity comes from the leading `\b` with no anchored qualifier group |
| 5 | Doc comment records span + `#[instrument]` as §11's zero-allocation invariant applied to its own enforcement, not scope creep | **GAP** | `grep -rin 'zero.allocation|scope creep|registry lookup'` across `crates/dt-guard/` + `scripts/guards/` returns exactly one hit, in `pos_metrics_describe_macros.rs:6`, about *describe* macros. The span/instrument rationale is absent everywhere |
| 6 | Cached-handle methods not flagged, with reason | **MET** | `ALLOWED_HANDLE_METHODS` doc + module doc §"The allow-list is satisfied BY CONSTRUCTION"; fixture `neg_cached_handles.rs` exercises all five and expects zero hits |
| 7 | Fixture layout follows `cite_extract` / `ts_retained_credentials` | **MET** | flat `pos_`/`neg_` naming matching both; README states the convention and the `*_e2e.rs` harness-naming constraint from `test_registration` |
| 8 | Self-test at `scripts/guards/media-telemetry-deny.test.sh`, never under `simple/` | **MET, claim verified independently** | file is at the required path; `scripts/guards/run-guards.sh:121` is `find "$SIMPLE_GUARDS_DIR" -name "*.sh" -type f -not -path '*/fixtures/*'`, so a `.sh` under `simple/` would indeed run as a production guard. Wired at `scripts/layer3.sh:148` |
| 9 | One flagged fixture per denied family | **MET** — map below | see family table |
| 10 | One clean allow-fixture of cached-handle calls | **MET** | `neg_cached_handles.rs`; catalog row expects `&[]` |
| 11 | Guard FAILS on missing directory and on directory-with-no-Rust-source | **MET for both named cases; PARTIAL on the distinction** | shell (d1) `scope_missing` asserts exit 1 + `…-scope-directory-missing`; (d2) `scope_empty` creates the directory holding only `README.txt` — i.e. *exists but no Rust source* — and asserts `…-scope-directory-empty`; (d3) asserts the tokens are distinct. Unit mirrors at `common/scope.rs:302,310,321`. **A literally-empty directory (zero entries) is covered by no case at any level** |
| 12 | Coverage-demonstration framing explicit and durable, citing the env-config precedent | **GAP** | `grep -rn 'env-config|warn.skip'` across the manifest, guard module, fixture README, e2e harness and self-test: **zero hits**. The precedent is real and documented at `crates/dt-guard/src/env_config.rs:23-27` ("silently skip `mc-service` and `mh-service` … still reported `STATUS=OK REASON=env-config-clean-4-services`, a clean verdict for a count that included the two services it had skipped"), but nothing in the media-deny artifacts cites it |

### Family → fixture map (clause 9)

| Denied family | Members | Fixture | Catalog row |
|---|---|---|---|
| tracing levels | `trace` `debug` `info` `warn` `error` | `pos_tracing_levels.rs` (all five bare + `tracing::` + `log::` spellings) | 7 hits |
| tracing event | `event` | `pos_tracing_event.rs` | 2 + 1 import |
| log crate | five level names + generic `log!` | `pos_log_crate_qualified.rs` (`log!`, `error!`, `::log::info!`) | 3 + 1 import |
| std print + dbg | `print` `println` `eprint` `eprintln` `dbg` | `pos_print_family.rs` (all five) | 5 hits |
| metrics label-bearing | `counter` `gauge` `histogram` | `pos_metrics_label_macros.rs` | 4 hits |
| metrics describe | `describe_counter` `describe_gauge` `describe_histogram` | `pos_metrics_describe_macros.rs` | 4 hits |
| span | `span` + `trace_span` `debug_span` `info_span` `warn_span` `error_span` | `pos_span_bare.rs`, `pos_span_suffixed.rs`, `pos_span_open_shape.rs` | 2+1, 5, 2 |
| `#[instrument]` any form | bare, `tracing::`, `::tracing::`, `cfg_attr` | `pos_instrument_bare.rs`, `pos_instrument_qualified.rs` | 2, 2 |

No family is unfixtured. Clause 9 is MET.

### A ninth denied form the audit found unfixtured — and unmatched

@security's Gate-1 note flagged it and it **reproduces**. `DENIED_MACRO_RE` is
`\b(<names>)!\s*[\(\[\{]` — the `!` must *immediately* follow the name. Rust
tokenisation permits whitespace, and `blank_non_code` turns an interposed comment
into spaces:

```
$ printf 'fn f() { info !("x"); }\nfn g() { tracing::warn  !("y"); }\nfn h() { info!("z"); }\n' \
    | grep -nP '\b(info|warn)!\s*[\(\[\{]'
3:fn h() { info!("z"); }
```

Two of three denied invocations walk straight through. `info !("x")` compiles and
logs. This is a one-character-class fix (`\s*` before the `!`) with no realistic
false-positive surface: in valid Rust there is no construct where a denied *name* is
followed by only whitespace, then `!`, then `(`/`[`/`{`, that is not a macro call —
`!=` is excluded because the character after `!` must be an open delimiter, and every
near-miss (`&&`, `||`, `|x|`, `;`, `=>`) interposes a non-whitespace token.

### Planned work

| # | Gap | Edit |
|---|---|---|
| G1 | clause 5 | New module-doc section in `media_telemetry_deny.rs`, **arguing** rather than asserting: `debug_span!`/`#[instrument]` are a per-frame *registry lookup* (callsite interest check through `Dispatch::current()`) plus, once enabled, a span allocation and field recording — which is exactly §11's "zero allocation and zero registry lookup" per-frame invariant, and §11 names **span attributes** alongside logs and metric labels as a retention surface. "The level is off" is not a defence for the identical reason §11 gives for level macros: *"A log level is not an acceptable gate. The incident motivating a level change is the same incident producing the sensitive trace."* `#[instrument]` without `skip_all` additionally records the forward function's arguments — connection identity, stream, frame — into span fields, which is §11's per-frame per-participant dimension by construction. So SPAN is §11's own invariant applied to the guard's own enforcement, **not an extension**. Mirrored in one sentence each on `pos_span_bare.rs` and `pos_instrument_bare.rs`, which is where a trimmer actually lands |
| G2 | clause 12 | Cite `env_config.rs:23-27` by name in the self-test's §D header and in the e2e module doc beside the *applies* half, so the two scope tests read as load-bearing rather than paranoid |
| G3 | clause 11 distinction | New shell case (d2c): configured directory exists and is **literally empty** (zero entries) → exit 1 + `…-scope-directory-empty`. Distinct from (d2)'s no-`.rs`-file case. Built in a temp copy via the existing `mk_root` rig — no in-place manifest mutation, no new env override |
| G4 | unmatched denied form | Fixture `pos_macro_space_before_bang.rs` + catalog row (mine), and the `\s*` anchor widening in `DENIED_MACRO_RE` (**infrastructure machinery — trailer required**). I will also probe `find_instrument_attributes` and `classify_use_line` for the same whitespace class and report; both may already be immune (the instrument pass keys on a bare `\binstrument\b`) |

### What this loop does NOT do

- No suppression path, no env/flag override, no `crate::ignore` import, no
  `is_test_path` composition, no path exemption. `pos_ignore_annotation_not_honored.rs`
  and `pos_macro_in_test_module.rs` stay positive.
- No output widening. No `matched`/`context`/`line_text` field on `Finding` or on the
  explain payload; `--explain` keeps routing through `SecretFinding`. The new fixture
  asserts on spelling + rule, never on argument text.
- No narrowing. `TelemetryGroup::Level` stays `{trace, debug, info, warn, error}`,
  `LogMacro` stays out of `Level`, and no sibling guard's detection set moves.
- The documented residual (sibling re-export as `crate::obs::note!`, deny-list not
  allowlist) is not closed and no edit makes that paragraph less accurate. G4 makes it
  *more* accurate, by removing a form it did not mention.
- `metric_coverage`'s warn-skip is not converted here (see wider class, above).

### Open questions to reviewers

1. **@media-handler** — is `crates/mh-service/src/media/` the final home of the
   forward path, and does the sibling-layout premise still hold as written at
   `media/mod.rs:33-37`? Anything scheduled to move *into* `media/`?
2. **@team-lead / @code-reviewer** — G4's regex anchor is infrastructure machinery
   and infrastructure is not on this team. Take it here under an
   `Approved-Cross-Boundary: infrastructure` trailer, or hand it back as a follow-up?
   My recommendation is to take it: a documented, reproduced evasion of a §11 control
   left open for a loop is the "reads as coverage" failure the ADR is about.
3. **@operations** — G3 adds no new reason token and no new exit code, so I read it as
   touching no runbook row in `docs/runbooks/devloop-validation.md`. Confirm?

### Reviewer input folded in at Gate 1 (before plan circulation)

Five reviewers sent pre-plan constraints. Three added work; all are accepted.

**G5 — @observability: the cached-handle co-location property is claimed three times and demonstrated zero times.** Accepted, and it is the strongest finding of the round. `neg_cached_handles.rs` closes with *"This fixture is the proof"* for the claim that a line filter would be a masking bug — but it holds five handle calls and **zero macro invocations**, so a hypothetical line filter passes it identically. `ALLOWED_HANDLE_METHODS`' doc and `cached_handle_methods_are_never_flagged` have the same hole. Fix: a **single physical line carrying both** a handle call and a denied macro — `h.frames.increment(1); counter!("mh_media_frames_forwarded_total", 1);` — asserted to yield exactly one finding with spelling `counter`. That is the only shape that distinguishes by-construction from subtraction. The fixture's over-claim is corrected down to what it actually shows, with a cross-reference to the new case. Policy content, mine.

**G6 — @dry-reviewer: `metric_labels.rs:59-64` re-inlines the six-name `MacroKind` alternation.** Taking it, not deferring. The task text's own words are *"driven off `metric_macros::MacroKind::ALL` rather than a re-inlined alternation, because that enum is the declared single source of truth … and auto-extends its consumers"*; `metric_labels` is the sole holdout among four consumers, `metric_macros.rs`'s module doc already claims it was converted, and the file **already imports `MacroKind`**. The re-point is value-neutral: `MacroKind::ALL` folds to `describe_counter|describe_gauge|describe_histogram|counter|gauge|histogram`, byte-identical to the inlined literal, pinned by a compiled-pattern equality test. Ownership: re-pointing a regex construct at its SSoT is **machinery** — infrastructure, `Approved-Cross-Boundary` trailer, same as G4.

**G1 widened — @observability: the module doc's framing actively licenses the trim.** The doc quotes §11's four sentences and then says *"Everything below follows from those four sentences."* Span and `#[instrument]` visibly do **not** follow from those four sentences, so a reader doing exactly what that line licenses removes them. The fix therefore edits that sentence too, marking SPAN/instrument as derived from a **different** §11 clause — the leak bullet, which names *"media-path logs, metric labels, **or span attributes**"* — rather than from the enforcement sentence. And the allocation argument must defeat the actual objection: a disabled level buys a cheaper `Span::none()`, **not** the elision of the argument expressions, which are still evaluated and recorded at the call site; `#[instrument]` records every non-skipped function argument by default, which on a forward function is the per-frame per-participant value set. So "the level is off in prod" is a runtime-configuration defence for a per-frame invariant, which is the defence §11 already rejects for level macros. Denying span while allowing it would make the guard's coverage depend on a subscriber config the guard cannot see.

**@operations — token set, lanes, budget.** The token set is **unchanged**: no rule is added, renamed, split or merged, `Rule::ORDER` is untouched, so no runbook row moves. No exit path other than 0/1 is introduced; the new shell case asserts exit **1** explicitly against that contract. (d2c) uses `mk_root … custom` and creates one empty directory — **no tree copy**, so the one-pristine-copy shape is preserved and cost is ~0. Accepted asks: re-measure and update the runbook's `~0.15–0.25s` / "73 hermetic assertions" figure — noting the count is currently wrong three ways (runbook 73, commit message 83, 54 assertion call-sites), so I propose replacing the literal with "the suite prints its own pass count on completion" plus a re-measured range, since a hand-copied count is exactly the two-encodings failure this loop is about. **That row is operations-owned — @operations, do you want the edit, or do you approve mine?** I will also add the cheap collision pin you asked for: an assertion that the precondition body literally contains `THIS IS A DIFF DEFECT`, which already exists at self-test `scope:missing:diff-defect` — I will extend it to the `-empty` and `-escapes-root` cases so all three carry it.

**@operations — the sibling loop's deferrals.** `SCOPE:`-line-discarded is agreed out of scope (infrastructure emitter/filter machinery). The "four `#[tracing::instrument` sites unmatched" entry (`docs/TODO.md:602-606`) **stays deferred, and the reason is in the entry itself**: flipping any consumer from `INSTRUMENT_ATTR_BARE_RE` to `INSTRUMENT_ATTR_ANY_RE` is measured to cost one false positive in `rust_pii` Check 3 at `crates/mh-service/src/webtransport/connection.rs`, and is explicitly blocked on the security-owned Check-3 fields-scoping fix landing first. Flipping today reds a correct file. `media_telemetry_deny` already uses the wide matcher, so nothing in *this* guard's coverage is affected.

**@security — all four constraints accepted verbatim**, and G4 is your pre-flagged item: reproduced, evidence in the section above. No suppression path, no output widening, no sibling-detection-set narrowing, scope tokens stay FAIL/exit 1 and distinct, and the new cases assert the *specific* token plus exit 1 in a temp root built by the existing rig — no in-place manifest mutation, no `DT_GUARD_*` root override.

**@semantic-guard — fixture content.** New fixtures use obvious placeholders only (`mh_media_*` metric names, `"x"`, integer literals). No plausible-format participant id, key or token. The co-location fixture asserts on rule + spelling, never on argument text.

**@observability #3/#4 — no vocabulary term is added.** `TelemetryGroup` membership, `Level` = {trace, debug, info, warn, error}, and `LogMacro`-out-of-`Level` are untouched. `DENIED_MACRO_ALTERNATION` is not modified by G4 either — G4 changes only the anchor *around* the alternation, not the alternation itself. The `MacroKind::ALL` reachability test stays a reachability test and is not "strengthened" into an equality pin against a literal list.

### Open questions (updated)

4. **@media-handler, completeness — the under-scoping question is the one I most want your answer on.** `crates/mh-service/src/media/` is the seeded entry. Is any *other* directory under `crates/mh-service/src/` on the per-frame path — `transport/`, `webtransport/`, `session/`, `routing/`? A denied list that is correct-but-incomplete is the silent failure; the scope tokens cannot detect it, because every configured entry resolves fine. Also: do `neg_cached_handles.rs`'s shapes match what MH actually writes, and would MH's real forward path survive the G4 whitespace widening? Any added entry widens blast radius, so I will run the guard against the **real tree** before Gate 2 either way (@operations' ask).

### @media-handler confirmation, and the scope finding it produced (G8)

Answered before plan circulation. **Path CONFIRMED**: `crates/mh-service/src/media/` is the correct and complete *directory* scope. Candidates ruled out by inspection: `transport/mod.rs` (seam trait defs only, self-denying, no concrete per-frame code), `webtransport/connection.rs` (connection lifecycle/handshake/JWT — a legitimate sibling that uses tracing and metrics), `session/mod.rs` (per-frame read is a lock-free `ArcSwap` load with no macro; the file also holds actor lifecycle that legitimately logs), `routing/mod.rs` (`for_each_source` is a borrowing lookup with no macro; the leak-prone visit closure runs in `media/forward.rs`), `process.rs` (epoch sampled once in `main`, not per-frame).

**Sibling-layout premise CONFIRMED as still true, not merely as written**: all seven files under `media/` are hot-path; `forwarder.rs:4` states "No setup and no teardown live here"; the only `tokio::spawn` under the scope is inside `#[cfg(test)] mod tests` at `queue.rs:254`. So no lifecycle code is being denied telemetry it legitimately needs — the false-positive half of the layout risk is empty today.

**G8 — the material finding, and it is exactly the under-scoping class the task warns about.** There is one per-frame surface OUTSIDE `media/`: the transport-seam adapter `crates/mh-service/src/webtransport/media_transport.rs` (`send_datagram` / `write_all` / `recv_datagram`, called per frame). It **cannot** be added to `denied_directories`: a lone file is the forbidden file-list shape §11 rules out, and its directory cannot be denied wholesale because `connection.rs` sits beside it and legitimately needs telemetry.

It **is** protected today — but by the in-crate walker at `crates/mh-service/tests/media_metrics_integration.rs:604-628`, which asserts `seam_adapter.is_file()` with the message *"@observability's ruling scopes the deny to it explicitly because it is a SIBLING of the hot path, not a child"*, and which carries its own anti-false-green directory and file-count assertions — **not** by the new dt-guard directory scope.

The hazard is a coverage illusion of exactly the shape this loop is about: once the dt-guard directory guard lands and looks authoritative, someone retires the walker as redundant and the seam adapter silently loses all coverage. Nothing in the manifest, the guard module or the walker currently says the two are **complements**.

Fix: record the split where each reader lands — a paragraph in the manifest header's existing "INCOMPLETE BY DESIGN" section (its natural home, and @media-handler's suggestion), cross-referenced from the guard module's residual-coverage section, and a reciprocal line in the walker naming the dt-guard scope as its complement so the pointer survives from either end.

**Ownership note for @code-reviewer.** The manifest header's own OWNERSHIP block says *"The schema, the header and the parsing are `infrastructure` machinery"*, which would make this edit infrastructure's. I read that sentence as over-claiming: under CLAUDE.md §Specialists, policy content is *"what a … deny-list entry … means"*, and a paragraph stating what the deny list does and does not cover is precisely that. I am classifying G8 as **content / observability** and flagging the tension rather than letting it pass silently. Upgrade me if you disagree.

### Gate-1 review round — conditions accepted, and the two things the round found

All eight reviewers responded before implementation. Three of them found things the audit had not.

**@security condition 1 — `OPEN_SPAN_RE` has the identical gap and I had missed it.** `\b(\w+_span)!\s*[\(\[\{]`, same anchor, same file. Fixing only `DENIED_MACRO_RE` would leave `custom_span !(…)` passing while `custom_span!(…)` is denied — an asymmetry **worse** than the current uniform gap, because the module doc's open-span rationale would then read as coverage. Both anchors widen in one hunk; the fixture carries a spaced open-span line. `INSTRUMENT_ATTR_ANY_RE` is `(?s)#\[[^\]]*\binstrument\b` — no `!` anchor, `[^\]]*` absorbs whitespace and path segments — so `#[ instrument ]` and `#[tracing :: instrument]` already land; pinned with fixture lines rather than rested on the argument, since that immunity is a property of `[^\]]*` that nothing currently tests.

**@security condition 2 — the G6 pin must be vacuity-proof.** The re-point is byte-neutral but not risk-neutral: it converts a future *narrowing* of `MacroKind::ALL` from a one-guard into a three-guard narrowing, and `metric_labels` is a PII-relevant surface (ADR-0029 bounded labels). The pin therefore compares the derived alternation against a **hand-written literal in the test body**, never against anything derived from `MacroKind::ALL` — a self-comparison passes unchanged when a variant is deleted, i.e. certifies nothing at the moment it matters. Precedent supplied by @observability: `telemetry_macros::log_macro_re_is_equivalent_to_historical_literal` does exactly this and was observability co-signed, so the shape is settled rather than novel. The comment must distinguish why a second encoding is correct here (a test oracle whose *failing on a narrowing is the point*) and objectionable elsewhere (a second production encoding drifts silently), or a future SSoT pass deletes a security control as cleanup.

**@security condition 3 — the sibling-anchor probe, run, and a second class found on top.** The gap is present in five anchors: `telemetry_macros.rs:252` (`LOG_MACRO_RE`), `:275` (`TRACING_NAMED_RE`), `metric_macros.rs:152` (`MACRO_INVOCATION_RE`), `:180` (`MACRO_INVOCATION_WITH_FIRST_ARG_RE`), `metric_labels.rs:59-64` (`MACRO_OPENER_RE`). Not affected: `INSTRUMENT_ATTR_ANY_RE`; `INSTRUMENT_ATTR_BARE_RE` (no `!` anchor — its own, different narrowness is already tracked at `docs/TODO.md:602-606`); the `\b(alternation)\b` vocabulary matchers in `rust_pii.rs:72`, `rust_log_secrets.rs:58`, `instrument_skip_all.rs:63`. Corroborating detail: `MACRO_INVOCATION_RE` tolerates `\s*` around `::` but not before `!` — the author thought about whitespace one token earlier and stopped, so this is an oversight rather than a decision.

@security then added **a second evasion in the same five anchors, one character rather than one space**: all end `!\s*\(` — paren only — so `info!{…}` and `info![…]` evade, both legal Rust. The severity is not a narrowed match. `rust_pii.rs:133` and `rust_log_secrets.rs:186` both gate their whole per-line check on `LOG_MACRO_RE.is_match(line)`, so a miss **skips the entire PII/secret vocabulary scan for that line**: `tracing::info!{"user {}", user_id}` is scanned by neither guard and both report clean. And unlike `media-telemetry-deny`, which fails loudly on its own scope, neither has a self-check — a narrowed anchor there is indistinguishable from a clean tree indefinitely. Deferred to a separate loop (five anchors, three owners, a shared canonical home, four downstream consumers); the `docs/TODO.md` entry must name **both classes per anchor**, the line-gate consequence, and the missing-self-check asymmetry, or the next author prices a coverage hole as a regex tidy-up. Note `metric_labels`' anchor survives G6 unchanged and must be named separately or it looks fixed.

**@observability — my `metric_coverage` enumeration was under-scoped, and the instance I missed is worse.** `application_metrics.rs:67-68`, `:117-118`, `:177-178` bail on a bare `return out` with **no `warn_skip` at all**, then reach `finalize()`'s only OK token, `"application-metrics-clean"` at `:587`. With the catalog or dashboards directory absent, the metric-to-dashboard coverage guard reports clean **silently** — not even the WARNING line `metric_coverage` manages. `grafana_datasources.rs:291,296` is the counter-example and the fix precedent: `emit_ok("grafana-datasources-no-dashboards")` / `-no-config` names the vacuity in the reason token instead of claiming "clean" — same crate, same owner, already-accepted policy. Stopping at the first same-owner instance and calling it the class was the same error the entry documents.

**@operations — the `metric_coverage` entry must cite its own design precedent.** `docs/TODO.md:1115` (env-config, RESOLVED 2026-08-28) already answered both questions the follow-up will ask: it made the no-workload skip a **hard FAIL**, and it changed the OK token to `env-config-clean-<S>-services-<W>-workloads` so a dropped service is visible in the STATUS line itself. `metric-coverage-all-covered`'s total absence of a count is the concrete remedy shape. Citing it converts the entry from "re-derive the design" into "apply the precedent".

**@operations — runbook row approved for me to author, with three constraints.** They verified both halves before answering: `scripts/lang/_test_helpers.sh::report_results` does print `<label>: <PASS> passed, <FAIL> failed` unconditionally on both arms, so the runtime source is real; and the drift is worse than I described — 54 is `assert_` *call sites*, but `filter:scope-line-known-gap` increments `PASS` directly without a helper and call sites inside blocks can fire more than once, so the static count is **not derivable by counting**. That kills "just correct the number": a re-counted literal would be a fourth hand-copy with the same half-life. Constraints: (1) keep the wall-clock range, labelled a range — with the literal gone it is the only remaining control on a suite that `run-guards.sh`'s `timeout` never wraps (`layer3.sh:148` invokes it via `run_and_emit`, deliberately outside `guards/simple/`); (2) state the measuring conditions, or a bare range is the next thing to drift, just more slowly; (3) **do not cite the c0538b56 commit-message figure** — it is immutable git history and the runbook must not become a third live copy of a wrong number.

**@operations hard condition on sign-off**: paste the output of `dt-guard media-telemetry-deny --root .` against the **real tree** before Gate 2. "I expect a pass" and "I ran it" are different evidentiary states, and a guard that reds on first contact is a team-wide availability event.

**@media-handler — G4 blast-radius evidence, supplied rather than assumed.** Every macro-invocation shape under `crates/mh-service/src/media/`: `assert!`/`assert_eq!`/`assert_ne!`, `format_args!` (forward.rs:114,127), `select!` (ingress.rs:75,115,137) — none are denied names. **Zero `name !(` forms anywhere under the scope.** The lone `debug!(` token at forward.rs:97 sits in a doc comment that `blank_non_code` strips. `compile_error!` is name-safe under widening independently, since `\berror\s*!` needs a word boundary before `error` and `_error` has none.

**@media-handler — G8 placement, and why three copies is not duplication.** They asked for the manifest header; I am writing it in three places because the three readers are making three different decisions — what the manifest covers (YAML header), what a green run certifies (guard residual section), and whether this test is still needed (the walker). **The walker-side line is the load-bearing one**: the person who retires the walker is standing in `media_metrics_integration.rs`, not in a YAML they will never open. A pointer that lives only at the manifest end is retired from the other end. @media-handler co-signs that hunk (`Approved-Cross-Boundary: media-handler`) rather than taking it, to keep one idea with one author.

**@test — d2/d2c token equivalence.** Confirmed at implementation: (d2) README-only and (d2c) literally-empty must emit the **same** token, `…-scope-directory-empty` — they are one equivalence class (zero `.rs` files), and a different token for empty-vs-non-`.rs` would itself be a smell. (d3)'s distinctness assertion is missing-vs-empty, not empty-vs-non-`.rs`. G4's widening stays bounded: `\s*`, never `[\s\S]*`; `\b` still holds so `information!` and a field `info:` cannot fire; no backtracking surface from the `blank_non_code` comment-to-space interaction.

**Verdict costs accepted up front**: @test and @observability both signalled RESOLVED-DEFERRED on account of the `metric_coverage`/`application_metrics` follow-up. That is the honest accounting of a cost shifted to future work and should stay visible at Gate 2 rather than be averaged away by the fixes.

### @code-reviewer ruling on the G8 ownership tension — UPGRADED, accepted

I argued the manifest-header paragraph was content on the merits: CLAUDE.md assigns *"what a … rule … means"* to the policy owner, and coverage-boundary prose is meaning rather than construct shape. @code-reviewer agreed the merits are reasonable **and ruled against me anyway**, on a ground I had not weighed:

> "the schema, the header and the parsing are infrastructure machinery" is infrastructure's OWN written classification. Observability reclassifying it to content strips infra's co-sign — that is a downgrade of an owner's claim, and ADR-0024 §6.2 monotonicity exists precisely to stop an owner being argued down. A reviewer blessing that downgrade is the failure mode the rule guards. I can upgrade, never bless-down.

That is correct and I withdraw the classification. The distinction I missed: the question is not whether *I* think coverage-prose is content, it is that the genuine ambiguity — does "the header" mean schema scaffolding only, or all header prose — belongs to **infrastructure to resolve**, not to me to resolve in my own favour while they are not on this team. Reclassifying an absent owner's claim downward is the move monotonicity exists to prevent, and it is worse when the owner cannot answer.

Resolution: **option (a)** — the row is Machinery / infrastructure with an `Approved-Cross-Boundary: infrastructure` trailer. Marginal cost is zero, since infrastructure is already trailering G4 and G6. Option (b) (getting infrastructure to narrow their own sentence to scaffolding-only and cede coverage-prose) is the better long-term fix but requires an owner who is not on this team; it is noted for whoever next touches that header rather than blocking on it here.

### Lead ruling on G8, recorded with its reasoning

@team-lead ruled option (a): the manifest-header hunk is **Machinery / infrastructure with the `Approved-Cross-Boundary: infrastructure` trailer**. Recording the reasoning rather than only the outcome, per the ruling:

- A reviewer may upgrade a classification, never accept an owner's own claim being argued down (ADR-0024 §6.2). Infrastructure's OWNERSHIP block states in their own words that the schema, the header and the parsing are machinery.
- **The merits were NOT decided against me.** The lead's words: *"I am not ruling that you are wrong on the substance."* CLAUDE.md does assign "what a … rule *means*" to the policy owner, and coverage-boundary prose is meaning rather than construct shape. What was ruled is that the ambiguity — does "the header" mean schema scaffolding only, or all header prose — is **infrastructure's to resolve**, and **option (b) was foreclosed by team composition**: infrastructure is not on this team, so there is nobody who can cede on-thread. A future reader re-opening this should see it was left undecided on substance, not settled against observability.
- The asymmetry is decisive: a wrongly-kept trailer is a redundant audit line; a wrongly-dropped one removes an owner's signature from a surface they claimed in writing.

### @semantic-guard's blocking question — answered, and the answer is "already closed, but only in unit tests"

Their question: does the deferral of the paren-only delimiter class across the sibling anchors leave **this** guard's anchors paren-only? **No.** `DENIED_MACRO_RE` and `OPEN_SPAN_RE` already end `!\s*[\(\[\{]` — all three delimiters — and the module doc at `media_telemetry_deny.rs:390` states the reason: *"`println!{"x"}` and `vec![..]` syntax are legal Rust, so a `(`-only anchor is a one-character evasion."* `media_telemetry_deny` is in fact the **only** consumer in the crate that got the delimiter class right; the siblings predate that reasoning. So G4 adds the whitespace class to a guard already closed on the bracket class, and the deferral leaves this deliverable whole.

One real shortfall their question exposed, though: the bracket class is pinned only by **unit tests** (`media_telemetry_deny.rs:1091-1092`, `println!{"x"}` and `info![x]`) and by **no fixture** — a grep for `![{[]` across the fixture directory returns one unrelated `vec![]`. The catalog is the artifact a future reader treats as the coverage inventory, so a class present in unit tests and absent from fixtures reads as uncovered. The new fixture therefore carries brace- and bracket-invoked denied macros alongside the spaced forms, closing both classes in the artifact people actually read.

### @dry-reviewer's three conditions on the G6 pin — accepted, and condition 1 is a genuine trap

**Condition 1 — do NOT copy `Level`'s ordering rationale across; it is false for `MacroKind`.** `log_macro_re_is_equivalent_to_historical_literal` rests on an explicit premise — *"no `Level` member is a prefix of another, so … alternation order cannot be load-bearing"* — which is why `level_group_membership_is_frozen` can sort before comparing. `MacroKind::ALL` asserts the **opposite** about itself at `metric_macros.rs:47-50`: *"describe-\* first so the regex engine prefers the longer match at each position (`describe_counter!` is NOT a `counter!` invocation with `describe_` junk)."* A sorted pin would silently drop an ordering guarantee the enum's own doc calls load-bearing. The pin is therefore **order-sensitive** — it pins the alternation string in `ALL` order, covering membership and order in one assertion — and the "no member is a prefix of another" sentence is **not** pasted across, since it is true of `Level` and unverified for `MacroKind`.

**Condition 2 — the authority goes in the assertion message, not only the doc comment.** The characteristic failure of an equality pin is not that it fails to fire; it is that it fires and the next engineer mechanically updates the literal to green CI, converting a control into a speed bump. The person staring at red CI is not reading the doc comment. So the failure message itself will say that `MacroKind::ALL` changed, that a deletion narrows `metric_labels` (a PII-relevant surface, ADR-0029 bounded labels) plus two sibling guards, and that updating the literal requires @security + @observability sign-off rather than being a mechanical fix — with a cite to `telemetry_macros::tests::level_group_membership_is_frozen` so the next reader sees a pattern rather than a one-off.

**Condition 3 — three touchpoints, ONE home for the argument.** @dry-reviewer confirmed the complement is real and did not ask me to cut the three placements, but this repo's precedent for the shape is one canonical home plus cross-pointers (their example: the post-deploy checklist owned in `docs/runbooks/mh-deployment.md` with the MC addendum pointing at it), because three independently-worded copies drift into disagreeing about *why*. Accepted: the **reasoning** lives once, in the manifest's §INCOMPLETE BY DESIGN block; the other two carry a one-line consequence plus a pointer. The walker line must still stand alone well enough to stop a retirement without restating the argument — and it will **extend** the walker's existing *"does not discharge the guard-pipeline obligation — see `docs/TODO.md`"* pointer rather than becoming a second, differently-worded neighbour of it.

Their standing ruling on the pin, which settles the question I had raised as open: `telemetry_macros.rs` already holds **both** oracle types side by side deliberately — `level_group_membership_is_frozen` (frozen literal, security-owned vocabulary) and `every_all_member_is_reachable_through_its_group` (derivation), the latter's doc stating the distinction unprompted: a derivation test proves a variant ripples out with zero consumer edits, *"which an equality-against-a-frozen-string test cannot show (that one passes just as happily against a hand-copied literal)."* The two sites deliberately hold **different** values — current membership versus the membership security signed off on — so collapsing them would be a false SSoT hiding the fork.

### @security withdrew their own deferral — two sibling anchors move INTO scope (G9)

@security accepted the sibling-anchor deferral on the premise that a broadening needs a false-positive sweep before it can land, and @observability independently named the same sweep as the expensive part. @security then **ran the sweep and disproved the premise**, and withdrew an acceptance they had already given rather than let it stand on a claim they no longer believed. Their numbers, whole tree, `--include='*.rs'`, excluding `target/`: whitespace-before-`!` form **0 occurrences**; brace/bracket form **3, all inside `media_telemetry_deny.rs` itself** (the doc comment at `:390` and its own two unit assertions at `:1091-1092`), none carrying PII vocabulary; for scale, **776 lines** match the paren form and enter the PII scan today.

**I re-ran it independently rather than take it** (CLAUDE.md §Fail loudly — an accepted deferral reversing on someone else's grep is exactly the claim to check):

```
$ grep -rEn --include='*.rs' '\b(trace|debug|info|warn|error|log|span|…|counter|gauge|histogram|describe_*)[[:space:]]+![[:space:]]*[([{]' . | grep -v '^./target/'
(no output)
$ grep -rEn --include='*.rs' '\b(trace|debug|info|warn|error)![[:space:]]*[[{]' . | grep -v '^./target/'
crates/dt-guard/src/media_telemetry_deny.rs:1092:  assert!(!check_file("x.rs", r#"fn f() { info![x] }"#).is_empty());
```

Confirmed. The one live match is dt-guard's own unit assertion, it carries no PII vocabulary token, and both consumers require a vocabulary hit **after** the line gate — so the widening produces zero new findings. The sweep *was* the cost case and it evaluates to empty.

**G9 — two anchors move in, three stay out.** Into scope, both in `crates/dt-guard/src/telemetry_macros.rs`:
1. `:252` `LOG_MACRO_RE` — `\b({level})!\s*\(` → `\b({level})\s*!\s*[\(\[\{]`
2. `:275` `TRACING_NAMED_RE` — **three** sub-gaps, not one: no whitespace around `::`, none before `!`, and paren-only. Target `tracing\s*::\s*({level})\s*!\s*[\(\[\{]`, with the `|#\[instrument` alternative preserved **verbatim** — dropping it silently removes `rust_pii` Check 2's instrument coverage, a false negative in a PII guard that nothing else would notice.

Staying deferred, and the TODO entry must say the other two were fixed here so it does not read as if all five are outstanding: `metric_macros.rs:152`, `:180`, `metric_labels.rs:59-64`. The line that separates them is **consumption, not mechanism**: `rust_pii.rs:133` and `rust_log_secrets.rs:186` consume the pattern as a **line gate**, so a miss skips the whole PII/secrets scan for that line; the other three gate metric cataloguing and label taxonomy, not a leak path.

Trailers: `Approved-Cross-Boundary: security` (offered by @security, their surface) and `Approved-Cross-Boundary: observability` for the policy half (offered by @observability — only the anchor moves; `TelemetryGroup::Level` membership stays exactly {trace, debug, info, warn, error} and the group freeze is untouched). Both pinned-literal equality tests move with the patterns, pinned against **hand-written literals in the test body**, never against anything derived from `LEVEL_ALTERNATION`.

One comment lands beside the new anchors, in @observability's words because they are the right ones: the pinned tests exist to make a *narrowing* red; **this pattern was never wide enough, so no pin redded and nothing noticed** — which is why it took a widening in an unrelated guard to surface it. Without that sentence a future reader treats the pins as coverage of adequacy rather than of preservation. @security's c0538b56 trailer stands: it certified that the promotion **preserved** those detection sets, which it did faithfully; it never certified they were adequate. The hole is inherited from the pre-promotion literals, not introduced by the promotion.

**Scope note**: this expands the diff into a security-owned file beyond the plan as circulated, so it is flagged to @team-lead rather than absorbed silently.

### G10 — @observability's re-decomposition: the seam is LINE GATES vs BODY SCANNERS, not owner

@observability rejected @security's severity split on the `metric_labels` half and produced a better cut. **Verified independently before accepting:**

- `metric_labels.rs:304` is `for caps in MACRO_OPENER_RE.captures_iter(src)` — the **discovery loop** for every metric macro invocation whose labels are then checked against the PII patterns at `:121-138` (`user_email`, `request_path`, `Uuid::new_v4`, `SystemTime::now`). A `counter!{…}` that evades the opener is never label-checked at all.
- `metric_labels.rs:769` is `if MACRO_OPENER_RE.is_match(&src)` — a **file-discovery** gate. A file whose metric macros all use brace form never enters the scan set.

So `counter!{"m", "email" => user_email}` ships an unbounded PII-bearing metric label with nothing firing. That is a leak path under ADR-0029, not a cataloguing concern, and @security's "not a leak path" line was wrong about this anchor.

But the same inspection yields the better decomposition. `metric_labels.rs:307` computes `let paren_open_idx = whole.end() - 1` and then depth-matches on **parens specifically** — confirmed in-tree. So the two classes have genuinely different costs at this anchor:

- **Whitespace half** — `!` → `\s*!`. No body-scanner impact (`whole.end()` still lands on the delimiter), under 5 LoC, and `metric_labels.rs` is **already in this changeset for G6**. Fix now.
- **Delimiter half** — the body scanner must learn `{`/`[` and match the correct closer. Genuinely task-sized. Defer.

**The real seam is line gates versus body scanners, not security-owned versus metrics-owned.** Line gates (`LOG_MACRO_RE`, `TRACING_NAMED_RE`, and `metric_labels.rs:769`) are `is_match` calls where widening is free; body scanners (`metric_labels.rs:304`, `metric_macros.rs:152`/`:180`) parse from the delimiter and cost real work. That framing goes in the TODO entry, because it tells the next author which half is cheap — which the ownership framing does not.

**Final scope after G9 + G10.** Fixed in this loop: `DENIED_MACRO_RE` + `OPEN_SPAN_RE` (whitespace; both already accept all three delimiters), `LOG_MACRO_RE` + `TRACING_NAMED_RE` (both classes), and **both halves** of `metric_labels::MACRO_OPENER_RE`. *(The delimiter half was planned as deferred here on a body-scanner cost argument, and that deferral did not survive review — see §Code Review Results. Deferred as landed: both halves of `metric_macros.rs:152`/`:180`, and the `rust_log_secrets.rs` Check-4 gate.)*

One correction to the record: my earlier message to @observability cited @security's deferral acceptance as authority for deferring the whole class **after** @security had withdrawn it. The messages crossed, but the lesson stands and is the same one this loop is about — I was resting a conclusion on a co-sign that no longer existed, and I would not have noticed if @observability had not checked. @observability also held themselves to it, retiring all three of their own sizing points as having been "my sizing of an unrun sweep and an unasked owner", both of which had since happened.

### Table correction — the three construct-shape rows must read alike (@observability, Gate 1)

Caught by @observability and fixed before commit. Three rows in the classification table change a regex construct's shape. Two named the edit Machinery and gave it to **infrastructure** with a trailer; the third — `telemetry_macros.rs` — used the same word **Machinery** and then omitted infrastructure from both the Owner column and the trailer list.

Their read of how it happened is the right one and I am not going to dress it up: the row was written while the scope was growing under me, inherited security+observability from the G9 conversation that produced it, and never got its machinery column reconciled against its two siblings. It is not a judgment I made and would defend; it is a cell I did not re-read.

It matters because it re-opens, **in the artifact itself**, the exact inference I had already agreed to close: that anchor changes are somehow not infrastructure's construct. Two rows saying they are and one saying they are not is worse than either answer consistently applied. Per ADR-0024 §6.4's intersection rule an edit spanning multiple owners needs **all** affected owners to co-sign, and this one spans three: infrastructure (the anchor construct), security (the detection surface), observability (the policy half, satisfied in-loop). Per §6.2 adding infrastructure is an **upgrade** in owner involvement, which is the direction a reviewer may move — nothing is downgraded and there is nothing to escalate.

Trailer reason clauses (ADR-0024 §6.7, ≥10 chars, naming the authority rather than the what):
- **infrastructure** — the anchor construct is widened **without touching membership**, so the blast radius is shape-only; that is the sentence an infrastructure reader needs.
- **security** — theirs to word; their surface.
- **observability** — co-signed on-thread by @observability: *`Level` group membership is unchanged at exactly {trace, debug, info, warn, error}, `LogMacro` stays out of `Level`, and only the anchor around the alternation moves — the c0538b56 freeze is not in play.*

Also folded in, since it lands in an observability-lane test: @security's pin-order correction. Prefix-freedom is a statement about **regex semantics** — reordering the alternation cannot change what matches — not a licence to write the literal in a different order from the emitted one. A byte-equality pin must reproduce emitted order or it is not a byte-equality pin. What prefix-freedom actually buys is that a *future* maintainer may reorder `Level` without redoing the security analysis, which is a different and weaker claim than the existing test name implies. That gets a sentence in the test comment so the next reader does not repeat the inference.

And their verification of my two corrections, which I had asserted rather than shown: `rust_pii.rs:128` puts `let Some(token) = pii_hit(line) else { continue; };` **above** Check 1's `LOG_MACRO_RE.is_match(line)`, and `rust_log_secrets.rs:206` is `if EXPOSE_SECRET_RE.is_match(line)` with no separate vocabulary precondition. Their instruction for the commit message is right and I am taking it: **the structural argument leads, ahead of the 0-occurrences sweep number** — the sweep result expires the moment someone writes new code; the structural one does not.

---

## Pre-Work

None.

---

## Implementation Summary

The audit found the tree already satisfied clauses 1, 2, 3, 4, 6, 7, 8, 9 and 10; those are cited above and were not re-authored. Ten items were closed.

### The gaps the task named

**G1 — the span / `#[instrument]` rationale** (`media_telemetry_deny.rs` module doc). The missing paragraph was only half the defect. The doc quoted §11's four sentences and asserted *"Everything below follows from those four sentences"* — and those sentences name log macros, metric macros and `event!`, not spans. **The doc actively licensed the trim it was supposed to prevent**, which is the same shape as the in-file annotation bypass the module deliberately refuses to implement; it just needs one more reader to fire. That sentence is now qualified, and the new §"SPAN and `#[instrument]`: §11's own invariant, NOT an extension" argues rather than asserts: §11's retention bullet names *"media-path logs, metric labels, **or span attributes**"* directly, so on the leak axis this is not an extension at all; field expressions are evaluated **at the call site** regardless of subscriber interest, so a disabled level buys a cheaper `Span::none()` and not the elision of your arguments; `#[instrument]` absent `skip_all` records **every function argument**, which on a forward function is exactly §11's per-frame per-participant set. The closing move is the one a trimmer must answer: allowing span forms would make the guard's coverage depend on **a subscriber configuration the guard cannot see** — not a narrower policy, a policy with a runtime escape hatch. Mirrored in one sentence each on `pos_span_bare.rs` and `pos_instrument_bare.rs`, where a trimmer actually lands.

**G2 — the coverage-demonstration precedent.** `env_config.rs:23-27`'s failure (warn-skipping `mc-service` and `mh-service` while emitting `STATUS=OK REASON=env-config-clean-4-services` — a confident count including the two it skipped) is now cited in all three places a reader decides whether the scope tokens are over-engineering: the manifest header, the self-test's §D banner, and the e2e harness's *applies*-half doc.

**G3 — the uncovered scope state.** New self-test case (d2c): a directory that exists and is **literally empty**. Only "exists with no `.rs` file" was covered before, at any level. (d2d) additionally asserts the two states emit the **same** token, since they are one equivalence class and a fork would send an operator hunting a difference with no bearing on the fix.

### What the audit found that the task did not anticipate

**G4 — a live evasion of the control being audited.** `DENIED_MACRO_RE` required `!` to follow the macro name immediately. Rust tokenises `path ! delim`, so **`info !("x")` compiles, logs, and walked through the deny**. `OPEN_SPAN_RE` had the identical gap; both widened in one hunk, because a one-anchor fix would leave `custom_span !(` passing while `custom_span!(` is denied and make the module doc read as coverage. The brace/bracket class was already closed in code here but pinned only by unit tests and by **no fixture** — so it moved into the catalog, which is the artifact a later auditor reads as the inventory.

**G5 — the allow-side property was asserted three times and demonstrated zero times.** `neg_cached_handles.rs` claimed "This fixture is the proof" that a line-level filter would be a masking bug, while holding five handle calls and no macro invocations — **a line filter passes it identically**. New `pos_handle_call_colocated_with_macro.rs` puts both on one physical line and expects exactly one finding: zero would mean a filter masked it, two would mean the handle method was flagged, which is §11's "it bans the pattern it exists to enforce". The over-claim is corrected to what the body shows, with the claim cross-referenced to where it is now proven.

**G6 / G10 — `metric_labels.rs`.** It re-inlined the six-name `MacroKind` alternation: the sole holdout of four consumers, in a file already importing `MacroKind`, while `metric_macros`' own module doc listed it among the converted. **The doc claimed SSoT coverage the code did not have.** Re-pointed, with a hand-written anti-narrowing pin. Its whitespace anchor half is closed too; the delimiter half is deferred, because `find_macro_invocations` computes `whole.end() - 1` and depth-matches **parens specifically**.

**G8 — the one genuine coverage hole @media-handler found.** `webtransport/media_transport.rs` is per-frame and structurally cannot enter `denied_directories` (a lone file is the file-list shape §11 rules out; its directory holds `connection.rs`, which legitimately needs telemetry). Its only coverage is the in-crate walker. The hazard is now **retirement, not absence** — once a guard-pipeline control exists and looks authoritative, that walker reads as redundant. Reasoning lives once in the manifest's §INCOMPLETE BY DESIGN; the guard module and the walker carry a consequence plus a pointer, and the walker gets a DO NOT RETIRE banner because that is where the person doing the retiring is standing.

**G9 — two PII-guard anchors, after two reviewers disproved their own positions.** `LOG_MACRO_RE` and `TRACING_NAMED_RE` had both classes (the latter with three sub-gaps: no whitespace around `::`, none before `!`, paren-only). These are consumed as **line gates** by `rust_pii.rs:133` and `rust_log_secrets.rs:186`, so a miss skipped the **entire** PII and secrets vocabulary scan for that line. Each sub-gap is pinned separately so a partial fix cannot pass, and `|#\[instrument` is preserved verbatim under its own assertion.

### Evidence, not expectation

@operations required the difference between "the argument says it cannot red" and "I ran it". Both are supplied — see §Devloop Verification Steps for pasted output. The structural argument leads the empirical one because the sweep expires the moment someone writes new code: **`rust_pii.rs` gates its entire per-line block on `pii_hit(line)` before any check runs**, and every post-gate check requires its own independent hit — a PII token, a secret-shape match, or `expose_secret` — so widening the gate alone cannot manufacture a finding, for any tree.

---

## Files Modified

*Review-round growth is marked **[review]**; everything unmarked was in the diff
at Gate 2. Per @team-lead: all review growth was reviewer-driven, none
self-initiated.*

**New (2)**
- `crates/dt-guard/tests/fixtures/media_telemetry_deny/pos_macro_space_before_bang.rs` — spaced-bang forms across both anchors, plus the brace/bracket forms that were code-covered and fixture-uncovered
- `crates/dt-guard/tests/fixtures/media_telemetry_deny/pos_handle_call_colocated_with_macro.rs` — the by-construction demonstration; its single-line layout **is** the test

**Modified (13)**
- `crates/dt-guard/src/media_telemetry_deny.rs` — G1 doc section + amended framing sentence; G8 residual pointer; G4 both anchors; three unit tests (spaced forms fire, widening bounded, co-location fires exactly once)
- `crates/dt-guard/src/telemetry_macros.rs` — G9 both anchors, both classes; pinned literals moved with hand-written strings and sign-off authority in the assertion messages; `tracing_named_re_closes_all_three_sub_gaps`
- `crates/dt-guard/src/metric_labels.rs` — G6 re-point + G10 **both** anchor halves; delimiter-generic body scanner (`open`/`close` derived, unknown opener routed to `PARSE_ERROR_RULE_ID` rather than dropped); `expected_close` carried so the parse-error message names the right delimiter; body-parse and fail-loud tests
- `crates/dt-guard/tests/media_telemetry_deny_e2e.rs` — two catalog rows; env-config precedent on the *applies* half
- `crates/dt-guard/tests/fixtures/media_telemetry_deny/neg_cached_handles.rs` — over-claim corrected
- `crates/dt-guard/tests/fixtures/media_telemetry_deny/pos_span_bare.rs`, `pos_instrument_bare.rs` — one-sentence mirrors of the G1 argument
- `crates/dt-guard/tests/fixtures/media_telemetry_deny/README.md` — the layout-is-the-test note
- `crates/mh-service/tests/media_metrics_integration.rs` — DO NOT RETIRE banner (@media-handler co-signed); **[review]** literal `name!(` substrings replaced with `contains_macro_invocation` (shape: name + `\s*` + `!` + `([{`), `print`/`eprint`/`warn_span`/`error_span`/`trace_span` added, level macros promoted from transitive to direct, and `the_walker_matcher_catches_every_legal_invocation_spelling` pinning all of it
- **[review]** `crates/dt-guard/src/metric_macros.rs` — canonical-home consumer enumeration (@infrastructure F3); the anti-narrowing pin re-homed here from `metric_labels` (@dry-reviewer F5)
- **[review]** `crates/dt-guard/tests/fixtures/media_telemetry_deny/neg_bare_identifiers.rs` — the widening's bounding negatives as fixture data (@dry-reviewer F4)
- **[review]** `crates/dt-guard/tests/fixtures/media_telemetry_deny/neg_comments_only.rs`, `pos_metrics_label_macros.rs`, `pos_span_open_shape.rs` — stale anchor spellings replaced with the properties the arguments depend on (@operations OPS-8, @test, @dry-reviewer F3)
- `scripts/guards/media-telemetry-deny.test.sh` — (d2c), (d2d), env-config banner, `THIS IS A DIFF DEFECT` extended to all five precondition-class tokens
- `scripts/guards/simple/media-telemetry-deny.yaml` — env-config precedent; G8 seam-adapter section
- `docs/runbooks/devloop-validation.md` — assertion literal deleted in favour of the suite's own count; re-measured range with conditions; **[review]** OPS-1/OPS-3/OPS-4 fixes by me under the operations trailer, and **three additions authored by @operations themselves** (§6.5 `--all-targets`, a new §4 grep-triage control, §2.1 re-keyed to the dangerous operation)
- `docs/TODO.md` — two new §Observability Debt entries; pre-existing seam-adapter entry updated to RESOLVED-with-both-remedies-ruled-out
- `docs/devloop-outputs/2026-09-08-media-telemetry-deny-policy/main.md` — this file

---

## Devloop Verification Steps

**Real-tree guard runs** (@operations' Gate-2 precondition, extended to the two PII/secrets guards whose gate this diff widens):

```
$ ./target/release/dt-guard media-telemetry-deny --root .
SCOPE: 1 configured directory, 7 .rs files, 24 enumerated macro forms, 0 hits
STATUS=OK REASON=media-telemetry-deny-clean-7-files-1-dirs        rc=0

$ ./target/release/dt-guard rust-no-pii-in-logs --root .
WARNING: crates/ac-service/src/services/token_service.rs:326 [pii_in_error_message] suspected PII identifier user_id (redacted)
STATUS=OK REASON=rust-no-pii-in-logs-clean-83-files               rc=0

$ ./target/release/dt-guard rust-no-secrets-in-logs --root .
STATUS=OK REASON=rust-no-secrets-in-logs-clean-83-files           rc=0

$ ./target/release/dt-guard metric-labels --root .
STATUS=OK REASON=metric-labels-clean-5-files                      rc=0

$ ./target/release/dt-guard rust-instrument-skip-all --root .
STATUS=OK REASON=rust-instrument-skip-all-clean-83-files          rc=0
```

**That `WARNING` is pre-existing, and I verified it rather than asserting it** — stashing the diff, rebuilding the release binary from the unmodified source, and re-running produced byte-identical output. It is `pii_in_error_message` on `users::update_last_login(pool, user.user_id)`, unrelated to the anchors.

**Whole-tree false-positive delta**, since the guards above are diff-scoped and a clean run over 83 changed files is weaker evidence than it looks. Every line the widened gate newly admits, across every `.rs` in the repo:

```
$ comm -23 <(grep -rEn --include='*.rs' '\b(trace|debug|info|warn|error)[[:space:]]*![[:space:]]*[([{]' . | grep -v '^./target/' | sort) \
           <(grep -rEn --include='*.rs' '\b(trace|debug|info|warn|error)![[:space:]]*\('     . | grep -v '^./target/' | sort)
```

803 lines match the new gate against 780 for the old. **All 23 newly-gated lines are ones this diff wrote** — test bodies, doc comments and the two new fixtures — and none carries PII vocabulary, so none can produce a finding. Zero pre-existing lines change classification.

**Whole-tree false-positive sweep for the METRIC-name delimiter widening** — a
separate measurement from the level-name sweep above, and the record previously
cited the level sweep as though it covered this class. It did not: @security's
2026-09-08 sweep covered the brace/bracket form for `trace|debug|info|warn|error`
only. `metric_labels::MACRO_OPENER_RE` widens the SIX METRIC names, and because
that anchor is a finding-producing discovery loop rather than a line gate, the
sweep is load-bearing rather than corroborative. Run independently by
@infrastructure and by @operations, and by me before landing the fix:

```
$ comm -23 <(grep -rEn --include='*.rs' '\b(describe_counter|describe_gauge|describe_histogram|counter|gauge|histogram)[[:space:]]*![[:space:]]*[([{]' . | grep -v '^./target/' | sort) \
           <(grep -rEn --include='*.rs' '\b(describe_counter|describe_gauge|describe_histogram|counter|gauge|histogram)![[:space:]]*\('     . | grep -v '^./target/' | sort)
```

16 newly-admitted lines, **all 16 written by this diff** — `metric_labels.rs`
test bodies and doc comments, one `media_telemetry_deny.rs` unit assertion, one
new fixture. **Zero pre-existing lines change classification.** My own
pre-landing run of the same class found 2 occurrences tree-wide, both inside
`metric_labels.rs` itself (a doc comment and a test assertion) — consistent, and
recorded as two independent runs rather than collapsed into one claim.

`metric-labels` still reports **`clean-5-files`** after the widening, which is
the number that would move if the file-discovery use of this same static had
pulled new files into the scan set. It did not.

**Post-review real-tree guard runs** (every guard whose matcher this diff moves):

```
media-telemetry-deny         STATUS=OK REASON=media-telemetry-deny-clean-7-files-1-dirs
metric-labels                STATUS=OK REASON=metric-labels-clean-5-files
rust-no-pii-in-logs          STATUS=OK REASON=rust-no-pii-in-logs-clean-83-files
rust-no-secrets-in-logs      STATUS=OK REASON=rust-no-secrets-in-logs-clean-83-files
rust-instrument-skip-all     STATUS=OK REASON=rust-instrument-skip-all-clean-83-files
application-metrics          STATUS=OK REASON=application-metrics-clean
metric-coverage              STATUS=OK REASON=metric-coverage-all-covered
```

**Suites**

| Check | Result |
|---|---|
| `cargo test -p dt-guard` | **500** unit + all integration suites pass, 0 failed |
| `cargo test -p dt-guard --test media_telemetry_deny_e2e` | 5 passed — incl. `real_media_tree_is_clean` and `configured_scope_is_a_real_non_empty_rust_tree` under the widened anchors |
| `cargo test -p mh-service --test media_metrics_integration` | walker passes |
| `scripts/guards/media-telemetry-deny.test.sh` | **92 passed, 0 failed** (was 87 pre-loop, 91 pre-review), now floored by `EXPECTED_CASES=92` |
| `scripts/guards/run-guards.sh` | **41/41 passed**, incl. `validate-cross-boundary-scope` |
| Layer 2 / 3 / 4 / 5 / 6 | OK / OK / N-A / OK / N-A — **superseded; see the Layer-2 note below and @team-lead's pipeline re-run** |

**Layer 2 went RED during review and is fixed.** @infrastructure ran `scripts/lang/rust/lint.sh` and got `STATUS=FAIL REASON=cargo-clippy-failed`: `clippy::panic` is denied workspace-wide (`Cargo.toml:49`) and `clippy.toml` grants `allow-expect-in-tests`/`-unwrap-in-tests` but deliberately **not** `allow-panic-in-tests`, so the `unwrap_or_else(|| panic!(…))` in a test I added in review was a hard error. Replaced with `assert!` plus a plain read, which keeps `{src}` in the message and spends no suppression. `./scripts/lang/rust/lint.sh` now exits 0 with `STATUS=OK REASON=cargo-clippy-passed`.

**Re-measured runtime** (warm, devloop container, four consecutive runs): guard 3.1–3.4 ms; self-test 0.226 / 0.228 / 0.265 / 0.289 s. Runbook updated to `~3–4ms` and `~0.22–0.29s` with the conditions stated, and the hand-copied assertion count **deleted** rather than re-counted — @operations established it is not even derivable by counting, since at least one case increments the pass counter without an assert helper.

**Trailers required on the commit**
- `Approved-Cross-Boundary: infrastructure` — anchor constructs in `media_telemetry_deny.rs`, `telemetry_macros.rs` and `metric_labels.rs` (plus that file's delimiter-generic body scanner); `metric_macros.rs`'s canonical-home doc and the re-homed anti-narrowing pin; and the manifest OWNERSHIP-block narrowing, which infrastructure authored as owner. Membership is untouched throughout, so the matcher blast radius is shape-only
- `Approved-Cross-Boundary: security` — `telemetry_macros.rs` detection surface
- `Approved-Cross-Boundary: observability` — `Level` membership unchanged at exactly {trace, debug, info, warn, error}, `LogMacro` still out of `Level`, only the anchor around the alternation moves; the c0538b56 freeze is not in play
- `Approved-Cross-Boundary: media-handler` — the walker hunk in `mh-service` (the DO-NOT-RETIRE banner and, at review, the shape matcher + missing names)
- `Approved-Cross-Boundary: operations` — the OPS-1/OPS-3/OPS-4 hunks in `docs/runbooks/devloop-validation.md`, approved on-thread by @operations as the file's owner; @operations' own three additions to that file (§6.5, §4, §2.1) carry no trailer and need none, because a trailer records a NON-owner editing an owned surface (see the classification row)

---

## Code Review Results

**The diff roughly doubled during review — 710 insertions / 15 files to ~1300 /
22.** Per @team-lead's direction, the split between reviewer-driven growth and
my own is stated rather than left for a reader to infer. **All of it was
reviewer-driven; none was self-initiated scope.** The one thing I raised
unprompted was re-pricing my OWN deferral, and I did it because the Lead warned
me it would be attacked — which is not the same as finding it myself.

### Three behaviour changes the review round added

1. **`metric_labels::MACRO_OPENER_RE` delimiter half + delimiter-generic body
   scanner.** Planned as a deferral on a "task-sized body scanner" argument.
   Re-priced at ~6 lines (read one opener byte, derive its closer, compare the
   depth loop against variables). @observability F4 and @operations OPS-7 both
   independently reached the same number. This closed a live ADR-0029 leak path:
   `counter!{"m", "email" => user_email}` was never label-checked.
2. **Fail-loud unknown-opener path** (@observability F8 + @infrastructure F6,
   found independently). My own fix shipped `_ => continue` under a comment
   claiming it would "red instead of mis-parse". It does neither — it drops the
   invocation and the guard reports `STATUS=OK`. Now routes to
   `PARSE_ERROR_RULE_ID`. **I introduced this defect in this session, in a hunk
   whose whole subject is guards that report clean over unchecked scope.**
3. **`mh-service` walker shape matcher** (@observability F1 + @dry-reviewer F2,
   found independently). The walker matched literal `name!(` substrings, so it
   missed the whitespace and delimiter classes this diff spends itself closing —
   and omitted `print!`, `eprint!`, `warn_span!`, `error_span!`, `trace_span!`
   outright. It is the seam adapter's ONLY coverage, and the DO-NOT-RETIRE
   banner this diff added is what made it read as parity. Level macros were also
   promoted from transitive to direct coverage (@observability F7): the trigger
   the old comment named had **already fired twice** unnoticed.

### One factual error in my own argument

@observability F3: I wrote that `tracing` evaluates field expressions "at the
call site regardless of subscriber interest". **False** — `valueset_all!` sits
inside the enabled arm. They supplied the vendored source. The corrected
argument is stronger, because the cost that survives (`level_enabled!`,
`CALLSITE.interest()`, `Dispatch::current()`) IS the registry lookup §11 names.
Worth recording that this was in the one paragraph written to answer a sceptical
reader, where a refutable premise would have discredited the whole section.

### The pattern across the round

Nearly every finding was of one class: **prose that outlived its support and
still read as authoritative** — stale anchor spellings in four fixtures and two
doc comments, a residual ordinal contradicting the manifest it points at, a
"reasoning lives once" claim in a file with three copies, an Outstanding list
that had gone stale in the direction that loses the items still outstanding.
That is this devloop's own subject, found throughout its own artifacts.
Recording it because the Lessons Learned entry written at planning predicted
exactly this and I still shipped a round's worth.

---

## Accepted Deferrals

Three, all reviewer-accepted. Any verdict carrying one of these is **RESOLVED-DEFERRED**, not RESOLVED-FIXED. Bodies live in `docs/TODO.md`; these are pointers.

- `docs/TODO.md` §Observability Debt, entry "Two remaining matcher anchors are narrower than Rust's grammar" — `metric_macros.rs:152` / `:180`, both anchor classes. Accepted by @observability and @infrastructure **on a corrected reason**: @infrastructure's consumer audit disproved the entry's original "task-sized body scanner" (all four consumers are `captures_iter` capture-group reads, so these are the cheapest shape in the class, not the most expensive); the real cost is a full-tree sweep plus pin updates across four consumer guards. The corrected reason is recorded in the entry, because a deferral resting on a false mechanism is how this recurs.
- `docs/TODO.md` §Observability Debt, same entry — `rust_log_secrets.rs:218`'s raw `contains` Check-4 gate. Raised by @security S3, accepted by @infrastructure with two corrections now carried in the entry: this diff introduced a **divergence and not a hole** (Check 4's coverage is byte-unchanged), and the obvious fix is a **trap** because the `contains` is wider on spelling than `TRACING_NAMED_RE`, so swapping it in would silently narrow a secrets guard.
- `docs/TODO.md` §Observability Debt, entry "Three `dt-guard` guards report `STATUS=OK` over a scope they did not check" — the `application_metrics` / `metric_coverage` vacuity class, pre-existing from Gate 1. Exit-semantics change across four services; needs @operations on the implementer-vs-operator lane decision and a runbook row in an operations-owned file.

## Rollback Procedure

**Safe to revert in isolation** — verified by @operations: no runtime code, no
wire format, no migration, no dependency change, and every code change here
only WIDENS matchers, so a revert cannot red anything that is green today.

**What a revert re-opens, so the decision is made with the cost visible:**
- the whitespace-before-`!` evasion on both `media_telemetry_deny` anchors
  (`info !("x")` compiles, logs, and walks through the deny);
- the whitespace and delimiter evasions on `LOG_MACRO_RE` / `TRACING_NAMED_RE`,
  which are **line gates** in `rust_pii.rs` and `rust_log_secrets.rs` — a miss
  there skips the entire PII and secrets scan for that line, not one finding;
- the delimiter evasion on `metric_labels::MACRO_OPENER_RE`, which is the
  discovery loop feeding the label PII checks — `counter!{"m", "email" =>
  user_email}` ships an unbounded PII-bearing label with nothing firing
  (ADR-0029).

**DO NOT cherry-pick the `mh-service` walker hunk out of a partial revert.**
`crates/mh-service/tests/media_metrics_integration.rs` is the ONLY coverage for
`webtransport/media_transport.rs` (§INCOMPLETE BY DESIGN in the manifest).
Dropping that hunk alone leaves every guard in the repo green while the seam
adapter loses its only control — the precise failure this devloop exists to
prevent, performed by the revert.

If this devloop needs to be reverted:
1. Start commit: `c0538b56f295b9bd3b36138be7e227243afd66a6`
2. Review: `git diff c0538b56..HEAD`
3. Soft reset: `git reset --soft c0538b56`
4. Hard reset: `git reset --hard c0538b56`

---

## Issues Encountered & Resolutions

**Self-inflicted data loss during review: `git checkout` on a modified file.**
While mutation-testing the new derivation test in `metric_labels.rs`, I broke
the derivation deliberately, confirmed the test redded, then ran
`git checkout crates/dt-guard/src/metric_labels.rs` to undo the mutation. That
reverts to HEAD, not to the pre-mutation working state, so it destroyed every
uncommitted review fix in that file — the G6/G10 anchor work, the
delimiter-generic body scanner, the fail-loud unknown-opener path,
`expected_close`, and five tests.

**This is enumerated in `docs/runbooks/devloop-validation.md` §8**, which I had
read earlier in this same session while fixing OPS-3 in that very file:
*"A scratch-state cleanup used `git checkout -- <path>`, which reverts to HEAD
and cannot see whose uncommitted delta it is destroying. Snapshot-and-restore
instead."* Knowing the runbook row is not the same as applying it.

**Contained and fully recovered.** One file; the other 17 were untouched;
nothing was committed, so nothing entered history; no other agent's work was in
that file. The `metric_macros.rs` half of @dry-reviewer F5's pin re-home had
already landed in a different file and survived. Reported to @team-lead
immediately with the tree marked NOT stable, rather than quietly rebuilt —
a silent repair would have meant reviewers signing off on code that had briefly
ceased to exist, which @code-reviewer independently caught mid-review and
correctly refused to issue a verdict over.

Reconstructed from the review record and re-verified from scratch. **The
mutation test was then re-run using `cp` snapshot-and-restore**, which is the
remedy the runbook names; it confirmed the derivation test reds on un-derivation
and passes after restore, with `git diff --stat` showing the file intact.

Worth recording beyond the mechanics: the loss happened while *verifying* a test
whose entire purpose is catching a silent regression, and the verification method
caused one. The runbook row exists because someone else already did this.

---

## Lessons Learned

*(Filled at Gate 3; one entry recorded during planning because it is about the planning itself.)*

**"Tests pass" and "Layer 2 passes" are different claims, and the gap is `--all-targets`.**
`cargo test -p dt-guard` compiles the test code, runs 500 assertions, and says
nothing about `clippy::panic` — which is denied workspace-wide at
`Cargo.toml:49`. Layer 2 runs `cargo clippy --workspace --all-targets`, and
`--all-targets` is what pulls in the `lib test` target where a test-only lint
violation lives. Two of us read a green `cargo test` as covering a red Layer 2.
@infrastructure has asked for this in the validation runbook and it belongs
there: the failure is generic, not specific to this diff.

**My own verification harness had the defect this loop exists to fix, and that
is the entry worth keeping.** I checked clippy with
`cargo clippy -p dt-guard --all-targets 2>&1 | grep -E "^error"`. The
invocation was correct — that command *does* surface the error. The **grep** was
wrong: clippy emits ANSI colour through a pipe, so the line begins
`\x1b[1m\x1b[91merror`, and `^error` matched zero lines against output
containing a hard failure. I reported "clippy clean" on the strength of a
matcher narrower than the thing it was matching, with no signal that it had
matched nothing.

**It is now a runbook control rather than only a lesson**, which is the part
that outlives this loop. @operations promoted the ANSI finding into a new §4
subsection — *"Never triage a tool by grepping its stdout — read the exit code
and the STATUS token"* — generalized past clippy, with the key sentence being
that **zero matches is indistinguishable from zero errors**, plus
`CARGO_TERM_COLOR=never` as the kill-it-at-the-source remedy if you must grep.
They measured the `--all-targets` claim rather than restating mine
(`cargo clippy -p dt-guard -v` passes `--test` 0 times; with `--all-targets`, 10),
so `#[cfg(test)] mod tests` is entirely unlinted without the flag. And they
re-keyed §2.1 from victim to operation, right-sizing the remedy to a two-line
`cp`/`mv` for the single-file case — on the reasoning that **a remedy heavier
than the task is a remedy that gets skipped**, which is a fair diagnosis of why
I did not reach for the `mktemp` recipe.

That is precisely the class this devloop spent 1400 lines closing — an anchor
too narrow, a clean result read as evidence rather than as the absence of a
match, and no self-check to distinguish the two. I widened five matchers for it
and then shipped it in my own tooling within the same session. The remedy is the
one the guards already use and I did not: **assert on exit codes, not on
filtered output.** Re-verification was done that way
(`lint.sh` → exit 0 → `STATUS=OK`), and every layer script was run rather than
approximated with a `cargo` command.

**THE LOOP'S ACTUAL FINDING: four half-fixes, four authors, and no author caught
their own.** This is stated as a structural result rather than as a confession,
because @operations pushed back on an earlier draft that framed it as a fact
about me, and they were right — "the same mistake twice" teaches nothing, while
four independent instances across four people is evidence.

1. @operations extended the `THIS IS A DIFF DEFECT` pin to five of seven
   precondition-class tokens. Caught by nobody until they re-derived
   `Rule::is_content()` themselves — and the two it missed were the two where
   the prose being pinned was actually wrong.
2. I closed the `metric_labels` delimiter class **in the code and not in the
   record**, leaving `docs/TODO.md` and `main.md` describing it as outstanding.
3. I scoped the ownership reconciliation at `main.md:93` and **not at its twin
   `:565`**, so the trailer's own justification claimed coverage the
   classification row explicitly denied.
4. @team-lead caught the fourth, and it was a premise @operations and I had both
   asserted: that `main.md` is *"the record of the validation, not an input to
   it."* **False.** Layer 3 runs `validate-cross-boundary-scope` and
   `validate-todo-tracking`, and both READ `main.md` — they are the two guards
   that caught my unclassified files and my inlined deferral bodies earlier the
   same day. A `main.md` edit can red Layer 3, so the cheap-unfreeze path we
   both reasoned toward never existed. True of most records, false of this one.

That is a better argument for a second same-domain reviewer than the assertion
sitting three paragraphs below, because it is four independent instances rather
than a claim. Note the shape is identical every time: **a fix or a check applied
correctly at one site and not at its twin**, by someone who had just demonstrated
they understood the principle.

**And the sharpest artifact this loop produced is @operations', not mine.**
Closing out a fully green run, they reached for `grep -cE "^STATUS=OK"` and got
**zero matches against 41/41 passing** — ANSI again, plus a token shape they had
assumed. They caught it only because they had the exit code in hand. That was
roughly **one hour after they authored the §4 runbook control that forbids
exactly this**, and they put it on the record themselves rather than quietly
using the exit code and moving on.

It matters more than my Layer-2 miss because mine is dismissible as
carelessness and theirs is not. An author cannot be careless about a rule they
wrote sixty minutes earlier. What it demonstrates is that **the adversary is a
reflex, not ignorance** — grepping a tool's stdout is what your hands do before
the rule loads — and a control has to beat a reflex, which no confidently-worded
rule achieves on its own. That is why the fix belongs in §4 of the runbook as a
standing control with a kill-it-at-the-source remedy (`CARGO_TERM_COLOR=never`),
and not in a Lessons Learned entry that only the people already involved will
ever read.

**The countermeasure that worked, every single time, was someone other than the
author re-running the thing.** @infrastructure audited all 21 deleted lines of
the reconstructed file against HEAD rather than checking that the new code was
present — the one test a findings-driven rebuild cannot pass from the inside,
since it is silent about anything the author did not know was lost. @operations
ran the sweep the diff's own conditions demanded and that the record omitted.
@security re-ran a sweep rather than inheriting it. @code-reviewer withheld a
verdict over a file that had briefly ceased to exist. Not one of these was
caught by its author, including all three of mine.

**Two observability specialists caught each other's errors and neither caught their own.** I let "no membership set moves" — the right test for a *membership* change — stand in for an *anchor* question, and later cited a co-sign as authority after it had been withdrawn. @observability sized a false-positive sweep nobody had run and an owner decision nobody had asked for, and that sizing was then quoted back at them as a deferral's justification. @security accepted a deferral on a cost premise, then disproved the premise themselves. Each of the three was found by someone other than its author. The failures were the same shape as the one this loop exists to guard against — a conclusion outliving its support and continuing to read as sound — applied to review reasoning instead of to a control. The argument for a second same-domain reviewer is that the class of error nobody catches in themselves is not a rare one.

---

## Gate 2 — Validation (resumed session, 2026-09-08) — **SUPERSEDED, DO NOT READ AS CURRENT**

> **This record certifies a tree that no longer exists.** It was taken at
> **710 insertions across 15 files**, before the review round. The review round
> took the diff to roughly **1300 insertions across 22 files**, including three
> behaviour changes that did not exist when this ran: the `metric_labels`
> delimiter widening plus its delimiter-generic body scanner, the fail-loud
> unknown-opener path, and the `mh-service` walker's shape matcher.
>
> @team-lead is re-running the full pipeline and will append the authoritative
> result below. Reading the block below as though one pass covered both trees
> is exactly this devloop's subject — a verdict outliving the thing it
> verified — so it is marked rather than deleted, and the diff size it actually
> covered is stated so the two cannot be confused.

### Original record (tree at 710 insertions / 15 files)

Session was interrupted after implementation; roster respawned per SKILL §Recovery
(headless infra-interruption exception). Pipeline re-run from scratch, unattended
mode (`DEVLOOP_FAIL_FAST=0`, all seven layers evaluated — no `NOT-RUN`).

```
LAYER=1 RESULT=OK   DURATION=2
LAYER=2 RESULT=OK   DURATION=2
LAYER=3 RESULT=OK   DURATION=48
LAYER=4 RESULT=N/A  DURATION=172
LAYER=5 RESULT=OK   DURATION=2
LAYER=6 RESULT=N/A  DURATION=2
LAYER=7 RESULT=OK   DURATION=247
TOTAL_DURATION=475 TOTAL_RESULT=N/A
```

`TOTAL_RESULT=N/A` is worst-child aggregation over the two **documented
intentional-gap placeholders**, not a failure and not an unmeasured layer:

- **Layer 4** — `cargo-test-passed` + `nx-test-passed`; the `N/A` is proto's
  registered placeholder (`STATUS=N/A REASON=not-applicable-to-this-lang`,
  proto has no `test.sh` per the always-run matrix), aggregated as
  `test-aggregate-na`. Self-justifying per SKILL §Layer N/A justification.
- **Layer 6** — `cargo-audit-passed` + `buf-breaking-passed`; `pnpm audit`
  reported `SKIPPED-NO-DIFF REASON=no-dep-changes` (the only remaining
  `SKIPPED-NO-DIFF` producer), proto's audit placeholder `N/A`.

No layer reported `FAIL`, `PRECONDITION_FAILURE` or `FAIL-MISSING-VERB`. Layer 3
ran all guards green including `media-telemetry-deny-selftest-passed`; Layer 7 ran
both Phase-2 suites (`env-tests-passed`, `browser-e2e-passed`). Gate 2 **PASS**,
attempt 1, no retries.


---

## Gate 2 — Validation (AUTHORITATIVE, 2026-09-08)

**PASS**, run by @team-lead against a FROZEN tree and bound to it by hash:
**`md5 f17efbbe`, 18 files, +1469/−123** — verified identical before and after
the run, so this verdict describes a tree that existed for the whole pipeline.

```
LAYER=1 OK  27    LAYER=2 OK  2     LAYER=3 OK  47    LAYER=4 N/A 175
LAYER=5 OK  1     LAYER=6 N/A 2     LAYER=7 OK  279
TOTAL_DURATION=533 TOTAL_RESULT=N/A
```

39 `STATUS=OK` lines; **zero** `FAIL`, `PRECONDITION_FAILURE`,
`FAIL-MISSING-VERB` or `NOT-RUN`. The two `N/A`s are the documented
intentional-gap placeholders (proto carries no `test.sh` or `audit.sh`), plus
`pnpm audit` reporting `SKIPPED-NO-DIFF REASON=no-dep-changes` —
self-justifying per SKILL §Layer N/A justification.

**Why this run and not the two before it.** The first re-run was discarded by
@team-lead despite reporting clean: it started at +1393 and my Layer-2 clippy
fix plus @operations' three runbook additions landed mid-run. A green result
over a moving tree is not evidence — this loop's own finding, applied by the
Lead to the Lead. The freeze, the hash, and the before/after comparison are what
make this record binding rather than merely favourable.

**A `main.md` edit is a Gate-2 INPUT, not just its record.** @operations and I
both argued the held record edits were cheap because `main.md` is "the record of
the validation, not an input to it." @team-lead ruled that false: Layer 3 runs
`validate-cross-boundary-scope` and `validate-todo-tracking`, and **both read
this file** — they are the two guards that caught the unclassified files and the
inlined deferral bodies earlier the same day. A `main.md` edit can red Layer 3,
so the cheap path never existed and the honest cost of landing the three queued
edits is a full re-run.

**Consequence, stated plainly so this section cannot be misread as final.** The
three queued record edits — the `:565` trailer scoping, the reflex-evidence
paragraph and the four-author finding in §Lessons Learned — were landed AFTER
the run recorded above, together with this section itself. **They therefore
invalidate it**, by the same rule that invalidated the two runs before it.
@team-lead re-hashes the tree and re-runs the full pipeline, and commits only if
it is green over an unchanged tree. Until that lands, the verdict above is bound
to `md5 f17efbbe` and to nothing later.

---

## Gate 2 — Validation (FINAL AUTHORITATIVE, 2026-09-08) — the record this commit ships on

**PASS.** Run by @team-lead over the post-record-edit tree, which is the tree in
the commit. This supersedes every Gate-2 section above; those remain in the file
because the sequence of discarded runs *is* the finding, not clutter.

```
LAYER=1 OK  1     LAYER=2 OK  2     LAYER=3 OK  48    LAYER=4 N/A 182
LAYER=5 OK  1     LAYER=6 N/A 2     LAYER=7 OK  274
TOTAL_DURATION=510 TOTAL_RESULT=N/A
```

39 `STATUS=OK`; **zero** `FAIL`, `PRECONDITION_FAILURE`, `FAIL-MISSING-VERB` or
`NOT-RUN`. Unattended mode (`DEVLOOP_FAIL_FAST=0`), so all seven layers were
evaluated — no layer is merely unmeasured. The two `N/A`s are proto's registered
intentional-gap placeholders plus `pnpm audit` `SKIPPED-NO-DIFF
REASON=no-dep-changes`, self-justifying per SKILL §Layer N/A justification.
Layers 1–6 ran on attempt 1; Layer 7 ran both Phase-2 suites green
(`env-tests-passed`, `browser-e2e-passed`) on attempt 1. No retries, no operator
lane.

**Binding, with its own limit stated.** Two hashes taken before and after, both
identical across the run:

| Scope | Hash | Before | After |
|---|---|---|---|
| `git diff` + `git status --porcelain` (tracked content + untracked *names*) | `f17efbbe` | ✓ | ✓ |
| `docs/devloop-outputs/2026-09-08-media-telemetry-deny-policy/main.md` | `f5ca4762` | ✓ | ✓ |

**Correction to the section above, and it is the sixth instance of this loop's
pattern — this one mine.** The previous section says the run was "bound to it by
hash: `md5 f17efbbe` … verified identical before and after." That hash is
computed over `git diff` plus `git status --porcelain`. `git diff` covers tracked
files only, and `--porcelain` lists untracked paths by *name* without hashing
their contents. `main.md` is untracked, inside an untracked directory. **So the
one artifact I was most concerned to freeze was the one my freeze mechanism did
not cover**, and "the tree is byte-identical" was a stronger claim than the
instrument supported. The conclusion held — @implementer independently verified
by `find -newermt` that nothing moved in the window — but the reasoning was
wrong, which is exactly the failure mode @operations identified one message
earlier: *a claim that is accidentally right never surfaces as a failure.* I
asserted it about their reasoning and then reproduced it in my own instrument,
inside the same hour. This run adds the second hash so the binding is real
rather than assumed.

**Gate 2 PASS. Proceeding to Gate 3 and commit.**

---

## Gate 3 — Final Approval (2026-09-08)

All nine reviewers returned verdicts. **Zero escalations.** Every cross-boundary
hunk was confirmed by its owner, on the panel, on-thread.

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-DEFERRED | 3 | 2 | 1 | `telemetry_macros.rs` detection surface CONFIRMED; widenings verified strict supersets structurally and empirically |
| Test | RESOLVED-DEFERRED | 1 | 1 | 0 (3 accepted, raised by others) | Re-verified the reconstructed `metric_labels.rs` by mutation, not by trusting Gate 2 |
| Observability | RESOLVED-DEFERRED | 10 | 10 | 0 (2 accepted, raised by others) | Second same-domain reviewer; found the walker-coverage gap and the false `tracing` claim |
| Code Quality | RESOLVED-FIXED | 3 (+2 record) | 5 | 0 | Withheld verdict while the tree was mid-morph — correctly |
| DRY | RESOLVED-FIXED | 7 | 7 | 0 | Re-verified G6 from scratch after the reconstruction; one tech-debt entry recorded, not a deferral |
| Operations | RESOLVED-FIXED | 8 | 8 | 0 | Runbook hunk CONFIRMED; authored three runbook controls as owner |
| Semantic Guard | CLEAR | 0 | — | — | Re-derived against the moved tree rather than carried forward |
| Infrastructure (cross-boundary owner) | RESOLVED-DEFERRED | 14 | 13 | 1 | All four machinery hunks CONFIRMED; narrowed its own OWNERSHIP claim |
| Media Handler (cross-boundary owner) | CLEAR | 0 | — | — | Walker hunk CONFIRMED, re-confirmed after it became a behaviour change |

**Totals: 38 findings, 36 fixed, 3 accepted deferrals, 0 escalations.** (Fixed
and deferred are counted per-finding across reviewers; a deferral accepted by
several reviewers appears once.)

### Team-composition ruling, recorded because it changed the outcome

The Gate-1 record states that the disputed manifest-header row was ruled
Machinery/infrastructure because *"option (b) was foreclosed by team composition
— infrastructure is not on this team, so there is nobody who can cede
on-thread,"* leaving the substance explicitly undecided. On resuming this loop
after the session loss, @team-lead added **infrastructure** and **media-handler**
to the Gate-3 panel as the actual owners of hunks the commit trailers certify.

That was not paperwork. @infrastructure's own accounting: their Gate-1 absence
is why `metric_labels`' delimiter half was deferred as "task-sized" on a claim
about a walker nobody had opened, and why the TODO entry told the next author
that the two cheapest anchors in the class were the most expensive. Both were
caught only because the owner was on the panel. They then **narrowed their own
ownership claim in-tree**, closing the G8 deadlock at its source rather than
exporting it to the next loop — and the substance went observability's way, as
the Gate-1 ruling had left open. @code-reviewer, who made the original
monotonicity ruling, confirmed the narrowing is consistent with it.

**Generalisable: a cross-boundary trailer whose owner was never on the team is a
signature without a reader.** Where a devloop's plan requires an
`Approved-Cross-Boundary:` trailer, the owner should be on the panel — not
merely named in the reason clause.

### Accepted deferrals

Three, all with bodies in `docs/TODO.md` and pointer bullets in §Accepted
Deferrals. Every one was accepted against a *corrected* reason after its original
justification was disproved in review — recorded that way deliberately, because
"accepted" and "accepted against a true reason" are different records.

### Lessons Learned — the loop's own recurring defect

Six instances of one pattern, across five authors including the Lead, and **no
author caught their own**:

1. @operations extended a collision pin to five of seven precondition tokens.
2. @implementer closed the delimiter class in code but not in the record.
3. @implementer scoped a classification at `:93` but not at its twin `:565`.
4. @implementer reported "clippy clean" from `grep -E "^error"` that matched
   **zero lines against a hard error**, because cargo emits ANSI through a pipe.
5. @operations ran `grep -cE "^STATUS=OK"` and got zero against a 41/41 green
   run — one hour after authoring the §4 control forbidding exactly that.
6. @team-lead bound a freeze to a hash that does not cover untracked file
   contents, i.e. does not cover `main.md` — the artifact the freeze was for.

@operations' refinement is the sharpest form of it: their "a record section is
not a thing Gate 2 can red" was **false reasoning that produced the right
answer**, because their edits happened to fall outside the two guards' parse
scope. A claim that decays eventually surfaces as a failure; a claim that is
*accidentally right* never does, and the next reader applies the reasoning, not
the conclusion.

The countermeasure that worked was never more careful writing. It was **someone
other than the author re-running the thing** — which is the argument for the
second same-domain reviewer, now resting on six instances rather than one
anecdote.

**Two incidents, and how they are scored.** @implementer destroyed 293 lines of
uncommitted work with `git checkout <path>` while mutation-testing, and reported
a green Layer 2 that was red. Both were self-reported immediately, the tree was
marked unstable rather than quietly repaired, and the reconstruction was
independently audited by @infrastructure (all 21 deletions against HEAD, later
all 58 across five files), @operations (against notes taken *before* the loss)
and @dry-reviewer (from scratch) — zero collateral loss, and the rebuilt code is
better than what was lost, since the original `_ => continue` asserted the
opposite of its own comment. The handling is the behaviour the working
conventions ask for and is not scored as the failure. The mechanisms are now
runbook controls, which is a better home than a lessons-learned anecdote.

**Follow-up named, not silently dropped** (@operations, owner): `docs/runbooks/
devloop-validation.md` §6.3 should state that `main.md` is a Layer-3 input, name
the two guards that read it (`validate-cross-boundary-scope`,
`validate-todo-tracking`), name the two sections they actually parse
(`## Cross-Boundary Classification`, §Accepted Deferrals) and the
self-referential exemption at `cross_boundary_scope.rs:65`. Deliberately not
landed here: it is a new finding about pre-existing prose, and a fourth edit
would have invalidated a re-run already paid for.

---

## Gate 2 — binding correction, and how the re-run regress was terminated

The FINAL AUTHORITATIVE section above binds to `main.md` `md5 f5ca4762`. Landing
the §Gate 3 record then changed that hash, so a fourth full run was made over the
resulting tree:

```
LAYER=1 OK 1   LAYER=2 OK 2   LAYER=3 OK 48   LAYER=4 N/A 178
LAYER=5 OK 1   LAYER=6 N/A 2  LAYER=7 OK 242
TOTAL_DURATION=474 TOTAL_RESULT=N/A
```

39 `STATUS=OK`; zero `FAIL`, `PRECONDITION_FAILURE`, `FAIL-MISSING-VERB` or
`NOT-RUN`. Both hashes identical before and after: tracked-diff `f17efbbe`,
`main.md` `1b2e5725`. **That is the run this commit ships on.**

**Terminating the regress, with the reasoning stated rather than assumed.** Every
`main.md` edit invalidating the run that preceded it is a genuine regress, since
this section is itself such an edit. It is closed here — and *not* by appealing
to "docs edits are harmless", which is precisely the false-but-convenient
reasoning @operations retracted and which is instance 5 of the pattern below.

@team-lead read the two guards rather than inheriting the claim:
`cross_boundary_scope.rs:167` calls `parse_table_under_heading(content,
"Cross-Boundary Classification")` — that table only. `todo_tracking.rs:149`
checks "Rule 2 — main.md §Accepted Deferrals pointer-only discipline" — that
section only. **Both of those sections are unchanged since `1b2e5725`**; this
append adds a new `## Gate 2 …` heading that neither guard parses. So the Layer-3
inputs are byte-identical to the validated tree even though the file is not, and
the run above still binds to everything Gate 2 can actually see.

The general rule, since a bare "I checked" would leave the next reader where
@operations was: **a docs edit is Gate-2-safe only if it leaves every
guard-parsed region byte-identical — which requires knowing which regions those
are.** That is why the named follow-up below (documenting the parsed sections in
the runbook) matters more than it looks: today the distinction exists only in
Rust source, and three people in this loop got it wrong reading around it.
