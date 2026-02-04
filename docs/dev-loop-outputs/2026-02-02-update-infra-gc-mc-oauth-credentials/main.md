# Dev-Loop Output: Update Infrastructure for GC/MC OAuth Credentials

**Date**: 2026-02-02
**Start Time**: 22:32
**Task**: Update infrastructure for GC/MC OAuth credentials. Add Kubernetes secrets and deployment configs for OAuth 2.0 client credentials that GC and MC need to authenticate with AC. Specifically: (1) Add AC database entries for gc-service and mc-service OAuth clients, (2) Create Kubernetes secrets (gc-oauth-credentials, mc-oauth-credentials) with client_secret values, (3) Update GC deployment to add AC_ENDPOINT, GC_CLIENT_ID, GC_CLIENT_SECRET environment variables, (4) Update MC deployment to add AC_ENDPOINT, MC_CLIENT_ID, MC_CLIENT_SECRET environment variables, (5) Remove deprecated GC_SERVICE_TOKEN and MC_SERVICE_TOKEN environment variables from both deployments. Reference infra/services/global-controller/deployment.yaml and infra/services/meeting-controller/deployment.yaml for deployment updates, infra/services/*/secret.yaml for secrets.
**Branch**: `feature/infra-gc-mc-oauth-config`
**Duration**: ~0m (in progress)

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `pending` |
| Implementing Specialist | `infrastructure` |
| Current Step | `validation` |
| Iteration | `1` |
| Security Reviewer | `pending` |
| Test Reviewer | `pending` |
| Code Reviewer | `pending` |
| DRY Reviewer | `pending` |

---

## Task Overview

### Objective

Update Kubernetes infrastructure to support OAuth 2.0 client credentials for GC and MC authentication with AC.

### Detailed Requirements

#### Context
The TokenManager integration (PR #39, now merged) replaced static service tokens with OAuth 2.0 client credentials flow. Services now need:
- GC: `AC_ENDPOINT`, `GC_CLIENT_ID`, `GC_CLIENT_SECRET`
- MC: `AC_ENDPOINT`, `MC_CLIENT_ID`, `MC_CLIENT_SECRET`, `MC_BINDING_TOKEN_SECRET`

Old env vars to remove: `GC_SERVICE_TOKEN`, `MC_SERVICE_TOKEN`

Note: MC also requires `MC_BINDING_TOKEN_SECRET` for session binding token master secret (base64-encoded, min 32 bytes for HMAC-SHA256).

#### Required Changes

**1. AC Database OAuth Client Entries**
- Add OAuth client for `gc-service` in AC database
- Add OAuth client for `mc-service` in AC database
- These need to be seeded/migrated so AC can validate token requests
- Reference: AC's OAuth client schema (likely in `migrations/`)

**2. Kubernetes Secrets**

Create or update:
- `infra/services/global-controller/secret.yaml`
  - Add `gc-oauth-credentials` secret with `client-secret` key
  - Generate secure random value for client secret

- `infra/services/meeting-controller/secret.yaml`
  - Add `mc-oauth-credentials` secret with `client-secret` key
  - Generate secure random value for client secret
  - Add `binding-token-secret` key with base64-encoded value (min 32 bytes raw)

**3. GC Deployment Updates**

File: `infra/services/global-controller/deployment.yaml`

Add environment variables:
```yaml
env:
  - name: AC_ENDPOINT
    value: "https://ac-service:8082"
  - name: GC_CLIENT_ID
    value: "gc-service"
  - name: GC_CLIENT_SECRET
    valueFrom:
      secretKeyRef:
        name: gc-oauth-credentials
        key: client-secret
```

Remove: `GC_SERVICE_TOKEN` environment variable (if present)

**4. MC Deployment Updates**

File: `infra/services/meeting-controller/deployment.yaml`

Add environment variables:
```yaml
env:
  - name: AC_ENDPOINT
    value: "https://ac-service:8082"
  - name: MC_CLIENT_ID
    value: "mc-service"
  - name: MC_CLIENT_SECRET
    valueFrom:
      secretKeyRef:
        name: mc-oauth-credentials
        key: client-secret
  - name: MC_BINDING_TOKEN_SECRET
    valueFrom:
      secretKeyRef:
        name: mc-oauth-credentials
        key: binding-token-secret
```

Remove: `MC_SERVICE_TOKEN` environment variable (if present)

**5. Kind Setup Script Updates**

File: `infra/kind/scripts/setup.sh`

Update to:
- Seed AC database with OAuth clients for gc-service and mc-service
- Apply the new secret manifests
- Ensure secrets are created before deployments

**6. Documentation Updates**

Update any deployment documentation to reflect OAuth credential requirements instead of static tokens.

#### Acceptance Criteria

- [ ] AC database has OAuth client entries for gc-service and mc-service
- [ ] Kubernetes secrets created with proper structure (OAuth + binding token)
- [ ] GC deployment has OAuth env vars (AC_ENDPOINT, GC_CLIENT_ID, GC_CLIENT_SECRET), no static token
- [ ] MC deployment has OAuth env vars (AC_ENDPOINT, MC_CLIENT_ID, MC_CLIENT_SECRET, MC_BINDING_TOKEN_SECRET), no static token
- [ ] Kind setup script properly seeds OAuth clients
- [ ] Local deployment works with OAuth flow (manual test after implementation)

### Scope

- **Service(s)**: Infrastructure (Kubernetes manifests, Kind setup)
- **Schema**: AC database OAuth clients (seeding/migration)
- **Cross-cutting**: Affects GC, MC, AC deployments

### Debate Decision

N/A - Infrastructure update following established OAuth pattern from PR #39

---

## Matched Principles

The following principle categories were matched:
- `docs/principles/crypto.md` (OAuth credentials, secrets handling)
- `docs/principles/logging.md` (Deployment configuration)
- `docs/principles/errors.md` (Deployment error handling)

---

## Pre-Work

Analyzed existing infrastructure to understand:
1. Current secret structure (using `stringData` for dev, `secretKeyRef` for deployment)
2. OAuth client seeding already in place (`seed_test_data` in setup.sh)
3. Environment variable requirements from GC and MC config.rs files
4. GC uses `AC_INTERNAL_URL` while MC uses `AC_ENDPOINT` (different naming conventions)

---

## Implementation Summary

Updated Kubernetes infrastructure to support OAuth 2.0 client credentials flow for GC and MC authentication with AC.

### Changes Made

1. **GC Secret** (`infra/services/global-controller/secret.yaml`)
   - Added `GC_CLIENT_SECRET` key with value matching seeded credentials

2. **GC Deployment** (`infra/services/global-controller/deployment.yaml`)
   - Added `GC_CLIENT_ID` as direct value (`global-controller`)
   - Added `GC_CLIENT_SECRET` from `secretKeyRef`
   - GC already has `AC_INTERNAL_URL` from configmap (used for TokenManager)

3. **MC Secret** (`infra/services/meeting-controller/secret.yaml`)
   - Added `MC_CLIENT_SECRET` key with value matching seeded credentials
   - Removed deprecated `MC_SERVICE_TOKEN` (static token approach)

4. **MC Deployment** (`infra/services/meeting-controller/deployment.yaml`)
   - Added `AC_ENDPOINT` direct value pointing to AC service
   - Added `MC_CLIENT_ID` as direct value (`meeting-controller`)
   - Added `MC_CLIENT_SECRET` from `secretKeyRef`
   - Removed `MC_SERVICE_TOKEN` env var reference
   - `MC_BINDING_TOKEN_SECRET` already present (for session binding, not OAuth)

5. **Setup Script** (`infra/kind/scripts/setup.sh`)
   - Updated `print_access_info` to document OAuth credentials usage
   - OAuth client seeding already present in `seed_test_data`

### Design Decisions

- **Client IDs as direct values**: Not sensitive, improves debuggability
- **Reused existing secrets**: Added keys to existing `*-secrets` resources rather than creating new ones
- **Matched seeded credentials**: K8s secrets use same values as `seed_test_data` bcrypt hashes

---

## Files Modified

| File | Changes |
|------|---------|
| `infra/services/global-controller/secret.yaml` | Added `GC_CLIENT_SECRET` |
| `infra/services/global-controller/deployment.yaml` | Added OAuth env vars |
| `infra/services/meeting-controller/secret.yaml` | Added `MC_CLIENT_SECRET`, removed `MC_SERVICE_TOKEN` |
| `infra/services/meeting-controller/deployment.yaml` | Added OAuth env vars, removed `MC_SERVICE_TOKEN` |
| `infra/kind/scripts/setup.sh` | Updated documentation in print_access_info |

---

## Verification

### 7-Layer Verification

| Layer | Command | Status | Notes |
|-------|---------|--------|-------|
| 1 | `cargo check --workspace` | PASSED | All crates compile |
| 2 | `cargo fmt --all --check` | PASSED | No formatting issues |
| 3 | `./scripts/guards/run-guards.sh` | PASSED | 9/9 guards passed |
| 4 | `./scripts/test.sh --workspace --lib` | PASSED | All unit tests pass |
| 5 | `./scripts/test.sh --workspace` | PASSED | All tests pass |
| 6 | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASSED | No warnings |
| 7 | `./scripts/guards/run-guards.sh --semantic` | PASSED | Manual review recommended |

---

## Code Review

Pending - awaiting review by Security, Test, Code, and DRY reviewers.

---

## Issues Encountered

### 1. Different AC URL Naming Conventions
- **Issue**: GC uses `AC_INTERNAL_URL` while MC uses `AC_ENDPOINT`
- **Resolution**: Used existing conventions for each service; both point to same AC service
- **Root Cause**: GC was designed before MC with different env var naming

### 2. MC_SERVICE_TOKEN Deprecation
- **Issue**: MC deployment had deprecated static token approach
- **Resolution**: Removed from both secret and deployment; replaced with OAuth credentials

---

## Lessons Learned

1. **Check existing seeding**: The `seed_test_data` function already had OAuth clients, no new seeding needed
2. **Env var conventions vary**: Different services may use different naming for similar concepts
3. **SecretString in config.rs**: Client secrets are already protected by SecretString, ensuring they won't be logged

---

## Tech Debt

None introduced. Existing tech debt addressed:
- Removed deprecated `MC_SERVICE_TOKEN` static token approach

---

## Next Steps

1. **Code Review**: Submit for Security, Test, Code Quality, and DRY review
2. **Manual Validation**: Test OAuth flow in local Kind cluster:
   - Run `./infra/kind/scripts/setup.sh`
   - Verify deployments have correct env vars
   - Test token acquisition from AC
3. **Merge**: After review approval, merge to main
