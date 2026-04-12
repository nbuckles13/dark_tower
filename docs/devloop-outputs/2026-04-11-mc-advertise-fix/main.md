# Devloop Output: Fix MC/MH Advertise Address for Devloop Clusters

**Date**: 2026-04-11
**Task**: Patch MC/MH WebTransport advertise addresses with correct host-gateway IP and dynamic ports so env-tests can reach MC/MH from the devloop container and host
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/mc-connect-investigation`
**Duration**: ~45m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `8e53898e4ad6abb95e4e7a88126f4bfc4fb1f815` |
| Branch | `feature/mc-connect-investigation` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@mc-advertise-fix` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@mc-advertise-fix` |
| Test | `test@mc-advertise-fix` |
| Observability | `observability@mc-advertise-fix` |
| Code Quality | `code-reviewer@mc-advertise-fix` |
| DRY | `dry-reviewer@mc-advertise-fix` |
| Operations | `operations@mc-advertise-fix` |

---

## Task Overview

### Objective
Fix the MC/MH WebTransport advertise address mismatch that prevents env-tests from connecting to MC. MC pods register `https://localhost:4433` with GC, but in devloop clusters the actual reachable endpoint is `https://${HOST_GATEWAY_IP}:${DYNAMIC_PORT}`.

### Scope
- **Service(s)**: devloop-helper (Rust), setup.sh (shell), MC/MH ConfigMaps (K8s)
- **Schema**: No
- **Cross-cutting**: Yes — affects how MC/MH register with GC, impacts env-test connectivity

### Debate Decision
NOT NEEDED - Implementation approach is clear from ADR-0030 and confirmed by diagnostic investigation.

---

## Planning

All 6 reviewers confirmed the plan (security, test, observability, code-quality, DRY, operations). Key reviewer requirements incorporated:
- Defense-in-depth: IP validated in both Rust (`validate_gateway_ip`) and bash (IPv4 regex + 0.0.0.0 rejection)
- Single rollout cycle: patch between apply and rollout status (not after)
- Extract `DEFAULT_HOST_GATEWAY_IP` constant to avoid tripling the literal
- Log actual patched address values for debuggability
- Cover `cmd_deploy()` path (not just `cmd_setup()`)

---

## Pre-Work

Diagnostic investigation confirmed:
- MC-0 QUIC responds on `host.containers.internal:26212` (correct allocated port)
- MC-0 advertises `https://localhost:4433` (wrong — hardcoded in ConfigMap)
- Database confirms: `webtransport_endpoint = https://localhost:4433` for both MC pods
- Decision: use raw HOST_GATEWAY_IP (`10.255.255.254`) in advertise address for container + host reachability

---

## Implementation Summary

### Change 1: Port-map.env additions (`commands.rs:write_port_map_shell()`)
Added MC/MH WebTransport port variables to the shell-sourceable port-map.env file:
`MC_0_WEBTRANSPORT_PORT`, `MC_1_WEBTRANSPORT_PORT`, `MH_0_WEBTRANSPORT_PORT`, `MH_1_WEBTRANSPORT_PORT`

### Change 2: DT_HOST_GATEWAY_IP propagation (`commands.rs:cmd_setup()`, `cmd_deploy()`)
Pass gateway IP as env var to setup.sh. Extracted `DEFAULT_HOST_GATEWAY_IP` constant. Added `validate_gateway_ip()` call in `cmd_deploy()`.

### Change 3: ConfigMap patching (`setup.sh:deploy_mc_service()`, `deploy_mh_service()`)
After `kubectl apply -k`, when `DT_HOST_GATEWAY_IP` is set: validate IP, patch per-instance ConfigMaps with correct advertise address, rollout restart, wait for readiness. Gated on devloop mode only.

### Additional
- New `test_write_port_map_shell` unit test validating all port variables and setup.sh regex compliance
- `DT_HOST_GATEWAY_IP` documented in setup.sh header
- TLS SAN limitation documented (env-tests use `with_no_cert_validation()`)

---

## Files Modified

```
 crates/devloop-helper/Cargo.toml          |   1 +
 crates/devloop-helper/src/commands.rs     | 105 ++++++++++++++++++++++-
 infra/kind/scripts/setup.sh              |  42 ++++++++-
```

### Key Changes by File
| File | Changes |
|------|---------|
| `crates/devloop-helper/src/commands.rs` | `DEFAULT_HOST_GATEWAY_IP` constant, 4 port vars in `write_port_map_shell()`, `.env("DT_HOST_GATEWAY_IP")` in `cmd_setup()`/`cmd_deploy()`, unit test |
| `crates/devloop-helper/Cargo.toml` | `regex = "1"` dev-dependency for test |
| `infra/kind/scripts/setup.sh` | `DT_HOST_GATEWAY_IP` validation, ConfigMap patching + rollout restart in `deploy_mc_service()`/`deploy_mh_service()` |

---

## Devloop Verification Steps

### Layer 1: cargo check
**Status**: PASS

### Layer 2: cargo fmt
**Status**: PASS

### Layer 3: Simple Guards
**Status**: ALL PASS (16/16)

### Layer 4: Tests
**Status**: PASS (1162 tests, 0 failures)

### Layer 5: Clippy
**Status**: PASS

### Layer 6: Audit
**Status**: PASS (3 pre-existing vulnerabilities in quinn-proto, ring, rsa — none from this change)

### Layer 7: Semantic Guards
**Status**: SAFE

---

## Code Review Results

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0

Defense-in-depth validation verified at both Rust and shell layers. ADR-0030 prohibitions maintained. Shell injection prevented by input validation.

### Test Specialist
**Verdict**: CLEAR
**Findings**: 0

New `test_write_port_map_shell` covers all 11 port variables with regex compliance check.

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0

Logging follows existing conventions. Actual patched address values logged for debuggability. No impact on observability port configuration.

### Code Quality Reviewer
**Verdict**: RESOLVED
**Findings**: 1 found, 1 fixed

- Finding: Missing `DT_HOST_GATEWAY_IP` documentation in setup.sh header. Fixed.

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings**: None
**Extraction opportunities**: `Context::gateway_ip()` helper (3 sites), setup.sh patching helper (2 sites). Both low priority.

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0

Idempotent merge patches. Single rollout cycle. `--only` path covered. Error propagation via `set -euo pipefail`. No teardown changes needed.

---

## Tech Debt

### Deferred Findings

No deferred findings.

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| `ctx.host_gateway_ip.as_deref().unwrap_or(DEFAULT_HOST_GATEWAY_IP)` | `commands.rs` (3 sites) | N/A | Extract `Context::gateway_ip()` method |
| ConfigMap advertise patching | `setup.sh:deploy_mc_service()` | `setup.sh:deploy_mh_service()` | Extract helper if third service needs it |

### Temporary Code (from Code Reviewer)

No temporary code detected.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `8e53898e4ad6abb95e4e7a88126f4bfc4fb1f815`
2. Review all changes: `git diff 8e53898..HEAD`
3. Soft reset (preserves changes): `git reset --soft 8e53898`
4. Hard reset (clean revert): `git reset --hard 8e53898`
5. For infrastructure changes: may require re-running `setup.sh` to restore original ConfigMaps

---

## Reflection

All teammates updated their INDEX.md files with pointers for:
- `DEFAULT_HOST_GATEWAY_IP` constant and `validate_gateway_ip()` in commands.rs
- `write_port_map_shell()` MC/MH WebTransport port additions
- `DT_HOST_GATEWAY_IP` validation in setup.sh
- ConfigMap patching in `deploy_mc_service()`/`deploy_mh_service()`
- DRY extraction opportunities logged in TODO.md

---

## Issues Encountered & Resolutions

None.

---

## Lessons Learned

1. KIND extraPortMappings work correctly for UDP — the QUIC path through KIND is reliable. The failure was purely an advertise address mismatch.
2. Defense-in-depth validation at both Rust and shell boundaries catches issues at each process boundary.
3. ConfigMap patching + rollout restart is a clean pattern for devloop-specific configuration without modifying static manifests.

---

## Appendix: Verification Commands

```bash
# Verify ConfigMap patching
kubectl get configmap mc-0-config -n dark-tower -o jsonpath='{.data.MC_WEBTRANSPORT_ADVERTISE_ADDRESS}'

# Verify DB registration
kubectl exec -n dark-tower postgres-0 -- psql -U darktower -d dark_tower \
  -c "SELECT controller_id, webtransport_endpoint FROM meeting_controllers;"

# QUIC probe
python3 -c "import socket; s=socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.settimeout(3); s.sendto(b'\xc0\x00\x00\x01'+b'\x00'*50, ('10.255.255.254', 26212)); print(s.recvfrom(4096))"

# Run env-tests
cargo test -p env-tests
```
