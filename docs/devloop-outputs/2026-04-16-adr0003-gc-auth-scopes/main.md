# Devloop Output: GC Two-Layer gRPC Auth (ADR-0003)

**Date**: 2026-04-16
**Task**: Add `service.write.gc` scope enforcement + Layer 2 `service_type` URI-path routing to GC's gRPC auth layer
**Specialist**: global-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/mh-quic-mh-notify`
**Duration**: in progress

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `6b9f41b931970657ab52af5f511bc88fabf3ab39` |
| Branch | `feature/mh-quic-mh-notify` |
| Slug | `2026-04-16-adr0003-gc-auth-scopes` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Main commit | `dc26e39` |
| Hash-stamp commit | `59f6a38` |
| Implementer | `implementer@gc-auth-scopes` |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |
| Security | `security@gc-auth-scopes` |
| Test | `test@gc-auth-scopes` |
| Observability | `observability@gc-auth-scopes` |
| Code Quality | `code-reviewer@gc-auth-scopes` |
| DRY | `dry-reviewer@gc-auth-scopes` |
| Operations | `operations@gc-auth-scopes` |

### Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

**Gate 1: PLAN APPROVED** (2026-04-16) — all 6 reviewers confirmed; sent "Plan approved" to implementer.

---

## Task Overview

### Objective

Complete the ADR-0003 two-layer gRPC auth rollout on GC. MC (commit `2c2613c`) and MH (commit `6b9f41b`) already have it; GC is the last service.

Scope enforcement + URI-path routing on `GrpcAuthLayer` at `crates/gc-service/src/grpc/auth_layer.rs`:

- **Layer 1 (scope)**: require `service.write.gc` on every gRPC request.
- **Layer 2 (service_type routing)**:
  - `/dark_tower.internal.GlobalControllerService/*` → `service_type == "meeting-controller"`
  - `/dark_tower.internal.MediaHandlerRegistryService/*` → `service_type == "media-handler"`
  - Missing `service_type` → fail closed with `PERMISSION_DENIED`
  - Unknown gRPC path → fail closed with `PERMISSION_DENIED`

Mirror the MC (`crates/mc-service/src/grpc/auth_interceptor.rs`) and MH (`crates/mh-service/src/grpc/auth_interceptor.rs`) implementations for structural consistency and to set up the eventual extraction into `common` (DRY tech-debt entry already tracked).

### Scope

- **Service(s)**: GC only (`crates/gc-service/`)
- **Schema**: No
- **Cross-cutting**: No (self-contained within GC's auth layer)

### Debate Decision

**NOT NEEDED** — Two-layer design already decided in `docs/debates/2026-04-16-grpc-auth-scopes/debate.md`, ratified in ADR-0003 Component 6. This devloop is implementation of the decided design.

### Requirements

1. **Layer 1 scope check**: add `REQUIRED_SCOPE = "service.write.gc"` constant; reject token with `UNAUTHENTICATED` if missing (matches MC/MH pattern).
2. **Layer 2 routing**: URI-path allowlist with two entries (`GlobalControllerService` → `meeting-controller`, `MediaHandlerRegistryService` → `media-handler`). Unknown paths and missing `service_type` fail closed with `PERMISSION_DENIED`.
3. **Metrics**:
   - Add `gc_jwt_validations_total{result, token_type, failure_reason}` (new — GC currently has no JWT validation counter) with `classify_jwt_error()` helper matching MC/MH.
   - Add `gc_caller_type_rejected_total{grpc_service, expected_type, actual_type}` with `record_caller_type_rejected()` helper.
   - Any non-zero `caller_type_rejected` value is a bug in production (per ADR-0003).
4. **Claims injection**: already injects `ValidatedClaims(Claims)` into `http::Request` extensions — keep that behavior. Decide whether to keep GC's `auth::Claims` type or migrate to `common::jwt::ServiceClaims` (note: GC's `Claims` has an extra `sub` field and is used by HTTP user-auth middleware too, so migration may not be in scope — implementer to decide and justify).
5. **Dead-code cleanup**: remove the legacy synchronous `GrpcAuthInterceptor` + `PendingTokenValidation` (precedent: MC removed `McAuthInterceptor`, MH removed `MhAuthInterceptor`). Update `crates/gc-service/src/grpc/mod.rs` export.
6. **Tests**: mirror MC/MH Layer 2 test suite:
   - Missing auth header → `UNAUTHENTICATED`
   - Invalid Bearer format → `UNAUTHENTICATED`
   - Empty / oversized token → `UNAUTHENTICATED`
   - Invalid signature / expired → `UNAUTHENTICATED`
   - Wrong scope → `UNAUTHENTICATED`
   - Valid MC token to `GlobalControllerService` → pass
   - Valid MH token to `MediaHandlerRegistryService` → pass
   - MC token to `MediaHandlerRegistryService` → `PERMISSION_DENIED`
   - MH token to `GlobalControllerService` → `PERMISSION_DENIED`
   - No `service_type` → `PERMISSION_DENIED` (fail closed)
   - Unknown gRPC path → `PERMISSION_DENIED`
   - Claims injected into extensions
7. **ADR-0003 status table update**: flip to ✅ Done with commit hashes:
   - ADR-0003 Scope Alignment → `2c2613c`
   - Two-Layer gRPC Auth (MC) → `2c2613c`
   - Two-Layer gRPC Auth (MH) → `6b9f41b`
   - Two-Layer gRPC Auth (GC) → **this commit**

### Non-goals

- Migrating GC to `common::jwt::ServiceClaims` (unless trivial)
- Scope Contract Tests (separate pending ADR-0003 item, not this devloop)
- Extracting the now-duplicated auth layer pattern into `common` (DRY tech-debt, waits for third service)

---

## Planning

Gate 1 passed in one round. Implementer drafted an approach that mirrored MC (`2c2613c`) and MH (`6b9f41b`) faithfully and reviewers concurred, with the following reviewer-confirmed design calls:

- **Keep GC's local `auth::Claims`** (not migrate to `common::jwt::ServiceClaims`). `Claims` already has `scope: String`, `has_scope()`, and `service_type: Option<String>` — sufficient for both layers. The validator is shared with HTTP user-auth middleware; migration would widen blast radius and is out of scope. Endorsed by Security, Code Quality, and Observability.
- **Inject bare `Claims` (drop `ValidatedClaims` wrapper)**: no current consumer reads it, and the HTTP handler at `middleware/auth.rs:112` already reads `.extensions().get::<Claims>()` — bare is the simpler, consistent shape.
- **Flatten the `async_auth` submodule**: removes a useless nesting layer that predated the single-stage design. No backwards-compat concern.
- **Remove dead `GrpcAuthInterceptor` + `PendingTokenValidation`**: both were `#[allow(dead_code)]` alternate APIs. Precedent: MC/MH already removed equivalents.
- **Add `validate_raw()` on GC's `JwtValidator`**: the auth layer needs the raw `JwtError` variant for bounded metric `failure_reason` classification, while the existing `validate()` that returns `GcError` is preserved for HTTP callers. This is a narrow adapter, not a reimplementation.
- **Mirror MC's test suite verbatim** with GC-specific additions (`test_auth_layer_rejects_unknown_kid`, `test_auth_layer_rejects_empty_scope`, `test_auth_layer_rejects_mismatched_scope_tokens`) that exceed MC's baseline — explicitly requested by the Test reviewer during planning.
- **Real proto RPC names in tests**: `/dark_tower.internal.GlobalControllerService/RegisterMC` and `/dark_tower.internal.MediaHandlerRegistryService/RegisterMH` (verified against `proto/internal.proto`).

All six reviewers confirmed the plan within ~3 minutes; no revision rounds needed.

---

## Implementation Summary

### `crates/gc-service/src/grpc/auth_layer.rs` (rewrite)

Single-module, two-layer `GrpcAuthLayer`/`GrpcAuthService` mirroring MC/MH:

| Step | Check | On failure |
|------|-------|------------|
| Structural | `Authorization` header present, valid ASCII, `Bearer ` prefix, non-empty, ≤ `MAX_JWT_SIZE_BYTES` | `UNAUTHENTICATED` (no metric — matches MC/MH) |
| Cryptographic | `validate_raw()` via JWKS, EdDSA signature, exp/iat with clock skew | `UNAUTHENTICATED`; `gc_jwt_validations_total{result="failure", failure_reason}` |
| **Layer 1 — scope** | `claims.has_scope("service.write.gc")` | `UNAUTHENTICATED`; `failure_reason="scope_mismatch"` |
| **Layer 2 — routing** | URI path → expected `service_type` allowlist | `PERMISSION_DENIED`; `gc_caller_type_rejected_total` |
| Inject | Bare `Claims` into `http::Request` extensions | — |

Layer 2 allowlist:
- `/dark_tower.internal.GlobalControllerService/*` → `service_type == "meeting-controller"`
- `/dark_tower.internal.MediaHandlerRegistryService/*` → `service_type == "media-handler"`
- Unknown path or `service_type == None` → fail closed

`classify_jwt_error()` helper maps `JwtError` variants → bounded `failure_reason` label (`malformed | signature_invalid | expired | none | scope_mismatch`), identical to MC/MH mappings.

Dead code deleted: `GrpcAuthInterceptor`, `PendingTokenValidation`, `ValidatedClaims`, `async_auth` submodule.

Tests (19 total): structural rejection (missing auth, invalid Bearer, empty, oversized); cryptographic (invalid signature, unknown kid, expired); scope (wrong scope, empty scope, mismatched scope tokens, `required_scope_constant`); Layer 2 happy paths (MC→GC, MH→GC) and fail-closed cases (MC→MH registry, MH→GC controller, no service_type, unknown path); `disabled_skips_validation`; claims injection into extensions.

### `crates/gc-service/src/auth/jwt.rs` (+10 lines)

New `validate_raw(token) -> Result<Claims, JwtError>` method that returns the raw validator error, enabling the auth layer's `classify_jwt_error()` to produce bounded labels. Original `validate()` returning `GcError` is retained for HTTP call sites.

### `crates/gc-service/src/observability/metrics.rs` (+77 lines)

- `record_jwt_validation(result, token_type, failure_reason)` — emits `gc_jwt_validations_total`. Cardinality: 2 × 1 × 6 = 12.
- `record_caller_type_rejected(grpc_service, expected_type, actual_type)` — emits `gc_caller_type_rejected_total`. Cardinality ≤ 16 (2 × 2 × bounded actual values).
- Unit tests exercise full label domain for coverage.

### `crates/gc-service/src/grpc/mod.rs` (-2 lines)

Dead `GrpcAuthInterceptor` re-export removed.

### `crates/gc-service/src/main.rs` (1 line)

Import path updated for the flattened module. Wiring signature (`GrpcAuthLayer::new(jwt_validator)`) preserved, so no other callers changed.

### Documentation & dashboard

- `docs/observability/metrics/gc-service.md` — new "gRPC Auth Metrics (ADR-0003)" section.
- `infra/grafana/dashboards/gc-overview.json` — new "gRPC Auth (ADR-0003)" row with "JWT Validations by Result & Type" and "Caller Type Rejections" panels (red threshold at 1 on the rejections panel — any non-zero in prod is a bug).
- `docs/decisions/adr-0003-service-authentication.md` — Implementation Status table: Scope Alignment ✅ (`2c2613c`), MC ✅ (`2c2613c`), MH ✅ (`6b9f41b`), GC ✅ (this commit).
- `docs/TODO.md` — DRY entries refreshed (lines 15, 18, 28, 31) to reflect GC as the 3rd service sharing the auth-layer pattern; no new entries.

---

## Files Modified

```
 crates/gc-service/src/auth/jwt.rs                  |   10 +
 crates/gc-service/src/grpc/auth_layer.rs           | 1286 +++++++++----------
 crates/gc-service/src/grpc/mod.rs                  |    2 -
 crates/gc-service/src/main.rs                      |    2 +-
 crates/gc-service/src/observability/metrics.rs     |   77 ++
 docs/TODO.md                                       |    7 +-
 docs/decisions/adr-0003-service-authentication.md  |    8 +-
 docs/observability/metrics/gc-service.md           |   29 +
 docs/specialist-knowledge/*/INDEX.md               |   41 +-
 infra/grafana/dashboards/gc-overview.json          |  219 ++++
 16 files changed, 1001 insertions(+), 680 deletions(-)
```

Net-new code is small; the large line delta on `auth_layer.rs` is driven by the test suite expansion plus removal of dead code.

---

## Devloop Verification Steps

| Layer | Result | Notes |
|-------|--------|-------|
| 1. `cargo check --workspace` | PASS | Clean compile |
| 2. `cargo fmt --all` | PASS | No changes introduced |
| 3. Guards (15 total) | PASS | All 15 simple guards pass (api-version, no-pii, metrics validation, kustomize, knowledge-index, ...) |
| 4. Workspace tests | PASS | Every crate passes; GC lib: **276 passed, 0 failed** including 17 new auth layer tests |
| 5. Clippy `-D warnings` | PASS | No warnings |
| 6. `cargo audit` | PASS (no delta) | 5 pre-existing baseline vulns; no Cargo.toml/Cargo.lock changes in this changeset |
| 7. Semantic guard | SAFE | Credential leaks, fail-open, cardinality, blocking — all clear. Doc-only nit about `missing_token` label (matches MC/MH behavior) |
| 8. Env-tests | PASS (after infra fix) | 50 in 24_join_flow pass; all observability tests pass |

### Layer 8 infrastructure notes (not code issues)

- **Stale postgres seed data**: Pre-existing Kind cluster had `{meeting:create,meeting:read,meeting:update,internal:meeting-token}` legacy scopes for GC. Remediated by `UPDATE service_credentials SET scopes = ARRAY['service.write.mc','internal:meeting-token'] WHERE client_id='global-controller'` + rolling restart to force token refresh. **Correction (per Operations review)**: `setup.sh:453-466` already uses `ON CONFLICT (client_id) DO UPDATE`, so seeding is idempotent. The actual mechanism for the stale data is unclear — possibly a postgres volume that persisted across a `kind delete` from a prior devloop session running pre-`2c2613c` code, or seed_test_data didn't run in that earlier setup. Not a gap in current seeding logic; no code follow-up needed.
- **Loki readiness**: Loki's `/ready` returns 200 with body `"Ingester not ready: waiting for 15s after being ready"` during a ~15-30s post-startup window. Test retry after full readiness passed. Known infrastructure behavior per ADR-0030.

---

## Code Review Results

### Running tally

| Reviewer | Verdict |
|----------|---------|
| Security | **CLEAR** — validation order, fail-closed, PII-free logs/metrics, dead-code removal all verified; no findings |
| Test | **CLEAR** — 19 tests pass; all 7 plan findings addressed |
| Observability | **CLEAR** — metrics/dashboard/tracing all ADR-compliant; one non-blocking parity note on unbounded `actual_type` label that applies to MC/MH equally |
| Code Quality | **CLEAR** — ADR-0003, ADR-0002, ADR-0011, ADR-0019 all compliant; no findings sent to implementer |
| DRY | **CLEAR** — 0 true duplications, 4 tech-debt observations all covered by existing TODO.md entries (lines 15, 18, 28, 31 updated) |
| Operations | **CLEAR** — dashboard/catalog/ADR status correct; no env-var or manifest changes; clean rollback. Corrected my infra note: `setup.sh` already uses `ON CONFLICT DO UPDATE`. Optional follow-ups: GC alert parity with MC, gc-service runbook |

---

## Tech Debt

No deferred findings (all reviewers returned CLEAR with zero findings). DRY observations are all covered by existing `docs/TODO.md` entries — no new entries added.

### Extraction opportunities observed (tech debt, not blocking)

All three services (GC, MC, MH) now share the structurally identical 5-step auth chain. Extraction to `common::grpc_auth` remains blocked by the `auth::Claims` vs `common::jwt::ServiceClaims` split:
- GC uses `auth::Claims` (shared with HTTP user-auth middleware).
- MC/MH use `common::jwt::ServiceClaims`.
- Unifying requires either migrating GC's HTTP middleware (out of scope here) or making the auth layer generic over the claims type.

Tracked in `docs/TODO.md`:
- Line 15 — `Claims` vs `ServiceClaims` unification (unblocks extraction)
- Line 18 — `build_pkcs8_from_seed` test helper (8 copies across services)
- Line 28 — `GrpcAuthLayer` extraction entry (now covers GC/MC/MH)
- Line 31 — `NoopService` test helper (now covers GC/MC/MH)

### Optional follow-ups flagged by Operations (not blocking)

1. `infra/docker/prometheus/rules/gc-alerts.yaml` has no alert mirroring MC's JWT-failure-rate rule at `mc-alerts.yaml:296-305`, and no alert on `gc_caller_type_rejected_total > 0` (any non-zero in prod is a bug). Worth an ops-signal-parity ticket.
2. No `docs/runbooks/gc-service.md` exists to document the Layer 2 rejection signal. Not required by this devloop.

### Non-blocking observability parity note

`actual_type` label on `*_caller_type_rejected_total` is populated from `claims.service_type` (an AC-issued claim). If AC is ever misconfigured to emit arbitrary strings, the label could grow unboundedly. This applies equally to MC and MH. If ever tightened, fix across all three services together — don't diverge GC alone.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Start commit: `6b9f41b931970657ab52af5f511bc88fabf3ab39`
2. Review: `git diff 6b9f41b..HEAD`
3. Soft reset: `git reset --soft 6b9f41b`
4. Hard reset: `git reset --hard 6b9f41b`

---

## Reflection

All seven teammates updated their INDEX.md files during reflection. Guard `validate-knowledge-index.sh` passed after two trim-fixes by the Lead (security and test INDEX files each nudged 1 line over the 75-line budget; security had a brace-expansion `{mh,gc}` pointer the guard couldn't resolve). Each reflection stayed pointer-only; no facts/gotchas/date-stamps introduced.

### Issues Encountered & Resolutions

**Issue 1: Stale postgres seed data in Kind cluster**
The Kind cluster's postgres volume carried `{meeting:create, meeting:read, meeting:update, internal:meeting-token}` — legacy GC scopes that predate commit `2c2613c`'s scope alignment. Initial env-test run failed with `GC→MC: Unauthenticated (missing service.write.mc)`. Not a bug in our code — source `setup.sh:seed_test_data()` has the correct values and uses `ON CONFLICT (client_id) DO UPDATE SET scopes = ...` (fact-checked by Operations reviewer; my initial diagnosis of "INSERT doesn't overwrite" was wrong). Root cause unclear — most likely the postgres volume persisted across a `kind delete` from an earlier devloop session running pre-`2c2613c` code, and current setup was reused without re-running seeding against the fresh cluster's DB. Remediated with a direct `UPDATE service_credentials` + rolling restart; env-tests then passed.

**Issue 2: Loki readiness race**
Loki's `/ready` endpoint returned HTTP 200 with body `"Ingester not ready: waiting for 15s after being ready"` during a ~15-30s post-startup window, causing `test_all_services_have_logs_in_loki` to fail on first attempt. Known infrastructure behavior per ADR-0030 (deferred readiness). Retried after a readiness wait-loop — test passed.

Neither issue was an introduced defect in this changeset. No code follow-ups required.

### Lessons Learned

1. **Trust the reviewer's fact-check over your own first-pass diagnosis.** I wrote up the stale-seed issue as "INSERT doesn't overwrite" and flagged an `ON CONFLICT` follow-up; Operations corrected me — the conflict clause was already in place. The real cause is volume persistence across `kind delete`, which is a test-infra observation, not a code gap.
2. **Layer 2 routing caught the pre-existing misconfiguration.** The devloop's own code was a diagnostic tool: within seconds of deploying GC's new auth layer, the stale seed data surfaced as a hard failure with a bounded `failure_reason=scope_mismatch` label. This is exactly the signal the ADR-0003 two-layer design was intended to produce. The bounded label made triage trivial.
3. **Aggressive test-coverage feedback pays.** The Test reviewer pushed for `unknown_kid`, `empty_scope`, `mismatched_scope_tokens` beyond MC's baseline during planning. All three landed and got called out as "good defensive coverage" in the Code Quality verdict — no reviewer asked for additions at Gate 3.
4. **Reviewer feedback loops should be tight.** Gate 1 cleared in ~3 minutes with zero revision rounds; Gate 3 cleared with zero findings. This was because the planning stage exposed every substantive design decision (Claims shape, wrapper removal, `validate_raw()` addition) and the implementation had no surprises.

### Non-goals honored

- Did not migrate GC to `common::jwt::ServiceClaims` (scope containment).
- Did not extract shared auth layer into `common` (DRY tech-debt waits on Claims unification).
- Did not write Scope Contract Tests (separate pending ADR-0003 item for a future devloop).
