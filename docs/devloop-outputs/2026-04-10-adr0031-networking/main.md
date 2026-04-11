# Devloop Output: Implement ADR-0030 devloop cluster networking

**Date**: 2026-04-10
**Task**: Implement ADR-0030 — host-gateway listenAddress for devloop cluster networking
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) - full
**Branch**: `feature/adr0030-helper-binary`
**Duration**: ~30m

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `14d0fbb0943118be7276b8c12b86ff588ac0af9f` |
| Branch | `feature/adr0030-helper-binary` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@adr0031-networking` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@adr0031-networking` |
| Test | `test@adr0031-networking` |
| Observability | `observability@adr0031-networking` |
| Code Quality | `code-reviewer@adr0031-networking` |
| DRY | `dry-reviewer@adr0031-networking` |
| Operations | `operations@adr0031-networking` |

---

## Task Overview

### Objective
Fix devloop container networking per ADR-0030: bind Kind NodePorts to podman host-gateway IP, switch to named podman network, add K8s API to extraPortMappings, detect gateway IP dynamically.

### Scope
- **Service(s)**: infra/kind/kind-config.yaml.tmpl, infra/devloop/devloop.sh, crates/devloop-helper/src/
- **Schema**: No
- **Cross-cutting**: No (dev tooling only)

### Debate Decision
ADR-0030 accepted via design debate (amends ADR-0030)

---

## Planning

Step 0 verification completed before this devloop — all networking tests passed. All 6 reviewers confirmed the plan.

---

## Implementation Summary

### `infra/kind/kind-config.yaml.tmpl`
- All 19 `listenAddress` values use `${HOST_GATEWAY_IP}` (was `0.0.0.0`)
- K8s API added to extraPortMappings (containerPort 6443, gateway-bound)
- `apiServerAddress: "127.0.0.1"` preserved for host-side kubectl
- Dual-binding strategy documented in comments

### `infra/devloop/devloop.sh`
- `detect_host_gateway_ip()`: podman info -> /etc/hosts -> actionable error (ADR-0030 S2)
- Named network `devloop-${TASK_SLUG}-net` replaces `--network container:$DB_CONTAINER`
- `DATABASE_URL` uses `${DB_CONTAINER}:5432` (container DNS)
- `--host-gateway-ip` passed to helper binary
- Network cleanup in cleanup(), --recreate, and orphan detection
- `is_helper_process_alive()` extracted as shared function

### `crates/devloop-helper/src/`
- `main.rs`: accepts `--host-gateway-ip` CLI arg, passes to Context
- `commands.rs`: `validate_gateway_ip()` rejects 0.0.0.0/:: (ADR-0030 S7), `CONTAINER_HOST` constant, kubeconfig rewrites both host and port for dual-binding
- `ports.rs`: `template_env_vars()` includes `HOST_GATEWAY_IP`, `host_urls` includes all 3 observability endpoints

### `infra/devloop/dev-cluster`
- Browser access section shows all 3 observability URLs

---

## Devloop Verification Steps

### Layers 1-5: PASS
cargo check, fmt, guards (16/16), tests (108 pass), clippy clean.

### Layer 7: Semantic Guard PASS

---

## Code Review Results

### Security Specialist
**Verdict**: RESOLVED — 1 finding fixed (gateway IP validation)

### Test Specialist
**Verdict**: RESOLVED — 5 findings fixed (kubeconfig rewrite, detection cascade, gateway guard)

### Observability Specialist
**Verdict**: RESOLVED — 1 finding fixed (host_urls + browser display)

### Code Quality Reviewer
**Verdict**: RESOLVED — 1 deferred (status command)

### DRY Reviewer
**Verdict**: CLEAR — no duplication

### Operations Reviewer
**Verdict**: RESOLVED — 2 findings fixed (orphan + recreate network cleanup)

---

## Tech Debt

| Finding | Reviewer | Deferral Justification |
|---------|----------|------------------------|
| `status` helper command | Code Quality | Diagnostic convenience, not on critical path for steps 4-5. Needed before Layer 8 (step 6). |

---

## Rollback Procedure

1. Start commit: `14d0fbb0943118be7276b8c12b86ff588ac0af9f`
2. `git reset --hard 14d0fbb`

---

## Reflection

All 8 teammates updated INDEX.md files. INDEX guard passed after fixing a stale debate pointer.

---

## Lessons Learned

1. `10.255.255.254` is podman's host-gateway on the loopback — reachable from containers but not LAN
2. Kind `apiServerAddress` controls BOTH the kubeconfig server URL and the port binding — can't set them independently
3. Dual-binding (apiServerPort on 127.0.0.1 + extraPortMappings on gateway IP) solves the K8s API access problem
4. Named podman networks are strictly better than `--network container:` for this use case
5. Step 0 verification scripts save enormous debugging time
