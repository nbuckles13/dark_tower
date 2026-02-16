# Devloop Output: Service Naming Rename

**Date**: 2026-02-16
**Task**: Rename global-controller → gc-service, meeting-controller → mc-service, media-handler → mh-service
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — Full
**Branch**: `feature/mc-token-metrics`
**Duration**: ~25m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c26931efe4774d03e5d0af7dcf1bf0fbf71f396a` |
| Branch | `feature/mc-token-metrics` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@service-rename` |
| Implementing Specialist | `infrastructure` |
| Iteration | `3` |
| Security | `security@service-rename` |
| Test | `test@service-rename` |
| Observability | `observability@service-rename` |
| Code Quality | `code-reviewer@service-rename` |
| DRY | `dry-reviewer@service-rename` |
| Operations | `operations@service-rename` |

---

## Task Overview

### Objective
Standardize service naming to the `*-service` pattern (matching existing `ac-service`). Rename crate directories, Cargo packages, K8s manifests, Docker configs, monitoring configs, and active documentation.

### Scope
- **Service(s)**: global-controller, meeting-controller, media-handler (all three renamed)
- **Schema**: No
- **Cross-cutting**: Yes (crates, infra, docs, monitoring)

### Debate Decision
NOT NEEDED - Naming standardization, not architectural change.

---

## Implementation Summary

### Phase 1: Crate Directory Renames (git mv)
1. `crates/global-controller/` → `crates/gc-service/`
2. `crates/meeting-controller/` → `crates/mc-service/`
3. `crates/media-handler/` → `crates/mh-service/`

### Phase 2: Cargo.toml Updates
- Workspace `Cargo.toml`: members list updated
- `gc-service/Cargo.toml`: `name = "gc-service"`, `[lib] name = "gc_service"`, `[[bin]] name = "gc-service"`
- `mc-service/Cargo.toml`: `name = "mc-service"`, `[lib] name = "mc_service"`, `[[bin]] name = "mc-service"`
- `mh-service/Cargo.toml`: `name = "mh-service"`, `[[bin]] name = "mh-service"`
- `gc-test-utils/Cargo.toml`: dep path `../gc-service`
- `mc-test-utils/Cargo.toml`: dep path `../mc-service`

### Phase 3: Rust Source Updates
- 10 files: `use global_controller::` → `use gc_service::`, `use meeting_controller::` → `use mc_service::`
- EnvFilter defaults: `global_controller=debug` → `gc_service=debug` (and similarly for MC, MH)

### Phase 4: K8s Manifest Renames (git mv + content updates)
- `infra/services/global-controller/` → `infra/services/gc-service/` (7 YAML files)
- `infra/services/meeting-controller/` → `infra/services/mc-service/` (7 YAML files)
- Redis NetworkPolicy: `app: meeting-controller` → `app: mc-service`

### Phase 5: Docker Config Updates
- `infra/docker/global-controller/` → `infra/docker/gc-service/`
- `infra/docker/meeting-controller/` → `infra/docker/mc-service/`
- `docker-compose.yml`, `infra/skaffold.yaml` updated

### Phase 6: Monitoring Config Updates
- Prometheus: `gc-alerts.yaml`, `mc-alerts.yaml`, `prometheus.yml`, `prometheus-config.yaml`
- Grafana: 7 dashboard JSON files (pod matchers, tags, labels, Loki queries)

### Phase 7: Script Updates
- `infra/kind/scripts/setup.sh`: image builds, manifests, credential seeding SQL, bcrypt hashes, display
- `infra/kind/scripts/iterate.sh`: SERVICES array
- `scripts/test-oauth-integration.sh`: kubectl labels, port-forwards
- `crates/env-tests/tests/40_resilience.rs`: K8s label selectors
- `crates/env-tests/src/canary.rs`: K8s label example

### Phase 8: Documentation Updates
- ~40 files: CLAUDE.md, ARCHITECTURE.md, PROJECT_STATUS.md, active ADRs, specialist knowledge content paths, runbooks

### Files NOT Modified (intentional)
- Agent filenames (`.claude/agents/global-controller.md`, etc.) — descriptive specialist names
- Specialist knowledge directories (`docs/specialist-knowledge/global-controller/`, etc.)
- Proto/gRPC service names (`GlobalControllerService` in protos)
- OAuth `client_id`/`service_type` database values
- Metric name prefixes (`gc_`, `mc_`, `mh_`)
- Historical devloop outputs, superseded ADRs, debate records

---

## Files Modified

```
158 files changed, 963 insertions(+), 884 deletions(-)
```

Includes crate directory renames (R100), K8s manifest renames, Docker config renames, and content updates.

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Guards
**Status**: 10/11 PASS
- infrastructure-metrics: FAIL (pre-existing — missing PyYAML in environment)

### Layer 4: Tests
**Status**: PASS (all tests pass, 0 failures)

### Layer 5: Clippy
**Status**: PASS

### Layer 6: Audit
**Status**: 2 pre-existing vulnerabilities (ring 0.16.20, rsa 0.9.10 — transitive dependencies, unrelated)

### Layer 7: Semantic Guard
**Status**: SAFE — caught credential seeding mismatch in setup.sh (fixed in iteration 2)

---

## Code Review Results

### Security Specialist
**Verdict**: APPROVED
No findings. Verified credential chain consistency (SQL seed ↔ K8s Secret ↔ K8s Deployment ↔ bcrypt hash). Network policies preserve same security posture. No auth logic changes. expose_secret() call sites unchanged.

### Test Specialist
**Verdict**: APPROVED (after fix)
1 MAJOR found and fixed: EnvFilter fallback strings in 3 main.rs files referenced old crate names, causing silent log loss. Fixed in iteration 2.

### Observability Specialist
**Verdict**: APPROVED (after fix)
1 MINOR found and fixed: `errors-overview.json` Loki query had stale `global-controller` in multi-service regex. Fixed in iteration 2.

### Code Quality Reviewer
**Verdict**: APPROVED (after fix)
1 MAJOR found and fixed: env-tests K8s label selectors used old `app=global-controller`. Fixed in iteration 2. 2 MINORs (Grafana, test-oauth script) also fixed.

### DRY Reviewer
**Verdict**: APPROVED
No findings. Naming consistency verified across all active files. Historical files correctly preserved.

### Operations Reviewer
**Verdict**: APPROVED (after fix)
2 MAJORs found and fixed: `test-oauth-integration.sh` stale refs, `errors-overview.json` stale ref. 1 MINOR tracked as tech debt.

---

## Tech Debt

| ID | Severity | Description | File |
|----|----------|-------------|------|
| TD-1 | MINOR | Old service names in usage help text (non-functional) | `scripts/register-service.sh:8-10` |

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit: `c26931efe4774d03e5d0af7dcf1bf0fbf71f396a`
2. Review: `git diff c26931e..HEAD`
3. Soft reset: `git reset --soft c26931e`
4. Hard reset: `git reset --hard c26931e`

---

## Reflection

### Observability
- **Gotcha added**: Multi-value regex fields survive partial renames — all old names in a regex must be checked simultaneously.

### Test
- **Gotcha added**: Crate rename silently breaks EnvFilter defaults — compiler and tests can't catch this.

### Code Quality
- **Gotcha added**: Crate rename vs domain identifiers — K8s labels in Rust strings must match renamed manifests.

### Security
- **Gotcha added**: Service rename breaks 4-layer credential chain (SQL seed, K8s Secret, K8s Deployment env, bcrypt hash).

### Operations
- **Gotcha added**: NetworkPolicy cross-references span more services than expected (Redis NP).
- **Gotcha added**: Grafana Loki queries use app labels that mirror K8s labels.
- **Pattern added**: Service rename checklist covering non-obvious cross-service label reference points.

### Infrastructure (Implementer)
- **Gotcha added**: Cross-cutting renames must distinguish infrastructure names from runtime identifiers.
- **Updated**: NetworkPolicy test labels example to match current state.
- **Fixed**: Stale paths in integration.md.

### DRY, Semantic Guard
- No reflections.

---

## Issues Encountered & Resolutions

1. **Credential seeding mismatch** (semantic guard): setup.sh SQL still seeded old `global-controller`/`meeting-controller` client_ids while K8s secrets used new names. Fixed by updating SQL INSERTs and regenerating bcrypt hashes.
2. **Grafana Loki query partial rename** (observability): `errors-overview.json` had one of two old names updated in a multi-service regex. Fixed.
3. **EnvFilter silent log loss** (test): main.rs fallback filter strings referenced old crate module names. Fixed.
4. **env-tests K8s label mismatch** (code-reviewer): Rust test code used old `app=global-controller` label selector. Fixed.
5. **test-oauth-integration.sh stale refs** (operations): kubectl commands referenced old service names. Fixed.

---

## Lessons Learned

Cross-cutting renames require checking more locations than initially obvious. The reviewers caught 5 distinct categories of stale references that the implementer's initial pass missed: credential seeding SQL, multi-value Grafana regex, EnvFilter tracing strings, Rust test K8s labels, and shell script kubectl commands. The specialist review process proved its value — each finding came from a different reviewer's domain expertise.

---

## Human Review (Iteration 3)

**Feedback**: "The local dev setup.sh script has an error — `valid_service_type` CHECK constraint rejects `gc-service` because the database only allows `global-controller`, `meeting-controller`, `media-handler`. The OAuth client_id, service_type, and client_secret values are domain identifiers, not service/crate names — they should not have been renamed. Revert these credential values back to original names in K8s deployments, secrets, and setup.sh."

**Mode**: Light (3 teammates — implementer + security + operations)

### Implementation Summary (Iteration 3)

Reverted OAuth credential values back to original database domain identifiers in 6 files:
1. `infra/services/gc-service/deployment.yaml`: GC_CLIENT_ID back to `"global-controller"`
2. `infra/services/gc-service/secret.yaml`: secret back to `global-controller-secret-dev-001`
3. `infra/services/mc-service/deployment.yaml`: MC_CLIENT_ID back to `"meeting-controller"`
4. `infra/services/mc-service/secret.yaml`: secret back to `meeting-controller-secret-dev-002`
5. `infra/kind/scripts/setup.sh`: SQL seeds, bcrypt hashes, display all reverted to original names
6. `docs/runbooks/gc-incident-response.md`: SQL query reverted to `client_id = 'global-controller'`

### Validation (Iteration 3)
All layers pass (same pre-existing issues as iteration 2).

### Review Results (Iteration 3)
- Security: **APPROVED** — Full 4-layer credential chain verified consistent
- Operations: **APPROVED** — End-to-end setup flow verified, all 6 files consistent
