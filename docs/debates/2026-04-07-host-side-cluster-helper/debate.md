# Debate: Host-Side Cluster Helper for Devloop Integration Testing

**Date**: 2026-04-07
**Status**: Complete
**Participants**: Infrastructure, Security, Test, Observability, Operations
**Prior Art**: `docs/debates/2026-04-05-devloop-cluster-sidecar.md` (sidecar approach — PoC failed)
**ADR**: `docs/decisions/adr-0030-host-side-cluster-helper.md`

## Question

How should we design a host-side cluster helper for autonomous integration testing in devloop containers?

## Context

Kind-in-container approaches (podman, Docker, k3s) all failed on WSL2/cgroup v2. Kind works on the host. The pivot: helper process on the host communicates with devloop container via unix socket.

## Final Positions

| Specialist | Score | Position |
|------------|-------|---------|
| Infrastructure | 95 | Host-native Rust helper, envsubst templating, registry-based ports, host-side test execution, host.containers.internal |
| Security | 95 | Compiled Rust binary eliminates injection class. Model A with HOME override. Socket auth token. Kubeconfig isolation. |
| Test | 95 | ClusterPorts::from_env() only test code change. Host-side execution simplifies everything. Exit code classification. |
| Observability | 93 | Flat JSON port map, host.containers.internal, --skip-observability, listenAddress 127.0.0.1, health-based feature gating |
| Operations | 93 | PID file lifecycle, multi-layer orphan cleanup, 200-stride ports, separate target-dir, cluster-ready flag |

## Key Decisions

1. **Helper is a host process** (not container) — compiled Rust binary at `crates/devloop-helper/`
2. **Env-tests run on the host** via the helper — localhost works for all Kind NodePorts
3. **Dynamic port allocation** — hash-preferred with 200-stride blocks and registry file
4. **Socket auth token** — random 32-byte hex, rotated on restart
5. **`host.containers.internal`** for container→host traffic with gateway IP fallback
6. **Kind config templated** via envsubst with `listenAddress: 127.0.0.1`
7. **Kubeconfig never enters dev container** — helper acts as proxy
8. **Health-based feature gating** — observability tests only run when stack is healthy

## Decision

See ADR-0030: `docs/decisions/adr-0030-host-side-cluster-helper.md`
