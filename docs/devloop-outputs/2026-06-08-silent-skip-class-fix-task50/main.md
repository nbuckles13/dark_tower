# Devloop Output: Silent-Skip Class Fix (task #50)

**Date**: 2026-06-08
**Task**: Close the silent-skip-at-pipeline-edge regression class — wrapper EXIT trap + dispatcher exit-code tightening so an unexpected `SKIPPED-NO-VERB` / `UNKNOWN` can no longer be rationalized as a deliberate skip.
**Specialist**: infrastructure (paired with operations + test)
**Mode**: Agent Teams (v2), full
**Branch**: `feature/browser-client-join-task-50`
**Duration**: ~ (in progress)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `44e8b194308a36c03b558700a8474a92c47d086b` |
| Branch | `feature/browser-client-join-task-50` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-task50` |
| Implementing Specialist | infrastructure |
| Iteration | 1 |
| Security | `security@devloop-task50` |
| Test | `test@devloop-task50` (paired) |
| Observability | `observability@devloop-task50` |
| Code Quality | `code-reviewer@devloop-task50` |
| DRY | `dry-reviewer@devloop-task50` |
| Operations | `operations@devloop-task50` (paired) |
| Semantic Guard | `semantic-guard@devloop-task50` |

### Gate 1 Plan Confirmation Tracking — CLOSED 2026-06-08 (7/7 confirmed)

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (env seam DEVLOOP_TEST-gated + hardcoded prod allowlist; trap fail-closed) |
| Test | confirmed (layer-edge E2E mandatory; SIGTERM deferral → TODO) |
| Observability | confirmed (parallel `__LAYER_REASONS`; loud attributable signal) |
| Code Quality | confirmed (doc-scope complete; reason-threading single-locus; exit-2 committed) |
| DRY | confirmed (all new logic SPOT-centralized in `_common.sh`) |
| Operations | confirmed (ADR §6 amendment + SKILL §403/§405 + runbook §3/§6/§7/§8) |
| Semantic Guard | confirmed (error-context preserved end-to-end; `:162` flatten hole closed) |

**Lead Gate-1 rulings:** (1) doc scope expanded to runbook §3/§7/§8 + SKILL §403 + ADR-0033 §6 (consistency with shipped behavior); (2) ADR-0033 §6 amendment is infra-Mine per ADR §11 (reversed earlier operations-owned call); (3) UNEXPECTED-verb-missing exit code = **2** (wiring-fault class, with UNKNOWN, disambiguated by REASON). Layer-B classification check: PASS (manual — mechanical guard unavailable, dt-guard not built; no GSA paths, all rows present/owned).

---

## Task Overview

### Objective
Eliminate the **silent-skip-at-pipeline-edge** regression class in the polyglot
validation pipeline (`scripts/lang/`, `scripts/layer*.sh`). Two edge cases let a
non-success outcome reach the orchestrator looking like success:

1. A per-language wrapper that aborts (e.g. `set -e`, an `exit 1` in a helper)
   **before** it emits a `STATUS=` line → the dispatcher reads an empty pipe.
2. The dispatcher's `SKIPPED-NO-VERB` is overloaded: it means both "operator
   filtered all langs" (intent) and "a verb script is missing/not-executable"
   (bug), and **both currently exit 0** — so a missing wrapper is silently
   skipped at the pipeline edge.

### Scope
- **Service(s)**: none (validation tooling only — `scripts/**`, docs)
- **Schema**: No
- **Cross-cutting**: Yes — affects every devloop's Gate 2 for all services.

### Debate Decision
NOT NEEDED — implementation follows the fully-specified task #50; the one genuine
design fork (how to distinguish intentional vs unexpected verb-missing without
breaking proto's documented `test`/`audit` gaps) is resolved below and confirmed
by paired operations + test at Gate 1/Gate 3.

---

## Cross-Boundary Classification

All code paths are infrastructure-owned (the validation pipeline). No Guarded
Shared Area paths are touched (no `proto/**`, no `crates/common` crypto, no
`db/migrations/**`). The operations-owned process-doc edits (SKILL.md §403/§405,
runbook §3/§6/§7/§8) are operations-adjacent → **Minor-judgment**; operations is a
**paired** reviewer, so confirmation is in-loop at Gate 1/Gate 3. The ADR-0033 §6
amendment is infrastructure-Mine (ADR §11 — infra owns the wrapper-contract/exit-code
plumbing; Lead ruling 2026-06-08), NOT operations Minor-judgment — it gets
code-reviewer's standing ADR-compliance confirmation at Gate 3, not an operations
sign-off.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/lang/_common.sh` | Mine | — |
| `scripts/lang/_dispatch.sh` | Mine | — |
| `scripts/audit.sh` (security finding: audit-slice fail-open close) | Mine (infra wrapper plumbing, ADR §11) | — |
| `scripts/lang/rust/*.sh` (compile, fmt, lint, test, audit) | Mine | — |
| `scripts/lang/ts/*.sh` (compile, fmt, lint, test, audit) | Mine | — |
| `scripts/lang/proto/*.sh` (compile, fmt, lint, breaking) | Mine | — |
| `scripts/lang/_common.test.sh` | Mine | — |
| `scripts/lang/_dispatch.test.sh` | Mine | — |
| `scripts/lang/_wrapper_trap.test.sh` (new) | Mine | — |
| `docs/TODO.md` (audit findings) | Mine | — |
| `.claude/skills/devloop/SKILL.md` §403 + §405 (2 hunks: §403 narrow "self-justifying" set, §405 add unexpected-verb-missing escalate case) | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` §3 exit-code table (line ~56) + enum-REASON-examples table (line ~75) | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` §6 (Layer 4/6 triage, line ~348) | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` §7 ("Two valid reasons" → three states, line ~425) | Not mine, Minor-judgment | operations |
| `docs/runbooks/devloop-validation.md` §8 symptom-index table (add `wrapper-aborted-early-exit` + UNEXPECTED-verb-missing rows, line ~487) | Not mine, Minor-judgment | operations |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` §6 (amendment note + exit-code table, lines ~229/247) | Mine (ADR §11 — infra owns wrapper plumbing; Lead ruling 2026-06-08 reversing earlier call) | — (code-reviewer ADR-compliance confirm at Gate 3) |
| `docs/user-stories/2026-05-02-browser-client-join.md` (tracking row) | Mine (tracking-table row only) | — |

---

## Planning

### The core tension (resolved)

`_dispatch.sh:148` (verb script missing-or-not-executable) is the **same code
path** used by proto's *intentional* gaps (proto has no `test.sh` for Layer 4 and
no `audit.sh` for Layer 6 — `breaking.sh` is its audit gate). Naively mapping all
verb-missing to non-zero would fail Layers 4 & 6 on every run (regression,
violates acceptance criterion (d)).

**Resolution — three distinct `SKIPPED-NO-VERB` reasons, reason-driven exit, no
precedence re-rank:**

| Source | REASON | Exit | Rank change? |
|--------|--------|------|--------------|
| `_dispatch.sh:99` all-langs-filtered (INCLUDE/EXCLUDE) | `all-langs-filtered` | **0** (operator-intent) | none |
| `_dispatch.sh:148` intentional gap (`proto:test`, `proto:audit`) | `<lang>-<verb>-sh-missing-or-not-executable` | **0** (documented allowlist) | none |
| `_dispatch.sh:148` unexpected verb-missing | `<lang>-<verb>-UNEXPECTED-verb-missing-or-not-executable` | **2** (wiring bug) | none |

**Exit code = 2, not 1 (code-reviewer Pin 1).** An UNEXPECTED verb-missing is a
pipeline-WIRING defect (wrapper deleted / chmod-stripped / a new lang dir missing a
verb), which is ADR-0033 §6 exit-2 semantics ("wrapper/dispatcher bug — investigate the
script itself"), NOT exit-1 ("work ran and detected a problem" — nothing ran). It
groups with `UNKNOWN→2` conceptually, but SKIPPED-NO-VERB is a real enum (not UNKNOWN),
so `status_to_exit_code` gets an EXPLICIT arm: `SKIPPED-NO-VERB` + a reason carrying the
`UNEXPECTED` marker → **2**. The §3 exit-code table + ADR §6 amendment state exit 2 for
this case unambiguously. (`layer-all.sh` folds any non-zero child into `final_exit=1` at
the orchestrator, so 2 still reds CI — the distinction is for the per-layer/per-wrapper
exit + triage legibility: 2 says "fix the wiring", 1 says "fix the code under test".)

The bug token deliberately carries an UPPERCASE `UNEXPECTED` segment (observability P3)
so it visually SHOUTS in a 3am log — distinguishing it at a glance from the
intentional-gap `-sh-missing-or-not-executable` token (which differs from the bug
token by more than one easily-misread word). The distinction (allowlist vs bug) is
the whole point of the fix, so the tokens must not look near-identical.

The precedence ladder (`FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB`,
Wave 2 #4 α) is **left unchanged** — re-ranking would break the locked
regression tests in `_common.test.sh:37-43` and `_dispatch.test.sh` plus the
ADR-0033 invariant. Instead, the **REASON is threaded to the exit-code
decision**: `status_to_exit_code` becomes `(status, reason)`-aware, and
`__layer_lifecycle_end` consults the *worst child's reason* (not just the
aggregated enum) so a layer whose aggregate winner is `SKIPPED-NO-VERB` +
verb-missing exits non-zero. Intentional-gap reason keeps the historical
`-sh-missing-or-not-executable` token → proto Layers 4/6 keep reporting
their normal aggregate (OK dominates rank-0 `SKIPPED-NO-VERB`), **no reported
status regression**.

### How REASON threads through enum-only `aggregate_worst_status` (code-reviewer item 2)

`aggregate_worst_status` stays **enum-only and byte-unchanged** (locked tests + ADR
invariant). The reason is threaded ALONGSIDE it, never INTO it, at BOTH aggregation
sites via index-parallel arrays:

**Dispatcher (`_dispatch.sh`):** today `child_statuses=()` holds enums only. Add a
parallel `child_reasons=()` populated at every push site (`:124` no-diff, `:140`
verb-run parse, `:148` verb-missing) with that child's REASON — for verb-missing the
reason is now reason-keyed: `__is_intentional_gap "$name" "$verb"` (the
`__intentional_missing_verbs` allowlist) → `${name}-${verb}-sh-missing-or-not-executable`
(exit-0 token), else `${name}-${verb}-UNEXPECTED-verb-missing-or-not-executable`.
- `:99` all-langs-filtered stays a separate early `return 0` — NOT in the child loop,
  reason `all-langs-filtered`, never enters the non-zero path (confirmed unchanged).
- Multi-lang aggregate arm (`:156-167`): after `agg=$(aggregate_worst_status …)`,
  compute `worst_reason` = among the children whose enum == `agg`, the reason whose
  `status_to_exit_code(agg, reason)` is highest (so an UNEXPECTED-verb-missing reason
  beats a same-enum intentional-gap reason). The `SKIPPED-NO-VERB)` emit arm emits
  `worst_reason` (not a flattened `${verb}-some-langs-missing-verb`), and the function
  returns `status_to_exit_code "$agg" "$worst_reason"` — so a `SKIPPED-NO-VERB`
  aggregate whose winning reason is UNEXPECTED returns non-zero instead of flattening
  to 0. (This is exactly the `:162`-flatten hole code-reviewer flagged.)
- Single-lang path (`:171`): `return status_to_exit_code "${child_statuses[0]}" "${child_reasons[0]}"`.

**Layer (`__layer_lifecycle_end`):** identical shape with `__LAYER_STATUSES` (enums,
unchanged) + parallel `__LAYER_REASONS`. After `result=$(aggregate_worst_status …)`,
pick the worst-exit-driving reason among children whose enum == `result`, put it in the
stderr `LAYER=…REASON=` field (observability P2), and `rc=$(status_to_exit_code "$result" "$worst_reason")`.

**Matching-key discipline (test-reviewer — must not key off a shared substring).**
The intentional (`-sh-missing-or-not-executable`) and unexpected
(`-UNEXPECTED-verb-missing-or-not-executable`) tokens share the suffix
`missing-or-not-executable`, so a naive substring match would misclassify the
intentional gap and flip proto Layers 4/6 non-zero. Two-layer defense:
- **Dispatcher (authoritative):** the exit decision keys off ALLOWLIST MEMBERSHIP —
  `__is_intentional_gap "$name" "$verb"` (is `lang:verb` in `__intentional_missing_verbs`),
  NOT the reason string. String shape cannot fool it; the reason string is chosen BY
  that decision (intentional → `-sh-missing-`, else → `-UNEXPECTED-verb-missing-`).
- **Layer (`__layer_lifecycle_end`) has no `lang:verb` context** — it sees only STATUS
  lines + reasons. So its `status_to_exit_code(status, reason)` keys off the marker that
  is UNIQUE to the unexpected path: the literal `UNEXPECTED` segment (anchored, e.g.
  reason matches `*-UNEXPECTED-verb-missing-or-not-executable`), NEVER the shared
  `missing-or-not-executable` suffix. `__is_intentional_gap_reason` is defined as "reason
  does NOT carry the `UNEXPECTED` marker" — so the `UNEXPECTED` token is load-bearing,
  not just forensic cosmetics. A test asserts the intentional token does NOT contain the
  unexpected matcher's anchor (belt-and-suspenders beyond the exit-0 assertion).
- **Single source of truth for the marker literal (test-reviewer item 4 — the new risk
  the two-layer split introduces):** the dispatcher's reason-DERIVATION and the layer's
  reason-MATCHER are two independent touch points on the same `UNEXPECTED` literal — a
  rename in one could silently desync them. Both now route through ONE `readonly`
  constant `DEVLOOP_UNEXPECTED_VERB_SUFFIX` in `_common.sh`: the dispatcher builds
  `"${name}-${verb}${DEVLOOP_UNEXPECTED_VERB_SUFFIX}"`, and `__is_intentional_gap_reason`
  matches `*"${DEVLOOP_UNEXPECTED_VERB_SUFFIX}"`. Plus a `_wrapper_trap.test.sh`
  shared-contract test pins producer + matcher + exit-classifier to the constant
  end-to-end, so a desync fails loud.

**`status_to_exit_code(status, reason)`** is the SINGLE locus encoding the reason rule:
`SKIPPED-NO-VERB` + a reason carrying the `UNEXPECTED` marker → **2** (wiring bug, code-
reviewer Pin 1 — explicit arm, since SKIPPED-NO-VERB is a real enum not UNKNOWN); all
other `SKIPPED-NO-VERB` reasons (intentional-gap `-sh-missing-`, `all-langs-filtered`)
→ 0; `UNKNOWN`/unrecognized → 2; `FAIL` → 1; `OK`/`SKIPPED-NO-DIFF`/`N/A` → 0. The marker
test is the `__is_intentional_gap_reason` complement — one predicate, consumed here and
in the dispatcher (dry-reviewer item 3).

### Known residual (documented, not fixed here)
With the ladder unchanged, an unexpected verb-missing in **one** lang while a
**sibling lang runs OK** is still dominated by OK (rank 2 > rank 0) and the layer
aggregates to OK. The new exit-non-zero fires when verb-missing is the aggregate
*winner* (single participating lang, or all-langs-missing) — which is the
"currently exit 0" case task #50 names. The cross-lang-masking tail requires a
precedence re-rank (raise unexpected `SKIPPED-NO-VERB` above OK), which has a
large blast radius (ADR-0033 ladder + multiple locked tests) and is **out of the
minimal-fix scope** → tracked in `docs/TODO.md` §Polyglot Pipeline Follow-ups.
The EXIT trap (change 1) independently covers the *crash-before-emit* tail, which
is the more common real failure (e.g. `rust/test.sh` DB bring-up `exit 1`).

### Change inventory
1. **EXIT trap** — `_common.sh`: `emit_status` sets `_status_emitted=1` at the
   single emit locus, so ANY STATUS emission (OK, FAIL, SKIPPED-*, N/A) marks
   emitted=1 — a normal `cargo audit` FAIL (`emit_status FAIL` then `return 1`
   under `set -e`) therefore does NOT re-trigger the trap, and the trap can never
   clobber a real rc (security item 3a). `install_wrapper_exit_trap` +
   `__wrapper_early_exit_trap`: the handler captures `rc=$?` as its FIRST line,
   and ONLY when `_status_emitted` is unset emits
   `STATUS=FAIL REASON=wrapper-aborted-early-exit-<rc>` and ends with
   `exit "${rc:-1}"` — never `exit 0`; rc defaults non-zero so a crash can never
   be converted to silent success (security item 3b). Called from all 14 verb
   wrappers (NOT `changed.sh` predicates — they legitimately `exit 1` for no-diff,
   a trap there would synthesize a false FAIL; NOT layer scripts which own their
   lifecycle trap).
2. **Dispatcher split** — `_dispatch.sh`: intentional-gap allowlist via
   `__intentional_missing_verbs()` — production reads a HARDCODED constant
   (`proto:test proto:audit`); the `DEVLOOP_INTENTIONAL_MISSING_VERBS` override is
   honored ONLY under `DEVLOOP_TEST == "1"`, mirroring `_audit_gate.sh:116`
   `__audit_pnpm_ignore_path` (security BLOCKING item 1 — an unconditional env
   surface would let any caller downgrade a genuinely-missing security gate from
   FAIL-closed back to exit 0, re-opening this class). Reusing `DEVLOOP_TEST`
   keeps the seam under the existing `assert_no_ci_sentinel_leak` trust boundary
   (_common.sh:124) — no parallel sentinel, no new uncovered CI hole (security
   item 2). Distinct reasons; reason-tracked return code.
3. **Exit tightening** — `status_to_exit_code(status, reason)` remains the SOLE
   status→code locus (dry-reviewer F1); `_dispatch.sh:167,171` and
   `__layer_lifecycle_end:267` keep CALLING it, no second `case` anywhere
   (dry-reviewer item 2). Explicit `UNKNOWN→2`. New `_common.sh` SPOT helpers
   (dry-reviewer item 3): `parse_status_reason` (REASON extraction, sibling to
   `parse_status_line`) and `__is_intentional_gap_reason` (the "is this an
   intentional-gap token" predicate) — consumed by `_dispatch.sh`,
   `__layer_lifecycle_end`, AND the tests; NO inline `sed`/`grep`/`=~` reason
   parsing duplicated in callers. `tee_collect_statuses` keeps populating
   `__LAYER_STATUSES` with BARE ENUMS (observability P1 — does NOT overload it;
   `_common.test.sh:112-114` + the verbatim-stream assertion :103-105 stay intact)
   and collects a PARALLEL `__LAYER_REASONS` array in the SAME loop.
   `__layer_lifecycle_end` finds the worst child enum, then pulls that child's
   reason from the parallel array to feed the reason-aware exit. The stderr
   `LAYER=…RESULT=…REASON=` line (the §CRITICAL 3am anchor, _common.sh:207-209)
   STILL emits BEFORE exit on the new non-zero verb-missing branch, and its REASON
   field carries the worst-child verb-missing cause (observability P2) — not a
   generic `layer<n>-summary` — so the loud exit code points at the real cause.
   `layer-all.sh` already propagates non-zero on any UNKNOWN child (verified +
   tested) — no change. The intentional-gap allowlist itself is the
   single `__intentional_missing_verbs()` source (item 4): production constant in
   its `else` branch, test seam via the `DEVLOOP_TEST`-gated env override — the
   `proto:test proto:audit` tuple is NEVER re-hardcoded in a test file; tests that
   need it call the function (or drive the env seam).
4. **Audit pass / TODO entries** — `docs/TODO.md` §Polyglot Pipeline Follow-ups:
   (i) the cross-lang-masking residual (unexpected verb-missing masked by a sibling OK
   — needs a precedence re-rank, out of minimal-fix scope); (ii) the SIGTERM/SIGINT
   pre-emit signal-trap coverage deferral (test-reviewer item 3 — EXIT-trap path
   already covered by explicit/`set -e` cases; add if a real signal-abort silent-skip
   is observed). Possibly (iii) mark TODO:268 `_ignored_rc` [x] if change 3 resolves
   it naturally (see dry-reviewer note above).
5. **Docs** — SKILL.md §403 + §405 (operations P2): §403 currently says a
   SKIPPED-NO-VERB is self-justifying and the implementer does NOT owe Gate 2 an
   explanation — TRUE only for intentional gaps now; edit it to stop framing an
   *unexpected* verb-missing as self-justifying. §405 currently names only an
   unexpected `STATUS=N/A` as the escalate-to-operations case; add unexpected
   `SKIPPED-NO-VERB` (verb-missing, layer now reds) alongside it. Runbook (code-reviewer
   item 1 doc-scope, expanded): §3 exit-code table (line ~56: `SKIPPED-NO-VERB` no
   longer unconditionally exit 0 — unexpected verb-missing exits non-zero) + §3
   enum-REASON-examples table (line ~75: ADD the new `-UNEXPECTED-verb-missing-or-not-executable`
   token alongside the existing intentional-gap examples, which stay valid) + §6
   (Layer 4/6 triage, line ~348: wrapper-abort-pre-emit now → FAIL via trap, not
   UNKNOWN) + §7 (line ~425: "Two valid reasons" → THREE states — the unexpected
   verb-missing third state reds the layer; without this §7 actively misleads an
   operator) + §8 symptom-index table (line ~487: ADD TWO rows — one for
   `wrapper-aborted-early-exit` → "wrapper crashed before emitting STATUS; previously
   surfaced as UNKNOWN" → §6.4/§7, one for the `-UNEXPECTED-verb-missing-or-not-executable`
   token → "a verb wrapper that should exist is missing/chmod-stripped — now reds the
   layer (was silently exit 0)" → §7; the two existing proto-intentional rows stay
   valid, per operations C — a new CI-redding token absent from the §8 grep-index is
   itself a legibility regression). The §6.4/§6.6
   intentional-gap worked examples (proto test/audit-sh-missing co-running with rust
   OK) stay accurate post-split — verified (OK still dominates rank-0 SKIPPED-NO-VERB,
   intentional reason → exit 0). All loci move together or the runbook contradicts
   shipped behavior (Lead pre-Gate-1 guidance + code-reviewer/operations doc-scope).
6. **ADR-0033 §6 amendment** (Lead ruling 2026-06-08, minimal — factual record of an
   already-approved change, NOT a new decision → no /debate):
   - APPEND a dated amendment note to §6: "**Amendment (2026-06-08, task #50):**
     SKIPPED-NO-VERB exit code is now REASON-dependent —
     `*-UNEXPECTED-verb-missing-or-not-executable` (unexpected missing/non-exec wrapper) is
     a WIRING fault and maps to the existing **exit 2** 'investigate the script itself'
     class (line 231); `*-sh-missing-or-not-executable` intentional gaps (proto test/audit)
     and `all-langs-filtered` remain in the exit-0 success class. UNKNOWN exits 2 explicitly.
     See docs/devloop-outputs/2026-06-08-silent-skip-class-fix-task50/."
   - **Minimal table edit (code-reviewer framing):** line 231's existing row already reads
     "2 | Wrapper / dispatcher bug (unexpected error; investigate the script itself)" — a
     word-perfect description of an UNEXPECTED missing/non-exec verb script. So the edit is
     minimal: line 229's row-0 keeps SKIPPED-NO-VERB ONLY for the intentional-gap +
     all-langs-filtered reasons (drop the unconditional claim), and the UNEXPECTED reason
     maps into the EXISTING row-2 semantics — no new table row needed, just a reason-qualifier
     note + pointer to the amendment. (If exit 1 had been chosen it would have needed a
     justification for "work ran and detected a problem" on a missing-script fault — exit 2
     avoids that, fits the existing taxonomy.)
   - Do NOT rewrite the original §6 decision text (lines 247/249).
   - **Infrastructure-Mine** (ADR §11 — infra owns the wrapper-contract/exit-code
     plumbing; Lead ruling 2026-06-08 reversing the earlier operations-owned call).
     NOT in operations' Minor-judgment sign-off set; gets code-reviewer's standing
     ADR-compliance confirmation at Gate 3.

**Token-consistency invariant (operations):** the bug token must appear as the EXACT
emitted literal `<lang>-<verb>-UNEXPECTED-verb-missing-or-not-executable` (UPPERCASE
`UNEXPECTED`) in ALL FOUR doc loci — runbook §3 table, §7 re-split, §8 catalogue row,
AND the ADR §6 amendment note — and in the code/tests. A doc that greps for the old
lowercase form would silently miss the real emitted token (the same silent-divergence
failure mode, relocated to the docs). The uppercase infix is an intentional loud-signal
convention deviation (cf. `WARN BUDGET_BREACH`, `PRECONDITION_FAILURE:`,
`CI-SENTINEL-LEAK:`), not drift — operations will note it as a deliberate choice in
the Gate 3 verdict.

### Test plan (test-reviewer confirmed)

Each acceptance row drives a REAL code path; negative/no-regression complements
included. The locked tests (`_common.test.sh:37-43` α-ladder, the `_dispatch.test.sh`
stream-verbatim/precedence tests, `_common.test.sh:112-114` bare-enum collection +
:103-105 verbatim) stay byte-unchanged (no precedence re-rank).

**A. Unit — wrapper trap (`_wrapper_trap.test.sh`, new, hermetic tempdir):**
- `trap_fires_on_explicit_early_exit`: source `_common.sh`, install trap, `exit 1`
  before emit → last STATUS = `STATUS=FAIL REASON=wrapper-aborted-early-exit-1` + exit 1.
- `trap_fires_on_set_e_abort` (test-reviewer Q2): a FAILING command (not explicit
  exit) trips `set -e` before `run_and_emit` → same FAIL-via-trap assertion. This is
  the rust/test.sh DB-bringup shape.
- `trap_silent_on_normal_ok`: install trap, `run_and_emit x true` → EXACTLY ONE
  `^STATUS=` line (`grep -c`==1), it's OK, exit 0 (proves `_status_emitted` guard).
- `trap_silent_on_explicit_fail`: `emit_status FAIL …; exit 1` → EXACTLY ONE
  `^STATUS=` line (`grep -c`==1, test-reviewer count assertion), exit 1 (no double-emit).

**B. Unit — dispatcher reason split (`_dispatch.test.sh`):**
- `unexpected_verb_missing_single_lang` (the HEADLINE criterion-(b) test, code-reviewer
  Pin 2): ONE touched lang (e.g. `fakeland`), no verb, not allowlisted → single-lang
  path returns `status_to_exit_code SKIPPED-NO-VERB <UNEXPECTED>` = **2**;
  `REASON=fakeland-<verb>-UNEXPECTED-verb-missing-or-not-executable` + exit 2.
- `unexpected_verb_missing_all_langs`: two touched langs both missing verb, neither
  allowlisted → aggregate winner is the UNEXPECTED reason → exit 2.
- `intentional_gap_verb_missing_zero`: allowlisted (via `DEVLOOP_INTENTIONAL_MISSING_VERBS`
  seam under `DEVLOOP_TEST=1`, or a `proto`-named fixture) → `-sh-missing-or-not-executable`
  + exit 0; assert the token does NOT carry the `UNEXPECTED` anchor (matcher-shape guard).
- **Pin 2 fixture note:** the EXISTING `test_stream_verbatim_contract` fixture
  (`_dispatch.test.sh:140-203`) — `touched_no_verb` (UNEXPECTED, enum SKIPPED-NO-VERB)
  co-running with `untouched` (SKIPPED-NO-DIFF, rank 1) — aggregates to NO-DIFF, so the
  UNEXPECTED child's enum ≠ agg and is EXCLUDED from the worst-reason pick → dispatcher
  exits **0**. Traced: the test only asserts STATUS lines (not exit code), so it STAYS
  GREEN. It is itself an instance of the documented cross-lang-masking residual; I'll add
  an INLINE COMMENT at that fixture saying so, so a future reader doesn't "fix" it
  expecting a red. (Fixture lang names stay clear of allowlist keys — no fixture named
  `proto` unless deliberately hitting the allowlist, per code-reviewer naming note.)

**C. Unit — `status_to_exit_code(status, reason)` (`_common.test.sh`):**
- SKIPPED-NO-VERB + `*-UNEXPECTED-verb-missing-*` → **2** (explicit arm); +
  intentional/`all-langs-filtered` → 0; UNKNOWN → 2; FAIL → 1; OK/NO-DIFF/N/A → 0.
- `__is_intentional_gap_reason` direct unit case (test-reviewer): feed it both literal
  tokens — intentional `-sh-missing-or-not-executable` → true, unexpected
  `-UNEXPECTED-verb-missing-or-not-executable` → false — proving the predicate anchors
  on the unique `UNEXPECTED` marker, not the shared suffix.
- Plus parallel `__LAYER_REASONS` collection assertion alongside the existing :112-114
  bare-enum one.

**D. End-to-end — LAYER EDGE (test-reviewer Q1, REQUIRED — the silent-skip escaped
HERE, not at the dispatcher boundary). A synthetic layer script (`synth_layer.sh` in
the tempdir) sources `_common.sh`, calls `layer_lifecycle_begin`, pipes a synthetic
dispatcher through `tee_collect_statuses`, and lets the EXIT trap fire. The dispatcher
is the LEFT of the pipe so its rc is discarded; `__layer_lifecycle_end` owns the final
exit.**

**CRITICAL test-construction guardrail (test-reviewer item 1):** the synthetic layer
MUST be invoked as a REAL SUBPROCESS (`bash "$tmp/synth_layer.sh"; rc=$?` or
`out=$(bash …)`), NEVER sourced into the harness — `__layer_lifecycle_end` runs in an
EXIT trap ending in `exit "$rc"`, so sourcing it would kill the test harness itself
(self-terminating test, or a FALSE GREEN if it exits 0 before later assertions run).
Same subprocess-isolation shape `_dispatch.test.sh` already uses. Assert:
- (b-e2e) unexpected verb-missing as winning child → LAYER exits **== 2** (Lead ruling:
  wiring-fault class, asserted as the exact code, not just non-zero).
- (b-neg-e2e) intentional-gap reason as winning child → LAYER exits **== 0** + RESULT
  aggregate unchanged (proto Layers 4/6 no-regression — operations §6.4/§6.6 worked examples).
- (b-tiebreak-e2e) intentional-gap SKIPPED-NO-VERB co-running with an UNEXPECTED
  verb-missing (both rank-0 enum, different reasons) → LAYER exits **== 2** (worst-exit-
  driving reason wins; observability/operations tie-break).
- (e-e2e) a child emitting NO STATUS → `__LAYER_STATUSES` gets UNKNOWN → assert BOTH
  halves (test-reviewer item 2): exit code **== 2** AND stderr `RESULT=UNKNOWN` — the code
  proves the gate fails, the token proves it failed for the RIGHT cause (UNKNOWN, not an
  unrelated FAIL). NOTE: (e-e2e) and (b-e2e) both assert == 2 but are disambiguated by
  RESULT/REASON token (UNKNOWN no-status-emitted vs SKIPPED-NO-VERB UNEXPECTED) — assert
  the token too, never the bare code, so the two can't false-pass for each other.
- (a-e2e) a crashing wrapper (trap-emitted FAIL) routed through the layer → LAYER
  exits **== 1** (the wrapper emits a real `STATUS=FAIL`, so the layer aggregates FAIL →
  exit 1, NOT 2 — distinct from the no-status UNKNOWN case; this is the rust/test.sh
  DB-bringup real-failure scenario). (Block lives in `_wrapper_trap.test.sh` — the
  trap+layer integration home.)

**Deferred:** SIGTERM/signal-trap test (test-reviewer agreed — EXIT trap fires on
signal-induced exit anyway, so the explicit/`set -e` (a) cases already exercise the
trap mechanism; a dedicated `kill -TERM`+wait test adds a CI race for marginal
coverage). TRACKED as a one-line `docs/TODO.md` §Polyglot Pipeline Follow-ups entry
(documented scoping decision, not a silent gap) per test-reviewer item 3.

### Semantic-guard diff-time checks (Lead-folded, enforce-not-just-satisfy)

Three invariants the implementation must hold, each verifiable at diff time:
1. **`rc=$?` is the LITERAL FIRST statement of `__wrapper_early_exit_trap`** — before
   any other command (a `local x=…`, a `printf`, anything) overwrites `$?`. Captures the
   true crash code; a non-first capture would record 0 from the preceding command.
2. **Non-empty `worst_reason` fallback** — both the dispatcher aggregate arm and
   `__layer_lifecycle_end` must default `worst_reason` to a non-empty value (e.g. the
   aggregate enum's generic reason) if no same-enum child reason is found, so
   `status_to_exit_code "$status" "$worst_reason"` never receives an empty 2nd arg and
   mis-defaults. (Guards the multi-child edge where the array lookup could miss.)
3. **`child_reasons` pushed 1:1 with `child_statuses`** (and `__LAYER_REASONS` 1:1 with
   `__LAYER_STATUSES`) — every push site appends to BOTH arrays in lockstep, so index
   alignment holds; a status pushed without its reason would desync the lookup. Verified
   by inspection at each of the 3 dispatcher push sites + the tee loop.

---

## Implementation Summary

All 6 changes landed as planned; all reviewer commitments + Lead rulings honored.

- **`_common.sh`**: `_status_emitted` set in `emit_status` (single emit locus, covers
  FAIL); `install_wrapper_exit_trap` + `__wrapper_early_exit_trap` (`rc=$?` literal
  first statement, emits `STATUS=FAIL REASON=wrapper-aborted-early-exit-<rc>` only when
  no status emitted, ends `exit "${rc:-1}"`); `parse_status_reason` +
  `__is_intentional_gap_reason` (anchors on the unique `UNEXPECTED` marker, not the
  shared suffix); reason-aware `status_to_exit_code(status, reason)` with an EXPLICIT
  `SKIPPED-NO-VERB + UNEXPECTED → 2` arm; `worst_reason_for_status` (worst-exit-driving
  reason among same-enum children, non-empty fallback); parallel `__LAYER_REASONS`
  collected 1:1 in `tee_collect_statuses`; `__layer_lifecycle_end` drives exit + the
  stderr `LAYER=…REASON=` off the worst child's reason.
- **`_dispatch.sh`**: `__intentional_missing_verbs` (hardcoded `proto:test proto:audit`;
  `DEVLOOP_INTENTIONAL_MISSING_VERBS` override gated behind `DEVLOOP_TEST=1`) +
  `__is_intentional_gap` (allowlist-membership, shape-proof; explicit space-IFS split);
  reason split at the verb-missing site; `child_reasons[]` pushed 1:1 with
  `child_statuses[]` at all 3 sites; aggregate arm emits the worst child's reason and
  returns the reason-aware exit code (closes the `:162` flatten hole); empty-pipe →
  UNKNOWN with a naming reason.
- **14 verb wrappers**: one-line `install_wrapper_exit_trap` after sourcing _common.sh
  (rust/ts {compile,fmt,lint,test,audit} + proto {compile,fmt,lint,breaking}); NOT in
  changed.sh predicates or layer scripts.
- **`scripts/audit.sh` audit-gate fail-open close** (security finding, Lead ruling —
  iteration 2): the cross-lang-masking residual fail-OPENed Layer 6 (a deleted/non-exec
  `rust`/`ts` `audit.sh` masked by the sibling's OK → dep-vuln scan silently skipped). The
  always-run audit dispatch output is captured (tempfile + `tee`, streamed once); if any
  `<lang>-audit${DEVLOOP_UNEXPECTED_VERB_SUFFIX}` token appears, audit.sh EMITS
  `STATUS=FAIL REASON=audit-gate-wrapper-missing-<lang>` (so the layer's
  `tee_collect_statuses` aggregates FAIL → layer exits 1 — the load-bearing part, since
  the layer keys on the STATUS stream not audit.sh's rc) AND folds `dispatch_rc=1` for the
  standalone path so BOTH paths agree at exit 1 (Lead req #2; chose FAIL/1 over a synthetic
  UNKNOWN/2 — UNKNOWN is documented as "no status emitted", which a precise REASON would
  contradict). No ladder change; proto's intentional `proto:audit` gap (`-sh-missing-`) is
  unaffected. The general non-audit cross-lang-masking tail stays deferred (TODO).
- **Tests**: new `_wrapper_trap.test.sh` (10 tests — trap unit A + layer-edge E2E
  block D, subprocess-isolated, `==2`/`==1`/`==0` + RESULT-token assertions); extended
  `_dispatch.test.sh` (+4 reason-split tests incl. the `DEVLOOP_TEST` security-gate
  case + the fixture residual comment); extended `_common.test.sh` (+reason-aware
  exit-code, `__is_intentional_gap_reason`, `worst_reason_for_status`, parallel
  `__LAYER_REASONS`).
- **Docs**: runbook §3 (exit-code table + line-75 enum examples, side-by-side
  intentional vs UNEXPECTED), §6.4 (trap → FAIL not UNKNOWN), §7 (three states), §8
  (two symptom rows); SKILL.md §403 (narrowed self-justifying set) + §405 (escalate
  case); ADR-0033 §6 (line-229 table qualifier mapping UNEXPECTED into the existing
  exit-2 row + dated amendment note, original decision text untouched); docs/TODO.md
  (cross-lang-masking residual, SIGTERM deferral, `_ignored_rc` note).

**Notable implementation catch**: `__is_intentional_gap` initially failed for
`proto:test` because `_common.sh` sets `IFS=$'\n\t'` (no space), so the space-separated
allowlist wasn't word-split — fixed with an explicit space-IFS `read -ra`. Without this,
proto Layers 4/6 would have emitted the wrong (UNEXPECTED) reason token (aggregate exit
stayed 0 via OK-dominance, but the reason would have been misleading). Caught + fixed in
smoke testing before tests.

---

## Files Modified

Code (infra-Mine): `scripts/lang/_common.sh` (+184/-…), `scripts/lang/_dispatch.sh`
(+104/-…), the 14 verb wrappers (1 line each; rust/test.sh +5 for the comment).
New test: `scripts/lang/_wrapper_trap.test.sh`. Extended tests:
`scripts/lang/_common.test.sh` (+72), `scripts/lang/_dispatch.test.sh` (+130).
Docs: `docs/runbooks/devloop-validation.md` (§3/§6.4/§7/§8, operations Minor-judgment),
`.claude/skills/devloop/SKILL.md` (§403/§405, operations Minor-judgment),
`docs/decisions/adr-0033-polyglot-validation-pipeline.md` (§6 amendment, infra-Mine,
code-reviewer ADR-compliance at Gate 3), `docs/TODO.md` (3 follow-up entries).
21 tracked files changed (+534/-42) + 1 new test file + this devloop output dir.

---

## Devloop Verification Steps

Run by the implementer at hand-off (Gate 2 env-limited steps — cargo/buf/cluster —
are the Lead's per the approval; `scripts/layer-all.sh` deliberately NOT run here):

- `bash -n` on all 20 touched shell files (incl. `scripts/audit.sh`) → clean (shellcheck
  NOT installed in this env, per docs/TODO.md:265; `bash -n` is the available gate —
  flagged for Gate 2).
- Four shell test suites (post-Gate-3-findings counts): `_common.test.sh` 33/0,
  `_dispatch.test.sh` 42/0, `_wrapper_trap.test.sh` 24/0, `_layer_skeleton.test.sh` 36/0
  — all green (135 assertions).
- Token-consistency grep across the changeset: zero stale lowercase
  `verb-missing-or-not-executable` (the bug token appears only as
  `-UNEXPECTED-verb-missing-or-not-executable`).
- Acceptance matrix (a)-(e) re-verified empirically (below).

---

## Acceptance Criteria Verification Matrix

| # | Criterion | Result |
|---|-----------|--------|
| (a) | wrapper crashing pre-`run_and_emit` emits `STATUS=FAIL` via the trap | **PASS** — `STATUS=FAIL REASON=wrapper-aborted-early-exit-1`, rc≠0 (`_wrapper_trap.test.sh` explicit-exit + set-e cases; empirical re-check) |
| (b) | `SKIPPED-NO-VERB` REASON=`<lang>-<verb>-UNEXPECTED-verb-missing-or-not-executable` → exits 2 (single-lang/winning-child); layer aggregate non-zero | **PASS** — dispatcher single-lang ==2, layer-edge E2E ==2 (`_dispatch.test.sh`, `_wrapper_trap.test.sh`) |
| (c) | `SKIPPED-NO-VERB` REASON=`all-langs-filtered` still exits zero | **PASS** — `status_to_exit_code SKIPPED-NO-VERB all-langs-filtered` = 0 |
| (d) | OK + SKIPPED-NO-DIFF + N/A + intentional-gap all still exit zero (no regressions) | **PASS** — all four → 0; locked precedence tests + proto-intentional-gap E2E green |
| (e) | UNKNOWN exits non-zero | **PASS** — `status_to_exit_code UNKNOWN` = 2; layer-edge no-status-child E2E ==2 + `RESULT=UNKNOWN` |
| (f) | audit-gate fail-open closed: a masked UNEXPECTED audit-wrapper-missing reds the LAYER (security finding, Lead ruling) | **PASS** — `scripts/audit.sh` emits `STATUS=FAIL REASON=audit-gate-wrapper-missing-<lang>` so `tee_collect_statuses` aggregates FAIL → **layer exits 1** (NOT just audit.sh's own rc, which the layer pipe discards); the **standalone** path folds `dispatch_rc=1` so BOTH paths AGREE at 1 (Lead req #2 — chose FAIL/1 over synthetic UNKNOWN/2 for honest semantics); proto intentional `proto:audit` gap stays exit 0. Locked by `_dispatch.test.sh::test_audit_security_guard_reds_layer` (drives the live scan-block through a real layer lifecycle; asserts layer==1, standalone==layer, proto-gap==0) |

---

## Code Review Results

Gate 3 CLOSED 2026-06-08 — all 7 reviewers confirmed against the final (1/1) tree;
zero ESCALATED. Verdicts re-confirmed against the post-finding-fix tree (the tree
moved during review with test/security/operations findings; reviewers who cleared
on an earlier tree re-confirmed the delta — Lead did not close on stale-tree verdicts).

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | Caught the audit-gate cross-lang fail-open (Layer 6 dep-vuln scan silently skipped); ESCALATED → fixed (Lead Option-A ruling) → re-verified at the real layer6 path. Gate-1 threat-model items (env-seam DEVLOOP_TEST-gating, fail-closed trap) all hold. |
| Test | RESOLVED-DEFERRED | 2 | 2 | 1 | Multi-entry allowlist word-split coverage + criterion-(c) rc==0 — both fixed, mutation-confirmed (3 load-bearing tests incl. prod-constant). Forced the acceptance test to the layer edge. Accepted deferral: SIGTERM/signal pre-emit trap coverage. |
| Observability | CLEAR | 0 | 0 | 0 | Loud attributable signal end-to-end; parallel `__LAYER_REASONS`; locked tests byte-unchanged. Re-confirmed audit-gate delta. |
| Code Quality | CLEAR | 0 | 0 | 0 | ADR-0033 incl. §6 amendment COMPLIANT; precedence ladder bodies byte-identical (no re-rank); Ownership Lens clean (no GSA). Flagged a PRE-EXISTING unrelated red (task #45, tracked separately). |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | All new logic SPOT-centralized in `_common.sh` (shared trap helper ×14, single exit-map locus, `DEVLOOP_UNEXPECTED_VERB_SUFFIX` constant). Finding: stale test-comment line-ref — fixed. |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | stdout/stderr REASON-split docs + `audit-gate-wrapper-missing` token docs (§6.6/§8/§9). Full hunk-ACK on all owned doc surfaces. |
| Semantic Guard | SAFE/CLEAR | 0 | 0 | 0 | Error-context preserved end-to-end; closed the `:162` multi-lang reason-flatten hole; no credential leak; all 4 diff-time anchors verified in code. |

**Lead Gate-3 rulings:** (1) security audit-gate fail-open → fix-now (Option A), not defer — the task's "unless evidence shows the minimal fix is insufficient" clause was triggered; (2) caught the first audit-gate fix as incomplete (closed only standalone audit.sh; layer6 path still exited 0) via independent layer-path simulation, routed back; (3) audit-gate exit code = FAIL/exit-1 consistently on both standalone + layer paths (reversed an earlier divergence-relax of my own that was based on stale state).

---

## Accepted Deferrals

- `docs/TODO.md` §Polyglot Pipeline Follow-ups — SIGTERM/signal pre-emit trap coverage (test-accepted; EXIT-trap path already covered by explicit-exit + set-e cases)
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — cross-lang-masking residual on NON-audit layers (1/2/4/5); the security-critical audit slice is CLOSED here, the general correctness tail needs the ladder re-rank (large blast radius)
- `docs/TODO.md` §Polyglot Pipeline Follow-ups — `_get_base_ref.behavior-equivalence.test.sh [7b]` pre-existing red (task #45 dt-guard migration drift; orthogonal to task #50, do-not-fold; surfaced + verified during this devloop's Gate 3)

---

## Rollback Procedure

1. Start commit: `44e8b194308a36c03b558700a8474a92c47d086b`
2. `git diff 44e8b19..HEAD`
3. Soft reset: `git reset --soft 44e8b19`
