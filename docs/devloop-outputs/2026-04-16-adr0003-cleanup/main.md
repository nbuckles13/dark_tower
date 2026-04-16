# Devloop Output: ADR-0003 Cleanup

**Date**: 2026-04-16
**Task**: Replace legacy scope strings in test fixtures with semantic placeholders; delete dead test_ids.rs constants; update ADR-0005 illustrative examples; stamp ADR-0003 Scope Contract Tests row as Done
**Specialist**: auth-controller
**Mode**: Agent Teams (light)
**Branch**: `feature/mh-quic-mh-notify`
**Duration**: in progress

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `59f6a388abfd845f46131e348108a3a69633cc63` |
| Branch | `feature/mh-quic-mh-notify` |
| Slug | `2026-04-16-adr0003-cleanup` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Commit | `d74d1e0` |
| Implementer | `implementer@adr0003-cleanup` |
| Implementing Specialist | `auth-controller` |
| Iteration | `1` |
| Security | `security@adr0003-cleanup` |
| Test (context) | `test@adr0003-cleanup` |

---

## Task Overview

### Objective

Post-ADR-0003-rollout cleanup. Four items:

1. **Rename arbitrary-scope test fixtures**: legacy scope strings (`meeting:create`, `meeting:read`, `meeting:update`, `meeting:list`, `media:process`, `media:forward`, `media:receive`, `media:send`, `session:manage`) appearing in test code as arbitrary test data should be renamed to semantic placeholders:
   - `valid_scope` / `invalid_scope` — where the test asserts accept/reject behavior
   - `scope_a` / `scope_b` / `scope_c` — where the test needs multiple *distinct* scopes (subset-matching tests)
   - SQL injection test at `crates/ac-service/src/services/registration_service.rs:262`: preserve the injection payload, change only the prefix (e.g., `scope_a'; DROP TABLE auth_events; --`)

2. **Delete dead constants**: `SCOPE_MEETING_CREATE`, `SCOPE_MEETING_READ`, `SCOPE_ADMIN_SERVICES` at `crates/ac-test-utils/src/test_ids.rs:35-37` are unused (grep-verified zero callers).

3. **Update ADR-0005 illustrative examples**: `docs/decisions/adr-0005-integration-testing-strategy.md` has code snippets at lines ~160, 187, 232, 495, 500 using `meeting:create` / `user.read.gc`. Replace with ADR-0003-compliant scopes (e.g., `service.write.mc`) so the ADR teaches current conventions.

4. **Stamp ADR-0003 status row**: `docs/decisions/adr-0003-service-authentication.md:741` — change "Scope Contract Tests ❌ Pending" to "✅ Done | `2c2613c`". Tests exist at `crates/ac-service/src/models/mod.rs:301-343` (5 tests covering all caller→target pairs).

### Non-goals

- **Do NOT touch load-bearing scope strings**. The contract tests at `models/mod.rs:301-343` MUST keep the real `service.write.{target}` strings — that's the whole point of those tests. Production source at `models/mod.rs:default_scopes()` and seed SQL at `setup.sh:seed_test_data()` must remain untouched.
- Do NOT change test names or assertion structure. This is pure fixture vocabulary cleanup.
- Do NOT rewrite any test to add/remove assertions.

### Scope

- **Service(s)**: AC only (`crates/ac-service/`, `crates/ac-test-utils/`)
- **Schema**: No
- **Cross-cutting**: No (test fixtures and docs only; no production behavior change)
- **Security-critical**: Debatable — touches auth test code. `--light` eligibility: renames are mechanical and security reviewer is present. Security can escalate to full if concerned.

### Debate Decision

**NOT NEEDED** — this is a follow-up cleanup to the completed ADR-0003 rollout. No new design decisions.

---

## Implementation Summary

**Item 1 — Fixture renames** (~40 sites across 10 files):

Legacy scope strings (`meeting:create`, `meeting:read`, `meeting:update`, `meeting:list`, `media:process`, etc.) in test fixtures renamed to semantic placeholders:
- `valid-scope` / `invalid-scope` — accept/reject assertions
- `scope-a` / `scope-b` / `scope-c` — subset-matching tests needing distinct scopes

Implementer note: AC's scope validation regex (alphanumeric + hyphen + dot + colon; no underscores) rejected initial `valid_scope` attempt. Caught on first test run of `test_handle_update_client_success`, corrected via global rename to hyphens. Good error-surface feedback from the validation logic.

SQL injection test at `crates/ac-service/src/services/registration_service.rs:262` preserved — only prefix changed: `scope-a'; DROP TABLE auth_events; --`. Injection escape-check still exercised.

Load-bearing scope strings **not touched**:
- `crates/ac-service/src/models/mod.rs:default_scopes()` — production source of truth
- `crates/ac-service/src/models/mod.rs:301-343` — contract tests (real `service.write.{target}` strings required)
- `infra/kind/scripts/setup.sh:seed_test_data()`
- `admin:services` and `internal:meeting-token` scope literals in admin/internal-token tests (those tests gate on the specific scope, so the scope name is semantically load-bearing)

**Item 2 — Dead constants deleted**: `SCOPE_MEETING_CREATE`, `SCOPE_MEETING_READ`, `SCOPE_ADMIN_SERVICES` removed from `crates/ac-test-utils/src/test_ids.rs`. Grep-verified zero callers.

**Item 3 — ADR-0005 illustrative examples**: `docs/decisions/adr-0005-integration-testing-strategy.md` — 5 code snippets updated to use `service.write.mc` instead of `meeting:create` / `user.read.gc`. One example that mixed both collapsed to `service.write.mc` (the ADR-0003-idiomatic choice).

**Item 4 — ADR-0003 status row stamp**: `docs/decisions/adr-0003-service-authentication.md:741` — "Scope Contract Tests" flipped to ✅ Done with commit `2c2613c` and pointer to `crates/ac-service/src/models/mod.rs:301-343`.

---

## Files Modified

```
 crates/ac-service/src/handlers/admin_handler.rs               | 16 ++---
 crates/ac-service/src/repositories/service_credentials.rs     | 32 +++++-----
 crates/ac-service/src/services/registration_service.rs        |  2 +-
 crates/ac-service/src/services/token_service.rs               | 72 +++++++++++-----------
 crates/ac-service/tests/README.md                             |  2 +-
 crates/ac-service/tests/integration/admin_auth_tests.rs       |  7 +--
 crates/ac-service/tests/integration/internal_token_tests.rs   |  6 +-
 crates/ac-service/tests/integration/key_rotation_tests.rs     |  2 +-
 crates/ac-test-utils/src/assertions.rs                        | 14 ++---
 crates/ac-test-utils/src/test_ids.rs                          |  5 --
 crates/ac-test-utils/src/token_builders.rs                    |  6 +-
 docs/decisions/adr-0003-service-authentication.md             |  2 +-
 docs/decisions/adr-0005-integration-testing-strategy.md       | 10 +--
 docs/devloop-outputs/2026-04-16-adr0003-gc-auth-scopes/main.md|  4 +-
 14 files changed, 87 insertions(+), 93 deletions(-)
```

---

## Devloop Verification Steps

| Layer | Result | Notes |
|-------|--------|-------|
| 1. `cargo check --workspace` | PASS | Clean |
| 2. `cargo fmt --all` | PASS | No changes introduced |
| 3. Guards (15 total) | PASS | All pass |
| 4. Workspace tests | PASS | ac-service 838 tests + ac-test-utils 17 tests all pass; one transient failure on first workspace-wide run (known flake), clean on retry |
| 5. Clippy `-D warnings` | PASS | Clean |
| 6. `cargo audit` | PASS (no delta) | No Cargo.toml/Cargo.lock changes |
| 7. Semantic guard | SAFE | Contract tests untouched, production code untouched, assertion shape preserved, distinct scopes stay distinct |
| 8. Env-tests | PASS | 50 + others all pass. `dev-cluster rebuild-all` skipped — zero production binaries changed in this diff, cluster from previous devloop still serving identical binaries |

---

## Code Review Results

| Reviewer | Verdict |
|----------|---------|
| Security | **CLEAR** — contract tests untouched, production sources of truth unchanged, SQL injection payload preserved, zero production attack surface touched. Light mode deemed appropriate. |
| Test | **CLEAR** — test counts match baseline, distinctness preserved in all 5 subset-matching sites, ADR-0005 snippets semantically coherent, dead consts cleanly removed. |

Zero findings. Zero deferrals. Zero escalations.

---

## Tech Debt

No new tech debt introduced. This devloop retired pre-existing inconsistencies (legacy scope strings in test fixtures, dead constants, out-of-date ADR illustrative examples, stale ADR-0003 status row).

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Start commit: `59f6a388abfd845f46131e348108a3a69633cc63`
2. Review: `git diff 59f6a38..HEAD`
3. Soft reset: `git reset --soft 59f6a38`
4. Hard reset: `git reset --hard 59f6a38`

---

## Reflection

Skipped per `--light` mode.
