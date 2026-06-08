# Devloop Output: Migrate GC integration tests to the camelCase wire shape (task #49)

**Date**: 2026-06-08
**Task**: Repair GC integration tests broken by task #23's camelCase wire migration; add dual-scope wire-shape lock tests (struct + HTTP-integration). The GC mirror of task #46.
**Specialist**: global-controller
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-49`
**Duration**: ~45m (setup 21:59 → commit ~22:45 UTC)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `44e8b194308a36c03b558700a8474a92c47d086b` |
| Branch | `feature/browser-client-join-task-49` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@gc-camelcase-task-49` |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |
| Security | `security@gc-camelcase-task-49` |
| Test | `test@gc-camelcase-task-49` |
| Observability | `observability@gc-camelcase-task-49` |
| Code Quality | `code-reviewer@gc-camelcase-task-49` |
| DRY | `dry-reviewer@gc-camelcase-task-49` |
| Operations | `operations@gc-camelcase-task-49` |
| Semantic Guard | `semantic-guard@gc-camelcase-task-49` |

---

## Task Overview

### Objective
Task #23 / R-53 (commit `92d963b`) flipped GC's request/response structs in
`crates/gc-service/src/models/mod.rs` to `#[serde(rename_all = "camelCase")]`
(several also `deny_unknown_fields`) but updated NO GC integration tests. Those
tests are DB-gated `#[sqlx::test]` and never ran in the task-#23 clone — the
same false-CLEAR mechanism task #46 found and fixed for AC. ~27 pre-existing
failures result. Repair the tests to the camelCase wire shape and add dual-scope
wire-shape lock tests so a future rename sweep cannot silently regress.

### Scope
- **Service(s)**: GC (`crates/gc-service`, `crates/gc-test-utils`) only
- **Schema**: No
- **Cross-cutting**: No (test-only; production wire structs are correct + out of scope)

### Known failing surfaces (from task #49 / docs/TODO.md §Test Debt)
- `crates/gc-service/tests/meeting_create_tests.rs` — 7 of 13 fail
- `crates/gc-service/tests/meeting_tests.rs` — 19 of 38 fail
- `crates/gc-test-utils/src/server_harness.rs:214` — 1 of 7 fail (readiness-probe stale snake key)

### Precedent (task #46)
- Struct-level wire-shape lock: pattern from `f5fc4b4` (AC `models/mod.rs` —
  `test_service_token_response_wire_shape_stays_snake`).
- HTTP-integration wire-shape lock: pattern from `265e56e`
  (`test_register_wire_shape_golden_lock` in `user_auth_tests.rs`) — BTreeSet
  key-set discipline at HTTP round-trip scope, "the scope that actually broke."
- **Dual-scope lock is the load-bearing protection** — add at BOTH struct and
  HTTP-integration scope so a rename sweep can't slip through at either.
- GC has no OAuth-spec'd endpoints (only AC issues OAuth tokens) → NO per-field
  snake_case carve-outs; keep all GC responses camelCase per task #23. (Note:
  `HealthResponse` is intentionally `snake_case` and `ReadinessResponse`
  camelCase today — lock the ACTUAL current production shape, not a blanket
  camelCase assumption.)

### Out of scope
- Production wire-struct changes (the wire shape is correct + being locked)
- Non-GC crates

### Debate Decision
NOT NEEDED — wire-shape decision was already ruled in task #23 / #46; this is the
mechanical GC mirror plus regression locks.

---

## Cross-Boundary Classification

<!-- Populated by implementer during planning; reviewed at Gate 1. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/gc-service/tests/meeting_create_tests.rs` | Mine — Mechanical (migrate request bodies + response key reads snake→camelCase) | — |
| `crates/gc-service/tests/meeting_tests.rs` | Mine — Mechanical (migrate guest/settings request bodies + join/settings response key reads snake→camelCase) | — |
| `crates/gc-test-utils/src/server_harness.rs` | Mine — Mechanical (single readiness-probe assertion `ac_jwks`→`acJwks`, matching `ReadinessResponse`'s camelCase) | — |
| `crates/gc-service/src/models/mod.rs` | Mine — Minor-judgment (add `#[cfg(test)]` struct-level wire-shape lock tests; new code, no production-struct edits) | — |
| `crates/gc-service/tests/meeting_create_tests.rs` (HTTP golden-lock test) | Mine — Minor-judgment (add HTTP-integration golden-lock test mirroring AC `test_register_wire_shape_golden_lock`) | — |
| `crates/gc-service/src/handlers/me.rs` | Mine — Minor-judgment (add `#[cfg(test)]` struct-level wire-shape lock for `MeResponse`; new test in existing test mod, no production edits) | — |

**`MeResponse` scope decision (Gate-1, @team-lead-prompted, Option (a) chosen)**:
`handlers/me.rs::MeResponse` is camelCase (`rename_all = "camelCase"`; `serviceType`,
`service_type` field has `skip_serializing_if`) and `/me` is NOT in #49's failing-test
surface (both test files are `/api/v1/meetings` only). But it is exactly the kind of
silent rename-sweep slip the dual-scope lock exists to catch. Chose **Option (a)**:
add a cheap `#[cfg(test)]` struct-level wire-shape lock for `MeResponse` in the
existing `handlers/me.rs` test mod (which already pins `serviceType` and carries the
`#[allow(clippy::unwrap_used, clippy::expect_used)]` convention). Lock populates
`service_type: Some(...)` so the full key-set is present. Directly strengthens the
anti-regression purpose at low cost; no production edits. (Option (b) — doc-only
out-of-scope + TODO pointer — declined since (a) is nearly free and serves intent.)

**Guarded Shared Areas check**: None of these paths are Guarded Shared Areas. All
are GC-domain test files or GC's own `models/mod.rs` (test module only). No
production wire structs are edited. `crates/gc-test-utils` is GC test
infrastructure (GC-owned). No protocol/, no migrations/, no other-crate files.

**Keys deliberately NOT migrated** (not GC wire structs — would be incorrect to camelCase):
- `body["error"]["code"]` — error envelope; `error`/`code` are single-word, scheme-invariant.
- Mock AC internal responses (`"token"`, `"expires_in"` in `set_body_json`) — AC's internal API contract.
- Outbound-request inspection keys (`home_org_id`, `meeting_org_id`) — AC internal meeting-token request contract (GC's outbound client, out of scope).
- `join_token_secret` forbidden-key guard — keep checking the snake form (and the response never contains it in either form).
- JWT claim fields in test fixtures (`service_type` in `create_service_token()`) — JWT claim names, not GC wire-shape.

**Third-file determination (Gate-1 record, prompted by @security)**: `meeting_join_metrics_integration.rs`
was raised as a possible fourth target. Investigated: its only snake_case matches
(`mc_assignment`, lines 53/70) are metric `error_type` LABEL VALUES in
`SHARED_ERROR_TYPES`/`ALL_ERROR_TYPES`, driving `record_meeting_join()` label
assertions — it is a pure-Rust metrics test with NO HTTP round-trip, NO `.json()`
request bodies, NO `body[...]` reads. `mc_assignment` there MUST stay snake_case
(it's a label name, not wire-shape). `meeting_creation_metrics_integration.rs`
likewise has no wire-shape JSON. Confirmed migration targets remain exactly the
three files above.

**Wire invariant — ALL camelCase, NO mixed scheme (user ruling at Gate-1 approval, supersedes earlier per-struct-shape framing)**:
Every GC wire RESPONSE is camelCase. There is NO OAuth/RFC-6749 carve-out in GC —
GC's join/guest-token endpoints are NOT RFC 6749 token endpoints, so
`JoinMeetingResponse.expires_in` serializes to `expiresIn` (confirmed: the
struct's `rename_all = "camelCase"` already produces this in current production;
migration is TEST-ONLY, no production struct change). Lock tests assert ALL-CAMELCASE
per response struct via BOTH:
  (a) full `BTreeSet` golden key-set equality (catches add / remove / rename), AND
  (b) an explicit "no serialized key contains `_`" assertion (catches a future
      PARTIAL rename that leaves a single field snake_case).

**HealthResponse framing CORRECTED (user ruling)**: The earlier note claimed
`HealthResponse` "stays snake_case" — that was a MISREAD. The `rename_all = "snake_case"`
at `models/mod.rs:13` is on the `MeetingStatus` ENUM, not `HealthResponse`.
`HealthResponse` (`:49`) has NO `rename_all`, only single-word fields
(`status`/`region`/`database`), and is DEAD CODE (`/health` returns plain text "OK"
per ADR-0012, see doc at `:45`). So there is NO HealthResponse snake carve-out;
its fields are single-word and trivially camelCase-equivalent. Locking it is OPTIONAL
(dead code + single-word) — if locked, it's a struct-only serialize tripwire with the
same all-camelCase / no-`_` assertions; NO HTTP-integration lock (no live endpoint).
(`MeetingStatus` enum VALUES like `"scheduled"` are lowercase single words — a
serde enum-variant rename, NOT a field-name camel/snake concern; not conflated.)

**Doc-prose deferral**: This devloop is TEST-ONLY. The R-11/R-53 rule-change
documentation (codifying "GC = all camelCase, no OAuth carve-out") is owned by
follow-up **task #51** — do NOT amend R-11/R-53 doc prose here.

---

## Gate 2 Execution Directive (USER, 2026-06-08) — VERIFY RUN, NOT JUST GREEN

**The whole point of #49 is that DB-gated `#[sqlx::test]` tests were SILENTLY SKIPPED
in the task-#23 clone and "Test CLEAR" was reported falsely.** A green summary alone
does NOT satisfy Gate 2. The Lead MUST prove the tests actually executed:

1. **DB must be live** so `#[sqlx::test]` tests run (not skipped on missing DATABASE_URL).
   Confirm via dev-cluster / DATABASE_URL before trusting any "passed" line.
2. **Enumerate + count**: each affected target's `test result:` line must show the
   expected count `passed`, **`0 failed`, `0 ignored`, `0 filtered out`**. Specifically:
   - `meeting_create_tests.rs` — all 13 (was 7 fail) + new HTTP golden-lock(s) PASS.
   - `meeting_tests.rs` — all 38 (was 19 fail) PASS.
   - `gc-test-utils` `server_harness` — all 7 (was 1 fail) PASS.
   - new struct-level lock tests in `models/mod.rs` + `handlers/me.rs` PASS and RAN.
3. **No silent skip**: grep output for `ignored` / `0 tests` / `filtered out`; verify the
   specific previously-failing test names appear as RUN (e.g. `cargo test … -- --list`
   or `--nocapture`), not absent. A target reporting `0 tests run` is a FAIL, not a pass.
4. Run with `--no-fail-fast` so one failure can't mask the count of the rest.

---

## Planning

### Gate 1 Plan Confirmations — ALL CONFIRMED → Plan Approved 2026-06-08

Layer B classification-sanity guard: `STATUS=OK REASON=cross-boundary-classification-clean-1-files`.

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (no auth assertions weakened; deny_unknown_fields preserved; metrics-file correctly NON-target) |
| Test | confirmed (verified response shapes; 2 Gate-2 conditions + MeResponse scope note → Option a) |
| Observability | confirmed (verified harness:214 is in-file test mod, not spawn() API) |
| Code Quality | confirmed (cross-boundary table complete; GSA-clear; webtransportEndpoint skip_serializing_if catch) |
| DRY | confirmed (2 non-blocking review-time watchpoints) |
| Operations | confirmed (.sqlx unaffected; no CI/runbook/manifest impact) |
| Semantic Guard | confirmed (test-only; will verify locked shapes match prod derives) |

---

## Pre-Work

None.

---

## Implementation Summary

Test-only migration to the camelCase wire shape + dual-scope wire-shape locks. No
production wire-struct edits (the structs were already migrated by task #23; this
repairs the tests that #23's clone never ran and adds regression locks).

**Repairs (3 surfaces):**
- `meeting_create_tests.rs` — migrated every request body (`displayName`/`maxParticipants`/
  `enableE2eEncryption`/`requireAuth`/`recordingEnabled`/`allowGuests`/
  `allowExternalParticipants`/`waitingRoomEnabled`) and every response read
  (`meetingId`/`meetingCode`/`displayName`/`maxParticipants`/`createdAt`/…) to camelCase.
  Kept `unknown_field` genuinely unknown (sibling `display_name`→`displayName`) and the
  `body.get("join_token_secret")` forbidden-key guard (snake form).
- `meeting_tests.rs` — guest-token bodies (`displayName`/`captchaToken`), settings PATCH
  bodies + response reads (`allowGuests`/`allowExternalParticipants`/`waitingRoomEnabled`),
  join response reads (`expiresIn`/`meetingId`/`meetingName`, `mcAssignment.mcId`/
  `grpcEndpoint`/`webtransportEndpoint`). Deterministic values now value-compare
  (`expiresIn`==900, `meetingName`=="Test Meeting"); type-probes kept only on
  non-deterministic `token`/`meetingId`, on the corrected camelCase key.
- `gc-test-utils/src/server_harness.rs:214` — readiness probe `ac_jwks`→`acJwks`
  (value-comparing `assert_eq!(… "available")` preserved).

**Deliberately UNCHANGED (not GC wire shape):** mock AC internal responses
(`"expires_in": 900` in `set_body_json`, lines 100/111); outbound-request reads
`body["home_org_id"]`/`["meeting_org_id"]` (AC internal meeting-token contract);
`body["error"]["code"]` envelope; DB SQL column names; `create_service_token()` JWT
`service_type` claim; `meeting_join_metrics_integration.rs` `mc_assignment` metric label.

**Dual-scope locks added (11 new tests):**
- Struct-level (`models/mod.rs`, 9 tests): `CreateMeetingResponse`, `MeetingResponse`,
  `JoinMeetingResponse`, `McAssignmentInfo` (nested, `webtransport_endpoint: Some(..)` to
  lock all 3 keys), `ReadinessResponse` (`acJwks`), `HealthResponse` (dead-code, single-word
  tripwire), + request deserialize locks for `CreateMeetingRequest`/`GuestJoinRequest`/
  `UpdateMeetingSettingsRequest` (camel accepted, snake rejected via `deny_unknown_fields`).
  Each response lock = full `BTreeSet` key-set equality + "no key contains `_`".
- Struct-level (`handlers/me.rs`, 1 test): `MeResponse` (`serviceType`; populates
  `service_type: Some(..)`).
- HTTP-integration golden-lock (`meeting_create_tests.rs`, 1 test):
  `test_create_meeting_wire_shape_golden_lock` — sends exact camelCase body, asserts
  **201 BEFORE body**, full `BTreeSet` key-set equality, no-`_` guard, forbidden-key loop
  on **both** `join_token_secret` AND `joinTokenSecret`.

**Verification (against the Gate-2 Execution Directive — tests RAN, not skipped):**
DB live via `DATABASE_URL=postgresql://…@devloop-browser-client-join-task-49-db:5432/dark_tower_test`.
- `meeting_create_tests`: `14 passed; 0 failed; 0 ignored; 0 filtered out` (was 7/13 failing; 13 original + 1 new HTTP lock).
- `meeting_tests`: `38 passed; 0 failed; 0 ignored; 0 filtered out` (was 19/38 failing).
- `gc-test-utils` server_harness: `7 passed; 0 failed; 0 ignored; 0 filtered out` (was 1/7 failing).
- gc-service `--lib`: `288 passed` incl. 10 new wire-shape locks (`me.rs` lock + 9 in models).
- Full gc-service suite: every target green, incl. untouched metrics files + auth_tests (15/15).
- `cargo clippy -p gc-service -p gc-test-utils --tests`: clean (0 warnings).
- **Tripwire smoke-test**: temporarily reverting `CreateMeetingResponse` to `snake_case`
  made `test_create_meeting_response_wire_shape_stays_camel` FAIL loudly; restored → passes.
  Confirms the lock actually catches a regression (not a vacuous assert).

---

## Files Modified

| File | Change |
|------|--------|
| `crates/gc-service/tests/meeting_create_tests.rs` | Request bodies + response reads → camelCase; added `test_create_meeting_wire_shape_golden_lock` (HTTP-integration) + `client_post_camel` helper |
| `crates/gc-service/tests/meeting_tests.rs` | Guest/settings bodies + join/settings response reads → camelCase (value-compare where deterministic) |
| `crates/gc-test-utils/src/server_harness.rs` | Readiness probe `ac_jwks`→`acJwks` |
| `crates/gc-service/src/models/mod.rs` | +9 `#[cfg(test)]` struct-level wire-shape locks + 2 helpers (`wire_key_set`, `assert_no_snake_keys`) |
| `crates/gc-service/src/handlers/me.rs` | +1 `#[cfg(test)]` `MeResponse` wire-shape lock |

---

## Gate 2 — Lead Independent Verification (2026-06-08) — PASS

Verified independently by Lead (not taken from implementer self-report). Attempt 1
caught a Layer 2 (fmt) failure → routed back → fixed via `cargo fmt`. Attempt 2 PASS.

**`./scripts/layer-all.sh` → LAYER_ALL_EXIT=0**, all layers:
| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | rust + dt-guard + ts |
| 2 Format | OK | (attempt 1 FAILED at models/mod.rs:1017/1197; fixed) |
| 3 Guards | OK | guards-passed incl. cross-boundary + audit-gate self-tests |
| 4 Test | OK | full workspace suite, 258s |
| 5 Lint (clippy) | OK | 0 warnings |
| 6 Audit | OK | cargo-audit + pnpm-audit + buf-breaking passed |
| 7 Env-tests | N/A in pipeline (`REASON=wave2-pending`) → **run manually by Lead** |

**Test-ran-not-skipped proof (per USER Gate-2 directive)** — DB live
(`…-task-49-db`); each target `0 failed / 0 ignored / 0 filtered out`:
- `meeting_create_tests`: **14 passed** (was 7/13 failing) — incl. new HTTP golden-lock.
- `meeting_tests`: **38 passed** (was 19/38 failing).
- `gc-test-utils` server_harness: **7 passed** (was 1/7 failing).
- `gc-service` lib: **288 passed** — incl. all **10 new wire-shape locks** (9 in
  models/mod.rs + `handlers::me::tests::test_me_response_wire_shape_stays_camel`),
  verified present via `--list` and previously-failing names confirmed RUN, not removed.

**Layer 7 env-tests (manual, live cluster, smoke+flows+observability)** — all
wire-shape-relevant suites PASS, confirming the camelCase contract end-to-end over
HTTP: `23_meeting_creation` 6/0, `24_join_flow` 9/0
(`test_gc_join_returns_meeting_token_and_mc_assignment`), `21_cross_service_flows`
12/0 (incl. `/me` `MeResponse` via `test_gc_validates_ac_token_via_me_endpoint`),
`26_mh_quic` 6/0 (`test_mh_url_present_in_join_response`), auth/security 9/0+5/0+5/0.
One flake — `test_all_services_have_logs_in_loki` failed under parallel load, **passes
on isolated retry** (Loki ingestion-timing; Loki is "optional"; our test-only diff
cannot affect log emission) → infra-flake, does not consume an attempt.

**Scope/Layer-A**: diff is exactly the 5 planned files; no production struct/derive
edits (additions to models/mod.rs are all `#[cfg(test)]`); no doc-prose changes
(R-11/R-53 untouched, owned by task #51). Layer B classification guard: `STATUS=OK`.

---

## Code Review Results

Gate 3 — all 7 verdicts in; none ESCALATED. Both code-reviewer and dry-reviewer
independently re-ran the tripwire (revert a golden set → lock FAILS) confirming the
locks are load-bearing, not tautological.

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | harness comment claimed "silent pass"; value-compare actually FAILS → comment corrected (comment-only) |
| Test | CLEAR | 0 | 0 | 0 | all 4 conditions met; both lock scopes load-bearing; MeetingStatus enum-value trap avoided; intent preserved |
| Observability | CLEAR | 0 | 0 | 0 | metric cluster untouched; mc_assignment Prometheus label unaffected |
| Code Quality | CLEAR | 0 | 0 | 0 | ADR-0002/0019/0024 compliant; field-for-field golden-set check; tripwire proven; (me.rs table item resolved — could not prove gap, revised to CLEAR) |
| DRY | RESOLVED-DEFERRED | 1 | 0 | 1 | 0 true-dup; 1 extraction opportunity (shared wire-key-set helpers AC+GC) → docs/TODO.md |
| Operations | CLEAR | 0 | 0 | 0 | .sqlx unaffected; harness API intact; no CI/guard/manifest impact |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 | all additions inside `#[cfg(test)]`; only credential-NON-leak asserts; no real secrets |

**Final re-verification after all post-Gate-2 fixes** (security comment + DRY TODO entry):
fmt EXIT=0; lib 288/288, meeting_create 14/14, meeting_tests 38/38, gc-test-utils 7/7
— every target 0 failed / 0 ignored / 0 filtered.

---

## Accepted Deferrals

- `docs/TODO.md` §Cross-Service Duplication (DRY) — shared wire-key-set test helpers (`wire_key_set`/`assert_no_snake_keys`/AC `wire_keys`) reimplemented inline across AC+GC; extraction to `common` deferred (cross-crate, outside GC-only scope)

---

## Rollback Procedure

1. Start commit: `44e8b194308a36c03b558700a8474a92c47d086b`
2. Review changes: `git diff 44e8b19..HEAD`
3. Soft reset: `git reset --soft 44e8b19`
4. Hard reset: `git reset --hard 44e8b19`
(Test-only change; no schema/infra rollback considerations.)

---

## Issues Encountered & Resolutions

### Issue 1: Plan misread HealthResponse as snake_case
**Problem**: The plan claimed `HealthResponse` is `#[serde(rename_all = "snake_case")]`
and proposed locking it as snake. In fact that derive is on the `MeetingStatus`
enum (`models/mod.rs:13`); `HealthResponse` has no `rename_all`, single-word fields,
and is dead code (`/health` returns plain text per ADR-0012).
**Resolution**: User-ruled invariant = all GC responses camelCase, no mixed scheme.
Lock reframed to `test_health_response_wire_shape_has_no_snake_keys` (scheme-invariant
single-word, struct-only). No snake carve-out exists in any GC wire response.

### Issue 2: OAuth carve-out ambiguity on JoinMeetingResponse.expires_in
**Problem**: `expires_in` is the canonical OAuth field name; unclear whether GC's
join/guest-token response should keep it snake per RFC.
**Resolution**: User ruled GC's endpoints are NOT RFC 6749 token endpoints → all
camelCase, `expiresIn`, no carve-out. Matches current production (rename_all already
yields it), so the devloop stayed test-only. Doc-prose codification deferred to task #51.

### Issue 3: Gate 2 attempt 1 — Layer 2 (fmt) failure
**Problem**: Hand-aligned trailing comments + an over-width string in the new lock
tests failed `cargo fmt --check` (implementer's self-report missed it — clippy doesn't
check fmt). Caught by Lead's independent Gate 2 verification.
**Resolution**: `cargo fmt` (let rustfmt own formatting); re-verified fmt + tests + clippy.

### Issue 4: Layer 7 env-tests N/A in pipeline
**Problem**: `layer-all.sh` Layer 7 is `wave2-pending` (env-tests not yet wired) → N/A.
**Resolution**: Lead ran env-tests manually vs the live cluster; all wire-shape suites
green end-to-end (one Loki-ingestion flake that passes on isolated retry).

---

## Lessons Learned

1. **Green ≠ ran.** DB-gated `#[sqlx::test]` tests silently no-op without a live DB and
   still report CLEAR — the false-CLEAR class behind tasks #23/#46/#49. Gate 2 must
   prove execution (counts, `0 ignored/0 filtered`, names present), not just exit 0.
2. **Lock the intended invariant, not a current-shape snapshot.** Locking "the actual
   shape" would have pinned the HealthResponse misread; the load-bearing lock encodes
   the all-camelCase / no-mixed-scheme rule (full key-set equality + no key contains `_`).
3. **Independent Lead verification earns its keep.** The implementer's self-report said
   clippy-clean but missed the fmt failure; re-running Gate 2 caught it.
4. **Surface planning ambiguities to the human.** The `expires_in` OAuth question was a
   genuine product decision, not something to resolve by inference — raising it produced
   a clean ruling + a tracked follow-up (task #51).
