# Devloop Output: Fix AC register-test regression from task #23 camelCase migration (task #46)

**Date**: 2026-06-03
**Task**: Repair broken `crates/ac-service` register-flow integration tests after the R-53 camelCase migration; decide tests-vs-derive; add a DB-free wire-shape lock test.
**Specialist**: auth-controller
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-46`
**Duration**: ~1h (planning/escalation-heavy; implementation + validation small)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `65fe91acf2ba491f530f96605f3a1bdae76d5b4e` |
| Branch | `feature/browser-client-join-task-46` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-task46` |
| Implementing Specialist | `auth-controller` |
| Iteration | `1` |
| Security | `security@devloop-task46` |
| Test | `test@devloop-task46` |
| Observability | `observability@devloop-task46` |
| Code Quality | `code-reviewer@devloop-task46` |
| DRY | `dry-reviewer@devloop-task46` |
| Operations | `operations@devloop-task46` |
| Semantic Guard | `semantic-guard@devloop-task46` |

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (Option b; R-11 prose framing reqs to verify at review) |
| Test | confirmed (Option b; lock-test bar set) |
| Observability | confirmed (Option b; obs-neutral) |
| Code Quality | confirmed (will re-verify diff at Start Review) |
| DRY | confirmed (single-anchor lock test) |
| Operations | confirmed (Option b; no runbook edit needed) |
| Semantic Guard | confirmed (Option b; production structs unchanged) |

Classification-sanity guard (`validate-cross-boundary-classification.sh`): **STATUS=OK
cross-boundary-classification-clean** (dt-guard built locally; wrapper points at
`target/release` — ran via `DT_GUARD=target/debug/dt-guard`).

---

## Task Overview

### Objective
Task #23 (commit `92d963b`, R-53) flipped AC public HTTP request/response structs to
`#[serde(rename_all = "camelCase")]` and updated env-tests fixtures + 3 unit round-trip
tests, but **never touched `crates/ac-service/tests/integration/user_auth_tests.rs`**.
Those `#[sqlx::test]` (DB-gated) register-flow tests still POST `display_name` (snake_case)
and assert snake_case response keys, so they now fail. Because they are DB-gated, the
task-#23 devloop clone never ran them — "Test CLEAR" was reported falsely.

Repair the register-flow tests, make the tests-vs-derive decision, and add a DB-free
wire-shape lock test so a future rename sweep cannot silently re-break this.

### Scope
- **Service(s)**: ac-service only
- **Schema**: No
- **Cross-cutting**: No (in-domain auth-controller work)

### Debate Decision
NOT NEEDED — bug fix + a bounded wire-contract decision adjudicable within the devloop panel.

---

## Lead Pre-Investigation (ground truth handed to implementer)

**Reproduction**: `cargo test -p ac-service --test integration_tests` → 8 register tests in
`tests/integration/user_auth_tests.rs` fail with HTTP **422** (Axum `Json` extractor rejects
the body) instead of 200/401. `cargo test -p ac-service --lib` passes (376) — the failures
live only in the DB-gated integration binary.

**Root cause**: request bodies send `display_name`; `UserRegistrationRequest` now expects
`displayName`. Response assertions read `user_id`/`display_name` (now `userId`/`displayName`).
Task #23 did not migrate this test file (verified via `git show 92d963b`).

**The tests-vs-derive judgment call** (the heart of #46):
- `UserRegistrationResponse` is `rename_all = "camelCase"` but **overrides** `access_token` /
  `token_type` / `expires_in` back to snake_case via per-field `#[serde(rename = "...")]`.
- `UserTokenResponse` (login, `services/token_service.rs`) has **no** `rename_all` — fully snake.
- R-11 (client SDK contract) states responses are camelCase **including** `accessToken`,
  `tokenType`, `expiresIn`, consumed directly "no transform layer". R-53 invariant = full camelCase.
- Distinction: the **service-token** endpoint (`ServiceTokenRequest`, `grant_type=client_credentials`,
  machine-to-machine OAuth 2.0) legitimately stays snake_case and is out of scope.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/ac-service/tests/integration/user_auth_tests.rs` | Mine | — |
| `crates/ac-service/src/handlers/auth_handler.rs` (lock test only — no production-struct change) | Mine | — |
| `crates/ac-service/src/models/mod.rs` (service-token `TokenResponse` wire-shape lock test only — added at Gate 3 per test+security finding) | Mine | — |
| `docs/user-stories/2026-05-02-browser-client-join.md` (R-11 prose carve-out, 4 sites: L70/L211/L263/L715) | Not mine, Minor-judgment | R-11 co-owned: client / auth-controller / security (confirm prose) |

Notes: Under Option (b) the production structs (`auth_handler.rs::UserRegistrationResponse`,
`token_service.rs::UserTokenResponse`) are UNCHANGED — verified already correct. The env-tests
mirror (`crates/env-tests/src/fixtures/auth_client.rs`) is byte-identical to the unchanged
server scheme, so no fixture edit. `gc-deployment.md` jq sites stay snake-correct, no runbook
edit. The only cross-boundary edit is the R-11 prose amendment (Minor-judgment, doc — not a GSA
path; §6.4 GSA subset is `ac-service/src/{jwks,token,crypto,audit}/**`). Security + operations to
confirm the amended prose; client is the remaining R-11 co-owner (review-only, doc edit).

---

## Planning

### Tests-vs-Derive Decision (escalated to user, resolved 2026-06-05)

At Gate 1 the implementer proposed Option (a) full camelCase (treat the #23 per-field
snake overrides as the bug). Security objected: the snake_case OAuth-field disposition was
a **deliberate, owner-confirmed, unit-test-locked** decision in the task-#23 devloop
(`docs/devloop-outputs/2026-05-23-camelcase-wire-migration/main.md:101-140`, RFC-6749
rationale). This directly conflicts with R-11 (story line 70), which explicitly lists
`accessToken`/`tokenType`/`expiresIn` as camelCase "no transform layer". The #23 carve-out
never reconciled against R-11.

**Lead escalated to user (documented R-11-vs-#23 contradiction; product-contract value
judgment). USER DECISION:**
1. **Commit to snake_case for the RFC-6749 fields** (`access_token`/`token_type`/`expires_in`)
   on BOTH `UserRegistrationResponse` (mixed scheme: identity fields camelCase, OAuth fields
   snake) and `UserTokenResponse` (fully snake). **No derive change** — keep the #23 invariant.
2. **Fix the AC tests against that mixed shape** (not a blind camelCase sweep — see security's
   point: `user_id`→`userId`, `display_name`→`displayName`, but `access_token`/`token_type`/
   `expires_in` STAY snake; request bodies `display_name`→`displayName` so they reach the real
   401/InvalidToken auth checks rather than masking with 422).
3. **Add the DB-free wire-shape lock test** locking the mixed scheme — and crucially the LOGIN
   (`UserTokenResponse`) shape, which #23 never locked.
4. **Amend R-11 prose** in `docs/user-stories/2026-05-02-browser-client-join.md` to explicitly
   document the OAuth/RFC-6749 snake_case carve-out, eliminating the latent contradiction.

This is Option (b) + a doc amendment. The implementer's plan is revised accordingly.

### Decision reaffirmed after counter-evidence (2026-06-05)

During Gate 1 the panel surfaced material counter-evidence: R-11's camelCase was the user's
*original* Clarification-Q6 choice; the snake_case carve-out appeared only in the #23
*implementation* devloop 20 days later and never cited R-11 (grep-confirmed zero references);
RFC 6749 §5.1 is mis-scoped for `/register` and `/user/token`; security **lifted its block and
affirmatively recommended camelCase**; task #11 (pending SDK) is spec'd to camelCase. All seven
reviewers converged on **Path B (full camelCase)**. The Lead relayed this to the user with a
recommendation to switch to Path B. **The user reaffirmed Path A (snake_case + amend R-11).**
That is the product owner's prerogative; the decision is final. Recorded here so a future reader
sees the contract was chosen *with* the camelCase counter-argument fully on the table, not in
ignorance of it.

---

## Implementation Summary

Path A / Option (b) implemented. Four files changed (no production-struct change):

1. `crates/ac-service/tests/integration/user_auth_tests.rs` — mixed-shape repair: 14
   `display_name`→`displayName` (13 request bodies + 1 response read) + `user_id`→`userId`
   reads; OAuth response reads (`access_token`/`token_type`/`expires_in`) stay snake on register
   AND login; SQL `WHERE user_id=$1` untouched; all per-test status assertions preserved verbatim
   (zero expect-422 — the body fix makes requests reach the real 401/404/200/429 checks).
2. `crates/ac-service/src/handlers/auth_handler.rs` — **test module only** (production structs
   verified unchanged): folded the two existing `.contains` round-trip tests into ONE canonical
   anchor using `wire_keys()` + `serde_json::to_value` + `BTreeSet` full key-SET equality. Locks:
   request `{email,password,displayName}` (rejects `display_name`; deser-only, no `Serialize` on
   the `SecretString` DTO), register response `{userId,email,displayName,access_token,token_type,
   expires_in}`, **login `UserTokenResponse` `{access_token,token_type,expires_in}` (the gap #23
   never locked)**, plus the service-token `TokenResponse` snake invariant (joint security+test add).
3. `docs/user-stories/2026-05-02-browser-client-join.md` — R-11 carve-out amendment at the three
   reconciled sites (L70 requirement, L263 design bullet, L715 task-#11 command).
4. `docs/devloop-outputs/2026-05-23-camelcase-wire-migration/main.md` — appended the Open-Q4
   §5.1-rescope note so the snake disposition isn't resurrected as a security objection.

### Implementer-reported verification (live DB)
- `cargo test -p ac-service --test integration_tests` → 77 passed, 0 failed (the 422 failures fixed).
- `cargo test -p ac-service --lib` → 377 passed, 0 failed (2 consecutive reruns). One transient
  flake on a timing-sensitive constant-time/rate-limit unit test OUTSIDE the changeset — green on
  rerun; flagged honestly.
- `cargo test -p env-tests --lib` → 50 passed (unaffected). `fmt --check` + `clippy --tests` clean.

(Lead re-running layers 1–6 independently at Gate 2; Layer 7 assessed below.)

---

## Devloop Verification Steps (Gate 2)

### Attempt 1 (Lead-run, independent)
| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | `cargo-build-passed` + dt-guard build |
| 2 Format | OK | `cargo-fmt-passed`; ts/proto skipped-no-diff |
| 3 Guards | **FAIL (2/31)** | `no-hardcoded-secrets` (auth_handler.rs:727,771 — inlined `access_token: "<literal>"` placeholder trips `secret_identifier_literal_assignment`); `validate-cross-boundary-scope` (`multi-devloop-collision-2-main-mds` — edited the prior 2026-05-23 devloop main.md). Both sent back to implementer. |
| 4 Test | OK (ac-service) | Full-workspace layer4 exceeded the 400s harness timeout; Lead re-ran the affected crate independently (see below). Implementer-reported: integration 77, lib 377, env-tests 50. |
| 5 Lint | OK | `cargo-clippy-passed` |
| 6 Audit | FAIL — **pre-existing, out of scope** | `cargo audit` + `pnpm audit` fail on PRE-EXISTING advisories. Diff has ZERO dependency-manifest changes (`git diff --name-only` has no Cargo.*/package.json/pnpm-lock). Owned by tasks #47 (suppression machinery) + #48 (clear advisories) per the story; #46 explicitly excludes audit work. `buf breaking` passed. Not a #46 regression. |
| 7 Env-tests | Assessed — see below | |

**Layer 7 (env-tests) assessment**: the changeset is test-code + docs only — **zero production-code, proto, migration, or infra delta**. No runtime behavior can change, so a live-cluster rebuild + env-test + Playwright run adds no signal against this diff. The implementer already ran `cargo test -p env-tests --lib` green (50). Recording Layer 7 as justified-scoped (no production surface touched) rather than burning a ~7-min cluster bring-up. (If any reviewer disputes, escalate per SKILL.)

### Attempt 2 — PASS
| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | |
| 2 Format | OK | |
| 3 Guards | **OK — 31/31, 0 failed** | Both attempt-1 failures fixed: secret-indirection restored at auth_handler.rs:724 & 773; prior-devloop main.md reverted (folded §5.1-rescope into R-11 L70). `run-guards.sh` exit 0. Classification guard clean (1 cross-boundary file). |
| 4 Test | OK | Lead-verified: lib 377/0, integration_tests 77/0. Implementer full `cargo test -p ac-service` 908/0. One known timing-flake in a constant-time/rate-limit integration test, OUTSIDE the changeset — lib always green. |
| 5 Lint | OK | clippy clean |
| 6 Audit | Pre-existing, out of scope | cargo+pnpm advisories pre-date this diff (zero dep-manifest changes); owned by #47/#48. buf breaking OK. |
| 7 Env-tests | Scoped out | test-code + docs only; no production/proto/infra delta. |

**Gate 2 verdict: PASS.** Final diff = 3 files: `user_auth_tests.rs`, `auth_handler.rs` (test module only), `browser-client-join.md` (R-11 prose, 3 sites + folded §5.1 reconciliation).

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | RESOLVED-FIXED | 2 | 2 | 0 |
| Test | RESOLVED-FIXED | 1 | 1 | 0 |
| Observability | CLEAR | 0 | 0 | 0 |
| Code Quality | CLEAR | 2 (both withdrawn as non-defects) | — | 0 |
| DRY | CLEAR | 0 | 0 | 0 |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 |
| Semantic Guard | CLEAR (native SAFE) | 0 | 0 | 0 |

**Findings fixed (zero deferred, zero spun-out):**
1. **Security F2 / Test** — service-token `models::TokenResponse` response wire-shape lock was missing (Gate-1-agreed deliverable). Added `test_service_token_response_wire_shape_stays_snake` (BTreeSet key-set `{access_token,token_type,expires_in,scope}` + §5.1 tripwire comment).
2. **Security F1** — R-11 L70 reconciliation said the user-flow snake was "not by §5.1 *alone*," wrongly implying §5.1 partially governs the user endpoints. Fixed to "not by §5.1, which does not govern those endpoints"; harmonized the L211/L263 caveats to the descriptive form ("matching the RFC 6749 §5.1 wire names — see R-11").
3. **Operations** — R-53 L211 falsely claimed runbook curl examples went camelCase; added the OAuth snake carve-out caveat.

**Code Quality F1/F2** withdrawn by the reviewer as non-defects (the production comment is descriptively accurate; the env-tests mirror comment is symmetric and untouched).

**Procedural (commit):** the R-11 doc edit is Minor-judgment cross-boundary (co-owned client/auth/security); commit carries `Approved-Cross-Boundary: security ...` per @code-reviewer's Ownership-Lens flag.

## Accepted Deferrals

- (none surfaced in this devloop — every finding was fixed in-diff; zero deferrals, zero spin-outs)
