# DRY Reviewer Checkpoint

**Date**: 2026-01-18
**Task**: Cross-service environment tests (AC + GC flows)
**Verdict**: APPROVED

## Files Reviewed

- `crates/env-tests/src/fixtures/gc_client.rs` (new - 671 lines)
- `crates/env-tests/tests/21_cross_service_flows.rs` (new - 550 lines)
- `crates/env-tests/src/fixtures/mod.rs` (modified)
- `crates/env-tests/src/cluster.rs` (modified)
- `crates/env-tests/src/lib.rs` (modified)
- `crates/env-tests/Cargo.toml` (modified)

## Common Crate Check

### Patterns in `common` crate:
- `common::secret::{SecretString, SecretBox, ExposeSecret}` - available
- `common::error::DarkTowerError` - available
- `common::types` - domain IDs available
- `common::config` - config types available

### Assessment:

**No BLOCKING issues**: The code does NOT duplicate anything from `common`.

1. **SecretString not used** - Intentional. The response types need standard `Serialize`/`Deserialize` with custom `Debug`. Using `SecretString` would complicate the API client pattern since:
   - `SecretString` doesn't serialize to expose the value
   - Test fixtures need to work with actual values from API responses
   - Custom Debug provides same redaction benefit

2. **Error types** - `GcClientError` is appropriate as a test fixture error type, not a domain error. Using `common::error::DarkTowerError` would be inappropriate here.

## Cross-Service Duplication Check

### Checked Locations:
- `crates/ac-service/src/` - No overlap (service code, not test fixtures)
- `crates/global-controller/src/` - No overlap (service code, not test fixtures)
- `crates/env-tests/src/fixtures/auth_client.rs` - Similar pattern, same crate

### Findings:

**No BLOCKING issues**: No code from service crates was duplicated.

### Same-Crate Patterns (ACCEPTABLE):

The following patterns are repeated within `env-tests` but this is acceptable for test utilities:

1. **Client struct pattern** (`base_url: String, http_client: Client`):
   - `AuthClient` - line 83
   - `GcClient` - line 200
   - `PrometheusClient` - metrics.rs

   **Assessment**: ACCEPTABLE - Test utilities benefit from self-contained implementations. Extraction would add complexity without significant benefit.

2. **Error enum pattern** (HttpError, RequestFailed):
   - `AuthClientError` - line 8
   - `GcClientError` - line 46

   **Assessment**: ACCEPTABLE - Each client has distinct error variants. Merging would reduce clarity.

### Improvement Over Existing Code:

`GcClient` adds `sanitize_error_body()` for credential redaction in error messages. This is NOT present in `AuthClient`. This is an improvement, not duplication.

**Recommendation**: Consider backporting `sanitize_error_body()` to `AuthClient` as a follow-up task (not blocking).

## Tech Debt Registry Check

Checked existing TD-N entries in `docs/specialist-knowledge/dry-reviewer/integration.md`:
- TD-1: JWT Validation Duplication (AC vs GC) - **Not applicable** (this is test code)
- TD-2: EdDSA Key Handling Patterns - **Not applicable** (this is test code)

No new tech debt entries needed.

## DRY Review Summary

| Severity | Count | Blocking? |
|----------|-------|-----------|
| BLOCKING | 0 | Yes |
| TECH_DEBT | 0 | No |

**Verdict**: APPROVED

No code exists in `common` that should have been used. No cross-service duplication was introduced. The similar patterns within `env-tests` are acceptable for test utilities.

## Status

Review complete. Verdict: APPROVED

---

## Reflection Summary (2026-01-18)

### Knowledge Files Updated

**patterns.md**: Added 1 entry
- Improvement vs Duplication Assessment

### Key Learnings

1. **Improvement vs duplication distinction**: `GcClient` having `sanitize_error_body()` while `AuthClient` doesn't is an improvement, not duplication. The right response is "backport this" not "DRY violation." This distinction is important for encouraging incremental improvement.
