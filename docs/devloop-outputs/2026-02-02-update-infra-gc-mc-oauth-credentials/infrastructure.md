# Infrastructure Specialist Checkpoint

**Task**: Update infrastructure for GC/MC OAuth credentials
**Date**: 2026-02-02
**Iteration**: 1

---

## Patterns Discovered

### 1. Kubernetes Secret Organization
The existing infrastructure already has a consistent pattern for secrets:
- Each service has its own `*-secrets` Secret resource
- Secrets use `stringData` for readability in dev environments (auto-base64 encoded by K8s)
- Comments document the purpose and production override expectations

### 2. Environment Variable Source Patterns
GC and MC use a consistent pattern for sourcing config:
- Non-sensitive values: Direct `value` or `configMapKeyRef`
- Sensitive values: `secretKeyRef`
- Client IDs are not sensitive (logged for debugging), so they use direct `value`

### 3. OAuth Client Credentials Pre-Seeding
The `seed_test_data` function in setup.sh already creates OAuth clients with pre-computed bcrypt hashes:
- `global-controller` / `global-controller-secret-dev-001`
- `meeting-controller` / `meeting-controller-secret-dev-002`

This means no new seeding code was needed - just ensuring the K8s secrets match these values.

---

## Gotchas Encountered

### 1. AC_INTERNAL_URL vs AC_ENDPOINT Naming
- GC uses `AC_INTERNAL_URL` (already in configmap) for TokenManager
- MC uses `AC_ENDPOINT` (required by config.rs)
- These different names exist because GC was designed first with a different convention
- Both point to the same AC service, just different env var names

### 2. MC_SERVICE_TOKEN Removal
- The `MC_SERVICE_TOKEN` was a deprecated static token approach
- It was removed from the MC secret and deployment
- MC now uses OAuth 2.0 client credentials exclusively via TokenManager

### 3. Secret Names Match Seeded Credentials
The K8s secrets must match the credentials in `seed_test_data`:
- GC: `global-controller-secret-dev-001`
- MC: `meeting-controller-secret-dev-002`

---

## Key Decisions

### 1. GC_CLIENT_ID as Direct Value
Client IDs are not sensitive data. Setting them as direct values in the deployment:
- Improves debuggability (visible in kubectl describe)
- Follows principle that only secrets (passwords, tokens) go in Secrets
- Consistent with how other non-sensitive config is handled

### 2. MC_BINDING_TOKEN_SECRET Remains Separate
The binding token secret is for HMAC session binding (ADR-0023), not OAuth.
It remains in the same Secret resource but is conceptually separate from OAuth credentials.

### 3. No New OAuth Client Seeding
The existing `seed_test_data` function already creates the OAuth clients.
No changes were needed to the seeding logic, just the K8s manifests to use them.

---

## Files Modified

1. `infra/services/global-controller/secret.yaml`
   - Added `GC_CLIENT_SECRET` for OAuth authentication

2. `infra/services/global-controller/deployment.yaml`
   - Added `GC_CLIENT_ID` (direct value)
   - Added `GC_CLIENT_SECRET` (from secret)

3. `infra/services/meeting-controller/secret.yaml`
   - Added `MC_CLIENT_SECRET` for OAuth authentication
   - Removed deprecated `MC_SERVICE_TOKEN`

4. `infra/services/meeting-controller/deployment.yaml`
   - Added `AC_ENDPOINT` (direct value - points to AC service)
   - Added `MC_CLIENT_ID` (direct value)
   - Added `MC_CLIENT_SECRET` (from secret)
   - Removed `MC_SERVICE_TOKEN` env var reference

5. `infra/kind/scripts/setup.sh`
   - Updated print_access_info to document OAuth credentials usage

---

## Current Status

**Implementation**: Complete
**Verification**: All 7 layers passed

| Layer | Status |
|-------|--------|
| 1. cargo check | PASSED |
| 2. cargo fmt | PASSED |
| 3. Guards | PASSED (9/9) |
| 4. Unit tests | PASSED |
| 5. All tests | PASSED |
| 6. Clippy | PASSED |
| 7. Semantic | PASSED (manual review recommended) |

---

## Acceptance Criteria Status

- [x] AC database has OAuth client entries for gc-service and mc-service (pre-existing in `seed_test_data`)
- [x] Kubernetes secrets created with proper structure (OAuth credentials in both GC and MC secrets)
- [x] GC deployment has OAuth env vars (GC_CLIENT_ID, GC_CLIENT_SECRET), uses existing AC_INTERNAL_URL
- [x] MC deployment has OAuth env vars (AC_ENDPOINT, MC_CLIENT_ID, MC_CLIENT_SECRET, MC_BINDING_TOKEN_SECRET), no static token
- [x] Kind setup script properly seeds OAuth clients (pre-existing functionality)
- [ ] Local deployment works with OAuth flow (manual test recommended)

---

## Notes for Validation

To validate the implementation manually:

1. **Tear down existing cluster** (if any):
   ```bash
   ./infra/kind/scripts/teardown.sh
   ```

2. **Set up fresh cluster**:
   ```bash
   ./infra/kind/scripts/setup.sh
   ```

3. **Verify GC deployment has OAuth env vars**:
   ```bash
   kubectl describe deployment global-controller -n dark-tower | grep -A5 "GC_CLIENT"
   ```

4. **Verify MC deployment has OAuth env vars**:
   ```bash
   kubectl describe deployment meeting-controller -n dark-tower | grep -A5 "MC_CLIENT\|AC_ENDPOINT"
   ```

5. **Test OAuth token acquisition** (using test-client):
   ```bash
   curl -X POST http://localhost:8082/api/v1/auth/service/token \
     -H 'Content-Type: application/x-www-form-urlencoded' \
     -d 'grant_type=client_credentials' \
     -d 'client_id=test-client' \
     -d 'client_secret=test-client-secret-dev-999'
   ```
