# DRY Review (ADR-0019)

**Reviewer**: DRY Reviewer
**Date**: 2026-02-02
**Verdict**: APPROVED

## Summary

The Meeting Controller TokenManager integration correctly uses the shared `common::token_manager` module established in the 2026-02-01 implementation. No BLOCKER findings exist - the implementation properly leverages existing common code rather than duplicating it. Two TECH_DEBT items are documented for monitoring as GC integration approaches.

## Findings

### BLOCKER (Safety/Security/Correctness Duplication)

None

### TECH_DEBT (Non-Blocking Duplication)

#### TD-16: Mock TokenReceiver Test Utility Duplication

| Field | Value |
|-------|-------|
| New code | `crates/meeting-controller/src/grpc/gc_client.rs:642-656` (`mock_token_receiver()`) |
| Existing code | `crates/meeting-controller/tests/gc_integration.rs:264-269` (`mock_token_receiver()`) |
| Occurrences | 2 (unit tests, integration tests) |
| Severity | TECH_DEBT (non-blocking) |

**Description**: Two identical `mock_token_receiver()` helper functions exist within MC itself (one in gc_client.rs unit tests, one in gc_integration.rs). When GC integrates TokenManager, it will likely need the same helper, creating a third occurrence.

**Rationale for non-blocking**: Currently internal to MC. The `TokenReceiver::from_test_channel()` constructor is already exposed via `test-utils` feature flag, so a shared helper could be added to `common` or `mc-test-utils` when GC integration occurs.

**Follow-up action**: When GC implements TokenManager integration, consider extracting `mock_token_receiver()` to either:
- `common::token_manager` (behind `test-utils` feature)
- A new shared test utilities crate

---

#### TD-17: Service Config OAuth Credential Pattern

| Field | Value |
|-------|-------|
| New code | `crates/meeting-controller/src/config.rs:96-105` (`ac_endpoint`, `client_id`, `client_secret`) |
| Potential duplicate | GC will need similar OAuth config when it integrates TokenManager |
| Occurrences | 1 (MC only currently) |
| Severity | TECH_DEBT (non-blocking) |

**Description**: MC's Config struct now contains OAuth credential fields (`ac_endpoint`, `client_id`, `client_secret`). GC currently uses `GC_SERVICE_TOKEN` environment variable (static token pattern). When GC migrates to dynamic token acquisition, it will need similar config fields.

**Rationale for non-blocking**: Only 1 occurrence currently exists. GC's current architecture is different (static token). When GC integrates, the orchestrator should evaluate whether to:
1. Keep service-specific config (acceptable duplication)
2. Extract `OAuthCredentialConfig` to common if pattern stabilizes

**Follow-up action**: Document pattern during GC TokenManager integration. If configs are structurally identical (>90% overlap), consider extraction.

---

## Observations

### Positive: Correct Use of Shared Code

The implementation correctly uses the shared `common::token_manager` module:

1. **Import pattern**: `use common::token_manager::{spawn_token_manager, TokenManagerConfig};`
2. **Config construction**: Uses `TokenManagerConfig::new_secure()` as designed
3. **Token distribution**: `TokenReceiver` cloned to `GcClient` as intended
4. **Error handling**: MC-specific error types (`McError::TokenAcquisition`, `McError::TokenAcquisitionTimeout`) appropriately wrap common errors

### Positive: Test Infrastructure Reuse

The integration tests correctly use `common = { path = "../common", features = ["test-utils"] }` in `Cargo.toml` to access `TokenReceiver::from_test_channel()`, which was the intended test utility pattern from the shared TokenManager design.

### Neutral: Service-Specific Startup Boilerplate

The MC `main.rs` startup sequence (~30 lines for TokenManager initialization) will be duplicated when GC integrates. This is acceptable because:
- Startup orchestration is inherently service-specific
- Services may have different startup constraints (timeout, error handling)
- The core logic (TokenManager itself) is already shared

### Monitoring: GC Integration Horizon

GC currently uses static `GC_SERVICE_TOKEN` (see `crates/global-controller/src/main.rs:100-104`). When GC migrates to dynamic TokenManager:
1. Monitor TD-16 (mock helper duplication)
2. Monitor TD-17 (config pattern)
3. Evaluate if `McClient::new()` in GC (which receives a static token) should migrate to TokenReceiver pattern

---

## Cross-Reference: Previous Tech Debt

Two TECH_DEBT items from the shared TokenManager implementation (2026-02-01) remain relevant:

| ID | Pattern | Status | Notes |
|----|---------|--------|-------|
| TD-14 | Exponential Backoff | Active | MC gc_client.rs and common token_manager.rs both have backoff |
| TD-15 | HTTP Client Builder | Active | common token_manager.rs and gc ac_client.rs |

These items are not worsened by this integration; MC uses the shared code correctly.

---

## Files Reviewed

| File | DRY Assessment |
|------|----------------|
| `crates/meeting-controller/src/config.rs` | No BLOCKER - new OAuth fields are service-specific |
| `crates/meeting-controller/src/main.rs` | No BLOCKER - correctly uses `common::token_manager` |
| `crates/meeting-controller/src/grpc/gc_client.rs` | No BLOCKER - uses `TokenReceiver` as designed |
| `crates/meeting-controller/src/errors.rs` | No BLOCKER - MC-specific error types appropriate |
| `crates/meeting-controller/Cargo.toml` | No BLOCKER - correct dependency on common |
| `crates/meeting-controller/tests/gc_integration.rs` | TECH_DEBT (TD-16) - mock helper duplication |
| `crates/common/Cargo.toml` | No changes needed |
| `crates/common/src/token_manager.rs` | Source of shared code - no duplication |

---

## Verdict Rationale

**APPROVED** because:

1. **No BLOCKER findings**: The implementation correctly uses existing `common::token_manager` code
2. **TECH_DEBT items are appropriate**: TD-16 (mock helper) and TD-17 (config pattern) are 1-2 occurrence patterns that don't warrant immediate extraction per ADR-0019
3. **Shared code strategy validated**: The proactive placement of TokenManager in `common` crate (2026-02-01) enabled this clean integration without cross-service duplication

---

## Tech Debt Registry

| ID | Pattern | Locations | Follow-up Action | Timeline |
|----|---------|-----------|------------------|----------|
| TD-16 | Mock TokenReceiver Helper | mc/grpc/gc_client.rs (tests), mc/tests/gc_integration.rs | Consider `common::token_manager::test_helpers` | When GC integrates TokenManager |
| TD-17 | OAuth Config Fields | mc/config.rs | Evaluate extraction to common if GC config is >90% similar | When GC integrates TokenManager |
