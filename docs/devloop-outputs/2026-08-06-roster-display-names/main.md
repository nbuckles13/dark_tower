# Devloop Output: Fix roster display names (registered display name in meeting roster)

**Date**: 2026-08-06
**Task**: Populate the USER meeting-token `display_name` from the registered user's `users.display_name` so the meeting roster shows real names instead of "Participant N".
**Specialist**: auth-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/user-story-run-test`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `78cc06344863dca2ff7da7af61bc1f0435f1ead7` |
| Branch | `feature/user-story-run-test` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` (Gate 3 passed: all verdicts CLEAR/RESOLVED-DEFERRED, no ESCALATED) |
| Implementer | `implementer` |
| Implementing Specialist | `auth-controller` |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `CLEAR` |
| Observability | `RESOLVED-DEFERRED` |
| Code Quality | `RESOLVED-DEFERRED` |
| DRY | `CLEAR` |
| Operations | `CLEAR` |
| Semantic Guard | `CLEAR` |

### Gate 3 verdicts (final)
| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | CLEAR | GSA co-sign on `jwt.rs` (incl. the 15 `guard:ignore` lines); fail-closed verified. `Approved-Cross-Boundary: security`. |
| Test | CLEAR | 3 coverage asks landed (happy-path, fail-closed 404, jwt serde/redaction). |
| Observability | RESOLVED-DEFERRED | Added `user_not_found` metric assertion; `lookup_error` assertion deferred (accepted). `Approved-Cross-Boundary: observability` (panel 44). |
| Code Quality | RESOLVED-DEFERRED | ADR-compliant; spun-out guard-precision follow-up (15 interim ignores tracked). |
| DRY | CLEAR | Reused `get_by_id`; deepened TODO #53 (DRY exception). |
| Operations | CLEAR | 3 fail-closed conditions landed; carrier inert; PK lookup. |
| Semantic Guard | CLEAR | Credential-leak/PII clean; 15 ignores audited as genuine false positives. |

### Gate 1 ruling (Lead) — missing `users` row: FAIL-CLOSED
Consensus reached (security + test + implementer favored fail-closed; operations withdrew its degrade requirement after verifying the facts). Ruling: **fail-closed** — issuance returns `AcError::NotFound` on a missing users row rather than minting a nameless token (per CLAUDE.md "fail loudly; never mask"; `None` is a genuine invariant violation today, a transient DB blip returns `Err` not `None`). Non-blocking in-changeset conditions folded in from operations: (1) surface user-not-found as `AcError::NotFound` → 404, not opaque 500; (2) distinct warn+bounded-counter on the `None` path vs the DB-error path (counter `reason` label a closed enum, no PII — per observability); (3) a code comment + `docs/TODO.md` note that fail-closed is correct only under the single-cluster assumption and must be revisited when cross-cluster federation is wired.

---

## Task Overview

### Objective
Two DISTINCT registered users join the same meeting from two browsers and each sees the other's **registered display name** in the roster. Today the roster shows generic "Participant 1 / Participant 2".

### Scope
- **Service(s)**: ac-service (issuance + users lookup), common (`jwt.rs` meeting-token claims — GSA). MC consumption spun out (see below).
- **Schema**: No (reuses existing `users.display_name` column).
- **Cross-cutting**: Yes — touches a Guarded Shared Area (`crates/common/src/jwt.rs`) requiring security co-sign; and the end-to-end feature depends on a meeting-controller spin-out.

### Debate Decision
NOT NEEDED — bounded fix within existing ADR-0020 token model; no new architectural boundary.

---

## Investigation Findings (Lead, pre-planning) — corrects the task's premise

The task assumed "downstream plumbing already works: MC populates `Participant.name` from the validated meeting-token `display_name`". **Investigation shows the meeting token does not carry `display_name` at all, and MC does not read one:**

1. **`common::jwt::MeetingTokenClaims`** (`crates/common/src/jwt.rs:356-380`) has **no** `display_name` field. This is the struct MC validates meeting tokens into (`crates/mc-service/src/auth/mod.rs:63-64`). (Contrast: `GuestTokenClaims` at `jwt.rs:425+` *does* carry `display_name`.)
2. **AC issuance** (`crates/ac-service/src/handlers/internal_tokens.rs:159-171`) builds a local serialize-only `MeetingTokenClaims` that also omits `display_name`. It does **not** look up the user.
3. **MC** ignores any token name: `crates/mc-service/src/actors/meeting.rs:604` sets `let display_name = format!("Participant {}", self.participants.len() + 1);` (`// MINOR-003: use generic display name, not derived from user_id`). That placeholder flows `ParticipantInfo.display_name` → `handler.rs:20` → proto `Participant.name` → roster. `join_connection` (`controller.rs:127`) carries no name param.
4. **GC** (`crates/gc-service/src/handlers/meetings.rs:424-436`) builds `MeetingTokenRequest` purely from `UserClaims` (`sub`, `org_id`, `email`, `roles` — **no `display_name`**) + the meeting row. GC has **no** users repository and never reads the `users` table.
5. **AC owns the users table**: `crates/ac-service/src/repositories/users.rs:60` `get_by_id(pool, user_id) -> Option<User>` returns `display_name` (currently `#[allow(dead_code)]`). Column: `users.display_name VARCHAR(255) NOT NULL` (`migrations/20250118000001_initial_schema.sql:29`).

### Chosen design (AC-side lookup — matches task steer, avoids GC/GSA churn)
Because AC both mints the meeting token **and** owns the users table, AC looks up `display_name` by `subject_user_id` at issuance and stamps it into the JWT. This avoids adding a user repository to GC and avoids changing the `MeetingTokenRequest` wire type (`crates/common/src/meeting_token.rs`, also a GSA). Only one GSA (`jwt.rs`) is touched, by its owner (auth-controller) with security co-sign.

### Scope boundary — MC consumption is spun out (meeting-controller, Domain-judgment)
Replacing the `meeting.rs:604` placeholder with the token-carried name, and threading it through `join_connection` / `ControllerMessage::JoinConnection`, is a **meeting-controller Domain-judgment** change that also reverses a security-motivated decision (MINOR-003). Per ADR-0024 §6.3 this is **owner-implements** — it must not be done inside an auth-controller devloop. This devloop delivers the **token carrier**; MC consumption is spun out. **End-to-end acceptance (roster shows real names) is therefore gated on the spin-out and cannot be manually verified by this devloop alone** — documented as an accepted, tracked dependency, not a silent gap.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/ac-service/src/handlers/internal_tokens.rs` | Mine | — |
| `crates/ac-service/src/repositories/users.rs` | Mine | — |
| `crates/common/src/jwt.rs` (add `display_name` to `MeetingTokenClaims`) | Mine — **Guarded Shared Area (auth/crypto)**, security co-sign required at review | auth-controller |
| `crates/ac-service/src/observability/metrics.rs` (add `record_meeting_display_name_outcome`) | Mine | — |
| `crates/mc-service/src/auth/mod.rs` (test builder: add `display_name` field to `MeetingTokenClaims` literal) | Not mine, Mechanical | meeting-controller |
| `crates/mh-service/src/auth/mod.rs` (test builder: add `display_name` field to `MeetingTokenClaims` literal) | Not mine, Mechanical | media-handler |
| `crates/mh-service/tests/common/tokens.rs` (test builder: add `display_name` field) | Not mine, Mechanical | media-handler |
| `crates/mc-test-utils/src/jwt_test.rs` (test builder: add `display_name` field) | Not mine, Mechanical | meeting-controller |
| `crates/ac-service/tests/integration/internal_token_tests.rs` | Mine | — |
| `crates/ac-service/tests/internal_token_metrics_integration.rs` | Mine | — |
| `docs/observability/metrics/ac-service.md` (catalog entry for new metric) | Mine | — |
| `infra/grafana/dashboards/*` (AC panel for new metric) | Not mine, Minor-judgment | observability |

<!-- The four Mechanical rows are compiler-forced struct-literal updates in `#[cfg(test)]`/test-util builders,
     caused by adding a required field to the shared `common::jwt::MeetingTokenClaims`. None are GSA paths, so
     Mechanical is permitted (ADR-0024 §6.2/§6.3 default-posture: proceed with review). code-reviewer verifies
     the Ownership Lens at Gate 3. MC *consumption* (meeting.rs:604 / controller.rs / connection.rs) is NOT in
     this diff — spun out to meeting-controller. -->

---

## Planning

### Mechanism restatement
The mechanism is: **the meeting token must carry the registered display name end-to-end** so the roster renders real names. That mechanism has two halves: (a) the **carrier** — the meeting JWT actually holds `display_name`, sourced from `users.display_name` at issuance; (b) the **consumer** — MC reads that name onto `Participant.name` instead of the `Participant N` placeholder. **This devloop owns half (a) only** (AC issuance + `common::jwt` claim). Half (b) is a meeting-controller Domain-judgment spin-out (reverses MINOR-003) and is explicitly out of scope. Consequence: end-to-end acceptance is gated on that spin-out and cannot be manually verified here.

### Findings verification
All five Lead findings verified against the code as stated:
1. `common::jwt::MeetingTokenClaims` (jwt.rs:355-380) has no `display_name`; Debug impl at :382-398 lists every field explicitly (so it needs a new redacted line). `GuestTokenClaims` (:425) carries `display_name`, redacted in Debug per its doc.
2. AC-local serialize-only `MeetingTokenClaims` (internal_tokens.rs:234-247) omits `display_name`; `issue_meeting_token_internal` (:133-180) never looks up the user.
3-4. (MC placeholder / GC has no users repo) — noted; both out of this diff.
5. `users::get_by_id` (users.rs:60) returns `User { display_name }`, currently `#[allow(dead_code)]`. `users.display_name` is `VARCHAR(255) NOT NULL`.

### Changes (carrier half only)
1. **`crates/common/src/jwt.rs`** (GSA, security co-sign):
   - Add `#[serde(default)] pub display_name: String` to `MeetingTokenClaims`. `#[serde(default)]` = backward-compat: in-flight tokens minted before this change (no `display_name` claim) still deserialize (empty string) during rollout. It does not weaken guest-vs-meeting anti-confusion (that is enforced by the `token_type` check in MC, untouched).
   - Update the `Debug` impl to render `display_name` as `[REDACTED]` (PII, mirrors `GuestTokenClaims`). Update the struct doc `# Security` note.
2. **`crates/ac-service/src/handlers/internal_tokens.rs`**:
   - In `issue_meeting_token_internal`, look up the user by `payload.subject_user_id` via `users::get_by_id(&state.pool, ...)` and stamp `display_name` into the JWT.
   - Add `display_name: String` to the AC-local serialize `MeetingTokenClaims` (:234) and populate it in the builder (:159).
   - **Missing-user behavior — FAIL CLOSED on any miss** (per "fail loudly; never mask"; resolves @security must-fix #2): extract a pure, testable helper `resolve_meeting_display_name(user: Option<User>) -> Result<String, AcError>`:
     - `Some(user)` -> `Ok(user.display_name)`.
     - `None` -> `Err(AcError::NotFound(..))` with a **generic** message (`"participant"` — NO user_id/email/name; ADR-0011 PII + generic-error discipline). We refuse to mint a nameless token rather than silently stamping an empty string (which would invisibly regress to "Participant N").
     - **Why unconditional (revised from the draft's External carve-out):** verified `crates/gc-service/src/handlers/meetings.rs:375-396` — an `External` participant today is still an AC-authenticated user whose `subject_user_id` is their own AC-issued `user_claims.sub`; in the current single-cluster deployment all orgs' users share the one `users` table, so `get_by_id` finds the row for Member **and** External. A miss only arises under true cross-cluster federation, which is not wired up. So failing closed has zero regression today and the empty-string branch would only mask genuine faults (deleted/corrupt id). **Future federation note:** when cross-cluster identity lands, the absent-local-row case needs a deliberate federated name-resolution source (NOT empty string) — recorded as a future concern, out of scope here.
     - DB *failure* (connection error) still propagates as `AcError::Database` (unchanged) — distinct from row-not-found.
     - Helper is pure/sync: no `#[instrument]`, no `tracing::`, no PII surface. The lookup uses the existing `users::get_by_id`, already `record_db_query`-instrumented with no arg-capturing attribute.
   - Remove the now-unnecessary `#[allow(dead_code)]` on `users::get_by_id`.
3. **`crates/ac-service/src/repositories/users.rs`**: remove `#[allow(dead_code)]` on `get_by_id` (struct-level allow stays; other fields still unread here).

### Not touched (deliberately)
- `crates/common/src/meeting_token.rs` (GSA): the `MeetingTokenRequest` wire type is NOT changed — AC does the lookup locally, so `display_name` never rides the GC->AC request. GC unchanged (no users repo added).
- MC (`meeting.rs`/`controller.rs`/`connection.rs`), GC — spin-out / out of scope.

### Tests
- **common/jwt.rs** (folds @test 3a/b/c): (a) extend a `MeetingTokenClaims` roundtrip test with a populated `display_name`; (b) BACKWARD-COMPAT lock — deserialize from a **literal JSON string that OMITS** `display_name` -> succeeds with `display_name == ""` (exercises `#[serde(default)]`; a struct roundtrip would NOT hit the default path); (c) Debug-redaction — update `test_meeting_token_claims_debug_redacts_pii` (jwt.rs:1754) with a sentinel like `"Secret Name"` and assert it is absent from Debug output. Non-Option field is compiler-enforced across all `MeetingTokenClaims` struct-literals in tests — give them meaningful values, not `""`.
- **ac-service internal_tokens.rs**: unit tests for the pure helper `resolve_meeting_display_name` — `Some(user) -> name`, `None -> Err(AcError::NotFound)` (fail-closed lock). Local serialize struct emits `display_name`. Update the `sign_meeting_jwt` test literal (:533) for the new field.
- **ac-service integration `internal_token_tests.rs`** (folds @test 1/2):
  - HAPPY-PATH POSITIVE (new, core behavior): seed a user via `create_test_user(org_id, email, pw, "Alice Example") -> user_id` (server_harness.rs:367), issue a meeting token with that `subject_user_id`, decode the JWT, assert `claims["display_name"] == "Alice Example"`. Mirror decode in `test_meeting_token_claims_structure` (:1027).
  - MISSING-USER (new): issue for a `subject_user_id` NOT in `users` -> assert `404` / `AcError::NotFound` (pins fail-closed).
  - RECONCILE EXISTING: `test_meeting_token_success` (:360) and `test_meeting_token_claims_structure` (:1036, uses `test_uuid(100)`) currently use un-seeded synthetic subjects; once issuance does a lookup they would 404. Update them to seed the subject via `create_test_user` first. (This is the compiler/DB-enforced consequence of fail-closed — do NOT weaken the policy to keep stale tests green.)
- Existing repo `get_by_id` sqlx test already proves `display_name` is returned.

### OPEN POLICY CONFLICT for @team-lead ruling — fail-closed vs degrade
@security (must-fix) + @test lean **fail-closed** on a missing row; @operations wants **degrade/fail-open** (omit claim, still issue) since `display_name` is cosmetic and issuance is on the join hot path. My recommendation is **fail-closed**, on verified facts that neutralize the ops premise:
- Ops premise (a) "cross-org/external subject may have no row here" is **false in the current single-cluster deployment**: `get_by_id` is keyed by `user_id` **only** (ops itself notes this), and all orgs' users share one `users` table, so an `External` subject (an AC-authenticated user, `sub` from an AC-issued token) **is** found. A `None` only arises under cross-**cluster** federation (not wired up) or a genuine data fault (deleted/corrupt id) — both of which fail-closed handles correctly (and a deleted user SHOULD be blocked).
- Ops premise (b) "transient DB blip" returns `Err(AcError::Database)`, not `None`, and is handled separately: issuance **already** hard-depends on the DB — it loads the signing key from `signing_keys` first (`get_active_key`, :143), so a DB-down request already fails before the users lookup. The added SELECT introduces **no new availability class**; degrading only it would give no resilience benefit while the signing-key load still hard-fails.
- Therefore: `None` -> fail-closed (`NotFound`); `Err` -> propagate (status quo). No degraded path, so no new "degraded" metric needed (failure recorded via existing `record_error`).
This is the one item blocking a final plan; requesting @team-lead adjudication. I will implement whatever the Lead rules; if ruled "degrade," I switch the helper to return `Option<String>`, add `#[serde(skip_serializing_if = "String::is_empty")]` on the local struct, and add a warn-log + counter per @operations/@observability.

### Observability / metrics
No new metric. The new failure path reuses `record_error("issue_meeting_token", ...)` via the existing handler match on `Err` (a `NotFound` maps to category/status 404). No user_id/display_name enters spans or logs (handler is `skip_all`; error message is generic).

### Spin-out to record
MC consumption (replace `meeting.rs:604` placeholder with token-carried `display_name`, thread through `join_connection`/`ControllerMessage::JoinConnection`) -> `docs/TODO.md`, owner meeting-controller.

---

## Pre-Work

None.

---

## Accepted Deferrals

- `docs/TODO.md` §Roster Display Names — `lookup_error` metric-emission assertion deferred (DB-error injection fragile in `sqlx::test`); observability-accepted
- `docs/TODO.md` §"Guard Precision — no-secrets-in-logs false-positive on `jwt` vocabulary" — spun-out (code-reviewer): narrow the guard, then remove the 15 interim `guard:ignore` lines in `jwt.rs` (owner: semantic-guard/security)
- `docs/TODO.md` §Cross-Service Duplication #53 — DEEPENED (dry-reviewer): AC-local vs `common::jwt` `MeetingTokenClaims` gained the `display_name` lockstep field (DRY exception; not fix-or-defer)

**Scope-decision pointers (tracked forward work, not findings-in-diff):**
- `docs/TODO.md` §Roster Display Names — MC-consumption spin-out (meeting-controller owner-implements); end-to-end roster acceptance is gated on it
- `docs/TODO.md` §Roster Display Names — revisit meeting-token display-name fail-closed when cross-cluster federation is wired

## Implementation Notes (ruling applied: FAIL-CLOSED)

Team ruling: FAIL-CLOSED on a missing `users` row (operations withdrew its degrade ask; consensus). Ops's three in-changeset conditions folded in: (1) `AcError::NotFound` -> 404 (not opaque 500); (2) distinct warn + bounded counter on the `None` path (`outcome="user_not_found"`) vs the DB-error path (`outcome="lookup_error"`), new metric `ac_meeting_token_display_name_total` with a CLOSED `outcome` enum {resolved,user_not_found,lookup_error}, no PII; (3) code comment at the fail-closed site + `docs/TODO.md` note bounding it to the single-cluster assumption.

### Files changed
- `crates/common/src/jwt.rs` (GSA): `#[serde(default)] display_name` on `MeetingTokenClaims`, Debug redaction, doc update; +backward-compat & Debug-redaction tests.
- `crates/ac-service/src/handlers/internal_tokens.rs`: users lookup + fail-closed `resolve_meeting_display_name` helper + warn/metric; `display_name` on the AC-local serialize struct; helper + local-struct unit tests.
- `crates/ac-service/src/observability/metrics.rs`: `record_meeting_display_name_outcome`.
- `crates/ac-service/src/repositories/users.rs`: dropped stale `#[allow(dead_code)]` on `get_by_id`.
- `crates/ac-service/tests/integration/internal_token_tests.rs`: `seed_meeting_subject` helper; seeded the 7 success-path meeting-token tests; happy-path display_name assertions; new `test_meeting_token_missing_user_fails_closed` (404).
- `crates/ac-service/tests/internal_token_metrics_integration.rs`: seeded the success case; asserts `outcome="resolved"`.
- Forced mechanical compile-fixes (shared-type field added; non-GSA test builders): `crates/mc-service/src/auth/mod.rs`, `crates/mh-service/src/auth/mod.rs`, `crates/mh-service/tests/common/tokens.rs`, `crates/mc-test-utils/src/jwt_test.rs`.

---

## Rollback Procedure

Start commit `78cc06344863dca2ff7da7af61bc1f0435f1ead7`. `git reset --hard` restores; no schema change to reverse.
