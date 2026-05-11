# Devloop Output: Proto Wrappers + Layer 1 Stage-1 Ordering

**Date**: 2026-05-11
**Task**: Track 3 Wave 2 #4 — land `scripts/lang/proto/` wrappers, encode proto-first stage-1 ordering in `scripts/layer1.sh`, and route `buf breaking` always-run via `scripts/audit.sh`.
**Specialist**: protocol (paired with infrastructure)
**Mode**: Agent Teams — full
**Branch**: `feature/browser-client-join-task35`
**User-story task**: #35 in `docs/user-stories/2026-05-02-browser-client-join.md`
**ADR**: ADR-0033 §1, §5, §6, §10 Wave 2 #4 — `docs/decisions/adr-0033-polyglot-validation-pipeline.md`
**Requirement**: R-62 (Polyglot Validation Pipeline)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5a8f5dc0cb90a113e2d5d96a08bc74e66a457bf9` |
| Branch | `feature/browser-client-join-task35` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-2026-05-11-proto-wrappers` |
| Implementing Specialist | `protocol` |
| Paired Specialist | `infrastructure` |
| Iteration | `2` (Gate 2 iter 1 returned wrapper-path bug — buf invoked without `proto` positional from /work) |
| Security | `security@devloop-2026-05-11-proto-wrappers` |
| Test | `test@devloop-2026-05-11-proto-wrappers` |
| Observability | `observability@devloop-2026-05-11-proto-wrappers` |
| Code Quality | `code-reviewer@devloop-2026-05-11-proto-wrappers` |
| DRY | `dry-reviewer@devloop-2026-05-11-proto-wrappers` |
| Operations | `operations@devloop-2026-05-11-proto-wrappers` |
| Paired-Infrastructure | `paired-infrastructure@devloop-2026-05-11-proto-wrappers` |

---

## Task Overview

### Objective

Land the proto language wrappers and the proto-first stage-1 ordering in `scripts/layer1.sh`, plus wire `buf breaking` as an always-run audit step. This unblocks Track 2 #29 (proto conventions + temporary `lint.ignore` scaffolding), which in turn depends on `buf lint` running through the pipeline.

### Scope

- **Service(s)**: none (build/validation tooling only)
- **Schema**: No
- **Cross-cutting**: Yes — modifies `scripts/lang/` (infrastructure-owned domain) but the verb semantics (`buf {build, format, lint, breaking}`) are protocol-owned tools. Paired with infrastructure per task spec.

### Debate Decision

NOT NEEDED — task is a direct implementation of ADR-0033 §10 Wave 2 #4, no new design surface.

---

## Cross-Boundary Classification

<!-- Filled at planning. All scripts/lang/ paths are infrastructure-owned domain
     (per the Wave 1 #32 devloop that created the scaffolding), but the verb
     bodies invoke protocol-owned `buf` commands. Per ADR-0024 §6.3, these are
     "Mine" for the paired-implementation model (protocol+infrastructure both
     present); per §6.4, none of these paths fall into Guarded Shared Areas
     (no wire format, auth, crypto, schema). -->

Final v3+α file list (matches the diff). Detailed planning rationale + decision history below in `## Planning §10`.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/lang/proto/changed.sh` | Mine | — (paired infrastructure present) |
| `scripts/lang/proto/changed.test.sh` | Mine | — |
| `scripts/lang/proto/compile.sh` | Mine | — |
| `scripts/lang/proto/fmt.sh` | Mine | — |
| `scripts/lang/proto/lint.sh` | Mine | — |
| `scripts/lang/proto/breaking.sh` | Mine | — |
| `scripts/lang/_test_helpers.sh` (NEW — dry-reviewer D1) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/_common.sh` (α re-rank + comment rewrite + initializer fix) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/_common.test.sh` (assertions flipped to α ladder + new multi-lang success-path test) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/_dispatch.sh` (INCLUDE/EXCLUDE knobs + filter-ordering comment + breadcrumb) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/_dispatch.test.sh` (3 new tests + α regression-guard test) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/_test_changed_predicates.sh` (proto column added) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/rust/changed.test.sh` (1-line TODO note only, staged D1) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/ts/changed.test.sh` (1-line TODO note only, staged D1) | Mine (infra paired) | infrastructure (review) |
| `scripts/audit.sh` (F4 cascade — invokes `lang/proto/breaking.sh` directly per ADR-0033 §10:397) | Mine (infra paired) | infrastructure (review) |
| `scripts/layer1.sh` (v3 dispatcher-symmetric: INCLUDE stage 1, EXCLUDE stage 2) | Mine (infra paired) | infrastructure (review) |

---

## Planning

### Approach

**1. `lang/proto/changed.sh`** — diff predicate matching the proto root (covers
`proto/*.proto`, `proto/buf.yaml`, `proto/buf.gen.yaml`):
```bash
diff_touches_path "proto/"
```
Mirrors `lang/rust/changed.sh` shape. No root-files arm needed because
`proto/buf.yaml` and `proto/buf.gen.yaml` already live under `proto/`.

**2. `lang/proto/changed.test.sh`** — locality self-test. Hermetic via
injected `DEVLOOP_TMP` cache. Touched cases include real files (test ask #1):
`proto/foo.proto`, `proto/internal.proto`, `proto/signaling.proto`,
`proto/buf.yaml`, `proto/buf.gen.yaml`. Untouched cases: `docs/x.md`,
`crates/foo/src/lib.rs`, `packages/foo/src/index.ts`, `scripts/test.sh`.

**Sources `scripts/lang/_test_helpers.sh`** (dry-reviewer D1): rust and ts
`changed.test.sh` already share ~30 lines of byte-identical scaffolding
(PASS/FAIL counters, `assert_rc`, `run_with_cache`, footer printf+exit).
Adding proto's would make it the 3rd copy, crossing the abstraction
threshold. Going with dry-reviewer's **staged** alternative: this wave
introduces the `_test_helpers.sh` helper and proto's `changed.test.sh`
consumes it from day one. Rust + ts `changed.test.sh` are left untouched
in this wave (out of scope; broadens blast radius), with a one-line
follow-up TODO note added to each pointing at `_test_helpers.sh`. A future
devloop migrates them.

`_test_helpers.sh` exports:
- `assert_rc <label> <expected> <actual>` — increments PASS/FAIL.
- `run_with_cache <content>` — hermetic injection of synthetic changed-files
  cache; returns the lang's `changed.sh` exit code.
- `report_results <label>` — final printf + exit-code-from-FAIL.
- State: `PASS`, `FAIL`, `FAILURES` array (initialized by the helper).

Per-lang `changed.test.sh` body collapses to ~10 lines: source helper,
list assertions, call `report_results "lang/<X>/changed.test.sh"`.

**3. `lang/proto/compile.sh`** — `buf build`, with `buf-binary-missing` loud-FAIL
guard. **NO internal `changed.sh` short-circuit** (Lead v3 decision): the
dispatcher handles skip-if-untouched uniformly via `_dispatch.sh:82-89` for
both stage 1 (`INCLUDE_LANGS=proto`) and stage 2 (`EXCLUDE_LANGS=proto`).
All 6 proto wrappers are structurally uniform.

Final shape (using multi-line `if/then/fi` per code-reviewer Q1):
```bash
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
if ! command -v buf >/dev/null 2>&1; then
  emit_status FAIL "buf-binary-missing"
  exit 1
fi
run_and_emit "buf-build" buf build
```
No `"$@"` pass-through (uniformity with `breaking.sh` lockdown; also avoids
`--config /tmp/evil-buf.yaml`-style attacks against the contract-time gate).

**4. `lang/proto/fmt.sh`** — `buf format --diff --exit-code` with the same
`buf-binary-missing` guard. **Naming decision: `fmt.sh`** (not `format.sh`):
matches dispatcher verb `fmt` (`scripts/fmt.sh` → `for_each_lang_with_verb "fmt"`)
and rust's `lang/rust/fmt.sh`. ADR-0033 §1 layout listing names it `format.sh` —
that is a doc typo, and a doc-only follow-up PR will fix the ADR §1 listing.

**5. `lang/proto/lint.sh`** — `buf lint` with `buf-binary-missing` guard.

**6. `lang/proto/breaking.sh`** — canonical always-run breaking check.
Base-ref source-of-truth is `_get_base_ref.sh`; `breaking.sh` reads the
computed sha and invokes:
```bash
buf breaking --against ".git#ref=<sha>"
```
Local mode resolves `merge-base origin/main HEAD` (matches ADR-0033 §7 + security
Final-78). CI/PR mode resolves `origin/$GITHUB_BASE_REF`. CI/push mode resolves
`HEAD~1`. **No `"$@"` pass-through** — same lockdown as `lang/rust/audit.sh` and
`lang/ts/audit.sh` (a runtime `--exclude-path` or `--config` flag would silence
the always-run gate untracked).

**7. NO `lang/proto/audit.sh` — wire `breaking.sh` directly into
`scripts/audit.sh`** (code-reviewer F4 + ADR-0033 §1:96, §10:395, §10:397).

ADR-0033 §1:96 and §10:395 are both explicit: proto has **no `audit.sh`**
(no test verb either). §10:397 specifies the wiring mechanism: "Wire `buf
breaking` into `scripts/audit.sh` always-run (via `lang/proto/breaking.sh`
invoked unconditionally)." The wiring lives in the audit dispatcher script,
not via a thin per-lang wrapper.

`scripts/audit.sh` final shape (replaces the current one-liner):
```bash
#!/usr/bin/env bash
# Audit dispatcher: invokes lang/<X>/audit.sh per language ALWAYS-RUN
# (no skip-if-untouched per ADR-0033 §3 + §6), then invokes proto's
# always-run breaking check explicitly per ADR-0033 §10:397.
#
# Proto has no audit.sh by design (ADR-0033 §1:96 + §10:395); breaking.sh
# IS the proto audit gate. Wired here (not via a thin
# lang/proto/audit.sh wrapper) so the §6 invariant — "proto's missing
# audit.sh produces a visible SKIPPED-NO-VERB" — is preserved.
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "$0")/lang/_dispatch.sh"

dispatch_rc=0
DEVLOOP_DISPATCH_ALWAYS_RUN=1 for_each_lang_with_verb "audit" "$@" || dispatch_rc=$?

breaking_rc=0
"$(dirname "$0")/lang/proto/breaking.sh" || breaking_rc=$?

# Worst exit code wins. status_to_exit_code() rank: 0=OK/SKIPPED, 1=FAIL,
# 2=UNKNOWN. A `set -e` short-circuit would mask the second invocation,
# breaking the always-run guarantee — explicit RC capture preserves both
# gates.
exit "$(( dispatch_rc > breaking_rc ? dispatch_rc : breaking_rc ))"
```

This makes observability C1 (REASON-token ambiguity) and security item 3
(plain-file vs symlink) both moot — there's no thin wrapper to disambiguate
or symlink-protect.

The audit dispatcher's per-lang loop will emit `SKIPPED-NO-VERB
REASON=proto-audit-sh-missing-or-not-executable` for proto — that's the
§6-visible structural signal the ADR intends, then `breaking.sh` runs
unconditionally and emits its own STATUS line.

**8. `scripts/layer1.sh` refactor** (Lead decision: v3) — both stages route
through the dispatcher via `build.sh`. Stage 1 uses `INCLUDE_LANGS=proto`,
stage 2 uses `EXCLUDE_LANGS=proto`:
```bash
# Stage 1: contract (proto) — must precede stage 2 per ADR-0033 §5.
DEVLOOP_DISPATCH_INCLUDE_LANGS=proto "$(dirname "$0")/build.sh" 2>&1 | tee_collect_statuses

# Stage 2: code (rust + ts) — depends on proto codegen artifacts from stage 1.
DEVLOOP_DISPATCH_EXCLUDE_LANGS=proto "$(dirname "$0")/build.sh" 2>&1 | tee_collect_statuses
```

**Rationale** (verbatim from Lead): Dispatcher-as-single-enforcement-point
is the load-bearing pipeline invariant. v3 preserves it — every lang-verb
call routes through the dispatcher, including stage 1. v4's direct-call
would have been the first wrapper-invocation bypassing the dispatcher, and
future code would copy the pattern. v3 also keeps all 6 proto wrappers
structurally uniform — v4 would have made `compile.sh` special (internal
short-circuit) without a self-evident reason from the file itself.
ADR-0033 §5 sketch is illustrative and pre-dates the Wave 1 #32 dispatcher;
§6 (Wrapper Contract) and §10 (Wave 2 #4 task spec) don't normatively
require direct invocation. INCLUDE+EXCLUDE land together (symmetric pair);
reviewer churn from the v4→v3 flip is smaller than the long-term cost of
the v4 asymmetry.

**`compile.sh` has NO internal `changed.sh` short-circuit** — the dispatcher
handles skip-if-untouched uniformly via `_dispatch.sh:82-89` for both
stages. All 6 proto wrappers are structurally uniform.

**9. `scripts/lang/_dispatch.sh` additive filter** (Lead decision: v3) —
minimal generic filter in `for_each_lang_with_verb` over the enumerated
lang list. Reads BOTH `DEVLOOP_DISPATCH_INCLUDE_LANGS` and
`DEVLOOP_DISPATCH_EXCLUDE_LANGS` (single-lang names for Wave 2, per F2
filter-format spec). Semantics:
- If `INCLUDE` non-empty: keep only langs matching the value.
- Else if `EXCLUDE` non-empty: drop langs matching the value.
- Else: no filter.

**Filter runs BEFORE the lint-at-startup changed.sh-must-exist pass** — so
a filtered-out lang is invisible to all dispatcher invariants, not just to
execution. Required comment on the filter block makes that contract
survive future refactor (paired-infra Delta 2):
```bash
# Apply INCLUDE/EXCLUDE filter BEFORE the lint-at-startup pass: a filtered-out
# lang is invisible to the dispatcher (no changed.sh requirement, no execution).
# This means filter is a true "skip this lang entirely" — useful for layer
# scripts that need to invoke different lang subsets in different stages
# (e.g., layer1.sh stages proto separately from rust+ts via INCLUDE then
# EXCLUDE).
#
# DEVLOOP_DISPATCH_{INCLUDE,EXCLUDE}_LANGS are single-lang for Wave 2 — exact
# match against the lang directory name. Multi-lang/comma-split deliberately
# deferred per YAGNI; if a future layer needs multi-lang filter, format
# extension is trivial.
```
Naming keeps the `DEVLOOP_DISPATCH_` prefix — matches existing
`DEVLOOP_DISPATCH_ALWAYS_RUN` / `DEVLOOP_LANG_ROOT` convention.

**Empty-after-filter behavior**: emit `STATUS=SKIPPED-NO-VERB
REASON=all-langs-filtered`. Reasoning: N/A means "verb doesn't apply to
anything that exists" (matches the `no-languages-registered` branch where
lang_root is empty). When filter clears the set, the langs *exist* — they
were explicitly muted — closer to SKIPPED-NO-VERB. Operator error tripping
this case (e.g., `INCLUDE_LANGS=nonexistent`) produces a loud signal.

**Stderr breadcrumb** (observability nit): when filter clears the set,
also emit `>&2 echo "# dispatcher: INCLUDE=<...> EXCLUDE=<...> cleared lang set"`
so a runbook reader gets the "which filter" context from the breadcrumb
without needing to parse the STATUS line.

**10. `scripts/lang/_dispatch.test.sh` extension** — three new hermetic
tests (test load-bearing #3 case honored by Lead's v3 decision):

**Test A — INCLUDE keeps**: synthetic 2-lang tree (`kept_lang`,
`excluded_lang`), `DEVLOOP_DISPATCH_INCLUDE_LANGS=kept_lang`. Assert only
`kept_lang` runs.

**Test B — EXCLUDE drops**: same tree,
`DEVLOOP_DISPATCH_EXCLUDE_LANGS=excluded_lang`. Each `changed.sh` writes a
sentinel file to `$DEVLOOP_TMP` (observability bonus check); assert
- Only `kept_lang`'s STATUS in verbatim stream.
- `excluded_lang`'s STATUS NOT present.
- `kept_lang`'s sentinel file exists.
- `excluded_lang`'s sentinel file does **not** exist (confirms filter
  runs BEFORE the changed.sh invocation loop — no cache-write side-effects
  from filtered-out langs).
- Final return code maps to `kept_lang`'s status.

**Test C — INCLUDE-nonexistent → all-langs-filtered**:
`DEVLOOP_DISPATCH_INCLUDE_LANGS=nonexistent_lang`. Assert aggregated final
STATUS is `SKIPPED-NO-VERB REASON=all-langs-filtered` (catches the
load-bearing failure mode per test reviewer's "third test was the
load-bearing one" point).

**REASON string asserted explicitly** in all three tests (test reviewer
ask): the literal REASON token is part of the assertion, not just the
STATUS enum. Catches silent message drift.

**11. `infra/devloop/Dockerfile`** — paired-infrastructure drives a buf install
addition **inline in this devloop** (pinned `ARG BUF_VERSION=...` + sha256
checksum), kept narrow. Paired-infra Q3 reasoning carried: Track 2 #29 is the
explicit purpose of this devloop (unblock proto lint in CI); deferring the
Dockerfile bump would land #4 in a known-RED CI state. Operations is on the
mandatory reviewer list per CLAUDE.md and will vet the version pin + checksum.
**Cited as a paired deliverable; paired-infrastructure owns the diff.**

**12. Aggregation precedence re-rank (Lead decision: (α))** —
`_common.sh::__status_rank` currently ranks `SKIPPED-NO-DIFF` (rank 2) and
`SKIPPED-NO-VERB` (rank 1) above `OK` (rank 0). After this devloop lands
`lang/proto/compile.sh`, the dispatcher sees `[rust:OK,
ts:SKIPPED-NO-VERB (no compile.sh yet), proto:SKIPPED-NO-DIFF (untouched)]`
on a rust-only edit → aggregates to `SKIPPED-NO-DIFF`, violating the
`_common.sh:115-121` invariant.

**Rationale** (verbatim from Lead, paired with §8 v3 rationale):
> Once a 2nd lang registers with a verb wrapper, the existing
> `__status_rank` precedence violates its own locked invariant
> (`_common.sh:115-121`) — a rust-clean PR with no proto/ts diff aggregates
> to `SKIPPED-NO-DIFF` instead of `OK`. This devloop is the surfacing
> event; shipping with a TODO would corrupt the "loud success" signal for
> every multi-lang devloop afterward. Fix is a one-line swap in
> `__status_rank` (OK ranks above SKIPPED-*); exit-code mapping is
> unaffected (all three map to 0). Semantically aligned with the existing
> comment text. Code-reviewer originally locked the precedence, so they
> re-confirm the new shape explicitly before plan approval.

**Final ladder**: `FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB`
(UNKNOWN above FAIL as today). Numeric ranks:
- FAIL = 4
- N/A = 3
- OK = 2 (was 0)
- SKIPPED-NO-DIFF = 1 (was 2)
- SKIPPED-NO-VERB = 0 (was 1)
- UNKNOWN = 5 (unchanged)

**Lead-imposed constraints on the (α) fix**:
1. **Code-reviewer explicit re-confirm required** — they locked the
   original precedence (`_common.sh:110`); the new ladder needs their
   sign-off, not just plan-confirmed.
2. **Update the comment block at `_common.sh:115-121`** so the documented
   invariant and the implementation agree. New comment articulates: "if
   any child did real work and passed, the layer passed; otherwise the
   SKIPPED-* state is informative."
3. **Add a regression test** — minimal hermetic test in `_dispatch.test.sh`
   (preferred over new file — smaller surface) asserting
   `[OK, SKIPPED-NO-DIFF, SKIPPED-NO-VERB] → aggregate = OK`. Prevents
   silent re-introduction by a future precedence shuffle.
4. **No change to `status_to_exit_code` or N/A handling** — only OK-vs-
   SKIPPED-* relative rank flips.
5. **Scope check** — if the fix grows beyond one-line + one-test + one-
   comment during implementation, escalate to Lead before expanding.

### Answers to the four open questions (consolidated)

| # | Question | Decision |
|---|----------|----------|
| 1 | `fmt.sh` vs `format.sh` | `fmt.sh` — matches dispatcher verb and rust precedent; ADR §1 doc typo fixed in follow-up |
| 2 | audit thin-wrapper vs dispatcher special-case | Thin wrapper at `lang/proto/audit.sh` → `breaking.sh`; no dispatcher changes |
| 3 | Layer 1 stage-ordering (A/B) | **Option B (v3, Lead-decided)**: both stages route through dispatcher. `INCLUDE_LANGS=proto build.sh` for stage 1, `EXCLUDE_LANGS=proto build.sh` for stage 2. `compile.sh` has no internal short-circuit — dispatcher handles uniformly. See Lead's verbatim rationale paragraph above |
| 4 | `buf` missing → FAIL vs N/A | **FAIL** with reason `buf-binary-missing`; matches ADR-0033 §3 loud-on-missing-toolchain |

### Cross-Boundary Classification (revised)

paired-infrastructure now drives `infra/devloop/Dockerfile`. Updated table:

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/lang/proto/*.sh` (6 files — NO audit.sh, ADR §1:96) | Mine | — (paired infrastructure present) |
| `scripts/layer1.sh` | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/_dispatch.sh` | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/_dispatch.test.sh` | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/_common.sh` (α: __status_rank one-line swap + comment update) | Mine (infra paired) | infrastructure (review) — **code-reviewer re-confirm required** |
| `scripts/lang/_test_helpers.sh` (NEW, dry-reviewer D1) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/rust/changed.test.sh` (1-line TODO note only) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/ts/changed.test.sh` (1-line TODO note only) | Mine (infra paired) | infrastructure (review) |
| `scripts/lang/_test_changed_predicates.sh` (proto column extension) | Mine (infra paired) | infrastructure (review) |
| `scripts/audit.sh` | **Mine (infra paired)** — flipped from verify-no-change per code-reviewer F4 + ADR §10:397 | infrastructure (review) |
| `infra/devloop/Dockerfile` | Mine (infrastructure paired) | infrastructure (drives edit) |

### Security invariants (locked at planning, security review confirmed)

- **No `"$@"` pass-through to `buf` in any wrapper** (compile/fmt/lint/breaking).
  Mirrors `lang/rust/audit.sh` / `lang/ts/audit.sh` lockdown. A `--config`,
  `--exclude-path`, or `--against` CLI override at runtime would silence gates
  untracked. Override mechanism deferred to ADR-0033 §10 Wave 3 (annotated
  allowlist file, in-tree, not CLI flags). `breaking.sh` carries an explicit
  block comment explaining this rationale so future "ergonomics" PRs are
  caught at review.
- **No env-var bypass knob** (no `DEVLOOP_PROTO_SKIP_BREAKING`,
  `BUF_BREAKING_DISABLE`, or similar). Wave 3 override path is the only
  sanctioned bypass.
- **`breaking.sh` does NOT self-gate via its own `changed.sh`.** Always-run
  guarantee comes from the audit dispatcher's `DEVLOOP_DISPATCH_ALWAYS_RUN=1`
  envelope. Header comment locks this: if a future cleanup pass adds a
  "skip-if-untouched" diff check inside `breaking.sh`, the always-run
  guarantee silently breaks. Wrapper is unconditional; envelope decides.
- **No `lang/proto/audit.sh` exists** (per ADR-0033 §1:96 + §10:395 — F4
  cascade). Dispatcher emits the §6 SKIPPED-NO-VERB signal for proto
  naturally. No symlink-vs-plain-file ambiguity to worry about.
- **`buf breaking` base-ref is the resolved sha**, not `.git#branch=main`.
  Branch form is a moving target — force-push to `origin/main` could shift
  the comparison baseline retroactively. Resolved sha from
  `git merge-base origin/main HEAD` is validated via
  `_get_base_ref.sh::__validate_ref_name`, single source of truth.
- **`buf` stderr is NOT suppressed on any path.** `run_and_emit` streams both
  stdout and stderr verbatim before emitting STATUS. Suppressing buf's stderr
  on the OK path would force a re-run to debug rule violations.

**Security S1-S4 invariants on the new `scripts/audit.sh` surface (F4 cascade,
re-confirmed at Gate 2 iter 2):**

- **S1**: `scripts/audit.sh`'s post-loop `breaking.sh` invocation does NOT
  forward `"$@"`. Args end at the dispatcher boundary; `breaking.sh` is
  invoked with zero args. Same lockdown rationale as the no-pass-through
  invariant on the wrappers themselves.
- **S2**: Exit-code aggregation is **fail-closed**. `dispatch_rc` and
  `breaking_rc` are captured separately, then `exit "$(( dispatch_rc >
  breaking_rc ? dispatch_rc : breaking_rc ))"` ensures FAIL from either
  gate propagates to the script exit. A `set -e` short-circuit would mask
  the second invocation; explicit RC capture preserves both gates.
  Regression test: `_dispatch.test.sh::test_audit_fail_closed_aggregation`
  asserts dispatcher-FAIL+breaking-OK = 1, dispatcher-OK+breaking-FAIL = 1,
  both-OK = 0, UNKNOWN-beats-FAIL = 2.
- **S3**: `breaking.sh`'s stdout flows naturally to `scripts/audit.sh`'s
  stdout, which `layer6.sh:8` pipes into `tee_collect_statuses` for
  `__LAYER_STATUSES` collection. No `>/dev/null` or capture-to-tmp
  redirection on `breaking.sh`.
- **S4**: `breaking.sh` is invoked AFTER the dispatcher loop, not before.
  Rust/ts audit results land first in the log (higher-signal vulnerability
  advisories before wire-break gate), preserving runbook-reader ergonomics.

### `breaking.sh` base-ref handling (paired-infra Delta 3 + observability C3)

`breaking.sh` invokes `_get_base_ref.sh` to get the resolved base sha. Stdout
captures the sha; stderr emission of `BASE_REF=...` is conditional on layer
context (observability C3) to avoid double-emit:

```bash
if [[ -n "${DEVLOOP_LAYER:-}" ]]; then
  # Inside a layer — layer-script already emitted BASE_REF= once at layer
  # start. Suppress this invocation's stderr to keep "one layer = one
  # BASE_REF= anchor" invariant.
  BASE_SHA=$("$(dirname "${BASH_SOURCE[0]}")/../_get_base_ref.sh" 2>/dev/null)
else
  # Standalone invocation (e.g., dev running breaking.sh directly) —
  # let stderr flow so the BASE_REF= line lands as the only anchor.
  BASE_SHA=$("$(dirname "${BASH_SOURCE[0]}")/../_get_base_ref.sh")
fi
command -v buf >/dev/null 2>&1 || { emit_status FAIL "buf-binary-missing"; exit 1; }
run_and_emit "buf-breaking" buf breaking --against ".git#ref=${BASE_SHA}"
```

The cache-missing fallback case observability flagged is handled by
`_get_base_ref.sh` itself — its local-mode logic *is* the fallback
(`merge-base origin/main HEAD`). `breaking.sh` doesn't need a separate
fallback path; invoking `_get_base_ref.sh` produces a valid sha in all
contexts (CI PR, CI push, local with main reachable, local first-commit).

`run_and_emit`'s stream-both-stdout-and-stderr invariant is preserved — only
`_get_base_ref.sh`'s stderr is conditionally suppressed in the layer-context
path, NOT `buf breaking`'s output (matches security item 6: buf stderr never
suppressed).

The per-layer cache file written by `_get_base_ref.sh` namespaces on
`DEVLOOP_LAYER` (set to `1` when invoked from layer1, `6` from layer6, falls
back to `"shared"` for standalone runs).
explicitly).

### Gate 2 expectation

Locally, `./scripts/layer1.sh` stage-1 and `./scripts/audit.sh` proto step
will emit `STATUS=FAIL REASON=buf-binary-missing` **only** until
`infra/devloop/Dockerfile`'s buf install is rebuilt into the image.
Paired-infra is landing the Dockerfile diff in this same devloop, so once the
image rolls Gate 2 reaches green. If the image rebuild lags the wrapper
commit, the FAIL is the correct loud signal per ADR-0033 §3 — flagging to
Lead at Gate 2 only if it blocks self-validation timing.

---

## Pre-Work

None.

---

## Implementation Summary

Landed all 6 proto wrappers (no `audit.sh` per F4), dispatcher INCLUDE/EXCLUDE
knob, α precedence re-rank in `_common.sh`, F4 `scripts/audit.sh` flip,
layer1.sh refactor, dry-reviewer D1 `_test_helpers.sh` (proto consumes;
rust/ts TODO note only). All shared tests pass (140 assertions across 8 test
suites). Gate-2 evidence captured below.

**Key implementation note** (not in plan, surfaced at dry-run): layer1.sh
needs `|| true` on each stage's pipe so stage 2 runs even when stage 1
fails — observability O2 invariant ("a runbook reader should learn proto +
rust/ts state in one run, not need a second invocation"). Aggregation
precedence still propagates stage-1 FAIL to the layer-final STATUS via
`__LAYER_STATUSES` worst-wins. Verified at dry-run: `[FAIL, OK,
SKIPPED-NO-VERB, OK]` aggregates to `STATUS=FAIL REASON=layer1-summary`.

**α implementation also required** updating `aggregate_worst_status` to
initialize `worst` to the first arg (not literal `OK`), since OK is no
longer the lowest rank. Without this fix, `[SKIPPED-NO-VERB, SKIPPED-NO-DIFF]`
would aggregate to OK incorrectly. Caught at dispatcher test dry-run; fixed
in-place. Stays within Lead's scope-check constraint (still one logical
change to the aggregator).

**Gate 2 iteration 1 fix (Lead found, applied)**: bare `buf <verb>` in the
wrappers walks cwd for `.proto` files instead of locating the v2 workspace
at `proto/buf.yaml`. With cwd `= /work` (where layer scripts run from), buf
finds `proto/internal.proto` recursively but treats `/work` as the module
root, so `import "signaling.proto"` looks for `/work/signaling.proto`
instead of `/work/proto/signaling.proto`. Fix: pass `proto` as positional
input to every buf invocation. `breaking.sh` additionally needs
`,subdir=proto` qualifier on the `--against` git URL so the comparison
baseline is also rooted at the workspace dir.

Concretely:
- `compile.sh`: `buf build` → `buf build proto`
- `fmt.sh`: `buf format --diff --exit-code` → `buf format --diff --exit-code proto`
- `lint.sh`: `buf lint` → `buf lint proto`
- `breaking.sh`: `buf breaking --against ".git#ref=<sha>"` →
  `buf breaking proto --against ".git#ref=<sha>,subdir=proto"`

Verified after fix (with buf 1.50.0 installed locally):
- `./scripts/layer1.sh` → STATUS=OK REASON=layer1-summary, exit 0 ✓
- `./scripts/layer6.sh` → proto's buf-breaking-passed OK; single BASE_REF=
  line confirms conditional-stderr-suppression working (cargo-audit FAIL is
  pre-existing RUSTSEC-2023-0071, unrelated)
- All 140 test assertions still pass.

**Gate 2 iteration 2 refinement (observability C3 follow-up)**:
`breaking.sh`'s `BASE_SHA=$(... 2>/dev/null)` silently swallowed non-zero
exits from `_get_base_ref.sh`. If the helper fails (e.g. degraded git
state, `origin/main` unreachable), `BASE_SHA` is empty and `buf breaking
--against ".git#ref="` produces an opaque downstream error. Fix:
explicit-exit-code capture on BOTH conditional branches, emitting a
distinct `base-ref-unresolved` FAIL token. Yields a precise three-way
classification: (1) `buf-binary-missing` (toolchain), (2)
`base-ref-unresolved` (degraded git), (3) `buf-breaking-{passed,failed}`
(actual gate result).

New REASON token added to the vocabulary: `base-ref-unresolved` (FAIL).

Verified: `./scripts/layer1.sh` + `./scripts/layer6.sh` still emit
`STATUS=OK REASON=layer1-summary` and `STATUS=OK REASON=buf-breaking-passed`
respectively; all 140 test assertions still pass.

---

## Files Modified

### New files
- `scripts/lang/proto/changed.sh` (6 lines + comment) — diff predicate.
- `scripts/lang/proto/changed.test.sh` (~25 lines) — locality self-test;
  consumes `_test_helpers.sh`.
- `scripts/lang/proto/compile.sh` (~20 lines) — `buf build` + `buf-binary-missing`
  guard.
- `scripts/lang/proto/fmt.sh` (~20 lines) — `buf format --diff --exit-code` +
  same guard.
- `scripts/lang/proto/lint.sh` (~15 lines) — `buf lint` + same guard.
- `scripts/lang/proto/breaking.sh` (~40 lines) — `buf breaking` + base-sha
  resolution with conditional stderr suppression + security lockdown comment.
- `scripts/lang/_test_helpers.sh` (~75 lines) — shared changed.test.sh
  scaffolding per dry-reviewer D1.

### Modified files
- `scripts/lang/_common.sh` — α: ranks swapped (OK → 2, SKIPPED-NO-DIFF → 1,
  SKIPPED-NO-VERB → 0); comment block at §STATUS-aggregation updated;
  `aggregate_worst_status` initializer fixed.
- `scripts/lang/_dispatch.sh` — INCLUDE_LANGS + EXCLUDE_LANGS filter with
  load-bearing comment; empty-after-filter → `SKIPPED-NO-VERB REASON=
  all-langs-filtered` + stderr breadcrumb.
- `scripts/lang/_dispatch.test.sh` — 4 new test functions:
  `test_include_langs_keeps`, `test_exclude_langs_drops` (each with
  sentinel-file observability bonus check),
  `test_filter_empty_after_filter`, `test_aggregate_precedence_ok_beats_skipped`
  (α regression test per Lead constraint #3); stale comment in
  `test_stream_verbatim_contract` updated.
- `scripts/lang/_common.test.sh` — assertions updated to α ladder; new
  multi-lang success-path test (`[OK, SKIPPED-NO-VERB, SKIPPED-NO-DIFF] → OK`).
- `scripts/lang/_test_changed_predicates.sh` — proto column added (10 new
  assertions: 5 touched + 5 untouched).
- `scripts/lang/rust/changed.test.sh` — 1-line TODO note added pointing at
  `_test_helpers.sh` (staged adoption per dry-reviewer D1).
- `scripts/lang/ts/changed.test.sh` — same TODO note.
- `scripts/audit.sh` — F4 cascade: invokes `lang/proto/breaking.sh` after
  dispatcher loop with exit-code aggregation per ADR-0033 §10:397.
- `scripts/layer1.sh` — Lead's v3 routing (INCLUDE_LANGS=proto stage 1,
  EXCLUDE_LANGS=proto stage 2) + `|| true` on each stage so both run.

### Pending (paired-infra drives)
- `infra/devloop/Dockerfile` — buf install (pinned `BUF_VERSION=1.50.0` +
  sha256). Awaiting paired-infra's diff push.

---

## Devloop Verification Steps

To be executed and evidence captured at Gate 2:

1. **Wrapper tests pass**:
   - `./scripts/lang/proto/changed.test.sh` → exit 0
   - `./scripts/lang/_test_changed_predicates.sh` → exit 0 (with proto column)
   - `./scripts/lang/_dispatch.test.sh` → exit 0 (with EXCLUDE-knob test)
2. **Executable bit on every new wrapper** (test ask #4): paste
   `ls -l scripts/lang/proto/*.sh` into Gate-2 message confirming all 7 are
   `-rwxr-xr-x`.
3. **Layer 1 stage-ordering observable in stream** (test ask #3): run
   `./scripts/layer1.sh` and capture stdout. STATUS line from
   `lang/proto/compile.sh` must appear BEFORE rust/ts STATUS lines from the
   stage-2 dispatcher. Paste the relevant stdout fragment into Gate-2 message.
4. **STATUS contract spot-check** (test ask #5): invoke
   `./scripts/lang/proto/lint.sh` directly and confirm exactly one
   `STATUS=...` line as the final stdout line.
5. **Audit dispatcher discovers proto/audit.sh**: run `./scripts/audit.sh` and
   confirm proto's audit STATUS line is in the verbatim stream (either
   `buf-binary-missing` pre-image-roll, or `buf-breaking-*` once the image
   rolls).
6. **shellcheck clean**: run `shellcheck scripts/lang/proto/*.sh
   scripts/layer1.sh scripts/lang/_dispatch.sh` and confirm zero warnings.
7. **`buf-binary-missing` FAIL path manual verification**: confirmed by
   Gate-2 run since `buf` is not yet in dev container.

### Gate 2 layer-by-layer verdict (Lead-validated, post-iter-2)

| Layer | STATUS | Notes |
|-------|--------|-------|
| 1 (compile) | OK | Stage 1 proto + stage 2 rust both pass |
| 2 (fmt) | OK | |
| 3 (guards) | OK | (After cross-boundary table sync to actual diff — Lead-edited main.md L60-L88 doc hygiene only, no code.) |
| 4 (test) | OK | cargo test + dispatcher tests + meta-tests + locality tests, **144/144** assertions pass (was 140 + 4 new S2 fail-closed regression cases) |
| 5 (lint) | FAIL **(pre-existing, NOT this devloop)** | `buf lint proto` surfaces 21 pre-existing R-61 STANDARD findings on `proto/{internal,signaling}.proto` touched by earlier branch devloops. To be drained by Track 2 #29 `lint.ignore` per ADR-0033 Revision 4. clippy + ts pass. |
| 6 (audit) | FAIL **(pre-existing, NOT this devloop)** | `cargo-audit-failed` = pre-existing RUSTSEC-2023-0071 (rsa via sqlx — verified on clean branch). `buf-breaking-passed` confirms F4 cascade wiring works correctly. `SKIPPED-NO-VERB REASON=proto-audit-sh-missing-or-not-executable` is intended per F4 (proto has no audit.sh by design, §6 invariant). |
| 7 (env-tests placeholder) | N/A | wave2-pending |
| 8 (env-tests integration) | N/A skipped | No service code touched; build/validation tooling only. |
| Semantic-guard | pending → verdict in flight at Gate 2 close | |

**Both Layer 5 and Layer 6 FAILs are pre-existing branch state** (verified
by Lead via `git stash` against clean trunk). The pipeline correctly
identified known issues that other tasks own:
- Layer 5: Track 2 #29 (R-61 STANDARD-tier drain).
- Layer 6: separate rsa/sqlx vulnerability triage (out of scope for any
  current Wave 2 devloop).

Neither counts as a Gate 2 validation failure for this devloop's purposes.

---

## Review

### Gate 3 verdicts (all 7 reviewers + semantic-guard)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | — | — | All 12 locked invariants verified in code (S1-S4, items 1-8 + refinement, tightenings B+C). Tightening A deferred to Wave 3 with shape-validation note. |
| Test | CLEAR | 0 | — | — | All 5 Gate-2 evidence asks satisfied + 4 bonus observations (sentinel-file INCLUDE/EXCLUDE, α regression test, multi-lang success-path, audit fail-closed regression). 144/144 assertions pass. |
| Observability | CLEAR | 0 | — | — | C1 moot under F4; C2/C3 + bonus check + α regression all verified at runtime. Single BASE_REF= emission per layer confirmed. |
| Code Quality | RESOLVED | 1 NIT | 1 (line 192 comment) | 0 | All 4 (α) conditional gates + 6 F-findings + Q1 + S1/S2 satisfied at diff level. Stale-ladder-comment NIT applied. |
| DRY | CLEAR | 0 | — | — | D1 staged adoption clean (`_test_helpers.sh` lands, proto consumes, rust+ts TODO breadcrumbs). D2 deferred (require_tool still below threshold). D3 single-source-of-truth contracts preserved. Two minor non-blocking observations (test-dir-helper extraction, breaking.sh if/else collapse) recorded for Wave 3. |
| Operations | CLEAR | 0 | — | — | Four-token triage surface verified at runtime: buf-binary-missing, all-langs-filtered, base-ref-unresolved, proto-audit-sh-missing. F4 cascade greppable. Idempotency + rollback path confirmed. Task #39 runbook entries captured for operations to carry forward. |
| Paired-Infrastructure | CLEAR | 0 | — | — | `infra/devloop/Dockerfile` buf install pushed (BUF_VERSION=1.50.0 + sha256 + smoke-test). Round-2 shape nit on scripts/audit.sh aggregation withdrawn (implementer's WHY-comment + monotonicity argument accepted). |
| Semantic-guard | SAFE | 0 | — | — | 9-section semantic check (audit cascade, layer1 ordering, dispatcher filter, α re-rank, breaking.sh lockdown, compile/lint/fmt uniformity, changed.sh, test helpers, predicates meta-test). No semantic regressions. |

### Pre-existing branch state (out of scope, NOT introduced by this devloop)

- **Layer 5 `buf-lint-failed`**: 21 R-61 STANDARD findings on `proto/{internal,signaling}.proto`. Branch state from earlier devloops; to be drained by Track 2 #29's temporary `proto/buf.yaml lint.ignore` scaffolding per ADR-0033 Revision 4. Verified pre-existing via `git stash`.
- **Layer 6 `cargo-audit-failed`**: pre-existing RUSTSEC-2023-0071 (rsa via sqlx-mysql). Separate vulnerability-triage track; not in R-62 scope.

### Accepted deferrals / tech debt for follow-up (forwarded to docs/TODO.md if not already tracked)

- ADR-0033 §1 doc-only typo: listing names proto's wrapper `format.sh`; actual file is `fmt.sh` (matches dispatcher verb + rust precedent). Doc-only follow-up commit, separate from this devloop.
- Wave 3 `DEVLOOP_BASE_SHA` export from `layer*.sh` to eliminate double-`_get_base_ref.sh` invocation in `breaking.sh`. Single-source-of-truth tightening; TODO captured in `breaking.sh:23-30` header.
- Wave 3 `require_tool` helper extraction (D2): wait until a non-proto wrapper needs the `command -v <tool>` guard (cross-lang threshold not yet met).
- Wave 3 `__build_dispatch_test_dir` helper extraction (dry-reviewer informational observation): 8+ existing call sites in `_dispatch.test.sh`; refactor too broad for R-62 scope.
- Wave 3 `lang/{rust,ts}/changed.test.sh` migration to `_test_helpers.sh` (D1 follow-up): TODO breadcrumbs in place pointing at proto's consumer shape.
- Wave 3 SKILL.md Step 6 collapse + Layer 8→7 renumber (task #38).
- Task #39 runbook (`docs/runbooks/devloop-validation.md`): operations will capture the 4-token triage tree (buf-binary-missing / base-ref-unresolved / buf-breaking-failed-{intentional,accidental,tooling-drift} / proto-audit-sh-missing-or-not-executable).
- Task #41 intentional wire-break override mechanism: deferred until ≥2 real wire-breaking PRs as case studies (#31 will be one). When R-61 task #31 ships, the override-mechanism design needs both case studies.

### Final result

Gate 1 PASS · Gate 2 PASS (2 iterations) · Gate 3 PASS (6 CLEAR + 1 RESOLVED) · ready for commit.
