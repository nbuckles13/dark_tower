# Devloop Cluster Sidecar — Integration Testing in Devloop Containers

**Status**: Pre-debate design document
**Date**: 2026-04-05

## Problem

Devloop containers can compile, run unit tests, and do static analysis — but they cannot deploy services or run integration tests (env-tests). This creates a long manual cycle:

1. Devloop makes a change (autonomous, fast)
2. Human runs `setup.sh` (~7 min full rebuild)
3. Human runs env-tests manually
4. Human investigates failures
5. Human kicks off another devloop to fix
6. Goto 2

The problems this misses until manual testing — contract mismatches, missing scopes, wrong addressing, deployment config errors — are exactly the kind of inter-service integration bugs that are common and expensive to iterate on.

## Goal

Make the devloop fully autonomous end-to-end: implement → build → deploy → integration test → iterate on failures — all without human intervention.

## Design

### Architecture: Three-Container Pod

Replace the current two-container setup (dev + postgres) with three containers sharing a network namespace:

```
┌──────────────────────────────────────────────────────┐
│  Shared network namespace (localhost)                 │
│                                                      │
│  ┌──────────────┐  ┌──────┐  ┌────────────────────┐  │
│  │ Dev container │  │ PG   │  │ Cluster helper     │  │
│  │ (unprivileged)│  │      │  │ (privileged)       │  │
│  │              │  │      │  │                    │  │
│  │ Claude Code  │  │:5432 │  │ podman, kind,      │  │
│  │ cargo, rust  │  │      │  │ kubectl             │  │
│  │              │  │      │  │                    │  │
│  │ dev-cluster  │──│──────│──│→ API on unix socket │  │
│  │ (client CLI) │  │      │  │   or TCP port       │  │
│  └──────────────┘  └──────┘  └────────────────────┘  │
└──────────────────────────────────────────────────────┘
```

- **Dev container** (unprivileged): Claude Code with `--dangerously-skip-permissions`. Same as today plus a `dev-cluster` client CLI.
- **PostgreSQL**: Same as today.
- **Cluster helper** (privileged): Runs podman, Kind, kubectl. Exposes a narrow API for cluster operations. This is where the Kind cluster lives.

The cluster helper needs `--privileged` to run podman inside the container (nested containerization for Kind). Claude Code never gets elevated privileges — it calls the helper via a restricted API.

### Cluster Helper API

The helper exposes a fixed set of operations. No shell access, no arbitrary command execution.

**Allowed operations:**

| Command | Description |
|---------|-------------|
| `setup` | Full setup.sh (first-time cluster creation) |
| `rebuild <service>` | Build one service image, load into Kind, restart |
| `rebuild-all` | Rebuild all service images |
| `deploy <service>` | Apply manifests only (no image rebuild) |
| `env-test [filter]` | Run env-tests against the cluster |
| `logs <service> [--tail=N]` | kubectl logs for a service |
| `status` | Cluster health, pod status, service readiness |
| `restart <service>` | Rollout restart without rebuild |

**Security hardening:**
- Strict command allowlist — no shell interpolation
- Arguments validated (service name must be one of ac/gc/mc/mh, test filter must be alphanumeric + underscores)
- All stdout/stderr streamed back to the caller
- Every request logged for auditability

**Implementation:** Shell script or small binary listening on a unix socket. The dev container gets a thin client script (`dev-cluster`) that writes to the socket and reads the response.

### Helper Image

The helper script should be bind-mounted from the source repo (`infra/devloop/cluster-helper/`) rather than baked into the image. This way changes to the helper logic take effect immediately without rebuilding the image. The image only contains base tools:

- podman
- kind
- kubectl
- socat or similar for the API socket

The source repo is bind-mounted at `/work` (same as dev container) so the helper can build service images from source.

Image caching: A persistent podman volume for the helper's image store so third-party images and cargo-chef dependency layers survive container restarts.

### devloop.sh Changes

- Always launch the cluster helper alongside dev and postgres (no opt-in flag needed; cheap when cached)
- `--rebuild` rebuilds both the dev image and the cluster helper image
- `--recreate` destroys all three containers
- Print service URLs after setup (Grafana, Prometheus, etc.)

### Network Namespace and Multi-Devloop Port Isolation

**Known constraint:** ADR-0025 documents that `--pod` is incompatible with `--userns=keep-id` in podman. The current workaround is `--network container:$DB_CONTAINER`.

For multiple concurrent devloops, each needs its own set of host ports (Grafana, Prometheus, service endpoints). Options:

1. **Dynamic host ports on the network-owning container:** Use `-p 0:3000 -p 0:9090 -p 0:8080 ...` on the DB container. Podman assigns random available ports. Query with `podman port`. Print URLs to user.
2. **Check if `--pod` + `--userns=keep-id` incompatibility has been resolved** in current podman version. If so, use podman pods with dynamic ports (cleaner abstraction).
3. **Separate podman network per devloop** with container-name DNS resolution. Host access via `podman port` or explicit forwarding.

**Debate question:** What's the best approach for multi-devloop port isolation given the `--userns=keep-id` requirement?

### MC/MH Per-Instance Addressing (Prerequisite)

MC and MH are stateful — meeting/media actors live on specific pods. Clients must connect to the assigned pod, not a load-balanced service. The current Kind setup has MC behind a load-balanced NodePort, which is architecturally wrong.

**Exposure model:**

| Service | Interface | Exposure | Why |
|---------|-----------|----------|-----|
| AC | HTTP | Load-balanced | Stateless auth API |
| GC | HTTP | Load-balanced | Stateless meeting API |
| MC | QUIC/WebTransport | Per-pod external | Clients connect to assigned pod |
| MH | QUIC | Per-pod external | Clients connect to assigned pod |
| GC→MC | gRPC | Internal (cross-cluster capable) | Assignment, heartbeats |
| GC→MH | gRPC | Internal (cross-cluster capable) | Registration, load reports |
| MC→MH | gRPC | Internal (cross-cluster capable) | Media routing |

**Kind implementation:** Convert MC/MH Deployment to StatefulSet. Each pod gets a unique host port via ordinal: MC-0 → 4433, MC-1 → 4435, MH-0 → 4434, MH-1 → 4436. Kind config declares all port mappings statically (replica count is a design-time decision in Kind, unlike production where ExternalDNS handles it dynamically).

Each pod computes its advertise address from its hostname ordinal.

**Env-test changes:** Tests use `gc_join.mc_assignment.webtransport_endpoint` (pod-specific address from GC join response) instead of a hardcoded service URL.

### setup.sh Incremental Mode

New flags for targeted operations:

| Flag | Behavior |
|------|----------|
| `--only <service>` | Rebuild + redeploy only the specified service |
| `--skip-build` | Apply manifest changes only (no image rebuild) |
| (no flags) | Full setup (existing behavior) |

Idempotent checks for each step:

| Operation | Full | `--only gc` |
|-----------|------|-------------|
| Create cluster | Yes | Skip if exists |
| Create namespaces | Yes | Skip if exist |
| Pre-load third-party images | Yes | Skip if present |
| Deploy postgres/redis | Yes | Skip if running |
| Build service images | All 4 | Only gc |
| Load images into Kind | All 4 | Only gc |
| Deploy services | All 4 | Only gc (`kubectl apply -k`) |
| Deploy observability | Yes | Skip |
| Seed data | Yes | Skip if exists |

`kubectl apply -k` is already idempotent — the missing piece is making image build and load conditional.

### Devloop Skill Changes

Add **Layer 8: Integration validation** to the validation pipeline, after semantic guard:

**Trigger heuristics** — run if `git diff --name-only` includes:
- `crates/*/src/handlers/` or `crates/*/src/grpc/` (API/contract changes)
- `crates/common/` (shared types)
- `proto/` (protocol changes)
- `infra/` (deployment changes)
- `crates/*/src/config.rs` (service config)
- `migrations/` (schema changes)

**Steps:**
1. `dev-cluster rebuild <affected-service>` (or `rebuild-all` if common/proto changed)
2. `dev-cluster env-test` (or filtered to relevant tests)
3. Parse results

**Failure handling:**
- **Clear env-test failure** (assertion error, 4xx/5xx): Send to implementer as findings, count toward 3-attempt validation limit
- **Infrastructure failure** (pod crash, timeout, cluster issue): Retry once, then escalate to user
- **First-time setup:** `dev-cluster setup` before first integration run (~7 min, only once per container lifetime)

### Grafana / Observability Access

The shared network namespace means all Kind NodePorts are reachable from the dev container on localhost. To access from the host machine (browser), ports need to be mapped when creating the containers. With dynamic port allocation (`-p 0:3000`), each devloop gets unique host ports. `devloop.sh` prints the URLs after setup.

## Open Questions for Debate

1. **`--pod` + `--userns=keep-id` compatibility**: Has this been fixed in recent podman versions? If not, what's the best multi-devloop port isolation approach?
2. **Kind-in-container feasibility**: Does podman-in-podman with Kind work reliably? Any known issues with nested containerization for Kind clusters?
3. **Privileged sidecar security**: Given Claude Code runs with `--dangerously-skip-permissions` in the adjacent unprivileged container, is the narrow API sufficient? What attack vectors exist through the shared network namespace or bind mount?
4. **Env-test trigger heuristics**: Are the proposed trigger patterns (handlers, grpc, common, proto, infra, config, migrations) the right set? Too broad means slow devloops. Too narrow means missed integration bugs.
5. **Resource requirements**: A Kind cluster + 4 services + postgres + redis + observability stack inside a container — what are realistic memory/CPU requirements? Can a dev machine run 2-3 of these concurrently?

## Dependency Order

1. **MC/MH StatefulSet + per-pod addressing** — prerequisite, independent
2. **setup.sh incremental mode** — prerequisite, independent
3. **Cluster helper sidecar** — image, API server, security hardening
4. **devloop.sh changes** — launch sidecar, install client, port management
5. **Devloop skill changes** — integration validation layer

Steps 1-2 are independent and can be done in parallel. Steps 3-5 are sequential.

## References

- ADR-0025: Containerized Dev-Loop Execution
- `infra/devloop/devloop.sh` — current wrapper script
- `infra/devloop/Dockerfile` — current dev container image
- `infra/kind/scripts/setup.sh` — cluster setup script
- `docs/TODO.md` — "Resumable setup.sh" and "Skip unchanged service image builds" items
