# Debate: Devloop Container ↔ K8s Cluster Networking

**Date**: 2026-04-09
**Status**: Complete
**Participants**: Infrastructure, Security, Test, Observability, Operations

> **Note**: When cross-cutting specialists (Security, Test, Observability, Operations) score < 70 satisfaction at consensus, this requires explicit user risk acceptance — not implicit majority override. See ADR-0024 §5.7.

## Question

How should devloop containers access a K8s cluster for autonomous integration testing? ADR-0030's current approach (host-side helper + container-side test execution via `host.containers.internal`) has a fundamental networking gap: the dev container shares a network namespace with the DB container (`--network container:$DB_CONTAINER`), so its localhost is the DB's localhost — not the host's. Kind NodePorts bind to `127.0.0.1` on the host, making them unreachable from inside the container. This breaks ALL Kind service access, kubectl, and the K8s API server.

## Context

### What Works Today (ADR-0025)
- Devloop containers compile, run unit tests, run guards, do static analysis
- `--dangerously-skip-permissions` provides full autonomy inside the container
- Container isolation prevents access to SSH keys, GitHub credentials, cloud creds, Windows filesystem
- `--network container:$DB_CONTAINER` gives localhost access to PostgreSQL

### What ADR-0030 Tried to Add
- Host-side helper binary managing a Kind cluster per devloop
- Container-side env-test execution via `host.containers.internal` to reach Kind NodePorts
- Unix socket protocol for build/deploy commands

### Why It Failed
- `--network container:$DB_CONTAINER` means container's localhost ≠ host's localhost
- `host.containers.internal` resolves to `10.255.255.254` (podman's host gateway IP)
- Kind NodePorts bound to `127.0.0.1` are unreachable via `10.255.255.254`
- This breaks: service HTTP/gRPC access, kubectl to K8s API, observability endpoints
- Even binding to `0.0.0.0` doesn't help for AC/GC (ClusterIP services, not NodePort)

### What Parts of ADR-0030 Remain Valid
- Steps 1-3 (ClusterPorts::from_env, setup.sh parameterization, Kind config template) are useful regardless of networking approach
- The port allocation scheme and env-test URL configuration work for any approach
- Helper binary for build/deploy orchestration (but not networking)
- The fundamental need (autonomous integration testing) is unchanged

## Options Discussed

### Option A: `--network=host` for Dev Container
Give the dev container host networking. Container's localhost IS host's localhost. Kind NodePorts, K8s API, everything reachable.

**Trade-off**: Container running `--dangerously-skip-permissions` Claude gains full host network access — can reach any localhost-bound service, scan the LAN, bind ports on the host.

### Option B: Dedicated K8s Cluster on Separate Machine
Real K8s cluster on a dedicated Linux box. Each devloop gets its own namespace.

**Trade-off**: Requires additional hardware, cluster administration. Over-engineered for local dev iteration.

### Option C: Keep Container Isolated, Run Tests on Host
Run env-tests from the host. Claude can't iterate on test failures autonomously.

**Trade-off**: Splits the workflow. Claude loses the feedback loop that makes devloops valuable.

### Option D: ADR-0030 with Networking Fix (listenAddress)
Fix the networking gap by binding Kind NodePorts to the podman host-gateway address (`10.255.255.254`) instead of `127.0.0.1`. This address is on the host's loopback interface (not LAN-reachable) but IS reachable from containers via `host.containers.internal`.

## Discussion

### Round 1: Initial Positions

- **Infrastructure** (85%): Favored Option A. Simplest solution — one-line change. Security trade-off acceptable since real isolation is filesystem/credentials, not network.
- **Security** (78%): Opposed Option A. `--network=host` removes network isolation entirely — Claude can reach all localhost services, scan LAN, bind ports. Favored Option D with verification.
- **Test** (75%): Favored Option A. Zero test code changes, fastest feedback loops, full portability.
- **Observability** (88%): Favored Option A. Direct localhost access to Prometheus/Grafana/Loki for debugging.
- **Operations** (40%): Leaned Option A. Massive operational simplification — eliminates helper binary, socket protocol, auth tokens.

### Round 2: Security's Rebuttal Shifts Test and Observability

Security made the argument that `--dangerously-skip-permissions` is the threat and the container is the mitigation — weakening the mitigation because the threat exists is circular reasoning. Key points:
- `--network=host` exposes ALL localhost services (other dev servers, admin panels — many assume localhost is trusted)
- LAN scanning/access enables reaching poorly-secured devices
- Port binding allows impersonating services or creating reverse shells
- Network isolation is a distinct defense-in-depth layer worth preserving

Test shifted to Option D (85%→92%): testing experience is identical since `ClusterPorts::from_env()` abstracts URLs.
Observability shifted to Option D (82%): only needs specific port access, not full host networking.

### Round 3: The `10.255.255.254` Breakthrough

Operations and Infrastructure independently discovered that `10.255.255.254` (the IP that `host.containers.internal` resolves to) is bound to the host's loopback interface:

```
$ ip addr show lo
inet 10.255.255.254/32 brd 10.255.255.254 scope global lo
```

Empirical verification:
- Services bound to `10.255.255.254:PORT` on host: reachable from container via `host.containers.internal:PORT`
- Same services via `127.0.0.1:PORT`: NOT reachable from container (different loopback binding)
- `10.255.255.254`: NOT LAN-reachable (loopback-scoped)

This means Kind NodePorts with `listenAddress: "10.255.255.254"` are:
- Reachable from the dev container (via `host.containers.internal`)
- NOT reachable from `127.0.0.1` (other localhost services remain isolated)
- NOT reachable from the LAN (no `0.0.0.0` exposure)

Infrastructure shifted from Option A to Option D (95%).
Operations shifted from Option A to Option D (92%).

### Round 4: Consensus

All five specialists aligned on Option D with `listenAddress: "10.255.255.254"`:
- No `--network=host` needed
- No scoped TCP forwarding needed
- No socat/tunnel complexity
- All ADR-0025 isolation guarantees preserved
- Helper binary scope reduced to build/deploy only

## Consensus

**Reached at Round 4. All participants at 90%+.**

| Specialist | Final Satisfaction | Final Position |
|---|---|---|
| Infrastructure | 95% | Option D with `listenAddress: "10.255.255.254"`. Empirically verified on WSL2. |
| Security | 95% | Option D. All ADR-0025 security properties preserved. `--network=host` explicitly prohibited. |
| Test | 95% | Option D. Zero test code changes. `ClusterPorts::from_env()` with `host.containers.internal` URLs. |
| Observability | 93% | Option D. Prometheus/Grafana/Loki directly reachable. Want observability access documented as requirement. |
| Operations | 92% | Option D. Gateway-IP binding makes it operationally clean. Dynamic IP detection, orphan cleanup. |

## Decision

Amend ADR-0030: Change Kind NodePort `listenAddress` from `"127.0.0.1"` to the podman host-gateway address (currently `10.255.255.254`, detected dynamically). This fixes the networking gap without `--network=host` or any additional forwarding layer.

See: ADR-0030 (amended 2026-04-09)
