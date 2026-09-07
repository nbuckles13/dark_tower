# Devloop Output: Media-Path Telemetry Deny Guard (ADR-0036 §11)

**Date**: 2026-09-07
**Task**: Mechanism half of ADR-0036 §11's media-path telemetry deny guard — `dt-guard media-telemetry-deny` subcommand + auto-discovered Layer-3 wrapper + config manifest + fixtures + self-test
**Specialist**: infrastructure (paired with observability)
**Mode**: Agent Teams (v2)
**Branch**: `feature/hear-yourself-through-handler`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c0228455c17f5e7cec7eb030a3c1b790314b470e` |
| Branch | `feature/hear-yourself-through-handler` |
| Lead Model | `claude-opus-5` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (infrastructure) |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `paired-observability` (paired) |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
-->

---

## Task Overview

### Objective
Build the **mechanism** half of ADR-0036 §11's media-path telemetry deny: a `dt-guard`
subcommand plus an auto-discovered Layer-3 wrapper that denies log, print, metric, event and
span macro *invocation forms* (including `event!`, `span!`, `*_span!` and `#[instrument]`)
inside a config-driven directory list seeded with `crates/mh-service/src/media/`, while
explicitly allowing cached-handle method calls. The guard must **fail on its own scope**, with
distinct reason tokens for a configured directory that does not exist and one that exists with
zero `.rs` files — so a rename cannot silently disarm it.

### Scope
- **Service(s)**: none. This is guard tooling (`crates/dt-guard/`) plus its Layer-3 wiring.
  `crates/mh-service/src/media/**` is the guard's *subject* and was **not edited** — zero rows
  in the classification table and zero lines in the diff.
- **Schema**: No.
- **Cross-cutting**: Yes — paired with `observability` (policy content), with `security`
  co-signing three guard-module re-points and `operations` owning the runbook rows.

### Debate Decision
NOT NEEDED — ADR-0036 §11 already specifies the control (directory-scoped deny, deny the event
macro, allow cached-handle calls) and ADR-0034 specifies the shape (subcommand-per-policy, thin
wrapper, canonical-home `Lazy<Regex>`). The two decisions that were genuinely open — the STATUS
lane for the scope tokens, and where the promoted macro vocabulary lives — were settled at
Gate 1 by the reviewer panel and @main, and are recorded in §12 with their reasoning.

---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/dt-guard/src/media_telemetry_deny.rs` (new — policy module) | Mine | — |
| `crates/dt-guard/src/telemetry_macros.rs` (new — promoted vocabulary home, sibling of `metric_macros.rs`; module is machinery, **membership is policy**) | Not mine, Domain-judgment | observability |
| `crates/dt-guard/src/common/scope.rs` (new — `assert_scope_live`, the S16b canonical predicate) | Mine | — |
| `crates/dt-guard/src/common/mod.rs` (one-line `pub mod scope;` registration) | Mine | — |
| `crates/dt-guard/src/common/status.rs` (add `emit_scope`) | Mine | — |
| `crates/dt-guard/src/common/test_code_filter.rs` (one lexer, two modes + raw-string support) | Mine | — |
| `crates/dt-guard/src/metric_macros.rs` (`MACRO_NAME_ALTERNATION` `static` -> `pub static`; visibility only) | Mine | — |
| `crates/dt-guard/src/rust_pii.rs` (re-point `LOG_MACRO_RE` + `TRACING_NAMED_RE` level half at the promoted home) | Not mine, Minor-judgment | security + observability |
| `crates/dt-guard/src/rust_log_secrets.rs` (re-point `LOG_MACRO_RE` at the promoted home) | Not mine, Minor-judgment | security + observability |
| `crates/dt-guard/src/instrument_skip_all.rs` (re-point `INSTRUMENT_RE` at `INSTRUMENT_ATTR_BARE_RE`; zero semantic change) | Not mine, Minor-judgment | security + observability |
| `crates/dt-guard/src/common/pii_vocabulary.rs` (comment-only; false `skip_all` premise corrected) | Not mine, Minor-judgment | security + observability |
| `crates/dt-guard/src/release_build_profile.rs` (route `SCOPE:` line through `emit_scope`) | Mine | — |
| `crates/dt-guard/src/no_insecure_browser_flags.rs` (route `SCOPE:` line through `emit_scope`) | Mine | — |
| `crates/dt-guard/src/lib.rs` (module registration) | Mine | — |
| `crates/dt-guard/src/main.rs` (clap `Command` variant + dispatch) | Mine | — |
| `scripts/guards/simple/media-telemetry-deny.sh` (new, thin wrapper, `chmod +x`) | Mine | — |
| `scripts/guards/simple/media-telemetry-deny.yaml` (new manifest — schema + header machinery; **the directory list is policy**) | Not mine, Domain-judgment | observability |
| `crates/dt-guard/tests/fixtures/media_telemetry_deny/**` (new, flat `pos_*.rs` / `neg_*.rs`) | Not mine, Domain-judgment | observability |
| `crates/dt-guard/tests/media_telemetry_deny_e2e.rs` (new — harness is mine, the expectation catalog is policy; classification is per-file so the file takes the higher tier) | Not mine, Domain-judgment | observability |
| `scripts/guards/media-telemetry-deny.test.sh` (new, hermetic self-test) | Mine | — |
| `scripts/layer3.sh` (`run_and_emit` wiring + neighbour-style comment) | Mine | — |
| `docs/runbooks/devloop-validation.md` (§6.3.1 + §8 rows, one per REASON token) | Not mine, Minor-judgment | operations |
| `docs/specialist-knowledge/infrastructure/INDEX.md` (navigation rows) | Mine | — |
| `docs/TODO.md` (§S16b progress note; the four narrow-`#[instrument]` sites; @dry-reviewer verdict-time appends) | Mine | — |

**Ownership notes (CLAUDE.md §Specialists "Guard-crate ownership (interim)"):**

* `crates/dt-guard/**` is **NOT** a Guarded Shared Area. No key is added to
  `scripts/guards/simple/cross-boundary-ownership.yaml` (that file is a GSA mirror;
  `dt-guard gsa-sync` rejects stray keys). The Owner column above is a *review-stringency*
  declaration under ADR-0024 §6.2, not a GSA claim.
* The split applied per-row is **what the edit changes**, not what I had to read:
  module layout, clap dispatch, matcher implementation, manifest *schema*, walker,
  scope-liveness mechanism, wrapper and self-test = machinery = infrastructure (mine).
  The denied-family membership, the allow-list membership, the directory list, the
  reason-token names and the fixture set = policy content = **observability** (I am
  `--paired-with=observability`, which is what a Domain-judgment row requires).
* `crates/mh-service/src/media/**` is **not** in this table and will not be edited.
  If the guard reds on the real tree that is a real §11 violation to report, not a
  reason to weaken the guard (CLAUDE.md §Fail loudly).

---

## Planning

### Gate 1 — Plan Confirmation Tracking

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability (paired) | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

### Implementer Plan

#### 0. Mechanism restatement (required reflection — instance vs mechanism language)

*Instance language*: "deny telemetry macros inside the media directory."

*Mechanism language*: **"a path-scoped forbidden-source-construct guard whose configured scope must
prove itself non-vacuous."** Restated that way, the task's own nouns (media, telemetry, ADR-0036) are
forbidden, and two independent mechanisms fall out:

1. **Forbidden-construct matching over Rust source.** This class is already large and same-owner:
   `rust_secrets`, `rust_pii`, `rust_log_secrets`, `instrument_skip_all`, `no_insecure_browser_flags`,
   `ts_dev_trust`. **More siblings than the task names** — and that is exactly why @dry-reviewer's
   finding #2 lands: two of those siblings already declare the log-level alternation I was about to
   declare a third time. I am acting on the wider class, not the instance (see §2).
2. **Scope liveness — "a guard whose scope is configuration must fail when that configuration
   resolves to nothing."** This is genuinely new *in Rust*, but it has three bash siblings that each
   hand-roll it: `validate-frame-vectors.sh` (vacuity PRECONDITIONs), `validate-subdomain-regex-sync.sh`
   (zero-hit vacuity), `validate-slug-class-sync.sh` (canonical-declaration-renamed). I am **not**
   pre-extracting a `common/scope_liveness` helper: one Rust consumer today, and the three siblings are
   shell, so a shared home would be a false SSoT across languages. **Named extraction trigger**: a
   second config-scoped `dt-guard` subcommand. Recorded here so the trigger is a decision, not an
   omission.
   **SUPERSEDED by A11.6 — do not read this paragraph as the plan of record.** @dry-reviewer showed the
   trigger has already fired *inside this crate*: `docs/TODO.md` §S16b enumerates 12 hardcoded path roots
   across 10 **Rust** `dt-guard` modules waiting on exactly this predicate. My "one Rust consumer today"
   premise was factually wrong — the cross-language argument is sound for the three bash siblings and
   irrelevant to the ten Rust ones. The predicate goes into `common/scope.rs` from the start.

#### 1. ADR-0031-style structured block — the ADR-0036 §11 two-halves table

Every row states the **concrete injected adverse condition** (fires) and the **concrete
premise-confirmation against the real artifact** (applies). Nothing in the right-hand column is
narrated in a comment; every cell is a machine assertion that runs each devloop.

| Gate element | Does it fire? (inject the adverse condition) | Does it apply? (confirm the premise against the real artifact) |
|---|---|---|
| **Metrics family** (from `MacroKind::ALL`) | Plant `counter!("x")` / `describe_gauge!(..)` into a copy of the **real** `crates/mh-service/src/media/` tree → exit 1, `REASON=media-telemetry-deny-macro-in-media-path`. Fixtures `pos_metrics_label_macros.rs`, `pos_metrics_describe_macros.rs`. | Unit test asserts the compiled alternation contains **every** `MacroKind::ALL` name, so a 7th variant widens this guard with zero edits in the policy module. |
| **Tracing / log / event families** | Plant `tracing::info!`, `log::error!`, `event!(Level::INFO, ..)` → red. Fixtures `pos_tracing_levels.rs`, `pos_log_crate_qualified.rs`, `pos_tracing_event.rs`. | `crates/mh-service/src/media/mod.rs` module doc already asserts "no `tracing` or `log` import" as the invariant — the guard mechanises a premise the media owner wrote down, in the directory that holds the forward path. |
| **Span family** | Plant `span!(..)` and `debug_span!(..)` → red. Fixtures `pos_span_bare.rs`, `pos_span_suffixed.rs`. | Same directory; span macros expand to `event!`-adjacent machinery §11 names explicitly. |
| **Print family incl. `dbg!`** | Plant `println!` / `eprintln!` / `dbg!` → red. Fixture `pos_print_family.rs`. | `media/mod.rs:15` names exactly this trio as the invariant. |
| **`#[instrument]` attribute** | Plant `#[instrument]`, `#[tracing::instrument(skip_all)]`, `#[cfg_attr(test, tracing::instrument)]` → red. Fixtures `pos_instrument_bare.rs`, `pos_instrument_qualified.rs`. | `crates/mh-service/src/webtransport/connection.rs` uses `#[tracing::instrument(skip_all, ..)]` **in the immediate sibling of the denied directory** — the realistic spelling is live one directory away, i.e. exactly what an inward refactor would carry in. |
| **Telemetry-crate import deny** | Plant `use tracing::info as note;` + `note!(..)` → red with `REASON=media-telemetry-deny-telemetry-crate-import`. Fixture `pos_import_renamed_tracing.rs`. | Five live `use crate::observability::metrics::…` lines under `media/` must stay **green**; `neg_import_crate_observability_metrics.rs` is a verbatim copy of `forwarder.rs:10`, so loosening the first-segment anchor reds the self-test. |
| **Allow-list (cached handles)** | Fixture `neg_cached_handles.rs` uses all five of `.increment/.record/.set/.absolute/.decrement` → guard stays green (`assert_absent` on any FAIL token). | **The strongest applies-evidence in this task**: the real tree already calls `.increment(1)` at `media/forward.rs:212,275,283,…`, `.set(..)` at `forward.rs:408`, `.record(..)` at `forward.rs:485` and `ingress.rs:170,174`. A guard that touched handle methods would red the real tree on its first run — §11's "it bans the pattern it exists to enforce", demonstrated rather than asserted. |
| **Comment / string-literal exclusion** | Fixture whose only hits are in `//`, `///`, `//!`, `/* */` and inside a string literal → green. | `media/forward.rs:97` is a doc comment reading ``a `debug!(?frame)` in``, and `media/mod.rs:14-16` + `ingress.rs:34` spell `counter!`, `println!`, `event!`, `#[instrument]` in prose. **A raw line matcher reds the real tree on day one.** The real tree is the fixture. |
| **Scope liveness — missing** | Synthetic root whose manifest names a directory that does not exist → exit 1, `REASON=media-telemetry-deny-scope-directory-missing`. | In-tree premise pin (Rust integration test): every manifest entry resolves, is a directory, and holds ≥1 `.rs` file — **7 today**. Machine-checked every devloop, not asserted in the header. |
| **Scope liveness — empty** | Synthetic root, directory exists holding only `README.txt` → exit 1, `REASON=media-telemetry-deny-scope-directory-empty`, asserted **distinct** from the missing token. | Same pin; the two tokens split the two opposite operator first-actions (OP-3). |
| **Scope liveness — manifest truncated** | Manifest with `denied_directories: []` (or all entries commented out) → exit 1, `REASON=media-telemetry-deny-scope-no-directories-configured`. | A one-line disarm with no other detector (@security S8); the in-tree manifest has exactly one entry, so the guard is armed rather than perpetually vacuous. |
| **No suppression path** | `pos_ignore_annotation_not_honored.rs` plants a denied macro carrying `// guard:ignore(long enough reason)` and asserts it **still** reds. | Without the fixture, "we honour no ignore marker" is prose. `crates/dt-guard/src/ignore.rs` is deliberately **not** imported by this module. |

#### 2. Module layout — and one deliberate deviation from the task's literal wording

The task says the tracing/log/print/span list gets "its own canonical-home list **in the new module**".
@dry-reviewer's finding #2 shows that taking that literally produces the **third** in-tree declaration
of `\b(info|debug|warn|error|trace)!\s*\(` (`rust_pii.rs:45`, `rust_log_secrets.rs:52`) — i.e. it would
violate the instruction's own purpose ("do not re-inline it elsewhere") while satisfying its letter.

**Recommendation (needs @team-lead ruling at Gate 1):** put the vocabulary in a new sibling module
`crates/dt-guard/src/telemetry_macros.rs`, an exact structural mirror of the crate's existing
family-vocabulary precedent `crates/dt-guard/src/metric_macros.rs` — same crate level, same
`ALL`-derives-the-alternation shape. The list is still new, still canonical, still declared once; only
its *address* deviates. Fallback if @team-lead prefers the literal reading: declare it inside
`media_telemetry_deny.rs` as `pub` and accept the third copy as a recorded `docs/TODO.md` entry — I do
not recommend this.

**Groups are separate so promotion does not create a false SSoT** (@dry-reviewer's own caveat). Each
consumer selects the groups it denies:

| Group | Members | Consumers |
|---|---|---|
| `Level` | `trace` `debug` `info` `warn` `error` | this guard **+** `rust_pii::LOG_MACRO_RE` **+** `rust_log_secrets::LOG_MACRO_RE` (byte-identical set → behaviour-preserving) |
| `Event` | `event` | this guard only |
| `LogMacro` | `log` (the `log!` form) | this guard only — kept out of `Level` precisely so re-pointing `rust_pii` does not silently widen it |
| `Span` | `span` `trace_span` `debug_span` `info_span` `warn_span` `error_span` | this guard only |
| `Print` | `print` `println` `eprint` `eprintln` `dbg` | this guard only |

`telemetry_macros.rs` also hosts `INSTRUMENT_ATTR_BARE_RE` (`#\[instrument`) — the *narrow* historical
shape — so `instrument_skip_all.rs`, `rust_pii.rs` and `rust_log_secrets.rs` can re-point their three
`INSTRUMENT_RE` copies at one home **with zero semantic change**. This guard does **not** use it; it uses
its own wider attribute matcher (§4), so @paired-observability's C2 holds: `instrument_skip_all`'s gap
is not widened here and stays theirs to carry.

Not denied, and stated in a comment so nobody widens it later: `format!`, `write!`, `writeln!`,
`assert*!`, `panic!`, `todo!` (@paired-observability A3). **@security S7 answer: `write!`/`writeln!` are
OUT — on the record, by decision, not by omission.**

#### 3. Files

| # | File | What |
|---|---|---|
| 1 | `crates/dt-guard/src/telemetry_macros.rs` (new) | Vocabulary canonical home: `TelemetryMacro` + `TelemetryGroup`, `ALL`, `alternation_for(&[groups])`, `LEVEL_MACRO_RE`, `INSTRUMENT_ATTR_BARE_RE` + `INSTRUMENT_ATTR_ANY_RE` (A11.4). No policy. |
| 2 | `crates/dt-guard/src/media_telemetry_deny.rs` (new) | The policy: manifest load, scope liveness, matchers, `run(&root, explain)`. |
| 3 | `crates/dt-guard/src/metric_macros.rs` | `MACRO_NAME_ALTERNATION` `static` → `pub static` (visibility only). |
| 4 | `crates/dt-guard/src/{rust_pii,rust_log_secrets,instrument_skip_all}.rs` | Re-point `LOG_MACRO_RE` / `INSTRUMENT_RE` at #1. Value-neutral; their existing unit tests are the pin, plus a new equality test asserting the derived alternation equals the historical literal. |
| 5 | `crates/dt-guard/src/common/test_code_filter.rs` | One lexer, two modes (§4). |
| 6 | `crates/dt-guard/src/{lib,main}.rs` | Module registration; clap `MediaTelemetryDeny { --root, --explain }` + dispatch. |
| 7 | `scripts/guards/simple/media-telemetry-deny.sh` (new) | 4-line wrapper sourcing `_dt_guard_wrapper.sh media-telemetry-deny`, with `# shellcheck source=`. |
| 8 | `scripts/guards/simple/media-telemetry-deny.yaml` (new) | Manifest (§5). Inert to `run-guards.sh` (`find … -name '*.sh'`), per the `cross-boundary-ownership.yaml` precedent. |
| 9 | `crates/dt-guard/tests/fixtures/media_telemetry_deny/**` (new) | @paired-observability's `pos_`/`neg_` set. |
| 10 | `crates/dt-guard/tests/media_telemetry_deny_e2e.rs` (new) | Fixture catalog + the in-tree premise pin. |
| 11 | `scripts/guards/media-telemetry-deny.test.sh` (new) | Hermetic self-test (§7). |
| 12 | `scripts/layer3.sh` | `run_and_emit` wiring + neighbour-style "deliberately NOT under guards/simple/" comment. |
| 13 | `docs/runbooks/devloop-validation.md` | §6.3.1 + §8 rows for every REASON token (OP-3). |
| 14 | `docs/specialist-knowledge/infrastructure/INDEX.md` | One navigation row. |
| 15 | `crates/dt-guard/src/common/scope.rs` (new) | `assert_scope_live` (A11.6). Consumed **here only** this loop per @main's boundary. |
| 16 | `crates/dt-guard/src/common/status.rs` | Add `emit_scope` (A11.7). |
| 17 | `crates/dt-guard/src/release_build_profile.rs` | Route its `SCOPE:` line through `emit_scope`; historical output literal pinned by an equality test. |
| 18 | `crates/dt-guard/src/no_insecure_browser_flags.rs` | Same. **If either #17 or #18 resists a clean re-point, that module is dropped from the re-point and reported — its output shape is not bent to fit the helper** (@main's A11.7 condition). |
| 19 | `docs/TODO.md` | §S16b cross-reference naming `common/scope.rs` as the reference implementation; the four narrow-`#[instrument]` sites + the `rust_pii` Check-3 defect (wording coordinated with @dry-reviewer, circulated to @security + @paired-observability before verdict). |

**The `## Cross-Boundary Classification` table above is authoritative for the file list** (22 rows;
this §3 table groups some of them and exists for the "what" column). Re-walked against it after the
amendment rounds — every file in one appears in the other.

#### 4. Matcher design — answers to C1–C5, S1–S9

* **C1 YES — the anchor is the design.** Macro forms match `\b(<alternation>)!\s*[\(\[\{]`. `\b` (not an
  anchored optional prefix) covers `info!`, `tracing::info!`, `::tracing::info!`, `log::info!`,
  `crate::obs::info!` — `\b` matches after `:` (@security S4). Local `let counter = 0;` at
  `forward.rs:212` and `fn record(&self)` cannot match: no `!`.
  **One widening beyond C1's `!\s*\(`**: `[`/`{` are accepted too, because `println!{"x"}` is legal Rust
  and a `(`-only anchor is a one-character evasion. Zero false-positive cost — `name!` is a macro
  invocation under any delimiter.
* **C3 YES — the allow-list is NOT a runtime subtraction.** Under C1 `.record(x)` structurally cannot
  match `record!(`. The allow-list is implemented as a **named const + the `neg_cached_handles.rs`
  fixture + a unit test asserting each of the five method-call forms produces zero hits** — never a
  line filter, which would mask a real `counter!(..)` co-located with a handle call (CLAUDE.md §Fail
  loudly).
* **C4/S1/OP-4 YES — comments and string literals are excluded, via the existing lexer, not a third one.**
  `common/test_code_filter.rs:263::strip_comments` already tracks line, block and string state. I will
  refactor it into a private `lex_line(line, in_block_comment, mode)` with two modes: `Remove`
  (existing output, byte-identical, existing tests are the pin) and a new `pub(crate)` `BlankNonCode`
  that **replaces** comment bodies and string bodies with spaces so column offsets survive for
  `--explain`. Rejected: "skip lines containing `//`" (S1 — trailing-comment bypass) and
  `instrument_skip_all::is_comment_line` (S1 — `*self.count = 0; tracing::info!(..)` is a
  one-character bypass). I will also add raw-string (`r#"…"#`) handling to the shared lexer: it is
  absent today, and an un-lexed raw string is an *under*-blank (silent miss). Media has zero raw
  strings today, so this is closing the hole before it exists.
* **C5 + A6-amendment YES — no `is_scan_exempt`, no `is_test_path`, no `is_line_in_test_block` inside the
  configured directories.** `is_test_path` matches `/fixtures/` and `is_guard_internal_path` matches
  `crates/dt-guard/`, so composing them would skip every `pos_*` fixture and make the entire
  "does it fire" half vacuous while reporting clean. `compute_test_block_ranges` is documented
  fail-safe-broad (extends to EOF on unbalanced braces) — here that is a silent disarm. **Scope is the
  configured directory list, full stop.**
  **OP-4 consequence, stated on the record and going into the module doc + runbook row**: a `println!`
  added inside a `#[cfg(test)] mod tests` under `media/` **reds the pipeline, and that is intended** —
  move the debug print to a sibling. Costs nothing today (five `#[cfg(test)]` blocks under `media/`,
  none using a denied form).
* **S5 YES — attribute form is path- and `cfg_attr`-agnostic.** `(?s)#\[[^\]]*\binstrument\b` over the
  **blanked** text: covers `#[instrument]`, `#[tracing::instrument(skip_all)]`,
  `#[::tracing::instrument]`, `#[cfg_attr(test, tracing::instrument)]`, and multi-line attributes.
  `\binstrument\b` does not match `instrument_skip_all` (`_` is a word char). Blanking is what makes
  this safe against `#[doc = "instrument"]`. Documented limitation: an attribute containing a literal
  `]` outside a string.
  **@security's fail-direction question, answered**: `[^\]]*` truncating early at such a `]` can only
  *shorten* the window searched for `instrument`, so it fails **away** from the finding — a miss, not a
  spurious hit. That is the wrong direction. Mitigation: the matcher runs a **second** pass keyed on
  `\binstrument\b` anywhere in blanked text and, for any hit not already claimed by the attribute
  regex, checks whether the nearest preceding unclosed `#[` exists on the blanked text; if the bracket
  structure cannot be resolved the line is reported rather than skipped. Fails toward the finding, and
  it is exercised by a fixture with a literal `]` inside an attribute.
* **S6 YES — no cfg-gate exemption.** `#[cfg(debug_assertions)] eprintln!(..)` reds. §11's dev-only
  per-frame facility already lives in a *sibling* (`crate::observability::per_frame_trace`, called as a
  plain function from `media/forward.rs:51,220`), which is precisely why no exemption is needed.
* **S2 / A6 / OP-5 YES — no suppression, at all.** No `guard:ignore`, no env bypass, no flag. Stated in
  the module doc AND the manifest header so it reads as a decision; pinned by
  `pos_ignore_annotation_not_honored.rs`.
* **A6 YES — import deny, first-segment anchored.** `^\s*(?:pub\s+)?use\s+(?:tracing|log|metrics|tracing_subscriber)\b(?!\s*::\s*)`-style
  **first-segment** match (expressed without lookaround — `regex` rejects it; I use the ADR-0034 §4
  positive-boundary-class technique). Explicitly not preceded by `crate::` / `self::` / `super::` / a
  leading `::`. The five live `use crate::observability::metrics::…` lines stay green;
  `neg_import_crate_observability_metrics.rs` is a verbatim copy of `forwarder.rs:10` so loosening the
  anchor reds the suite.
* **S9 YES — containment.** Each manifest entry goes through `common::path_safety::resolve_cited_path`,
  not a raw `join`. Escape-outside-root gets its **own** token
  `media-telemetry-deny-scope-directory-escapes-root`, distinct from the ENOENT case, so a symlinked-away
  media directory never reports as the benign "missing". `walkdir` runs with `follow_links` left at its
  `false` default.
* **File collection — @dry-reviewer's design question, answered.** Neither `get_all_changed_files`
  (arms only on touched files — a media file not in the diff must still be scanned) nor
  `get_tracked_files` (misses **untracked** files, so a new `media/leak.rs` before `git add` walks
  through; and it shells `git ls-files`, which hard-fails in the self-test's non-repo synthetic roots).
  I use `walkdir` over the resolved directory: it sees the working tree as it is, needs no git, and the
  zero-`.rs` case falls out of the same walk. This is a deliberate departure from the SoT-only-git rule
  with the reason stated in the module doc.

#### 5. Manifest (`scripts/guards/simple/media-telemetry-deny.yaml`)

Typed `serde::Deserialize` with `#[serde(deny_unknown_fields)]`, parsed with **`serde_norway` 0.9** —
the crate's actual YAML parser; no `serde_yaml` is added (@security S10). Schema: one key,
`denied_directories:`, a list of repo-relative directory paths with a trailing slash, **no globs** (a
glob matching nothing is a silent disarm). Seeded with exactly `crates/mh-service/src/media/`.
Missing or unparseable manifest → its own token, hard fail, never "0 directories → OK" (OP-9).

Header records, in these terms: the manifest path and the on-disk directory are two encodings of one
thing, but this is **not derivable duplication** — a policy *scope* is a decision, not a fact about the
tree, so there is nothing to derive it from. The drift is closed not by derivation but by
`media-telemetry-deny-scope-directory-missing` / `-empty`, **which fail the build the moment the two
encodings disagree — the scope-missing failure IS the drift guard.** Both tokens named in the header.
Also records the §11 layout co-obligation (lifecycle/setup/teardown are *siblings*, not children),
points at `crates/mh-service/src/media/mod.rs`, and is honest about the boundary: the guard mechanically
checks "non-empty `.rs` tree", it cannot check "this is the hot path". Plus: no suppression annotation
is honoured.

#### 6. STATUS lane — decided, and defended in the module doc-comment

**Both scope-failure tokens are IMPLEMENTER lane: `STATUS=FAIL`, exit 1.** @operations OP-1 and
@paired-observability A5 both land here independently, and I reached the same conclusion from
`run-guards.sh` before either arrived. Three reasons, going verbatim into the module doc:

1. **Substantive.** A configured directory that vanished or emptied is a **diff-caused** event with a
   named fixer and a one-line fix (update the manifest, or restore the directory). ADR-0033 §6's
   operator lane is for machine facts a retry can fix (timeout, OOM, no cluster). Routing a rename
   there burns a retry on a deterministic failure and then pages @operations for a code change.
2. **Mechanical — the operator lane is not available to a simple guard.**
   `scripts/guards/run-guards.sh::classify_guard_exit` classifies on the exit **value**: only 124 and
   137 reach `PRECONDITION_GUARDS`; every other non-zero, **including 2**, falls into the `*)` arm →
   `FAILED_GUARDS` → implementer lane. No `simple/` guard emits `PRECONDITION_FAILURE` today. A guard
   exiting 2 would print operator-lane *text* while being *counted* as a violation — the STATUS line
   and the aggregation would disagree, which is worse than either lane.
3. **Contrast with the frame-vectors precedent, stated so the divergence is deliberate.**
   `validate-frame-vectors.sh` treats vacuity as PRECONDITION because *its* vacuity means the vector
   file or the codec declaration is absent — a wrong-root/wrong-checkout fact. `release_build_profile`
   routes zero-Dockerfiles the same way for the same reason. Ours means someone edited the tree. Same
   shape, different cause, different lane.

**Distinguishability**: the two tokens are literal, distinct, greppable strings on the STATUS line —
`…-scope-directory-missing` vs `…-scope-directory-empty` — asserted distinct in the self-test, not merely
"guard failed".

**Output channel (OP-1 + A5, reconciled — please confirm).** Scope-failure lines print **first**, ahead
of any macro-hit line, because `run-guards.sh`'s non-verbose arm caps re-emission at `head -5`. Prefix:
`VIOLATION: [<token>] …` on **stdout** (A5 — "PRECONDITION" wording would mislead a reader into thinking
operator lane, which is exactly the confusion reason 2 warns about), **plus** an `ERROR: [<token>] …`
line on **stderr** so the runbook §4 one-pass `^(ERROR|PRECONDITION_FAILURE):` stderr triage finds it
(OP-1's legibility requirement). Both greps match; neither reader is misled. @operations
@paired-observability — flag if you want this the other way round.

Hit lines follow OP-8: `VIOLATION: [<token>] <path>:<line> — <matched construct> — <fix>` where the fix
names the cached-handle alternative concretely and cites ADR-0036 §11.

#### 7. Self-test (`scripts/guards/media-telemetry-deny.test.sh`) and wiring

**Seam: none — and that is the answer to @security S10 / @paired-observability E4.** `--root` is already
a production clap flag, and the manifest is read relative to the root, so the self-test builds synthetic
roots (`mktemp -d`, `trap 'rm -rf' EXIT`) containing `scripts/guards/simple/media-telemetry-deny.yaml`
plus a tree, and invokes the binary directly. **No `DEVLOOP_TEST`-gated override of the manifest path or
scan root exists, so there is no disarm switch to gate.** Strictly better than a gated seam. The binary
is `${DT_GUARD:-$REPO_ROOT/target/release/dt-guard}` (built by Layer 1
`scripts/lang/rust/compile.sh`); **missing binary is a loud failure, never a skip**.

Cases: (a) planted macro per denied family into a **copy of the real `media/` tree** — Half 1b, proving
the *configured path* is what gets scanned; (b) allow-list clean case; (c) comment/string clean case;
(d) `scope-directory-missing`; (e) `scope-directory-empty`, asserted **distinct** from (d);
(f) `scope-no-directories-configured`; (g) import-deny fires / `crate::observability::metrics` stays
green; (h) `guard:ignore` not honoured; (i) the real tree via the **wrapper** exits 0.

**OP-2 — `STATUS=` leakage.** Uses `scripts/lang/_test_helpers.sh` (`assert_exit`, `assert_status`,
`assert_absent`, `report_results` — the `'  - '` prefix is load-bearing), and any raw guard output is
indented before printing (`sed 's/^/      /'`, the `validate-frame-vectors.test.sh` precedent). Plus an
explicit assertion that no bare `^STATUS=` escapes the suite.

**OP-7 — cost.** One pristine copy of `media/` (~64 KB, 7 files) made once, `cp -r`'d per case; no
`cargo`, no network, no cluster. Target well under 5 s; measured runtimes for both guard and self-test
recorded in main.md and the §6.3 row **as ranges**. The guard walks only the configured directories,
never the full tree.

Wired into `scripts/layer3.sh` via `run_and_emit`, with the neighbours' comment explaining it is
deliberately NOT under `guards/simple/` because `run-guards.sh`'s `find … -name '*.sh'` would auto-run
it as a production guard.

#### 8. Ownership / GSA

`crates/dt-guard/**` is **not** an ADR-0024 §6.4 Guarded Shared Area. I checked the new modules against
the §6.4 criteria directly — a source-text validation control is not a wire-format runtime coupling, an
auth-routing policy, a detection-**forensics contract** (that clause covers `ac-service/src/audit/**`,
the evidentiary record), or a schema-evolution surface. **No key is added to
`scripts/guards/simple/cross-boundary-ownership.yaml`** — it is a GSA mirror under five-way sync and
`dt-guard gsa-sync` rejects stray keys. @security offered to co-sign this reading; please do.

`crates/mh-service/src/media/**` gets **zero** rows and zero edits. Planted macros live only in temp
copies and under `tests/fixtures/`. If the guard reds on the real tree, that is a real §11 violation to
report to @paired-observability and media-handler — not a reason to weaken the guard.

#### 9. OP-5 — rollback

The whole guard (both Rust modules, clap dispatch, wrapper, manifest, fixtures, self-test, layer3
wiring, runbook rows) lands as **one separately-revertable commit** containing no `crates/mh-service/`
change, so `git revert` of that one commit removes the guard and nothing else. Recorded in
§Rollback Procedure and in the module doc, per the `ts-no-retained-credentials` precedent.

#### 10. Open questions for Gate 1

1. **@team-lead** — ruling on §2: `telemetry_macros.rs` as a sibling canonical home (my recommendation,
   backed by @dry-reviewer) vs. the task's literal "in the new module" (which would create the third copy
   of the log-level alternation). This is a deliberate deviation from authoritative task wording and I
   will not implement it without your approval.
2. **@paired-observability + @security** — is re-pointing `rust_pii` / `rust_log_secrets` / `instrument_skip_all`
   at the shared home in-loop acceptable to you? It is behaviour-preserving by construction (identical
   member sets, pinned by a new equality-vs-historical-literal test), and CLAUDE.md §Fix-don't-defer
   says do it now — but it touches two guards in your domains.
3. **@operations + @paired-observability** — confirm the stdout `VIOLATION:` / stderr `ERROR:` split in §6.
4. **@paired-observability** — yes please, write the fixture files; directory shape is
   `crates/dt-guard/tests/fixtures/media_telemetry_deny/<pos|neg>_<slug>.rs`, flat, slug matching the
   `rule_id` in `--explain` output (the `cite_extract/` convention). I will land the harness and the
   catalog skeleton; you own the content and the expected-token column.
5. **@dry-reviewer** — your 105/106 question: resolved by classifying `media_telemetry_deny_e2e.rs`
   Domain-judgment/observability alongside the fixtures, since the expectation catalog *is* the fixture
   policy. Table updated.

#### 11. Amendments folded in from reviewer pre-input (2026-09-07, before Gate 1)

All of the following supersede the corresponding text above. Each names its source so the ruling
is attributable.

**A11.1 — Import deny is keyword-anchored, not punctuation-anchored (@security correction, co-signed
@paired-observability A6-corrected).** My §4 sketch excluded a leading `::`. That inverts the rule:
in Rust 2018+ `use ::tracing::info as note;` forces extern-prelude resolution and is the *most*
unambiguous spelling of the external crate — excluding it lets the one spelling that cannot resolve
locally walk through. Corrected rule: skip an optional `pub` / `pub(crate)` / `pub(super)` / `pub(in …)`
qualifier **and** an optional leading `::`, then deny when the first *identifier* segment is
`tracing` / `log` / `metrics` / `tracing_subscriber`; exclude **only** when that first segment is the
keyword `crate`, `self` or `super` — a closed keyword list, not a punctuation predicate. Brace-group
forms (`use {tracing::info, std::fmt};`, `use tracing::{info, warn};`) and whitespace forms
(`use   tracing :: info ;`, `pub(crate) use …`) are handled. **New fifth token
`media-telemetry-deny-unparseable-use`, same FAIL lane**: a `use` line the classifier cannot resolve
fails loudly rather than passing — "a shape I didn't anticipate" must not fall through to green.
Cost is nil: @security enumerated all 40 `use` lines under `media/` and found zero collisions.

**A11.2 — The sixth negative fixture is the important one.** `media/forward.rs:51` is
`use crate::observability::per_frame_trace;` — §11's dev-only per-frame facility, whose whole design
point is living in a *sibling* so the `#[cfg]` decision never enters `media/`, and it contains the
token `trace`. `neg_import_crate_local_paths.rs` copies all six real lines **verbatim**, so any future
loosening to a bare-token match reds the self-test.

**A11.3 — Vocabulary home moves to `crates/dt-guard/src/telemetry_macros.rs`
(@dry-reviewer #2 + @paired-observability A7, who ruled with them).** Address is crate-level, an exact structural mirror of `metric_macros.rs` — the crate's existing
macro-family vocabulary home. (@paired-observability said `common/`, @dry-reviewer said crate-level on
review because `metric_macros.rs` is the closer analogue; I took the latter. `common/pii_vocabulary.rs`
is the competing precedent and either is defensible — flagging the swap, not hiding it.) Groups are separately
named and each consumer selects — a flat union is the false-SSoT shape, where narrowing the list to
unbreak the PII guards would silently disarm the media deny. **This is still the deviation from the
task's literal "in the new module" wording that needs @team-lead's ruling** (§10 item 1); the argument is unchanged.

**A11.4 (REVISED TWICE) — `#[instrument]`: the home hosts BOTH shapes; nothing stays private
(@dry-reviewer item 2, reconciling @paired-observability A7 with their own earlier option 3).**

Draft 1 took @paired-observability's rider: home carries the *correct* wide pattern, three legacy
consumers keep narrow copies behind a TODO. @dry-reviewer rejected that — three private regexes under a
module claiming to be their canonical home gives the home authority it does not have. Draft 2 took their
option 3 (promote nothing for `#[instrument]`), which is safe but leaves the four narrow copies scattered.

@dry-reviewer's item 2 proposes a fourth option and it dominates both: **host both constants in
`telemetry_macros.rs`, adjacent and named for what they are.**

* `INSTRUMENT_ATTR_BARE_RE` — the narrow historical `#\[instrument` shape. The three legacy consumers
  (`instrument_skip_all.rs:49`, `rust_pii.rs:63`, `rust_log_secrets.rs:100`) re-point at it. **Zero
  semantic change** at any of them, so none of the measured blast radius below is incurred. A comment on
  it names the four live `#[tracing::instrument` sites it misses — `gc-service/src/handlers/metrics.rs:27`,
  `gc-service/src/handlers/health.rs:44`, `mh-service/src/session/mod.rs:1009`,
  `mh-service/src/webtransport/connection.rs:130` — and points at the `docs/TODO.md` entry for closing it.
  **Narrowness is documented rather than inherited silently**, which is the property that was missing.
* `INSTRUMENT_ATTR_ANY_RE` — the correct shape (this guard's blanked-text `#\[[^\]]*\binstrument\b`,
  which also covers `#[cfg_attr(test, tracing::instrument)]` and multi-line attributes). Consumed here.

This satisfies @paired-observability (the home carries the correct pattern), @dry-reviewer (no encoding
of this construct lives outside the home), and the blast-radius constraint (no legacy consumer's
behaviour moves). **No private copy survives anywhere.**

Why widening the legacy consumers is still out of scope, with @dry-reviewer's measurement so the
decision is on the record rather than assumed: `instrument_skip_all` (both checks) and
`rust_log_secrets.rs:181` are gated on absence of `skip_all` and take **zero** new hits, but
`rust_pii.rs:164` Check 3 takes **one false positive** at `mh-service/src/webtransport/connection.rs:130`
— `"skip_all"` does not contain `"skip("`, and `name` is a `PII_TOKENS_CATEGORY_B` member. The clean fix
is making Check 3 honour `skip_all`, a real latent bug at ~18 sites including the MC twin at
`mc-service/src/webtransport/connection.rs:186` — but that makes @security's PII guard **more
permissive**, which is squarely their call and not something an infrastructure loop decides. Routed to
@security with the site list.

`rust_pii.rs:53 TRACING_NAMED_RE` is the fifth encoding of this vocabulary: its **level half** derives
from the promoted LEVEL group and its `#\[instrument` alternative derives from `INSTRUMENT_ATTR_BARE_RE`
— so after this change it is fully derived, with no hand-maintained copy left.

**A11.5 — Span group: enumerated six, plus a media-only open shape (@paired-observability A8).**
Promoted SPAN = `span` `trace_span` `debug_span` `info_span` `warn_span` `error_span` — the complete
public `tracing` span surface, which is the only membership a shared home can honestly export. The
media deny **additionally** applies an open `\b\w+_span!\s*[\(\[\{]`, **media-only, not promoted**:
we are spending an import-deny to close `use tracing::info as note;`, and a sibling-defined
`custom_span!` is the same evasion class. One shared reason token; the message names the matched
spelling. `\w+_span` requires a literal `_`, so bare `span!` still needs its enumerated entry.

**A11.6 — Scope liveness goes into `common/`, not inline. My §0 stance is withdrawn
(@dry-reviewer A).** `docs/TODO.md` §"Guard Coverage Gaps — path roots and globs that resolve to
nothing" (S16/S17, S16b) already files this as a general predicate — *"the assertion is the
load-bearing deliverable"* — and enumerates **12 hardcoded path roots across 10 dt-guard modules**
waiting on it. My §0 said "no extraction until a second consumer"; the second-through-thirteenth
consumers are already written down and owned. So `crates/dt-guard/src/common/scope.rs` gets
`assert_scope_live(repo_root, &[dirs]) -> Result<Vec<PathBuf>>`, returning the collected files and
hard-failing with the distinct tokens. **This guard is the first whose *purpose* is that assertion,
which makes it the natural home rather than an awkward one.** The definition cites `gsa_sync.rs`'s
`CANON_MISSING_RULE_ID` overloading ("the module's own emitted behaviour teaches the wrong inference")
as why the two tokens are distinct, and contrasts explicitly with `metric_coverage.rs:66-67`, the
in-tree precedent for a *safe*-empty scope — ours is the opposite polarity and says so at the
definition.

**A11.7 — `SCOPE:` line becomes `common::status::emit_scope` (@dry-reviewer B).** Third consumer:
`release_build_profile.rs:1130` and `no_insecure_browser_flags.rs:436` both hand-roll
`println!("SCOPE: …")`. Both are routed through the shared emitter, preserving their hard-won
count/scope-drift comments (`no_insecure_browser_flags.rs:372`,
`release_build_profile.rs:1125-1128`) at the call sites. Note `release_build_profile` is §11's
*sibling* control, so a third bare `println!` here would be the same third-consumer trigger as the
vocabulary.

**A11.8 — `neg_comments_only.rs` is the fixture that decides whether Layer 3 is red for the whole
team on day one (@dry-reviewer, @operations OP-4, @paired-observability).** @dry-reviewer's count:
`mod.rs` 4 lines, `forward.rs` 2, `ingress.rs` 1, zero elsewhere; **zero production-code hits**. The
fixture must carry a verbatim copy of `forward.rs:97`'s ``/// … a `debug!(?frame)` in`` (matches the
`!\s*(` anchor exactly) and of `mod.rs:16`'s ``//! `#[instrument]`.`` (the attribute matcher has no
`(` anchor to save it), and must be asserted **clean**. `compile_error!` appears in that same doc
block and is **not** in any denied list — which is why the alternation is enumerated rather than a
`\w+!\s*\(` shape.

**A11.9 — Fixture + harness conventions (@dry-reviewer F).** Flat
`crates/dt-guard/tests/fixtures/media_telemetry_deny/<pos|neg>_<slug>.rs`, slug matching the `rule_id`
emitted in `--explain` so fixture-vs-EXPLAIN drift is one grep; each fixture closes with an
`// Invariant:` paragraph restating the expectation in words. Harness locates fixtures via
`PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/media_telemetry_deny")` — never
`current_dir()`; table-driven `catalog()` with an explicit `assert_eq!(entries.len(), N)` count pin;
**one** test walking the catalog, per `cite_extract_e2e.rs`'s deliberate counter-pattern note.

**A11.10 — Manifest parsing (@dry-reviewer C).** `cross-boundary-ownership.yaml` is the precedent for
*placement and header style only* — it is parsed by two hand-rolled recognizers
(`cross_boundary_classification.rs:49`, `gsa_sync.rs:170-179`), which is a defect in the precedent,
not a pattern to extend. Typed `serde_norway` struct with `deny_unknown_fields`. Header copies the
four conventions that ARE worth copying: name the ADR §; state key/value semantics; an explicit
"INCOMPLETE BY DESIGN" scope-limit paragraph; and an explicit statement of what a PASS does **not**
certify — which is exactly where "the scope-missing failure IS the drift guard" lands.

**A11.11 — No ignore hatch, and therefore no `crate::ignore` import (@security S2,
@paired-observability, @operations OP-5, @dry-reviewer H).** Unanimous. The module does not import
`crate::ignore`; `pos_ignore_annotation_not_honored.rs` pins it. `ts_retained_credentials.rs:41-50`
documents what earns that right.

**A11.12 — Residual, stated so it cannot be read as coverage (@security, correcting
@paired-observability's first wording).** Module doc will say, in this form: *an alias that keeps a
denied name (`pub use tracing::info as debug;`) is still caught by the invocation matcher; an alias
renaming outside the list (`… as note;`) whose re-export lives in a sibling is caught by **neither**
matcher, and only inverting the invocation matcher to an allowlist of macro names permitted inside
`media/` would close it — not in scope for this loop.*

**A11.13 — Row 106 upgraded to Domain-judgment / observability (@paired-observability, @dry-reviewer).**
Accepted; the table above reflects it. An expectation sitting at a lower tier than its fixture could be
re-pointed without owner involvement, decoupling two halves of one decision.

**A11.14 — Blast-radius note for @team-lead.** A11.3, A11.6 and A11.7 are all "promote at the third
consumer" findings and each is individually well-evidenced, but together they take the changeset from
11 files to 21 and touch **five existing guard modules** (`rust_pii`, `rust_log_secrets`,
`metric_macros`, `release_build_profile`, `no_insecure_browser_flags`). Every one of those edits is
value-neutral by construction and pinned by an equality-vs-historical-literal test, and CLAUDE.md
§Fix-don't-defer is against parking them. I recommend taking all three. Flagging the aggregate rather
than each individually, because the risk is cumulative and the decision is yours.

**A11.15 — Output-channel question CLOSED by @operations + @paired-observability; my split proposal is
withdrawn.** They converged directly: prefix is `ERROR: PRECONDITION [<token>]`, exit 1, implementer
lane — **but the message body must say "THIS IS A DIFF DEFECT" explicitly**, rather than inheriting the
neighbouring `release_build_profile.rs` boilerplate that says the opposite. Same prefix, inverted triage.
@operations is writing the §6.3 / §8 rows to match, including a pointer on the `release-build-profile-*`
and `env-config-*` rows warning that this one family inverts. Scope-failure records still print **first**,
ahead of any macro-hit record, because of the `head -5` cap.

**A11.16 (@operations OP-11) — every `VIOLATION:` / `ERROR: PRECONDITION` record is exactly ONE physical
line.** `run-guards.sh` surfaces guard output through `grep -E "(VIOLATION|…)" | head -5`, so a record
wrapped for readability loses its continuation lines silently or burns the 5-line cap on one finding.
The pinned message bodies are ~400 chars and wrapping them is the natural instinct. **This is invisible
from inside the module, which is how it gets broken**, so it goes in the module doc as a named invariant,
not just in the formatting code.

**A11.17 (@operations OP-12) — truncation must be visible from the STATUS line.** A diff with 8 macro
violations prints 5 and nothing says there were 8. The two *content* tokens
(`…-macro-in-media-path`, `…-telemetry-crate-import`) carry the existing `-<n>-of-<m>-findings` suffix
convention used by `env-config-*` and `release-build-profile-*`. The four scope/parse tokens are singular
conditions and do not. A trailing "…and 3 more" line does **not** work — it loses to the same cap.
Token spelling is @paired-observability's; the property is @operations'.

**A11.18 — fixture-safety sweep, recorded here so the next person adding `.rs` fixtures does not repeat
it (@operations).** These are the first `.rs` files under any `fixtures/` directory in the repo
(`cite_extract/` is `.md`; `ts_retained_credentials/` is `.ts`/`.svelte`), so "does a fixture full of
`counter!(`, `println!` and `#[instrument]` red some other guard on day one?" was a live question. It
does not:

* `rust_secrets` / `rust_pii` / `rust_log_secrets` / `instrument_skip_all` exclude via
  `common::test_code_filter::is_test_path` (the `/fixtures/` segment).
* `metric_labels` / `histogram_buckets` / `application_metrics` / `metric_coverage` scope to
  `crates/<canonical-service>/src/observability/metrics.rs`.
* `test_coverage` excludes `/tests/`; `test_rigidity` is scoped to `crates/env-tests/tests`.
* Unreferenced `.rs` under `tests/` is in **no cargo target**, so Layer 2 `cargo fmt --all --check` and
  Layer 6 clippy never see it — which is what makes a deliberately-malformed
  `pos_unparseable_use.rs` safe.

**Live trap avoided (@operations).** `test_registration` pairs `crates/*/tests/<X>_tests.rs` with a
sibling `tests/<X>/` directory and demands `#[path = …]` registration for every `.rs` inside it. A
harness named `crates/dt-guard/tests/fixtures_tests.rs` would point it straight at the fixture tree. The
harness is `crates/dt-guard/tests/media_telemetry_deny_e2e.rs` — the `_e2e.rs` / `_fixtures.rs` suffix
convention (`cite_extract_e2e.rs`, `ts_retained_credentials_fixtures.rs`) exists precisely to dodge this.

**A11.19 — two things @dry-reviewer will check at Gate 2, designed for now rather than reworked later.**
(a) *Derivations must be real, not cosmetic.* Beyond the equality-vs-historical-literal test (which
proves behaviour-preservation but **not** derivation), each alternation gets a test that iterates the
full `ALL` set and asserts every member is matched — so a new `MacroKind` variant widens
`media_telemetry_deny` with zero edits in the policy module, and a new LEVEL-group `TelemetryMacro`
widens `rust_pii` / `rust_log_secrets` with zero edits in theirs.
(b) *`assert_scope_live` must be callable by S16b's ten modules.* Signature is shaped for the general
case, not for this manifest: it takes a plain `&[impl AsRef<Path>]` of repo-relative roots plus the
extension filter, so `application_metrics.rs:176`'s single hardcoded root calls it directly with a
one-element slice and no wrapper. If the natural S16b call site needed a wrapper, the extraction would
not have happened.

#### 12. Gate-1 rulings from @main (2026-09-07) — recorded for the story-close audit

**LEAD-APPROVED DEVIATION FROM AUTHORITATIVE TASK WORDING.** The story-runner task text says the
tracing/log/print/span list gets "its own canonical-home list **in the new module**". @main ruled that
this is instance-language for a mechanism whose stated purpose in the same sentence is *"do not
re-inline it elsewhere"*, and that a third byte-identical declaration of
`\b(info|debug|warn|error|trace)!\s*\(` alongside `rust_pii.rs:44` and `rust_log_secrets.rs:51` would
satisfy the words and defeat the intent, against CLAUDE.md §Single source of truth. **Approved address:
`crates/dt-guard/src/telemetry_macros.rs`**, crate-level as a sibling of `metric_macros.rs` — the
structural-mirror argument beats the `common/pii_vocabulary.rs` analogy because `metric_macros.rs` is
literally the other half of this guard's alternation. The list is still new, still declared exactly
once, still not re-inlined; **only its address moves.** Group-structured with per-consumer selection is
approved and is the load-bearing half.

*This paragraph exists so the deviation is legible to the story-close audit as an approved decision
rather than as drift.*

**Ruling 2 — all three promotions approved, with two boundaries:**

* **A11.3** is entailed by Ruling 1, not optional: *"a canonical home that two existing modules bypass
  with private copies is not a canonical home; it is a fourth copy wearing the title."*
* **A11.6 boundary**: `common/scope.rs` is written and consumed **HERE ONLY**. The ten S16b modules are
  **not** re-pointed this loop — that is S16b's task and its owner's call. A cross-reference from
  `docs/TODO.md` §S16b to this module as the reference implementation is the deliverable.
* **A11.7 condition**: `release_build_profile.rs` and `no_insecure_browser_flags.rs` must come through
  with their existing self-tests and e2e green and their historical output literal pinned by an equality
  test. **If either resists cleanly, drop that module from the re-point and say so — do not bend its
  output shape to fit the helper.**

**SCOPE CAP (binding).** Twenty-one files is the ceiling for this devloop. A fourth
promote-at-the-third-consumer opportunity, should one surface mid-implementation, is **filed and
escalated to @main, not taken** — the three approved were approved on their individual merits; a fourth
would be approved on momentum, *"which is how a guard-mechanism task becomes a guard-crate refactor."*

**Confirmed parked, not to be reopened in this loop:** `#[instrument]` widening (@security's call,
`docs/TODO.md`); the `rust_pii` Check-3 `skip_all` / `fields(...)` narrowing (@security has co-signed
@paired-observability's version — their loop to land); the `gc-service/src/services/ac_client.rs:84`
PII finding (cross-owner, global-controller).

#### 13. Final reconciliations (post-ruling) — prefix, commits, and the shape of the cap

**A13.1 — Output prefix CLOSED, and my hybrid was mechanically impossible (@operations).** A11.15
recorded the convergence; @operations then showed the stdout/stderr split I had proposed could not have
worked at all. `run-guards.sh:252` captures the guard as
`OUTPUT=$(timeout … "$guard" "$SEARCH_PATH" 2>&1)` — a guard subprocess's stderr is **merged into that
capture** and re-emitted on run-guards.sh's *stdout*. Only run-guards.sh's own `>&2` writes (lines 196,
205, 316) reach `layer-3.stderr.log` via `layer-all.sh:151`'s `2>>`. So my `ERROR:`-to-stderr line would
never have reached the §4 one-pass stderr triage; it would have landed on stdout beside the
`VIOLATION:` line, emitting each finding **twice into one channel and burning two of the five `head -5`
slots per finding** — the exact truncation problem OP-11/OP-12 exist to prevent. Withdrawn.

**Final form:**

| Token class | Prefix | Why |
|---|---|---|
| The two **content** tokens (`…-macro-in-media-path`, `…-telemetry-crate-import`) | `VIOLATION: [<token>] <path>:<line> — <matched> — <fix>` | A policy finding you go fix at a cited line. |
| The four **scope / parse** tokens | `ERROR: PRECONDITION [<token>] …` | The guard checked nothing. Different first action; collapsing both under `VIOLATION:` loses that distinction. |

`ERROR: PRECONDITION` is deliberately the **same prefix** as `release_build_profile.rs:1165`, whose
established local meaning is the opposite triage. Inventing a second vocabulary for this class in one
guard would itself be the drift; the runbook already teaches repo-wide (§6.3.1 item 4) that this prefix
on a `simple/` guard means implementer-lane exit 1, and §8 has a catalogue row keyed on the literal
string. **The inversion is carried by the body, verbatim and loud: "THIS IS A DIFF DEFECT, not a
machine fault"** — never inheriting `release_build_profile.rs`'s neighbouring "NOT a diff defect"
boilerplate — plus a warning pointer that @operations is adding to the `release-build-profile-*` and
`env-config-*` rows saying this one family inverts. @paired-observability withdrew A5's `VIOLATION:`
preference on the consistency argument.

**A13.2 (@operations item 3) — TWO commits, not one. This supersedes A11.9/§9's single-commit
rollback.** OP-5's premise was "revert the guard commit and nothing else changes"; with the three
extractions folded in, a single-commit revert would also roll `rust_pii`, `rust_log_secrets`,
`metric_macros`, `release_build_profile` and `no_insecure_browser_flags` back to their pre-extraction
state. Functionally fine today, but a 3am operator can no longer eyeball the revert as obviously safe,
and the revert stops applying cleanly the moment any later commit builds on `emit_scope` or the promoted
vocabulary — which is when it is most needed.

* **Commit 1** — the three promote-at-third-consumer extractions (`telemetry_macros.rs`,
  `common/scope.rs`, `common::status::emit_scope`) plus the five re-pointed modules. Value-neutral,
  pinned by the per-module equality tests.
* **Commit 2** — the new guard: `media_telemetry_deny.rs` + clap dispatch + wrapper + manifest +
  fixtures + `media_telemetry_deny_e2e.rs` + self-test + `layer3.sh` wiring + runbook rows.

"Revert the guard commit" stays literally true, the extraction survives the revert, and it matches the
`ts-no-retained-credentials` precedent the runbook already cites. Ordering only; no code change.

**A13.3 (@operations item 4) — `emit_scope` must NOT impose a common field shape.** The two existing
call sites have genuinely different shapes (`release_build_profile.rs:1130` is
`{n} cargo Dockerfiles, {} enumerated premise channels, {} hits`;
`no_insecure_browser_flags.rs:436` is `{} candidate files, {} enumerated settings, {allowlisted_hits}
allowlisted …`). Flattening them into a common schema would be **a value-changing edit wearing an
extraction's clothes**. The helper owns the `SCOPE: ` prefix and the single-line emission contract;
each site keeps its own field list. **Each site's full formatted line is pinned by equality against its
historical literal**, not just the prefix — `no_insecure_browser_flags.rs:372` carries a comment about a
past bug where the `SCOPE:` count and the real count disagreed, so this line has history of being subtly
wrong. `SCOPE:` appears in no runbook and no script grep, so the entire risk is "did the numbers stay
the same", and that is what the equality test asserts.

**A13.4 — the cap is three promotions, not a file count (@main).** Recorded so a successor does not read
the number as the constraint: **three promote-at-the-third-consumer extractions are approved
(`telemetry_macros.rs`, `common/scope.rs`, `emit_scope`); a fourth is filed and escalated to @main, not
taken.** File count follows from that, not the other way round. The classification table stands at 22
rows because `instrument_skip_all.rs` is an edit already inside approved work A11.3 (omitting its row
manufactures inbound scope-drift at Gate 2) and `docs/runbooks/devloop-validation.md` is @operations'
OP-3 deliverable (the alternative is REASON tokens with no triage path). Neither is a new extraction.

**A13.5 — OP-11 is pinned by the self-test, not just by the formatting code.** A named case asserts every
`VIOLATION:` / `ERROR: PRECONDITION` record is exactly one physical line. A multi-line record is the sort
of thing that regresses the first time someone makes a message friendlier, and it would make the guard's
output quietly useless in the pipeline while looking correct when run by hand.

**A13.6 — A11.18's load-bearing sentence goes in the fixture directory's own doc (@main).** A
`crates/dt-guard/tests/fixtures/media_telemetry_deny/README.md` carries the sweep result and, in
particular, *"unreferenced `.rs` under `tests/` is in no cargo target, so Layer 2 `cargo fmt --all
--check` and Layer 6 clippy never see it"* — the fact that makes a deliberately-malformed
`pos_unparseable_use.rs` safe. main.md is not where the next person adding a fixture will look.

#### 14. Recorded Gate-1 conditions and one routed request

**A14.1 (@security, condition on their "Plan confirmed") — the equality test pins BOTH patterns in
`rust_pii.rs`, not one.** Their first condition covered `LOG_MACRO_RE`; but the `rust_pii.rs` row
re-points **two** templates fed by the same LEVEL member list, and `TRACING_NAMED_RE` (`rust_pii.rs:54`)
is a materially different shape — literal `tracing::` prefix, **no** `\b` anchor, and a second
alternative `|#\[instrument`. Two failure modes a members-only or single-regex assertion would miss:

* reconstructing `TRACING_NAMED_RE` from the shared full-pattern form widens it from `tracing::info!`
  to bare `info!`, so Check 2 (`TRACING_NAMED_RE.is_match(line) && line.contains('=')`) fires on every
  bare `info!(…)` carrying an `=` and duplicates Check 1 — a behaviour change, not value-neutral;
* **the sharper one** — dropping `|#\[instrument` in the rebuild silently removes Check 2's
  instrument-attribute coverage. A **false negative in a PII guard, introduced by an edit classified
  Mechanical**, which nothing else in the pipeline would notice.

Binding: full **compiled pattern string** equality against the historical literals, with
`|#\[instrument` explicitly present in the expected text so its removal reds the test —
`LOG_MACRO_RE == \b(info|debug|warn|error|trace)!\s*\(`,
`TRACING_NAMED_RE == tracing::(info|debug|warn|error|trace)!\s*\(|#\[instrument`, same treatment for
`rust_log_secrets.rs`. Complementary to @dry-reviewer's members-over-`ALL` test: theirs proves the
derivation is real, this proves the derived artifact is unchanged. **With this, @security accepts
`Not mine, Mechanical` on all three re-point rows** — no upgrade, no escalation.

**A14.2 (@security) — the `INSTRUMENT_ATTR_BARE_RE` comment must name the COUPLING, not just the
sites.** It names the four `#[tracing::instrument` sites it misses (`gc-service/src/handlers/metrics.rs:27`,
`gc-service/src/handlers/health.rs:44`, `mh-service/src/session/mod.rs:1009`,
`mh-service/src/webtransport/connection.rs:130`) **and** states that flipping a consumer to `ANY`
requires the Check-3 fields-scoping fix to land first, or `connection.rs:130` reds falsely. *"The
coupling is the part that will be forgotten, not the site list."*

**A14.3 — `docs/TODO.md`: point at @security's entry, do not restate it.** They have filed the
`pii_vocabulary.rs:317-320` correction under §Observability Debt with the full reasoning, **including
the rejected `skip_all` reading marked as rejected** so the next reader does not re-derive it. This
loop's entry covers only the `#[instrument]` promotion (the four qualified sites, with the coupling)
and the multi-line `fields(` gap, and cross-references theirs. Two entries carrying one diagnosis is
the second-encoding problem that file already has three entries about.

**A14.4 — ROUTED TO @main, not acted on: @paired-observability asks to reopen the Check-3
fields-scoping fix in-loop.** Their argument, which I judge to be the strongest reopening case on the
table: the fields-scoping is **independent** of the `#[instrument]` promotion, `rust_pii.rs` is already
a row in this table, it adds no new file, and it is sub-5-LoC in a file already being touched — the
shape the review protocol's suspicious-deferral check says not to park. Against it: @main's Gate-1
ruling names it in the parked list, @security has reversed their own in-loop instruction on that basis,
and implementing it properly needs `metric_labels.rs:383 split_top_level_args` promoted to
`pub(crate)` — **a fourth extraction**, which is exactly what the cap exists to stop. **Not taken
pending @main.** If amended, @security's two conditions are binding (unterminated `fields(` falls back
to treating the line remainder as in-scope, failing toward the finding, live case
`crates/mc-service/src/actors/participant.rs:332-336`; single-line limitation stated in the comment with
that worked example and the multi-line gap filed).

**A14.5 — fixtures received from @paired-observability and verified on disk**: 22 files at
`crates/dt-guard/tests/fixtures/media_telemetry_deny/`, 17 `pos_` + 5 `neg_`, each closing with an
`// Invariant:` block. `catalog()` rows are built **from those blocks**. **If the walker disagrees with
a count, it goes back to @paired-observability as a policy question — the expectation is never edited to
match the implementation.** A fixture whose expectation was quietly relaxed to match the code is the
precise failure this loop exists to prevent, and it would be a bad way to fail it.

#### 15. @semantic-guard's binding requirement — the guard must not reproduce the leak it forbids

**A15.1 — CONFIRMED, and it changes the implementation.** @semantic-guard's lens is the one that caught
this: the identifiers §11 exists to keep out of logs, metric labels and span attributes live inside the
denied macro's **argument list** (`info!(participant_id = p, stream = s, …)`). A guard that echoes the
matched span to CI stdout reproduces the exact leak — into CI logs and build artifacts, a surface
arguably worse than a debug log, and one with longer retention.

My §6/OP-8 wording "`<matched construct>`" was genuinely ambiguous between *the macro name* and *the
matched line*, and the default I would have reached for was wrong: `common/explain.rs::print_finding`
emits `matched="<span>"` with a ±20-char window around the hit, which is precisely the argument text.

**Binding, on the record:**

1. **Hit records and `--explain` emit the macro spelling + `path:line` ONLY** — e.g. `debug_span!`,
   `event!`, `#[instrument]`, `use tracing::…` — **never** the argument text and never the remainder of
   the source line. The `--explain` path uses
   [`common::explain::print_secret_finding`] / [`SecretFinding`], **not** `print_finding`:
   `SecretFinding` has **no `matched` field by design**, and `explain.rs:275-278` already carries a
   construction-based structural assertion that fails to compile if a future contributor adds one. That
   compile-time property is stronger than a convention and is the reason to use the redacted variant
   rather than passing a truncated string to the general one. Same precedent as
   `instrument_skip_all` and `rust_log_secrets`, both of which redact for the same reason.
2. **`emit_scope` echoes no file contents** — confirmed. It emits counts and the configured directory
   paths from the manifest, nothing read out of a scanned file. The two existing call sites
   (`release_build_profile.rs:1130`, `no_insecure_browser_flags.rs:436`) are counts-and-labels only and
   A13.3 pins each site's full formatted line by equality, so the extraction cannot introduce a content
   echo either.
3. **Pinned by a fixture, not asserted.** A `pos_` fixture plants a denied macro whose arguments carry a
   distinctive sentinel identifier; the self-test asserts that sentinel appears **nowhere** in the
   guard's stdout or stderr on any path, including `--explain`. `assert_absent` from
   `scripts/lang/_test_helpers.sh` is exactly this shape — proving a control stays silent, which a
   positive-only suite structurally cannot show.

This is also why OP-8's "fix" clause is safe: it names the *cached-handle alternative* (`.increment` /
`.record` / `.set` / `.absolute` / `.decrement`, resolved once at setup) and cites ADR-0036 §11 — it is
static advice text, carrying nothing read from the file.

#### 16. @code-reviewer's Gate-2 pin list (confirmed at Gate 1) — one new fact

Their five checks are all already in the plan (derived-alternation equality **plus** an
`ALL`-iteration test proving real derivation; LEVEL-group-only selection for `rust_pii` /
`rust_log_secrets`; exact `TRACING_NAMED_RE` recomposition; no-panic `Result` returns from
`assert_scope_live` and manifest parse with `expect` only on const-regex `Lazy` inits behind the
crate-precedent `#[expect(clippy::disallowed_methods, clippy::expect_used, reason = …)]`;
`telemetry_macros.rs` reading like `metric_macros.rs`).

**New fact to act on: `rust_pii.rs:99` and `rust_log_secrets.rs:230` use plain
`line.contains("#[instrument")`** — bare string checks, **not** `INSTRUMENT_RE`. They are **left
untouched**. They are a sixth and seventh encoding of the attribute shape, but they are a different
construct (substring test, not regex) with different semantics, and folding them into the promotion
would be a behaviour-affecting change smuggled inside a Mechanical row. Noted in the `docs/TODO.md`
entry alongside the four qualified sites so the inventory is complete rather than silently partial.

#### 17. @test's Gate-1 finding — the scoped-boundary NEGATIVE case (accepted)

**A17.1 — ACCEPTED. Over-application was untested, and the gap is real.** Half 1b proves the configured
path *is* scanned (plant in `media/` → reds). **Nothing proved that a denied macro OUTSIDE the
configured list stays GREEN.** That negative direction is §11's own property — *"the directory boundary
and the hot-path boundary are the same boundary"* — and lifecycle/setup/teardown are siblings precisely
so they are out of scope and may legitimately carry telemetry.

New hermetic case: synthetic root, manifest = `crates/mh-service/src/media/` only, a denied form planted
in a sibling **not** in the manifest (`crates/mh-service/src/session/leak.rs`), assert the run stays
**GREEN**.

@test's failure analysis is the part worth preserving: a regression widening the walk root from the
resolved directory to the crate or repo root **passes the entire existing self-test** — every fires case
still fires, every scope case still fails correctly — and is only caught when it reds the real pipeline
on an unrelated sibling. This is precisely what the `walkdir`-over-resolved-directory design buys, and it
was the one design decision with no test pinning it. Cheap: the self-test already builds these roots.

**A17.2 (@test CONFIRM 1) — YES, the fires cases use the REAL manifest verbatim.** The synthetic root
receives a byte copy of the shipped `scripts/guards/simple/media-telemetry-deny.yaml` (naming
`crates/mh-service/src/media/`), with the real media files placed at that **same relative path** inside
the temp root. This is the strong form: it proves the *actual configured path* is scanned. A synthetic
manifest re-pointed at a convenient temp path would degrade the case to "the matcher works" and say
nothing about what the shipped manifest points at. The scope-liveness cases (d/e/f) necessarily use
synthetic manifests — that is the point of those cases — and the boundary case A17.1 uses the real
manifest too, since its whole claim is about what the shipped scope excludes.

**A17.3 (@test CONFIRM 2) — YES, token-substring pins, not byte-exact bodies.** The self-test asserts
REASON tokens via `assert_status` substring matching. For the two content tokens carrying the
`-<n>-of-<m>-findings` suffix (A11.17) it pins the token **and the finding-count semantics**, not the
cosmetic ~400-char body. Positive fixtures assert on the token, never on a byte-exact line — pinning
prose would make every message improvement a test failure, which is how rigid tests get deleted rather
than fixed. The one deliberate exception is A11.16's OP-11 case, which asserts a **structural** property
(exactly one physical line per record) rather than the line's content.

#### 18. Two items held open, and a structural form for @test's boundary case

**A18.1 — REASON precedence ladder: NOT settled, routed to @paired-observability, and it blocks
"Ready for validation".** @operations raised that several tokens can fire in one run while only **one**
reaches `STATUS=FAIL REASON=`, and that token selects the runbook row an operator lands on. **Print
order being settled (A13.1) is not the same as REASON precedence being settled** — they are different
decisions and I had conflated them. Their recommended fail-closed ladder is
`manifest-unusable > scope-missing > scope-empty > parse > content`, which is right in shape: a run
where the guard could not read its own scope must never report a content token, because a content token
tells the operator "go fix the cited line" when the truth is "the guard checked nothing." Token
semantics are @paired-observability's, so the ruling is theirs; asked, and I will not signal Ready
without it. Whatever lands is pinned by a self-test case with **two** conditions live at once.

**A18.2 — commit shape HELD.** *(RELEASED — see §20: @operations reported, two commits confirmed with a verified ordering.)* A13.2 adopted
@operations' two-commit split. @main then flagged that commit 1 moves HEAD and may invalidate the
Gate-2 verdict signature for commit 2 under `scripts/lang/_gate2_binding.sh`, forcing a second full
`layer-all.sh` pass. @operations is verifying. **The plan of record remains A13.2's two commits, but the
shape is not final** — @main will say which is binding. Recorded so a reader does not take A13.2 as
settled.

**A18.3 — a structural form for @test's boundary case (@main's ask).** The test case in A17.1 stays —
it is the evidence. But per ADR-0036 §11's own remedy, *prefer structural impossibility over a control
that has to notice*, the scoping also gets a type:

`common::scope::assert_scope_live` returns `Vec<ScopeRoot>`, where `ScopeRoot` is a newtype with **no
public constructor** — the only way to obtain one is to have passed through the liveness assertion. The
file walker's signature takes `&[ScopeRoot]`, not `&[PathBuf]`. So widening the walk from the resolved
directories to the crate or repo root is no longer a one-character path edit that compiles; it requires
**adding a constructor**, which is a visible, reviewable act rather than an invisible one.

This is the same move as @semantic-guard's `SecretFinding`-without-a-`matched`-field: the property is
enforced by the type system rather than by a rule someone has to remember. The test proves it holds
today; the type makes the regression hard to write. Neither substitutes for the other — @test's case is
what catches a widening introduced *through* a new constructor, and the type is what stops the
accidental version. It also gives `ScopeRoot` to S16b's ten future call sites for free.

**A18.4 — @test's sibling location is a real one, deliberately (@main).** The planted-and-must-stay-green
file goes at `crates/mh-service/src/session/leak.rs`, not an arbitrary `other/`. §11's layout constraint
is that lifecycle, setup and teardown are **siblings** of `media/`, not children, precisely so the
directory boundary and the hot-path boundary coincide — so a negative case planted in a real sibling
location also pins that layout premise. An arbitrary directory would prove the scoping and nothing else.

#### 19. @security's classification upgrade — three rows Mechanical -> Minor-judgment (accepted)

**A19.1 — ACCEPTED without pushback; they are right and I would not have caught it.** The three
re-point rows (`rust_pii.rs`, `rust_log_secrets.rs`, `instrument_skip_all.rs`) move from
`Not mine, Mechanical` to **`Not mine, Minor-judgment`**, Owner `security + observability`. Table
amended.

The reasoning is worth preserving because it is the loop's own thesis pointed at the review process:
Mechanical requires three things — value-neutral, structure-preserving, **and guard-pipeline covers the
change-pattern**. The first two hold and were verified directly. The third does not. @security searched:
the only drift-sync guards under `scripts/guards/simple/` are `validate-gsa-sync.sh`,
`validate-slug-class-sync.sh` and `validate-subdomain-regex-sync.sh`, none covering guard-internal regex
literals, and `crates/dt-guard/tests/` has zero references to `LOG_MACRO_RE` or
`PII_TOKENS_CATEGORY_B`. **The only thing catching an alternation drift is the equality test being
written in this same diff by this same author** — which is not the independent coverage the Mechanical
tier presumes. The tier assumes something *else* catches the author's mistake. (Contrast the protocol's
own Mechanical worked example, ADR-0031 FU#3a, where the coverage was a pre-existing metric-labels
guard.)

Second, independent reason: @security has attached substantive conditions to these rows (pin **both**
compiled patterns as full literals; keep `|#\[instrument` in the expected value; freeze LEVEL at five
members; comment the blast radius). Recording who certified behaviour-preservation is what a hunk-ACK
trailer is for — Mechanical leaves no artifact, Minor-judgment does.

**A19.2 — cost is two commit trailers, no code change.** Per ADR-0024 §6.7, one
`Approved-Cross-Boundary:` trailer per owner, reason clause naming the authority (≥10 chars, the *why*
not the *what*). @security supplied theirs verbatim; @paired-observability is asked to supply the
vocabulary-membership one **in their own words** — the point of the trailer is that the owner wrote it,
so I am not drafting it for them.

**A19.3 — recording @security's own note, because it is the most useful thing in this thread.** They
accepted the lower tier and then upgraded, which is more disruptive than getting it right once, and they
named why: *"I verified the patterns myself and treated that as coverage. That's a control that has to
notice standing in for a structural one — the same substitution I've been flagging all loop in your
guard."* That is the identical failure shape as ADR-0036 §11's "a guard reporting clean while the
offending line ships reads as coverage", applied to a review tier rather than to source. Worth keeping
in the record: the substitution is easy to make even by the person actively hunting it.

**A19.4 — THREE instances of the same substitution in one loop, and the tally is the argument.**
Recorded together because each alone reads as an individual lapse and together they read as
structural:

1. **@security** verified three regexes byte-for-byte and treated that as satisfying Mechanical's
   *"guards catch every partial version"* prong. Caught by **@test** — a person.
2. **I** read the same tier the same way and classified the rows accordingly. Caught by
   **@security** — a person.
3. **I** re-walked the §3 file list against the classification table after the amendment rounds and
   *believed* I had reconciled them. `crates/dt-guard/src/common/mod.rs` had no row. Caught by
   **`validate-cross-boundary-scope`, in seconds** — a guard.

Two were caught by careful people who happened to look; one by a control that could not fail to
look. **The two human catches were luck of attention; the third was not.** That asymmetry is a
stronger argument for structural controls than any instance alone, and it is why this loop spent
effort on `ScopeRoot`'s missing constructor and `SecretFinding`'s missing field rather than on more
careful review.

**A fourth arrived later and is the strongest of the five, because it was in the VERIFICATION
rather than the work — and because of a property sharper than "it hid eight lints".**

My `cargo clippy … | grep -E "^(error|warning)"` could never have reported a failure: clippy
prefixes every line with an ANSI escape, so the `^` anchor matched nothing. The important part is
not that it missed things. It is that **the check returned the same result on success and on
failure.** A check with identical output in both states carries **zero bits of information** while
looking exactly like verification — it is not a weak control, it is not a control at all, and no
amount of running it more carefully would have helped.

That is precisely the structural property that makes `SKIPPED` indistinguishable from `PASS` in a
gate, which is what ADR-0036 §11 is about. It is the same defect as a directory-scoped deny pointed
at a directory that no longer exists: not "the guard is bad at its job", but "the guard's output
does not depend on the thing it is supposed to be measuring". Stated that way, the remedy is
obvious and general — **prefer a signal that cannot be silently absent**: an exit code over parsed
text, `assert_absent` over "I looked", a scope-missing token over an empty walk. Every structural
choice in this changeset is an instance of it.

Caught by Layer 5, a control, which surfaced 8 real lint errors. See §Issues Encountered. (@main's
framing, Gate 2.)

**A SIXTH was a near-miss I authored, and it is the one I would most easily have talked myself
into: the tempting green.**

I had a run in which **all seven layers passed** — Layer 7 included, against a live cluster, with
8/8 browser E2E. I declined to report it as the verdict, because it had been produced while an
orphaned second orchestrator was writing to the same `${DEVLOOP_TMP}` state (Issue 8).

The result was almost certainly true. Reporting it would have been convenient and probably
harmless. **That is exactly what made it tempting, and exactly why it would have been the sixth
instance** — authored by the person maintaining this list, in the loop whose subject is controls
reporting results that do not correspond to what happened.

The principle that decided it, stated so it survives me: **a verdict's authority comes from the
integrity of the run that produced it, not from the plausibility of its content.** A green
obtained from a contaminated run is not a weak green; it is not a green at all, for the same reason
a grep with an unmatched anchor is not a weak check — in both cases the output has been decoupled
from the thing it is supposed to measure. Had I reported it, nobody would ever have known, which is
the property that makes this class dangerous rather than merely wrong.

**A SEVENTH — and it is the encouraging one, because it says how the others were found.**
Of the twelve Gate-2 findings, **none came from re-reading**:

* **@security** found the lifetime-tick bypass from a *structural prior* — that a rewrite done
  under lint pressure is where a defect would survive — and then reproduced it against the built
  binary rather than reasoning about it. They predicted the location before they had the bug.
* **@dry-reviewer** and **@code-reviewer** independently found the same 1-of-3 `INSTRUMENT_RE`
  promotion by **disbelieving a doc-comment's own claim** — `telemetry_macros.rs` asserted "the
  three legacy consumers re-point at it" and two did not. The home had authority it had not
  earned, which is the failure this loop kept naming, occurring in the artifact built to fix it.
* **@paired-observability F1** and **@operations F2** are the *same missing mechanism*, found
  independently by its two authors. `MIXED_CONDITIONS` was ruled in as a printed mechanism
  explicitly replacing runbook prose; between two exchanges the prose shipped and the mechanism
  did not.
* **@test** found that the telemetry half lacked the `ALL`-iteration forcing function the metrics
  half had — a gap that is invisible today (the selection happens to be complete) and only bites
  when someone adds a vocabulary member a year from now.

The common method: **they asked what the artifact could not prove about itself.** Not "is this
right?" but "what would be true if this were wrong, and would anything say so?" That is the same
question the guard asks of the media path, turned on the guard — and it is why the review found
twelve things that careful re-reading had not.

**AN EIGHTH — the one where three of us were confidently wrong together, and a check nobody
strictly needed found the truth.**

The Gate-2 Layer-3 red (`run-story-selftest`) was attributed to `${DEVLOOP_TMP}` contention.
@main proposed it as "likely, not certain"; @test built on it; I wrote it into `docs/TODO.md` as
that entry's worked example. It was a good story: the bug is real, the suite is the most
`${DEVLOOP_TMP}`-exposed thing in the pipeline, and orphaned orchestrators had demonstrably been
running. **It was wrong.**

@operations declined to accept it and asked for a falsifier. `/tmp/devloop/layer-3.stderr.log`
had one: `HEAD` unchanged, `sha256(git status --porcelain)` unchanged — the same file *list* —
and `sha256(git diff HEAD)` **changed**. Same files, different contents, mid-run. **That was me**,
editing `main.md` and `docs/TODO.md` inside the run's window, after being told those paths were
excluded from the verdict binding.

Three things worth separating, because they are different mistakes:

1. **Mine**: I edited the working tree during a validation run. `gate2_is_excluded` covers
   `docs/devloop-outputs/**` — that is a property of the *verdict signature*, not of the *run*.
   `run-story.test.sh`'s containment proof does not care what the signature excludes. Two different
   properties, and I treated one as the other.
2. **The collective one**: a plausible mechanism, independently corroborated, accepted by three
   people without anyone checking whether it was *this* event's mechanism. The evidence for
   `${DEVLOOP_TMP}` contention was real — it just wasn't evidence about this failure.
3. **The one that saved it**: @operations asked for a falsifier for a conclusion everyone already
   agreed with. That is the step that felt unnecessary and was the only one that worked.

**And the trap itself is the sharpest instance in this whole list.** `run-story.test.sh` *detected*
correctly — the repo did change under the run — and then *attributed* wrongly, reading "stable
across two post-run samples ⇒ no concurrent writer ⇒ the suite is the actor", which fails whenever
the writer stopped before sampling. It then printed **"Treat as a real containment failure. Do NOT
weaken this check."** A confidently-worded misattribution that instructs the reader not to question
it is the failure mode most likely to end an investigation early — and it did, for three of us.
Filed with the fix direction: **a narrower claim, not a weaker check.**

**@paired-observability's refinement of the method, which is better than mine.** I wrote that the
reviewers found things by "asking what the artifact cannot prove about itself". They corrected it:
their F5 came from checking a fixture's stated Invariant against its body **because they had
already been wrong about exactly that twice, in fixtures they wrote themselves**. So the
generalisation is narrower and more useful — *check the claim against the artifact in the direction
you have already been wrong.* Not a universal skepticism, which does not scale, but a directed one
aimed at your own known failure mode.

**A NINTH — and it is not "we both slipped". It locates where this class of rule stops being
applied.**

@paired-observability made a Gate-3 condition of it: print order and REASON precedence must derive
from **one** array, because *"two lists that agree on the day they are written are two encodings of
one ordering"*. That landed in the Rust as `Rule::ORDER`, with a test asserting every variant
appears in it exactly once.

**In the same changeset, I wrote the full ladder out again in prose** in the `docs/runbooks/`
§6.3 row — a second encoding with nothing detecting drift between them. **@operations reviewed that
row, spotted the duplication on their first pass, and dropped it** when a louder finding surfaced in
the same line.

So: the author implemented the rule in the code and violated it in the document describing that
code, and the reviewer enforcing that rule as a Gate-3 condition let it through in a row they
authored — simultaneously, on one artifact. Neither of us lacked the principle; it was in front of
both of us, in writing, that hour.

**That is why this is evidence rather than an anecdote.** The boundary where the rule stopped being
applied is the *code/prose boundary*, not the competence of either person. Mechanical enforcement
follows a compiler or a guard across that boundary and human attention does not — `Rule::ORDER`'s
one-array property is pinned by a test, and the runbook's copy of it was pinned by nothing. That is
a stronger argument for mechanical enforcement than either miss alone.

The fix (@operations' verbatim text) does not restate the ordering in a corrected form; it **points
at the array and says why it is not restated**. Restating it correctly would have re-created the
problem at half scale. And the sharpest reason to remove it rather than fix it: after the ladder
ruling, `Rule::ORDER` drives STATUS precedence *and* print order from one array by design, so a
stale runbook copy would be wrong in **two** directions from one sentence.

**A fifth, found by @main while checking a flag I raised, is the most consequential of the ones I
found before review, because it is a GATE rather than a guard.** `shellcheck` is listed in the devloop skill's artifact-specific table as
mandatory at Gate 2 for any diff touching `*.sh`. The binary is not installed, and **no layer
wrapper and no CI job invokes it** — the only in-tree occurrences are `# shellcheck source=` and
`# shellcheck disable=` directives *inside* scripts, annotations for a tool that never runs. Every
devloop touching a shell file has ticked that box with nothing under it. Filed in `docs/TODO.md`
with the enumerated evidence and with the fix split into its two halves, because installing the
binary without wiring it into a layer reproduces the current state with a binary present.

#### 20. Verified commit sequence (@operations) — A18.2 released, two commits confirmed

@operations verified the split against `scripts/lang/_gate2_binding.sh` and `.githooks/pre-commit`.
**The two-commit split costs no extra pipeline run — but only in this exact order.** Wrong order and the
Gate-2 hook blocks commit 2, forcing a second full `layer-all.sh` including Layer 7.

1. **Implement all 22 files. The worktree must be COMPLETE before either commit.**
2. `git add` **only** the extraction files (the five re-pointed modules + `common/status.rs`).
   **Do NOT stage `main.md`.** → commit 1.
3. Run `./scripts/layer-all.sh` **once**.
4. `git add -A`, set `Phase=complete`, → commit 2.

**Why the order is load-bearing.** `gate2_validate_commit` recomputes the signature over the *staged*
set and requires **exact equality — there is no subset rule**. The producer's universe is
`git diff -z --name-only HEAD` ∪ untracked. Run layer-all *before* commit 1 and the verdict binds all 22
records against the old HEAD, so commit 2's staged set mismatches and the hook blocks. Run it *after*
commit 1 and the extraction files are in HEAD, drop out of `git diff HEAD`, and the verdict binds
exactly what commit 2 stages.

**Three traps in step 2:**
* **`main.md` is not staged in commit 1** — two independent reasons: staged at `Phase=complete` it trips
  Gate-2 conjunct 1 with no verdict yet, and the hook separately rejects any staged devloop `main.md`
  containing `pending` reviewer rows, which is its state at that moment.
* **Commit 1 is made from the COMPLETE worktree, not a partial one.** The pre-commit hook runs
  `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` against the
  **worktree**, not the index — committing extractions before the guard module exists fails clippy on
  the dangling `media_telemetry_deny` reference in `main.rs`. Write everything, commit twice from one
  tree; only the staging differs.
* Commit 1's hook takes a full clippy pass. Normal cost, not a signal.

`docs/devloop-outputs/**` is excluded from the binding entirely (`gate2_is_excluded` rule 1), so
`main.md` is the *trigger* but never a bound record. `run-story.sh` tolerates two commits — it asserts
only `head_after != head_before` and uses `head_before` as a range baseline.

**Two honesty notes @main asked be explicit rather than implicit:**
(a) Commit 1's content is validated as part of the **final tree** by layers 1/2/4/5/6 and again by the
runner's post-devloop `run_gate` — **but never as an independent intermediate state.**
(b) If layer-all reds at step 3, **commit 1 is already on the branch with no verdict.** Recovery is
fix-and-rerun, but a §Rollback reader should know that window exists.

**A20.1 — §Rollback wording corrected (@operations).** I had written that §Rollback would carry both
SHAs. **Commit 2's SHA is not knowable when §Rollback is written — it is the commit that contains
§Rollback.** Corrected to the `ts-no-retained-credentials` form: name **commit 1 by SHA** (it exists by
then), identify **commit 2 by description plus file list**, and give the instruction as
`git log --oneline -2` then `git revert <the guard commit>`. *"A §Rollback that cites a SHA it cannot
contain is the kind of thing that reads fine and fails at 3am."*

**A20.2 — @test's boundary case strengthened twice (@operations).**
* **Two siblings, not one.** `crates/mh-service/src/session/leak.rs` catches a widening to the **crate**
  root; a second plant **outside the crate entirely** catches a widening to the **repo** root. One
  sibling only pins one of the two regressions.
* **Assert green as exit 0 AND the absence of any `VIOLATION:` / `ERROR:` line** — not merely a zero
  exit. Correction to A17.1: an exit-code-only assertion is satisfied by a guard that stays quiet for
  the wrong reason. ADR-0036 §11's own framing turned on the guard itself — alive, but applied to the
  wrong thing.

---

## Pre-Work

None. The dependency — media-handler's task creating `crates/mh-service/src/media/` with the
§11 sibling layout — had already landed, so the guard had a real scope to point at from the
first run.

---

## Implementation Summary

### Priority 1 — The guard itself

| Item | Before | After |
|------|--------|-------|
| `dt-guard media-telemetry-deny` | did not exist | new subcommand; `--root` + `--explain`; auto-discovered wrapper `scripts/guards/simple/media-telemetry-deny.sh` (5 lines, sources `_dt_guard_wrapper.sh`) |
| Scope | — | config-driven, `scripts/guards/simple/media-telemetry-deny.yaml`, typed `serde_norway` with `deny_unknown_fields`, seeded with exactly `crates/mh-service/src/media/` |
| Metrics deny family | — | derived from `metric_macros::MACRO_NAME_ALTERNATION` (itself derived from `MacroKind::ALL`), flipped `static` -> `pub static`. **Zero edits in the policy module widen it when a 7th variant lands** — pinned by a test that iterates `MacroKind::ALL` |
| Telemetry deny families | — | `telemetry_macros::alternation_for(&[Level, Event, LogMacro, Span, Print])` |
| Invocation anchor | — | `\b(<alternation>)!\s*[\(\[\{]` — shape, not vocabulary. `[`/`{` accepted because `println!{"x"}` is legal Rust and a `(`-only anchor is a one-character evasion |
| Attribute form | — | `(?s)#\[[^\]]*\binstrument\b` over blanked text + a conservative second pass, covering `#[tracing::instrument]`, `#[::tracing::instrument]`, `#[cfg_attr(test, tracing::instrument)]` and multi-line attributes |
| Import deny | — | keyword-anchored first-segment match on `tracing`/`log`/`metrics`/`tracing_subscriber`; excludes only `crate`/`self`/`super`; handles `pub(...)`, leading `::`, whitespace and brace groups |
| Allow-list | — | satisfied **by construction** — `.record(x)` cannot match `record!(`. `ALLOWED_HANDLE_METHODS` is a const + fixture + test, never a line filter |
| Reason tokens | — | nine, all prefixed `media-telemetry-deny-`; six scope/parse + two content + one unparseable-use |
| Output | — | macro **spelling** + `path:line:col` only; `--explain` via `SecretFinding`, which has no `matched` field |

### Priority 2 — Three promote-at-the-third-consumer extractions (Gate-1 approved)

| Item | Before | After |
|------|--------|-------|
| Level-macro vocabulary | `\b(info\|debug\|warn\|error\|trace)!\s*\(` declared **byte-identically twice** (`rust_pii`, `rust_log_secrets`) | one home, `crates/dt-guard/src/telemetry_macros.rs`, group-structured; both consumers select `Level` only |
| `#[instrument]` regex | declared **three times** | `INSTRUMENT_ATTR_BARE_RE` (narrow, the three legacy consumers) + `INSTRUMENT_ATTR_ANY_RE` (correct, this guard) hosted together; **no private copy survives** |
| `TRACING_NAMED_RE` | a fifth hand-maintained encoding | derived from the `Level` group + `INSTRUMENT_ATTR_BARE_RE` |
| Scope-liveness predicate | 12 hardcoded roots across 10 modules, none asserted (`docs/TODO.md` §S16b) | `common/scope.rs::assert_scope_live`; consumed **here only** this loop per @main's boundary, with S16b updated to name it the reference implementation |
| `SCOPE:` operator line | two bare `println!` with bespoke formats | `common::status::emit_scope`; each site keeps its own field list, each pinned by full-line equality |
| Comment/string lexer | `strip_comments`, no raw-string handling | one `lex_line` with two modes; `BlankNonCode` preserves byte offsets; raw strings now handled |

### Priority 3 — Structural controls chosen over controls that have to notice

> **`ScopeRoot` covers PROVENANCE. The reason tokens cover CARDINALITY. Neither subsumes the
> other, and a later reader who deletes one because the other exists will reopen the exact
> failure this guard was built to close.**
>
> `assert_scope_live` returns `Vec<ScopeRoot>`, and `ScopeRoot` has no public constructor — so a
> walker whose signature takes `&[ScopeRoot]` structurally *cannot* be pointed at a path that did
> not pass the assertion. That is a provenance guarantee and it is **all** it is.
>
> It says nothing whatsoever about holding **zero** roots. `Ok(vec![])` type-checks perfectly and
> would hand the walker an empty slice that walks nothing, finds nothing, and reports clean —
> ADR-0036 §11's "alive, never applied", arrived at through a type system that looks like it
> should have prevented it. Cardinality is held by `ScopeFailure::NoDirectoriesConfigured` and the
> per-root `-missing` / `-empty` / `-escapes-root` variants, which exist independently of the type
> and must keep existing.
>
> The attractive error is specific and worth naming: *"the type guarantees the scope is live, so
> why are the tokens still here?"* They are still here because the type guarantees the scope is
> **validated**, not that it is **non-empty**. This is the same alive-versus-applied split the ADR
> is built on, one level down — and it is why a structural control did not remove the need for the
> runtime one. Recorded in `common/scope.rs`'s module doc as well, because that is where the
> deletion would be attempted. (@operations, Gate 1.)

Two places where the reviewers' framing (§19.3) was applied to the code rather than only recorded:

- **`ScopeRoot`** is a newtype with **no public constructor**, and the walker takes `&[ScopeRoot]`.
  Widening the walk from the resolved directories to a crate or repo root is no longer a
  one-character path edit that compiles; it requires adding a constructor, which is visible in
  review. The type covers **provenance**; the tokens cover **cardinality** — `Ok(vec![])`
  type-checks fine, so neither subsumes the other, and the module doc says so.
- **`SecretFinding` has no `matched` field**, and `explain.rs` carries a construction-based
  assertion that fails to compile if one is added. That is what keeps the guard from echoing the
  denied macro's arguments — where the per-participant identifiers live — into CI logs.

### Additional Changes

- `docs/runbooks/devloop-validation.md`: a §6.3 row and seven §8 catalogue rows, plus a warning
  pointer on the `release-build-profile-*` row that this family **inverts** the
  `ERROR: PRECONDITION` prefix's usual meaning.
- `docs/TODO.md`: §S16b gains the reference-implementation note; a new entry records the four
  `#[tracing::instrument` sites, the BARE→ANY coupling, and the two unpromoted
  `line.contains("#[instrument")` substring checks so the inventory is complete rather than
  silently partial.
- `docs/specialist-knowledge/infrastructure/INDEX.md`: navigation rows (condensed to stay under
  the 75-line cap, which the first draft breached at 77).

### What the guard reports on the real tree

```
SCOPE: 1 configured directory, 7 .rs files, 24 enumerated macro forms, 0 hits
STATUS=OK REASON=media-telemetry-deny-clean-7-files-1-dirs
```

`crates/mh-service/src/media/` is genuinely clean — **zero denied forms in production code**.
Every textual hit is prose documenting the ban (`mod.rs` 4 lines, `forward.rs` 2, `ingress.rs` 1),
which is why comment/string blanking was a correctness requirement rather than a nicety: a raw
line matcher would have redded Layer 3 for the whole team on the first run.

---

---

## Files Modified

```
 crates/dt-guard/src/telemetry_macros.rs            | new  (vocabulary canonical home)
 crates/dt-guard/src/media_telemetry_deny.rs        | new  (the policy)
 crates/dt-guard/src/common/scope.rs                | new  (assert_scope_live + ScopeRoot)
 crates/dt-guard/src/common/status.rs               |  +33 (emit_scope)
 crates/dt-guard/src/common/test_code_filter.rs     | +295 -50 (one lexer, two modes)
 crates/dt-guard/src/common/mod.rs                  |   +1
 crates/dt-guard/src/common/pii_vocabulary.rs       |   +5 -2 (owner-authored comment fix)
 crates/dt-guard/src/metric_macros.rs               |   +7 -1 (alternation -> pub)
 crates/dt-guard/src/rust_pii.rs                    |  +14 -19 (re-point)
 crates/dt-guard/src/rust_log_secrets.rs            |   +4 -9  (re-point)
 crates/dt-guard/src/instrument_skip_all.rs         |  +11 -8  (re-point)
 crates/dt-guard/src/release_build_profile.rs       |  +38 -5  (emit_scope + pin)
 crates/dt-guard/src/no_insecure_browser_flags.rs   |  +37 -5  (emit_scope + pin)
 crates/dt-guard/src/lib.rs, main.rs                |  +20    (registration + clap)
 crates/dt-guard/tests/media_telemetry_deny_e2e.rs  | new  (406 lines, 5 tests)
 crates/dt-guard/tests/fixtures/media_telemetry_deny/ | new (23 fixtures + README)
 scripts/guards/simple/media-telemetry-deny.sh      | new  (6 lines)
 scripts/guards/simple/media-telemetry-deny.yaml    | new  (105 lines, mostly header)
 scripts/guards/media-telemetry-deny.test.sh        | new  (456 lines, 73 assertions)
 scripts/layer3.sh                                  |  +29 (run_and_emit wiring)
 docs/runbooks/devloop-validation.md                |  +10 (§6.3 row + 7 §8 rows)
 docs/TODO.md                                       |  +34
 docs/specialist-knowledge/infrastructure/INDEX.md  |   +3 -1
```

**`crates/mh-service/**` appears nowhere in the diff** — verified with
`git status --porcelain | grep -c mh-service` => `0`. The guard's subject was not edited to make
the guard pass.

### Key Changes by File

| File | Changes |
|------|---------|
| `crates/dt-guard/src/media_telemetry_deny.rs` | The policy. `Rule` enum with a single `ORDER` array that is **both** the print order and the REASON precedence (two lists that agree on the day they are written are two encodings); `check_file` as a pure `(rel_path, content) -> Vec<Finding>` so fixtures drive it without a filesystem; `classify_use_line` handling `pub(...)`, leading `::`, whitespace and brace groups; `find_instrument_attributes` with a conservative second pass because `[^\]]*` fails away from the finding. |
| `crates/dt-guard/src/telemetry_macros.rs` | Vocabulary home mirroring `metric_macros.rs`. Five groups with per-consumer selection — the shape that makes silent widening of a PII guard impossible by construction rather than by care. Hosts both `INSTRUMENT_ATTR_BARE_RE` (narrow, legacy consumers, with the four missed sites and the BARE→ANY coupling named at the declaration) and `INSTRUMENT_ATTR_ANY_RE`. |
| `crates/dt-guard/src/common/scope.rs` | `assert_scope_live` returning `Result<Vec<ScopeRoot>, Vec<ScopeFailure>>` — all failures, not the first. Four distinct token suffixes. Existence is checked **before** containment so a symlinked-away directory reports `escapes-root` rather than the benign `missing`, whose remediation would complete the evasion. |
| `crates/dt-guard/src/common/test_code_filter.rs` | `strip_comments` refactored into `lex_line(line, in_block, mode)`; `StripComments` output byte-identical (the ten pre-existing `brace_counter_*` tests are the pin), new `BlankNonCode` blanks comment and string bodies to spaces so column offsets survive. Raw-string handling added — absent before, and its absence failed as a *silent miss*. |
| `scripts/guards/simple/media-telemetry-deny.yaml` | 105 lines, ~95 of them header: why the manifest path and the on-disk directory are not drift-prone duplication (there is nothing to derive a *policy scope* from — the scope-missing failure IS the drift guard), the §11 layout co-obligation the guard structurally cannot check, what a PASS does not certify, and the no-suppression decision. |
| `scripts/guards/media-telemetry-deny.test.sh` | 73 assertions. Includes the case with no counterpart anywhere else: a denied macro planted in **two** non-configured siblings must stay green, asserted as exit 0 **plus** absence of any `VIOLATION:`/`ERROR:` line. |
| `scripts/layer3.sh` | `run_and_emit "media-telemetry-deny-selftest"`, with the neighbours' comment on why it is not under `guards/simple/`. |

---

---

## Devloop Verification Steps

**Binding run** — @team-lead (`main`), `DEVLOOP_FAIL_FAST=0`, detached, quiet uncontested machine,
single-commit tree:

```
PIPELINE_MODE=run-all SOURCE=headless
LAYER=1 OK(1)   LAYER=2 OK(2)   LAYER=3 OK(49)   LAYER=4 N/A(172)
LAYER=5 OK(2)   LAYER=6 N/A(1)  LAYER=7 OK(252)
WRAPPER_EXIT=0
GATE2=PASS  LAYER_ALL_EXIT=0  SLUG=2026-09-07-media-telemetry-deny-guard
```

### Layer 1: Compile — PASS (1s)
buf + cargo + the `dt-guard` release build that Layer 3's wrapper and the self-test both consume.

### Layer 2: Format — PASS (2s)
`cargo fmt --all --check` clean across the touched Rust files.

### Layer 3: Guards — PASS (49s)

| Guard / self-test | Status |
|-------------------|--------|
| `media-telemetry-deny` (new) | PASS — `STATUS=OK REASON=media-telemetry-deny-clean-7-files-1-dirs` |
| `media-telemetry-deny-selftest` (new) | PASS — 83 assertions, 0 failed |
| all other `simple/` guards (41 total) | PASS |
| all other wired self-tests | PASS |

**Layer 3 redded three times before this run, with three DISTINCT causes, none of them a defect in
the changeset.** Recorded in full because "it went red three times" is exactly the shape that gets
compressed into "flaky" and absorbed:

1. **`run-story-selftest`**, first Lead run. Its containment trap proved the real repository changed
   mid-run — `HEAD` unchanged, `sha256(status --porcelain)` unchanged, `sha256(diff HEAD)`
   **changed**. Cause: the implementer editing `main.md` and `docs/TODO.md` while the run was
   executing, having wrongly treated "excluded from the Gate-2 signature" as "safe to edit during a
   run". Two different properties. The trap detected correctly and *attributed* wrongly, and the
   whole team initially accepted a plausible `${DEVLOOP_TMP}`-contention explanation until
   @operations asked for a falsifier. See §Issues Encountered Issue 8, and the two `docs/TODO.md`
   entries it produced.
2. **`validate-cross-boundary-scope`**, second Lead run, against the two-commit tree: five
   `scope_drift_planned_untouched` violations naming exactly the five files commit 1 had absorbed.
   Structural, not a defect: that guard resolves scope as *the active edit* (working-tree-vs-HEAD
   when dirty) **by design**, so once commit 1 lands its files are in HEAD and the plan legitimately
   lists paths the active edit no longer touches. **A two-commit devloop cannot pass it**, and
   nothing says so anywhere. The split was withdrawn; see §Rollback for the two-step revert that
   preserves the property the split existed for.
3. **This run: clean**, which confirms diagnosis (2) end to end.

### Layer 4: Test — PASS (`RESULT=N/A`, 172s)
**`N/A` is the documented proto-placeholder case, not an unrun layer.** Real children beneath:
`STATUS=OK REASON=cargo-test-passed` and `STATUS=OK REASON=nx-test-passed`. Proto's intentional-gap
`test.sh` emits `N/A`, which ranks above `OK` in `_common.sh`'s ladder and so wins the aggregate
while still mapping to exit 0. Recorded explicitly because the summary table is otherwise easy to
misread as two unrun layers.

### Layer 5: Lint — PASS (2s)
`cargo clippy --workspace --all-targets -- -D warnings` clean, including the `clippy.toml`
canonical-home `Lazy<Regex>` `disallowed_methods` rule and the `indexing_slicing` / `panic`
restriction lints.

*This layer earned its keep earlier in the loop*: it caught 8 real lint errors that the
implementer's own clippy invocation had hidden behind a broken grep anchor — a check that returned
identical output on success and failure (§Issues Encountered Issue 6).

### Layer 6: Audit — PASS (`RESULT=N/A`, 1s)
Same placeholder mechanism as Layer 4; real children `cargo-audit-passed` and
`buf-breaking-passed`. No dependency changed, so the ADR-0033 §11 dep-change gate did not fire.

### Layer 7: Env-tests — PASS (252s)
Full env-test suite plus browser E2E against the live Kind cluster.

Layer 7 failed repeatedly earlier in the evening with
`PRECONDITION_FAILURE REASON=cluster-rebuild-failed` — traced to `rejected_busy` behind an orphaned
`rebuild-all` and two `stream write failed: client disconnected` entries in `helper.log`, i.e.
podman builds severed when killed runs took their orchestrators down mid-flight. Contention from
this devloop's own process hygiene, not a broken environment: disk held 378G free throughout and a
clean `rebuild-all` completed at 22:10. On a quiet machine it is green, and it has now passed twice
(498s against the two-commit tree, 252s here).

### New-guard runtimes (ranges, per @operations OP-7)

| Item | Measured |
|------|----------|
| `dt-guard media-telemetry-deny` (7 files, 1 directory) | ~3 ms |
| `scripts/guards/media-telemetry-deny.test.sh` (83 assertions) | 0.15–0.25 s |
| Layer 3 total, with both added | 48–53 s across runs |

Ranges rather than point estimates, because a single number produces both false alarms and false
comfort in reproduce-on-retry triage. The self-test is **not** `timeout`-wrapped (only
`guards/simple/*.sh` are), so nothing will fail because it got slower — this baseline is the only
control. It re-uses **one** pristine copy of the media tree across cases rather than copying per
case, deliberately avoiding the shape that costs `validate-frame-vectors.test.sh` 23–27 s. The
guard+audit fast tier stays well inside the 90 s p95.

(Semantic-guard relocated to the Gate 2 reviewer panel per ADR-0033 Wave 3 #9. See § Code Review
Results → Semantic Guard Reviewer below.)

---

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

Two bypasses of the primary control, both found by running the binary rather than reading it. (1) An unpaired `'` (any Rust lifetime) put `lex_line` into string mode to end-of-line, blanking every denied form after it and defeating BOTH the invocation deny and the import deny; live in the protected directory at 13 mis-lexed lines, so the prior `0 hits` was partly blind spot rather than clean tree. (2) The import deny matched `use` only as a line's first token, so `#[rustfmt::skip]` + `fn b() { use tracing::warn as w; w!("x"); }` returned `STATUS=OK`. Both fixed and re-verified in both directions, including a composition case. Seven hunks ACKed per-hunk against each control's detection set.

### Test Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

(1) The telemetry half lacked the metrics half's `ALL`-iteration forcing function, so a future `TelemetryMacro` variant in an unselected group would silently escape the §11 whole-surface deny. (2) **The lifetime-tick fixture did not red on its own regression** — every case was cross-line, and `lex_line` resets `in_string` per line, so the pre-fix blank never reached the macro. The security fix had zero reversion coverage. Proven by mutation, fixed, and re-verified by mutation.

### Observability Specialist
**Verdict**: RESOLVED-FIXED
**Findings**: 6 found, 6 fixed, 0 deferred

F1 `MIXED_CONDITIONS` ruled in but absent from the diff (not covered by the `-<n>-of-<m>-findings` suffix, which is gated on `winner.is_content()`). F2 the pinned "resolve it in a sibling" clause dropped from `fix_advice`. F3 equality tests named `*_matches_historical_literal` asserting a *reordered* alternation. F4 doubled colon, fixed at the class by folding punctuation into `prefix()`. F5/F6 the new lifetime fixture's `// Invariant:` blocks claimed a `counter!` inside a char literal — a construct that cannot exist — and placed cases cross-line where they pin nothing.

### Code Quality Reviewer
**Verdict**: RESOLVED-FIXED
**Findings**: 2 found, 2 fixed, 0 deferred

(1) Two surviving private `#\[instrument` statics contradicting `telemetry_macros.rs`'s claim that all three consumers re-point — a canonical home asserting authority it did not hold, which is worse than honest triplication. (2) The lexer doc called the rustfmt mitigation a property of the lexer. ADR compliance verified against ADR-0036 §11, ADR-0034, ADR-0033 §6, ADR-0002 and CLAUDE.md. Ownership Lens clean across all 24 rows; no upgrades, no GSA paths, no stray key in `cross-boundary-ownership.yaml`.

### DRY Reviewer
**Verdict**: RESOLVED-FIXED

**True duplication findings** (entered fix-or-defer flow):
- Two of the three legacy `INSTRUMENT_RE` copies were never re-pointed at the promoted home — `crates/dt-guard/src/rust_pii.rs:60-61` (live at `:161`) and `crates/dt-guard/src/rust_log_secrets.rs:96-97` (live at `:177`). Only `instrument_skip_all.rs` had moved. This mattered beyond tidiness: pre-fix the construct was declared three times and visibly so; post-fix-attempt it was declared three times **plus** a canonical home whose doc-comment claimed no private copy survives, and a `docs/TODO.md` entry filed this loop describing all three consumers as reading from it. A home with authority it has not got is worse than honest duplication. **Fixed** — exactly one declaration now exists, at `telemetry_macros.rs`.

**Extraction opportunities** (appended to `docs/TODO.md`):
- `docs/TODO.md` §Code Quality — the 2026-07-30 comment-lexer entry **amended rather than duplicated**: its stated non-BLOCKER premise (`strip_comments` "is not `pub`, so it could not have been imported") is now void, since `lex_line`/`blank_non_code` are `pub(crate)`; a fourth in-crate lexer is a BLOCKER. Also records that the `strip_noise` shape it prescribed now exists in the module it told readers not to standardise on, and that a capability gap opened between the two Rust lexers (raw-string handling landed in one, not `metric_labels.rs`) whose failure direction is a silent miss.
- `docs/TODO.md` §Guard Coverage Gaps (S16b) — `ts_metric_naming.rs:345-348` fails open on absent scope roots under an expired "scaffold-now / fire-later" rationale. Filed because it is the variant neither of S16b's enumeration methods finds: it *has* a presence check, so the tell is not whether a check exists but what it does when it fails.
- Three further planned filings were **withheld** after checking: they were already covered by entries added during this loop. Re-filing them would have been the second-encoding defect this reviewer spent the loop flagging.

### Operations Reviewer
**Verdict**: RESOLVED-DEFERRED
**Findings**: 5 found, 5 fixed, 1 accepted spin-out

F1 the `-scope-directory-empty` runbook rows contradicted the guard's own `THIS IS A DIFF DEFECT` line (cause: their own Gate-1 instruction importing a split that does not fit). F2 `MIXED_CONDITIONS` shipped as prose without the mechanism. F3 the filter-survival test absent — the test that would have caught F2's class. F4 `SCOPE:` carries none of the re-emission filter's five substrings and is dropped in the mode Layer 3 runs, while the helper's doc claimed it guards against silent vacuity. F7 their own §6.3 row re-encoded `Rule::ORDER` in prose with no drift guard. Both Gate-1 conditions verified: the two-commit split and per-site full-formatted-line equality pins on `emit_scope`.

### Semantic Guard Reviewer
**Verdict**: CLEAR
**Native verdict**: SAFE (mapped by Lead per `.claude/agents/semantic-guard.md` §Verdict Mapping)
**Findings**: 0 found, 0 fixed, 0 deferred

No findings.

The one in-scope semantic concern for this diff is ADR-0036 §11's own purpose — the control must not itself emit the per-participant / per-stream identifiers it exists to keep out of telemetry. Verified across **every** output channel rather than the happy path: the per-finding `VIOLATION:` record (macro name plus a `&'static str` fix advice), `--explain` (routed through `SecretFinding`, which has no `matched` field, with a compile-time construction assertion blocking reintroduction), the scope-failure paths (`ScopeFailure::detail()` carries the configured manifest path and a static condition only), `emit_scope` (counts only), and the STATUS/summary lines (reason tokens only). Backed by a placeholder-sentinel fixture and a four-channel self-test.

The 23 planted-violation fixtures are the point of the fixture set, not findings; none plants genuinely sensitive content. Credential Leak, Client Credential Lifetime, Actor Blocking, Error Context Preservation and Metrics Path Completeness give no signal on a guard-machinery diff.

---

## Accepted Deferrals

**Each entry here is an issue the devloop chose NOT to fix.** Every bullet is a cost shift: the implementer didn't pay the fix-now cost, so a future reader will pay fix-later cost + tracking overhead. List only what was actually deferred — not "follow-ups" or "future improvements" or "potential extractions." If something was fixed, it doesn't belong here.

**Tech debt entries themselves live in `docs/TODO.md`. This section holds only pointers to those entries.** Do not create a `TODO.md` at the repo root or anywhere else — there is exactly one `docs/TODO.md` for the whole project. Do not inline the debt body here — multi-line entries belong in `docs/TODO.md`, not in this section.

Each pointer is exactly one bullet of the form `- \`docs/TODO.md\` §SECTION-NAME — one-line hook (≤80 chars)`. If you wrote more than one line per entry, you're writing it in the wrong file — move the body to `docs/TODO.md` and leave only the pointer here.

- `docs/TODO.md` §Observability Debt — four `#[tracing::instrument` sites unmatched
- `docs/TODO.md` §Guard Coverage Gaps — S16b's ten modules not re-pointed at `assert_scope_live`
- `docs/TODO.md` §Observability Debt — `shellcheck` Gate-2 row has no tooling behind it
- `docs/TODO.md` §Observability Debt — concurrent `layer-all.sh` runs corrupt `${DEVLOOP_TMP}`
- `docs/TODO.md` §Observability Debt — `rust_pii` Check-3 fields-scoping fix (Lead-scoped, out of scope)
- `docs/TODO.md` §Observability Debt — `ac_client.rs:84` `user_id` reaches a span attribute (GC-owned)
- `docs/TODO.md` §Guard Coverage Gaps — S18, `cite_extract`'s `IN_SCOPE_DIRS` narrower than its property
- `docs/TODO.md` §Guard Coverage Gaps — `SCOPE:` line discarded by `run-guards.sh` (operations spin-out)

Both were scoped out by @main rather than chosen here; each `docs/TODO.md` entry carries its own reasoning, and the `pii_vocabulary.rs` false-premise comment was fixed in this changeset rather than deferred.

---

## Rollback Procedure

**This devloop lands as ONE commit, and reverting it is therefore a TWO-STEP operation.**

A two-commit split was planned and confirmed at Gate 1 precisely so that reverting the guard would
not revert the value-neutral extractions it sits on. It was withdrawn at Gate 2 as structurally
impossible: `validate-cross-boundary-scope` resolves scope as working-tree-vs-HEAD when dirty, so
once commit 1 lands its files are in HEAD and absent from the active edit, and the guard emits
`scope_drift_planned_untouched` for every file the plan lists but the remaining edit does not
touch. The plan describes the whole changeset; the active edit is half of it. The guard is right
and the convention was wrong — see `docs/TODO.md` §Devloop tooling assumes one commit per devloop.

### The two halves

* **The extraction half** — `crates/dt-guard/src/common/status.rs` (`emit_scope`),
  `common/pii_vocabulary.rs`, `telemetry_macros.rs` (new), and the re-pointed
  `instrument_skip_all.rs`, `no_insecure_browser_flags.rs`, `release_build_profile.rs`,
  `rust_pii.rs`, `rust_log_secrets.rs`. Value-neutral; every changed pattern pinned by a
  full-formatted-line equality test against its historical literal.
* **The guard half** — `media_telemetry_deny.rs`, `common/scope.rs`, `common/test_code_filter.rs`,
  `metric_macros.rs` (visibility), clap dispatch, wrapper, manifest, fixtures, e2e harness,
  self-test, `layer3.sh` wiring, runbook + TODO + INDEX rows.

### To remove the guard but keep the extractions

```bash
git log --oneline -1                    # identify this commit
git revert --no-commit <this commit>
git checkout HEAD -- \
  crates/dt-guard/src/common/status.rs \
  crates/dt-guard/src/common/pii_vocabulary.rs \
  crates/dt-guard/src/telemetry_macros.rs \
  crates/dt-guard/src/instrument_skip_all.rs \
  crates/dt-guard/src/no_insecure_browser_flags.rs \
  crates/dt-guard/src/release_build_profile.rs \
  crates/dt-guard/src/rust_pii.rs \
  crates/dt-guard/src/rust_log_secrets.rs
cargo check --workspace && cargo test -p dt-guard --lib
git commit
```

**This commit is identified by DESCRIPTION, not by SHA** — its SHA cannot appear in this section,
because this section *is* in it. A rollback note citing a SHA it cannot contain reads fine and
fails at 3am.

### The evidence this rests on

The extraction half **was** verified in isolation before the reset, at commit `6d0619f2`:
`cargo check` clean and **445 lib tests passing on the extraction half alone**, in a detached
worktree with a separate target dir. That commit no longer exists — it was soft-reset when the
scope-drift incompatibility was found — so **this paragraph is now the only record that the
two-step revert lands on a green tree.** Without it the procedure above is an assertion rather
than a tested claim.

### Three things a reader needs to know before relying on this

1. **Two-step revert is weaker than a commit boundary, and that is a known cost.** It does not
   survive later commits building on `emit_scope` or the promoted macro vocabulary: once they do,
   `git checkout HEAD -- <extraction files>` restores files those commits depend on to a state
   that predates them. Read the diff; do not trust the recipe blindly at that point.
2. **`git revert <this commit>` alone leaves a tree without the extractions.** That tree builds —
   the extractions are additive and their consumers were re-pointed in the same commit — but it
   discards value-neutral work for no reason. The `git checkout HEAD --` step is the point.
3. **Nothing outside this repository is affected.** No schema change, no migration, no deployed
   artifact, no infrastructure manifest. In particular **`crates/mh-service/src/media/**` was
   never edited**, so no revert can affect the media path itself.

### If the guard is wrong rather than the code

Use the procedure above. **Do not reach for a suppression instead** — there deliberately is none
(no `guard:ignore`, no env var, no flag), and adding one under time pressure is exactly the
failure ADR-0036 §11 argues against.

---

## Issues Encountered & Resolutions

### Issue 1: The real media tree would have redded the guard on its first run — from its own documentation
**Problem**: `crates/mh-service/src/media/mod.rs` documents the §11 ban in prose, so it contains
`counter!`, `histogram!`, `gauge!`, `describe_*!`, `println!`, `eprintln!`, `dbg!`, `event!`, span
macros and `#[instrument]` as *text*. `forward.rs` carries ``/// … a `debug!(?frame)` in``, which
matches a correctly-shaped invocation anchor exactly. A raw line matcher reds Layer 3 for the
whole team on day one — on the very file that documents the rule.
**Resolution**: every matcher runs over blanked text. Reused the existing lexer in
`common/test_code_filter.rs` rather than writing a third stripper, refactored into one `lex_line`
with two modes. Rejected the two tempting shortcuts explicitly: skipping lines containing `//`
(a trailing-comment bypass — `tracing::info!("leak"); // note` ships silently) and
`instrument_skip_all::is_comment_line`, which skips any trimmed line starting with `*`, making
`*self.n = 0; tracing::info!(…)` a one-character bypass.

### Issue 2: The operator lane was not actually available to this guard
**Problem**: the task asked for a deliberate STATUS-lane choice, and `PRECONDITION_FAILURE`/exit 2
looked right for a scope rename by analogy with `validate-frame-vectors`.
**Resolution**: reading `run-guards.sh::classify_guard_exit` showed only exits 124 and 137 reach
`PRECONDITION_GUARDS`; **every other non-zero, including 2, falls into the `*)` arm and is counted
as a violation**. A guard exiting 2 would print operator-lane text while being counted
implementer-lane — the STATUS line and the aggregation disagreeing, which is worse than either
lane. Both scope tokens are `STATUS=FAIL`/exit 1, defended in the module doc against the
frame-vectors precedent (whose vacuity means wrong-checkout, a machine fact; ours means someone
edited the tree in this diff).

### Issue 3: A stdout/stderr output split that could not have worked
**Problem**: I proposed printing scope failures as `VIOLATION:` on stdout plus `ERROR:` on stderr,
to satisfy two reviewers who disagreed on the prefix.
**Resolution**: withdrawn on two independent disproofs. `run-guards.sh:252` captures the guard
with `2>&1`, so guard stderr merges into stdout and never reaches `layer-3.stderr.log` — the
stderr line would have landed beside the stdout one, emitting each finding twice into one channel
and burning two of the five `head -5` slots per finding. Separately, stdout is block-buffered when
piped while stderr is not, so the two would interleave nondeterministically and "scope failures
print first" would stop being true exactly when it mattered.

### Issue 4: The self-test's exit-code assertions were testing nothing
**Problem**: `out="$(run_guard "$root")"` runs the function in a **subshell**, so its `RC=$?`
never reached the caller. Nineteen exit-code assertions were reading a stale `0`.
**Resolution**: caught because they failed loudly rather than passing — the expected codes were
`1` and the stale value was `0`. Had the polarity been reversed, all nineteen would have passed
while asserting nothing. Switched to globals with the reason recorded at the function, since the
next person will reach for the command-substitution form.

### Issue 5: A hardcoded scope size in the self-test
**Problem**: two assertions pinned `"7 .rs files"`. Adding a file under `media/` would have redded
the self-test for a change it has no opinion about — how suites get weakened rather than fixed.
**Resolution**: derived `REAL_RS_COUNT` from the tree. The property under test is "the guard
reports the size of the real configured scope", not "the scope has exactly N files". The
non-empty half is asserted in the Rust harness, also without a count.

### Issue 6: My clippy verification could never have reported a failure
**Problem**: I ran `cargo clippy … | grep -E "^(error|warning)"` and reported clean. Clippy prefixes
every line with an ANSI escape sequence, so the `^` anchor matched nothing on a **failing** run
exactly as it matched nothing on a passing one — an empty result read as success, inside the
verification of a guard built to prevent that.
**Resolution**: Layer 5 found **8 real lint errors** the grep had hidden — six
`indexing_slicing` / `needless_range_loop` in the rewritten lexer, one `clippy::panic` in the e2e
harness, one `useless_vec` in a test I added. All fixed: the lexer now walks on `.get()` and
iterators (the lint is right — a lexer indexing a counter it also mutates is where an off-by-one
becomes a panic in the thing that is supposed to not fail), and the harness carries the same
crate-level `#![expect(clippy::panic, reason = …)]` its two sibling integration tests already use.
Verification now strips ANSI first and checks the exit code rather than the text.
**Footnote worth keeping**: my first `#![expect]` listed `clippy::unwrap_used` too, and
`unfulfilled_lint_expectations` rejected it — the compiler refuses an expectation that never fires.
The same principle this guard applies to source, applied to annotations.

### Issue 7: An unlisted file in my own classification table
**Problem**: Layer 3 failed with
`VIOLATION: … [scope_drift_inbound] "crates/dt-guard/src/common/mod.rs"` — the one-line
`pub mod scope;` registration had no row.
**Resolution**: row added. Worth recording rather than quietly fixing: I had re-walked the file
list against the table after the amendment rounds and *believed* they were reconciled. That
reconciliation was a control that had to notice, and it didn't; the guard caught it in seconds.
Third instance in this loop of the same substitution, and the first where the structural control
existed and simply did its job.

### Issue 8: Two `layer-all.sh` runs silently shared one state directory
**Problem**: a pipeline run reported every layer green and then exited 2 with **no
`LAYER_SUMMARY` block**, and its `gate2-verdict` carried `GATE2=FAIL` with rows for LAYER 1–6 only —
`layer_status[7]` never assigned.
**Cause**: `TaskStop` kills the wrapper shell but **not** the detached `bash ./scripts/layer-all.sh`
beneath it. An orphan from an earlier "stopped" run was still alive at Layer 7, writing to the same
`/tmp/devloop/layer-N.log` files. Timestamps proved it: the run's own log ended at 22:27:32 while
`layer-6.log` was 22:28:41 and `layer-7.log` 22:29:00. `layer-all.sh` does
`status=$(parse_status_line "${DEVLOOP_TMP}/layer-${n}.log")` under `set -e`, so a log clobbered by
the other process aborts the loop and fires the EXIT trap with a partial verdict.
**Resolution**: orphans killed, state cleared, and the result discarded rather than reported — a
verdict produced under contaminated shared state is worth nothing even when every layer says OK.
**Worth filing beyond this loop**: `${DEVLOOP_TMP}` has no concurrency guard, and the failure mode
is not "the second run waits" or "the second run errors" — it is **a green run producing a FAIL
verdict with silently missing layer rows**. Same shape as everything else here: a control reporting
a result that does not correspond to what actually ran. A PID/flock guard emitting a loud
`PRECONDITION_FAILURE` would close it.
**And a second-order lesson at my own expense**: while clearing that state I ran
`rm -f /tmp/devloop/layer-*.log` without checking what else was running — and @main's Gate-2 run was
live. I reported it immediately rather than waiting to see whether it mattered. Diagnosing a
concurrency hazard is not the same as being immune to it.

### Issue 9: A Rust lifetime blanked the rest of the line — a live bypass of the primary control
**Problem**: `lex_line` treated every `'` as a string-literal opener. A lifetime has one tick and no
closer, so `in_string` stayed true to end-of-line and **every denied form to its right was
invisible**. @security reproduced it against the built binary with the real manifest: 4 of 8 planted
lines passed clean, including `fn a(s: &'static str) { tracing::info!("A"); }`.
**Why this was the finding of the loop, not a bug**: (a) **both layers failed together** — the
import deny exists precisely as the mitigation for the alias evasion the invocation deny cannot see,
and on a line with an unpaired tick neither fires, so the guard had *no* coverage rather than
degraded coverage; (b) **it was live in the protected directory** — `media/forward.rs` carries
`fmt::Formatter<'_>` and `enum FrameSource<'a>` in code position today, and nothing was missed only
because no denied form happened to sit to their right. That is line layout, not the control working.
`&'static str` is the ADR-0029 bounded-label type, i.e. the construct most likely to share a line
with telemetry added next month.
**Resolution**: the `'` arm now distinguishes a char literal from a lifetime by lookahead (`\`
next → char literal; char-after-next is `'` → char literal; otherwise lifetime, emitted as code).
This resolves `'_'` versus `'_` correctly because the discriminator is the trailing quote. **The
failure direction is deliberate and documented**: mis-reading a char literal as a lifetime leaves
its contents as code, which can only cause a false positive — loud; mis-reading a lifetime as a
string opener causes a miss — silent.
**The shared-lexer constraint @main imposed, and how it was discharged.** `test_code_filter.rs` is
shared, and in `compute_test_block_ranges` the same bug is a *documented fail-safe-broad*
over-exemption other guards rely on. Rather than fork the lexer per mode, I measured: both lexers
were simulated over every `.rs` file in `crates/`, and **`StripComments`-mode output differs on
zero files** — the change is provably inert for every existing consumer on the current tree. In
`BlankNonCode` mode it changes **13 lines across 2 files inside the protected media directory**,
which quantifies exactly how blind the guard had been.
**Pinned by**: `pos_lifetime_tick_does_not_blank.rs`, whose 9 expected hits include the
`use tracing::info as note;` variant (the import-deny half) and a `counter!` *inside a char literal*
that must stay invisible — so a future "simplification" that stops entering string mode for `'`
reds the fixture rather than passing.

### Issue 10: The import deny matched `use` only at line start — both layers evaded inside one file
**Problem**: `classify_use_line` anchored on `use` as a line prefix, so
`fn b() { use tracing::warn as w; w!("x"); }` passed clean. **Both layers missed it**: the mid-line
`use` escaped the import deny, and `w` is not on the invocation deny list. This is the residual the
module doc described — except the doc said it required a sibling re-export, and it did not. Two
lines, one file, `STATUS=OK`.
**Severity, honestly**: materially lower than the lifetime bypass. `cargo fmt` (Layer 2) splits the
inline form, after which a line-anchored match catches it, and there are zero `rustfmt::skip` and
zero inline-`use`-after-brace occurrences in `crates/` today. So it was a *reachable* gap, not a
live miss. But `#[rustfmt::skip]` survives `cargo fmt --check` by design, and @security verified
that pair reports OK — and "rustfmt would have reformatted it" is precisely the
control-that-has-to-notice reasoning §11 rejects.
**Resolution**: try the whole line first, unchanged; only on failure, split on `{` / `;` and test
each segment. **The ordering is load-bearing** — splitting first would cut the brace-group form
`use {tracing::info, std::fmt};` at its own `{` and downgrade a correct `Denied` to `Unparseable`,
which a test now pins. The module-doc residual paragraph was corrected too: @security would not
accept that deferred, and they were right — it is the sentence a future reader relies on, and it
under-stated the class it was written to disclose.

### Issue 11: A fixture that disagreed with itself
**Problem**: `pos_metrics_label_macros.rs`'s `// Invariant:` block claimed five hits over a body
containing four invocations, and its prose said "four bare forms" where the body had three.
**Resolution**: routed to `@paired-observability` as a question rather than adjusted — the
standing commitment was that the expectation never moves to match the implementation. They ruled
the body correct and the prose wrong twice over. A second, smaller disagreement
(`pos_unparseable_use.rs`'s per-rule split) turned out to encode an unanswered *policy* question —
whether a `use` line that resolves to a denied root but is also malformed reports as an import
violation or as unparseable — which is now stated explicitly rather than left inferred. Both
would have been silently absorbed under the ordinary reflex, and the second would have pinned the
inverse rule.

---

---

## Lessons Learned

1. **"I verified it myself" is not "the pipeline covers it", and the substitution is invisible from
   the inside.** @security accepted `Mechanical` on three re-point rows after byte-comparing the
   regexes, then withdrew it: Mechanical's third prong is *guards catch every partial version*,
   and the only thing catching a partial application was an equality test written in the same diff
   by the same author — **the control had the same author as the risk**. I had made the identical
   substitution reading the same tier. Two people made it inside a loop whose entire subject is
   that substitution, which argues it is structural rather than a competence problem. §19.3 keeps
   the record.

2. **Reading the mechanism beat reasoning about it, every single time it was tried.** Four
   plausible readings turned out wrong and each was falsified in minutes by opening the file:
   the operator lane looked available (it isn't — `classify_guard_exit` routes only 124/137); the
   stdout/stderr split looked additive (it isn't — `2>&1` merges the capture); the two-commit split
   looked like it cost a second pipeline pass (it doesn't, in one specific ordering); and my own
   `§Rollback` cited a SHA it could not contain. None of these were subtle once looked at.

3. **A structural control and a runtime control can look redundant and not be.** `ScopeRoot` with
   no public constructor makes it impossible to walk an unvalidated path — and says nothing about
   walking *zero* paths, since `Ok(vec![])` type-checks perfectly. Provenance and cardinality are
   different properties. The attractive error ("the type guarantees the scope is live, so why are
   the tokens still here?") is exactly how a later reader deletes the half that was doing the work.

4. **The two-halves frame kept catching things in the control rather than in the code it guards.**
   Four times: the guard's own scope going vacuous; the guard reproducing §11's leak into CI logs
   by echoing macro arguments; the deny leaking *outside* its scope with every existing test still
   green; and an exit-0 assertion satisfied by a guard staying quiet for the wrong reason. Each is
   "alive but applied to the wrong thing", one level up from where the ADR aims it.

5. **The expectations-never-move rule paid for itself twice in one message, and both times the
   fixture was wrong.** Routing two count disagreements back to the owner rather than adjusting
   them found a fixture whose prose disagreed with its own body, and an unanswered policy question
   about token precedence that a silent adjustment would have pinned backwards. The rule's value
   is not that implementations are usually wrong — it is that the *disagreement* is information,
   and absorbing it destroys the information.

6. **Promoting less, honestly, beat promoting a pattern nobody runs.** The `#[instrument]`
   promotion went through three shapes before landing on hosting both the narrow historical regex
   and the correct one side by side. The rejected shapes each satisfied one reviewer's principle by
   violating another's; the one that worked satisfied all three because it stopped trying to make
   the canonical home carry a single answer to a question that genuinely has two.

7. **A check whose output is the same on success and on failure carries zero bits of information,
   however careful the person running it.** `cargo clippy … | grep -E "^(error|warning)"` returned
   nothing on eight real errors, because clippy colours its output and the `^` anchor never
   matched. The failing run and the passing run were byte-identical *to me*. This is not "a weak
   check" — it is not a check, and running it more attentively could not have helped. It is the
   same structural property that makes `SKIPPED` indistinguishable from `PASS` in a gate, and it is
   what ADR-0036 §11 is actually about. The general remedy applies to the checking as much as to
   the checked: **prefer a signal that cannot be silently absent** — an exit code over parsed text,
   `assert_absent` over "I looked and it wasn't there", a scope-missing token over an empty walk.

8. **A guard's runtime cost is a design input, not an afterthought.** The self-test lands at
   0.15–0.25s because it re-uses one pristine tree copy rather than copying per case — the shape
   that costs the frame-vectors self-test 23–27s and makes it the largest single item in Layer 3.
   Choosing that at design time cost nothing; retrofitting it would have meant rewriting the suite.

---

---

## Appendix: Verification Commands

```bash
# Full pipeline (the authority run — headless selects PIPELINE_MODE=run-all)
./scripts/layer-all.sh

# The guard, against the real tree
cargo build --release -p dt-guard
./target/release/dt-guard media-telemetry-deny --root "$(pwd)"
./target/release/dt-guard media-telemetry-deny --root "$(pwd)" --explain

# Through its auto-discovered wrapper (what Layer 3 actually runs)
./scripts/guards/simple/media-telemetry-deny.sh

# The self-test — every failure branch the real tree never reaches
./scripts/guards/media-telemetry-deny.test.sh

# Fixture catalog, the in-tree scope premise, and the redaction assertion
cargo test -p dt-guard --test media_telemetry_deny_e2e

# Behaviour-preservation for the three promoted re-points
cargo test -p dt-guard --lib telemetry_macros
cargo test -p dt-guard --lib rust_pii
cargo test -p dt-guard --lib rust_log_secrets
cargo test -p dt-guard --lib instrument_skip_all

# The two guards that caught real defects in this changeset
./target/release/dt-guard cross-boundary-scope --root "$(pwd)"
./target/release/dt-guard cross-boundary-classification --root "$(pwd)" \
    --main-md docs/devloop-outputs/2026-09-07-media-telemetry-deny-guard/main.md

# Style + lint
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash -n scripts/guards/media-telemetry-deny.test.sh \
        scripts/guards/simple/media-telemetry-deny.sh scripts/layer3.sh
```

### Proving the guard actually fires (do this before trusting a green)

A guard that reports clean is indistinguishable from a guard that checked nothing. Two ways to
falsify that here, both non-destructive:

```bash
# 1. Plant a denied macro in a THROWAWAY copy — never in the real media tree.
tmp="$(mktemp -d)"
mkdir -p "$tmp/scripts/guards/simple" "$tmp/crates/mh-service/src"
cp scripts/guards/simple/media-telemetry-deny.yaml "$tmp/scripts/guards/simple/"
cp -r crates/mh-service/src/media "$tmp/crates/mh-service/src/media"
echo 'pub fn x() { tracing::info!("leak"); }' >> "$tmp/crates/mh-service/src/media/forward.rs"
./target/release/dt-guard media-telemetry-deny --root "$tmp"   # expect exit 1
rm -rf "$tmp"

# 2. Rename the scope out from under it — the "alive, never applied" case.
tmp="$(mktemp -d)"
mkdir -p "$tmp/scripts/guards/simple"
cp scripts/guards/simple/media-telemetry-deny.yaml "$tmp/scripts/guards/simple/"
./target/release/dt-guard media-telemetry-deny --root "$tmp"
# expect: STATUS=FAIL REASON=media-telemetry-deny-scope-directory-missing
rm -rf "$tmp"
```

**Do not plant into `crates/mh-service/src/media/` itself** — that reds Layer 3 for everyone until
it is removed, and a forgotten plant is worse than no test.
