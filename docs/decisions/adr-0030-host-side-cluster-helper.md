# ADR-0030: Host-Side Cluster Helper for Autonomous Integration Testing

**Status**: Accepted

**Date**: 2026-04-07 (amended 2026-04-09)

**Deciders**: Infrastructure, Security, Test, Observability, Operations (via design debate)

---

## Context

Devloop containers can compile, run unit tests, and do static analysis — but cannot deploy services or run integration tests (env-tests). This creates a manual cycle: devloop implements → human runs setup.sh (~7 min) → human runs env-tests → human investigates failures → repeat.

The original ADR-0030 design proposed a privileged sidecar container running Kind inside the devloop pod. **PoC testing revealed this is not feasible on WSL2 with cgroup v2:**

- **Kind-in-podman**: nftables fails (netavark), journald logging fails, `firewall_driver=none` → API server static pods never start
- **Kind-in-Docker-in-podman**: dockerd starts, but Kind fails at "Preparing nodes" — systemd can't initialize in nested cgroup v2
- **k3s-in-podman**: cpuset cgroup not found → fixed, then /proc/sys permission denied → fixed with KubeletInUserNamespace, then mkdir /sys/fs/cgroup/kubepods permission denied

**Root cause**: All three fail because K8s control plane components need cgroup write access that nested containers on cgroup v2 don't provide.

**Kind works perfectly on the host** (tested: cluster creation in 13s, pods schedule normally). This ADR documents the pivot to a host-side helper architecture.

**Constraints:**
- ADR-0025 requires `--userns=keep-id` for file ownership (incompatible with `--pod` in podman)
- Dev container on a named podman network cannot reach `127.0.0.1`-bound host services (separate loopback binding)
- Multiple concurrent devloops must be supported with isolated clusters
- Dev machines range from 16 GB to 64 GB RAM

## Decision

Run a **host-side helper process** that bridges the devloop container and a host-managed Kind cluster. Each devloop gets its own named cluster with dynamically allocated ports.

### Architecture

```
Host
├── Helper process (per-devloop, background)
│   ├── Compiled Rust binary (crates/devloop-helper/)
│   ├── Listens on unix socket at /tmp/devloop-{slug}/helper.sock
│   ├── Has direct access to podman, kind
│   └── Manages dedicated Kind cluster: devloop-{slug}
│
├── Kind cluster: devloop-{slug} (host-level, normal Kind)
│   └── NodePorts bound on ${HOST_GATEWAY_IP} (podman host-gateway address)
│
└── Named podman network: devloop-{slug}-net
    ├── Dev container (unprivileged, Claude Code)
    │   ├── kubectl + kubeconfig (read-only, targets devloop cluster)
    │   ├── Runs env-tests directly (cargo test -p env-tests)
    │   ├── Reaches Kind services via host.containers.internal → ${HOST_GATEWAY_IP}
    │   └── dev-cluster client → unix socket → helper (build/deploy only)
    └── DB container (postgres, reachable via container DNS name)
```

### Helper Process

**Implementation**: Compiled Rust binary at `crates/devloop-helper/` in the workspace. Dependencies: `serde_json` (port map, protocol), `std::os::unix::net::UnixListener` (blocking socket). No service crate dependencies. No async runtime needed — one-client-at-a-time semantics are sufficient.

**Role**: The helper is a **build-and-deploy tool only**. It manages cluster lifecycle (create, teardown) and image builds (podman build, kind load). It does NOT run tests, proxy kubectl, or serve logs — the dev container has kubectl + kubeconfig and runs env-tests directly. This keeps tests portable and helper-agnostic.

**Why Rust binary, not shell script**: The helper parses untrusted input from a unix socket (controlled by Claude Code with `--dangerously-skip-permissions`) and executes host-level commands. A compiled binary with `Command::new().arg()` makes injection structurally impossible — arguments are passed as OS-level argv entries with no shell interpretation. Shell scripts are vulnerable to word-splitting and quoting bugs even with strict discipline. (Note: a shell script with security's 5 hardening conditions — allowlist validation, array-based commands, shellcheck strict, injection test suite, no eval — is an acceptable intermediate for rapid prototyping if needed, but the target implementation is a compiled binary.)

**Injection regression tests**: The helper crate includes `#[cfg(test)]` tests that send malformed inputs through the socket (shell metacharacters, oversized payloads, null bytes, newlines in arguments) and verify they are all rejected before any command execution.

**Invariant**: The helper binary is built from and launched via `$REPO_ROOT/target/release/devloop-helper`. Claude's modifications inside the dev container (at `$CLONE_DIR/work`) cannot alter the helper binary — it is compiled from the source repo on the host.

**Build-context trichotomy**: Three distinct path-roots, each protecting a different concern:

| Layer | Path | Purpose |
|---|---|---|
| Helper *binary source* | `REPO_ROOT` | Security: container cannot tamper. Used by `build_helper()` (`cargo build --manifest-path $REPO_ROOT/Cargo.toml`). |
| Helper *runtime project-root* | `CLONE_DIR` | Function: service builds, kind-config, setup.sh must reflect the devloop's branch state. Passed as `--project-root` arg to the helper at launch; consumed as `ctx.project_root` in `cmd_setup` / `cmd_rebuild` / `cmd_deploy`. |
| Pod runtime | container fs | Sandboxed; no host access. |

The runtime project-root determines where `podman build` looks for source, where `setup.sh` reads manifests from, and where the kind-config template is read from. Setting it to `REPO_ROOT` (the original implementation, fixed 2026-05-01) caused service rebuilds to silently produce stale images: `/work` inside the dev container is mounted from `CLONE_DIR`, so edits made by Claude landed there — but `podman build` ran against `REPO_ROOT` and saw whatever was last committed to the user's main checkout. Builds full-cache-hit and looked successful in <3s; the rebuilt image was byte-identical to whatever was last built. The fix is `--project-root "$CLONE_DIR"` in `devloop.sh`'s helper launch, with the clone-creation step hoisted to run before `launch_helper` so `CLONE_DIR` always exists when the helper starts.

The trichotomy is the load-bearing safety property: pointing the helper's *runtime* commands at a container-writable path is *correct* because that's what the user is editing; pinning the helper *binary* to a container-immutable path is what blocks the otherwise-obvious tamper attack (edit `crates/devloop-helper/src/`, exit, re-run, persistent host-side compromise).

**Lifecycle**:
- `devloop.sh` builds the helper (`cargo build --release -p devloop-helper`) on first use, caches the binary
- Launched as a background process with PID file at `/tmp/devloop-{slug}/helper.pid`
- Stale PID detection on startup (`kill -0`); reuse if alive, clean up and restart if dead
- SIGTERM → graceful shutdown (finish current operation, exit)
- Crash recovery: Kind cluster persists on host; dev container detects dead socket and prints actionable error

**Cleanup**:
- `devloop.sh` cleanup function kills helper, deletes Kind cluster (`kind delete cluster --name devloop-{slug}`), removes `/tmp/devloop-{slug}/`
- Orphan detection on startup: scan for `devloop-*` Kind clusters with no corresponding running devloop container; prompt user to delete

### Helper API (Unix Socket)

The helper handles only operations that require the host's container runtime (podman) and Kind CLI. Everything else (tests, logs, status) is done directly from the dev container via kubectl.

| Command | Description | Why host-only |
|---------|-------------|---------------|
| `setup` | Allocate ports, generate kind-config, create cluster, run setup.sh | Requires `kind create cluster`, `podman build` |
| `rebuild <service>` | Build one service image, load into Kind, restart deployment | Requires `podman build`, `kind load image-archive` |
| `rebuild-all` | Rebuild all service images | Same |
| `deploy <service>` | Apply manifests only (no image rebuild) | Uses setup.sh which manages kind-specific operations |
| `teardown` | Delete Kind cluster, clean up all state | Requires `kind delete cluster` |
| `status` | Report cluster existence, API reachability, port allocations, readiness flag | Helper-internal state + kubectl connectivity check |

**Input validation**: All arguments validated via Rust enums/match. Service names: `ac`, `gc`, `mc`, `mh` (exhaustive enum match). No shell interpolation — all commands use `Command::new().arg()`.

**Socket authentication**: Helper generates a random 32-byte hex token at startup, writes to `/tmp/devloop-{slug}/auth-token` (chmod 0600). The `dev-cluster` client reads the token from the bind-mounted file. Every socket request must include the token; helper rejects invalid/missing tokens. Token rotates on helper restart.

**File permissions**: All files in `/tmp/devloop-{slug}/` are chmod 0600 (socket, PID file, auth token, log, ports.json). The directory itself is 0700. This prevents other users on the host from accessing the helper's state.

**Audit logging**: Every request logged to `/tmp/devloop-{slug}/helper.log` with timestamp, command, arguments, duration, exit code.

### Container-Side Test Execution

Env-tests run **inside the devloop container**, consistent with ADR-0025's containerized compilation principle. The dev container has:

1. **kubectl**: Installed in the dev container image
2. **kubeconfig**: Generated by the helper, pointing to `host.containers.internal:${K8S_API_PORT}`, bind-mounted read-only into the container
3. **Network access to Kind services**: Via `host.containers.internal` (or gateway IP fallback) to dynamically allocated NodePorts

This keeps tests portable and helper-agnostic — the same env-tests work when run manually on the host (where `localhost` reaches Kind) or inside the devloop container (where `host.containers.internal` reaches Kind). The only difference is the URL env vars.

**Security assessment**: The kubeconfig grants cluster-admin on a dev-only Kind cluster containing only dev fixtures. Claude Code with `--dangerously-skip-permissions` can already read/write all source code, execute arbitrary commands, and access the internet. The kubeconfig adds direct K8s cluster manipulation but this is a shortcut to capabilities Claude already has via code changes + rebuild. The blast radius is contained to a single developer's local Kind cluster.

**kubectl tests**: The existing `00_cluster_health.rs` tests that call `kubectl` directly (secrets-not-in-env-vars, secrets-not-in-logs) work unchanged inside the container since kubectl and kubeconfig are available.

### Dynamic Port Allocation

**Scheme**: Hash-preferred with registry file and 200-stride blocks.

1. Hash task slug to preferred index in [0, 49]
2. Base port = 20000 + (index × 200)
3. Check registry file (`~/.cache/devloop/port-registry.json`) for collision
4. If collision, next free index
5. Verify all ports are free (unprivileged TCP connect via `nc -z localhost $PORT`)
6. Register allocation with PID for orphan detection

**Port assignment** (offset from base):

| Offset | Service | Protocol |
|--------|---------|----------|
| +0 | AC HTTP | TCP |
| +1 | GC HTTP | TCP |
| +2 | GC gRPC | TCP |
| +10 | MC-0 Health | TCP |
| +11 | MC-0 gRPC | TCP |
| +12 | MC-0 WebTransport | UDP |
| +13 | MC-1 Health | TCP |
| +14 | MC-1 gRPC | TCP |
| +15 | MC-1 WebTransport | UDP |
| +20 | MH-0 Health | TCP |
| +21 | MH-0 gRPC | TCP |
| +22 | MH-0 WebTransport | UDP |
| +23 | MH-1 Health | TCP |
| +24 | MH-1 gRPC | TCP |
| +25 | MH-1 WebTransport | UDP |
| +100 | Prometheus | TCP |
| +101 | Grafana | TCP |
| +102 | Loki | TCP |
| +103 | K8s API (host, 127.0.0.1) | TCP |
| +104 | K8s API (gateway, ${HOST_GATEWAY_IP}) | TCP |

MC/MH replica counts are read from the deployment manifests at setup time (e.g., `kubectl get deployment mc-service -o jsonpath='{.spec.replicas}'`). Ports are allocated per instance (MC-0, MC-1, etc.) to support per-pod addressing. Observability ports placed at +100 to leave room for additional service instances. 200-port stride accommodates growth.

### Port Map File

**Format**: Flat JSON at `/tmp/devloop-{slug}/ports.json`

```json
{
  "cluster_name": "devloop-td-42",
  "host_gateway_ip": "10.255.255.254",
  "host": "host.containers.internal",
  "ports": {
    "ac_http": 24200,
    "gc_http": 24201,
    "gc_grpc": 24202,
    "mc_0_webtransport": 24212,
    "mc_1_webtransport": 24215,
    "mh_0_webtransport": 24222,
    "prometheus": 24300,
    "grafana": 24301,
    "loki": 24302,
    "k8s_api": 24303,
    "k8s_api_gateway": 24304
  },
  "container_urls": {
    "ac": "http://host.containers.internal:24200",
    "gc": "http://host.containers.internal:24201",
    "k8s_api": "https://host.containers.internal:24304",
    "prometheus": "http://host.containers.internal:24300",
    "grafana": "http://host.containers.internal:24301"
  },
  "host_urls": {
    "ac": "http://10.255.255.254:24200",
    "gc": "http://10.255.255.254:24201",
    "k8s_api": "https://127.0.0.1:24303",
    "grafana": "http://10.255.255.254:24301"
  },
  "created_at": "2026-04-07T10:30:00Z"
}
```

- `container_urls`: for env-tests and kubectl inside the dev container (use `host.containers.internal`)
- `host_urls`: for host-side tools (use `${HOST_GATEWAY_IP}` for NodePorts, `127.0.0.1` for K8s API)
- Human browser access to Grafana requires `kubectl port-forward` or a separate `127.0.0.1` port mapping
- Written by helper after cluster creation, read by `dev-cluster` client and `devloop.sh`
- Host location: `~/.cache/devloop/devloop-{slug}/ports.json` (XDG-compliant, survives reboots)
- Bind-mounted into dev container at `/tmp/devloop/ports.json`

### Container → Host Networking

With container-side test execution, the dev container needs reliable host access for **all** Kind cluster traffic:
- Env-test HTTP/gRPC/WebTransport calls to services (critical path)
- kubectl commands to K8s API server (critical path)
- Grafana/Prometheus/Loki access for observability tests and debugging (critical path)
- Unix socket to helper (bind-mounted, no TCP needed)

#### Key Discovery: Host-Gateway Address on Loopback

`host.containers.internal` resolves to `10.255.255.254` (or similar), which podman's slirp4netns binds to the **host's loopback interface**:

```
$ ip addr show lo
inet 10.255.255.254/32 brd 10.255.255.254 scope global lo
```

This means services bound to `${HOST_GATEWAY_IP}` are:
- **Reachable from containers** via `host.containers.internal` (podman gateway routing)
- **NOT reachable from `127.0.0.1`** (different loopback binding — other localhost services remain isolated)
- **NOT reachable from the LAN** (loopback-scoped, not on any routable interface)

By setting Kind NodePort `listenAddress` to `${HOST_GATEWAY_IP}` instead of `127.0.0.1`, all cluster services become reachable from the dev container without `--network=host` or any TCP forwarding layer. Empirically verified on WSL2 with named podman networks.

#### Dynamic Host-Gateway Detection

The host-gateway IP must be detected at runtime, not hardcoded. Detection order:

1. `podman info --format '{{.Host.NetworkBackendInfo.DNS.HostGatewayIP}}'`
2. Parse `/etc/hosts` for `host.containers.internal` entry
3. Fail with actionable error if neither works

`devloop.sh` detects the IP once at startup and passes it to the Kind config template via `envsubst`.

#### Named Podman Network

Replace `--network container:$DB_CONTAINER` with a **named podman network** per devloop:

```bash
podman network create "devloop-${TASK_SLUG}-net" 2>/dev/null || true

podman run -d --name "$DB_CONTAINER" \
    --network "devloop-${TASK_SLUG}-net" \
    -e POSTGRES_PASSWORD=postgres \
    docker.io/library/postgres:16-bookworm

podman run -d --name "$DEV_CONTAINER" \
    --userns=keep-id \
    --network "devloop-${TASK_SLUG}-net" \
    -e DATABASE_URL="postgresql://postgres:postgres@${DB_CONTAINER}:5432/dark_tower_test" \
    ...
```

Named networks are strictly better than `--network container:`:
- **Container DNS**: DB reachable via container name (`$DB_CONTAINER:5432`) instead of `localhost:5432`
- **Bridge networking**: proper `eth0` interface instead of slirp4netns tap device
- **Per-devloop isolation**: each devloop gets its own network
- **`host.containers.internal` works**: resolves to `${HOST_GATEWAY_IP}`, routes to host-gateway-bound ports
- **`--userns=keep-id` compatible**: empirically verified (unlike `--pod`)

```
Container (--network devloop-${TASK_SLUG}-net)
├── $DB_CONTAINER:5432 → PostgreSQL (via container DNS on named network)
└── host.containers.internal:$PORT → Kind NodePorts (via podman gateway → ${HOST_GATEWAY_IP})
```

The helper writes the verified host address into `ports.json` so `ClusterPorts::from_env()` and kubeconfig both use the correct address.

### Kind Config Templating

**Template**: `infra/kind/kind-config.yaml.tmpl` with placeholders:
```yaml
name: ${CLUSTER_NAME}
nodes:
  - role: control-plane
    extraPortMappings:
      - containerPort: 30090
        hostPort: ${HOST_PORT_PROMETHEUS}
        listenAddress: "${HOST_GATEWAY_IP}"
        protocol: TCP
      # K8s API accessible from containers (dual-port pattern)
      - containerPort: 6443
        hostPort: ${HOST_PORT_K8S_API_GATEWAY}
        listenAddress: "${HOST_GATEWAY_IP}"
        protocol: TCP
networking:
  apiServerAddress: "127.0.0.1"
  apiServerPort: ${HOST_PORT_K8S_API}
```

**Generation**: Helper runs `envsubst < kind-config.yaml.tmpl > /tmp/devloop-{slug}/kind-config.yaml`

**Security**: All NodePorts bind to `${HOST_GATEWAY_IP}` — reachable from containers via `host.containers.internal`, NOT reachable from `127.0.0.1` or the LAN.

**Dual-port K8s API pattern**: `apiServerAddress: "127.0.0.1"` keeps host kubectl working via `localhost:${HOST_PORT_K8S_API}`. A separate `extraPortMappings` entry for port 6443 on `${HOST_GATEWAY_IP}:${HOST_PORT_K8S_API_GATEWAY}` makes the API reachable from containers. The container kubeconfig points to `host.containers.internal:${HOST_PORT_K8S_API_GATEWAY}`.

**Static file preserved**: Existing `kind-config.yaml` kept for manual `setup.sh` usage with default ports.

### setup.sh Parameterization

```bash
CLUSTER_NAME="${DT_CLUSTER_NAME:-dark-tower}"
PORT_MAP_FILE="${DT_PORT_MAP:-}"
```

- If `DT_CLUSTER_NAME` set, use it; otherwise default to "dark-tower" (backward compatible)
- If `DT_PORT_MAP` set, source port variables from it
- All `kubectl` commands use `--context kind-${CLUSTER_NAME}`
- Interactive prompts skippable via `--yes` flag for automated use
- Interactive prompts auto-skipped when stdin is not a TTY (`[[ -t 0 ]]`), so automated callers don't need `--yes`
- `--only <service>`: Rebuild + redeploy single service (~30-60s with cargo-chef cache)
- `--skip-build`: Apply manifests only (~15-20s)

### Env-Test URL Configuration

Add `ClusterPorts::from_env()` that reads:
- `ENV_TEST_AC_URL` → full URL (e.g., `http://host.containers.internal:24200`)
- `ENV_TEST_GC_URL` → full URL
- `ENV_TEST_PROMETHEUS_URL` → full URL
- `ENV_TEST_GRAFANA_URL` → full URL
- `ENV_TEST_LOKI_URL` → full URL
- Fallback to current hardcoded defaults when env vars unset

MC/MH endpoints come from GC join response (`mc_assignment.webtransport_endpoint`), not configuration. Only AC and GC entry-point URLs need env vars.

The devloop agent runs env-tests directly inside the container:
```bash
ENV_TEST_AC_URL=http://host.containers.internal:24200 \
ENV_TEST_GC_URL=http://host.containers.internal:24201 \
ENV_TEST_PROMETHEUS_URL=http://host.containers.internal:24209 \
ENV_TEST_GRAFANA_URL=http://host.containers.internal:24210 \
ENV_TEST_LOKI_URL=http://host.containers.internal:24211 \
cargo test -p env-tests
```

When run manually on the host, the same tests work with `localhost` URLs (or no env vars, using defaults). The env-test code has no dependency on the helper — it just needs the correct URLs and kubectl access.

### Devloop Skill Integration (Layer 8)

The purpose of integrating Kind and env-tests into the devloop is to **catch integration boundary regressions that unit tests cannot detect**. Bugs like the `home_org_id` fix (commit `146234d`) pass all unit tests but break at the cross-service boundary — wrong token claims, mismatched gRPC contracts, deployment config errors, secret leaks, NetworkPolicy violations. Layer 8 catches these before the human reviews the code.

**Triggers**: handlers/, grpc/, routes/, common/, env-tests/, proto/, infra/kind/, infra/kubernetes/, infra/docker/, config.rs, migrations/

**Execution**: smoke first (~30s gate), then all remaining features. **All env-test features run by default** when the cluster has the corresponding stack deployed. Do not skip observability or resilience tests — the whole point is preventing bug escapes at integration boundaries. If tests are flaky, fix the tests rather than excluding them.

- If `--skip-observability` was used at setup (no Prometheus/Grafana/Loki deployed), observability tests are automatically skipped.
- If the observability stack is deployed but unhealthy, the devloop agent should report this as an infrastructure failure (retry once, then escalate) rather than silently skipping tests.

**Attempt budgets**: 3 unit/clippy/semantic + 2 integration. Infrastructure failures don't consume attempts. First-run setup (~7 min) doesn't consume attempts.

**First-run setup**: Helper tracks cluster readiness via `~/.cache/devloop/devloop-{slug}/cluster-ready` flag file. If absent when `env-test` is called, helper auto-runs `setup` first. The ~7 min first-run cost does not count toward any attempt budget.

**Failure classification** (via exit code):
- Exit 0: all tests passed
- Exit 101: test assertion failure → count toward 2-attempt budget
- Compile error: build failure → count toward 3-attempt unit budget
- Other: infrastructure failure → retry once, then escalate

### Resource Requirements

| Configuration | RAM | Feasibility |
|---|---|---|
| Devloop without cluster | 4-8 GB | Any machine |
| Single devloop with cluster | 6-10 GB | **16 GB minimum** |
| 2 concurrent devloops | 12-20 GB | **32 GB recommended** |
| With `--skip-observability` | Saves ~1.5 GB per cluster | Resource-constrained option |

**Optional**: `--skip-observability` flag skips Prometheus/Grafana/Loki/Promtail deployment, saving ~1.5 GB per cluster. Observability env-tests automatically skipped when stack not deployed.

### System Limits (inotify)

Running multiple Kind clusters requires increased inotify limits. Each Kind node uses inotify for kubelet, kube-proxy, and controllers. The default `max_user_instances=128` is insufficient for 2+ concurrent clusters — kube-proxy crashes with "too many open files," cascading to Calico and networking failures.

```bash
# Check current limits
sysctl fs.inotify.max_user_instances fs.inotify.max_user_watches

# Increase (immediate)
sudo sysctl fs.inotify.max_user_instances=1024
sudo sysctl fs.inotify.max_user_watches=1048576

# Persist across reboots (add to /etc/sysctl.conf or /etc/sysctl.d/)
echo "fs.inotify.max_user_instances=1024" | sudo tee -a /etc/sysctl.d/99-kind.conf
echo "fs.inotify.max_user_watches=1048576" | sudo tee -a /etc/sysctl.d/99-kind.conf
```

The helper should check these limits at startup and warn if too low for multi-cluster operation.

## Implementation Guidance

### Dependency Order

| Step | Task | Blocked By |
|------|------|------------|
| 0 | **Verify full networking chain** with named podman network and `listenAddress: "${HOST_GATEWAY_IP}"`. See Verification section below. This is a **hard blocker** for container-side test execution. | — |
| 1 | `ClusterPorts::from_env()` in env-tests | — |
| 2 | setup.sh parameterization (`--cluster-name`, `--yes`, `--only`) | — |
| 3 | Kind config template (`kind-config.yaml.tmpl`) with per-instance MC/MH ports | — |
| 4 | Compiled Rust helper binary (`crates/devloop-helper/`) | Steps 0-3 |
| 5 | `devloop.sh` changes (launch helper, bind-mount socket + kubeconfig, port map, kubectl in image) | Step 4 |
| 6 | Devloop skill Layer 8 integration | Step 5 |

Step 0 is verified (see below). Steps 1-3 are independent. Steps 4-6 are sequential.

### Step 0 Verification (Complete)

Before any other implementation, the full networking chain was verified:

1. **Gateway IP on loopback**: `ip addr show lo` confirms `10.255.255.254/32` on loopback
2. **Named network + container DNS**: DB container reachable from client via container name (`pg_isready -h $DB_CONTAINER`)
3. **Gateway-IP port binding isolation**: Service bound to `${HOST_GATEWAY_IP}:PORT` — reachable from host via gateway IP, NOT reachable from `127.0.0.1` (isolation preserved)
4. **Container → gateway-IP-bound port**: Container reaches gateway-IP-bound host port via `host.containers.internal` (THE KEY TEST)
5. **Kind NodePort via gateway IP**: Kind cluster with `listenAddress: "${HOST_GATEWAY_IP}"` for NodePort — reachable from container
6. **K8s API via extraPortMappings**: Dual-port pattern — `apiServerAddress: "127.0.0.1"` for host kubectl + `extraPortMappings` for port 6443 on gateway IP for container kubectl

All tests passed (verified manually on WSL2 before implementation).

### Explicit Prohibitions

- **`--network=host` MUST NOT be used** for the dev container. It removes network isolation that ADR-0025 relies on for defense-in-depth.
- **Podman socket MUST NOT be mounted** into the dev container. The podman socket grants full user-level host privileges — a container with socket access can run arbitrary containers with host filesystem mounts, defeating all ADR-0025 isolation guarantees. Build/deploy operations go through the helper's unix socket API with authentication.
- **`listenAddress: "0.0.0.0"` MUST NOT be used** for Kind NodePorts. It exposes dev services to the LAN.

### Observability Access

Observability endpoint access (Prometheus, Grafana, Loki) from inside the container is a **stated requirement**, not a side effect. The networking solution supports:
- Ad-hoc Prometheus queries for metric debugging
- Grafana dashboard validation
- Loki log searches for failure diagnosis

The `--skip-observability` flag remains supported — when the observability stack isn't deployed, those ports aren't mapped and observability tests are auto-skipped.

### Orphan Cleanup

On `devloop.sh` startup:
1. Scan port registry (`~/.cache/devloop/port-registry.json`) for entries whose PID is dead (`kill -0 $PID`)
2. Scan for `devloop-*` Kind clusters with no corresponding running devloop container
3. Clean up stale PID files (`/tmp/devloop-{slug}/helper.pid`) and state directories when associated clusters and containers are gone
4. Prompt user to clean up stale clusters and remove dead registry entries

### Key Files

- `crates/devloop-helper/src/main.rs` — helper binary, build-and-deploy only (new)
- `crates/env-tests/src/cluster.rs` — `ClusterPorts::from_env()` (modify)
- `infra/devloop/devloop.sh` — launch helper, bind-mount socket + kubeconfig, kubectl in image (modify)
- `infra/devloop/Dockerfile` — add kubectl to dev container image (modify)
- `infra/devloop/dev-cluster` — client CLI script for build/deploy commands (new)
- `infra/kind/kind-config.yaml.tmpl` — Kind config template with per-instance ports (new)
- `infra/kind/scripts/setup.sh` — parameterize cluster name, ports, `--only`, read replica counts (modify)

## Consequences

### Positive
- Devloop becomes fully autonomous: implement → build → deploy → test → iterate
- Kind runs on the host in its best-supported mode (no nesting fragility)
- Multiple concurrent devloops with isolated clusters and ports
- Container-side test execution consistent with ADR-0025 containerized compilation
- Env-tests are portable — work on host or in container, no helper dependency
- No privileged containers needed (security improvement over sidecar design)
- Compiled Rust helper makes injection structurally impossible
- All env-test features run by default — full integration boundary coverage
- Incremental rebuild ~30-60s with cargo-chef cache

### Negative
- Helper runs on the host outside container isolation boundary (accepted risk for dev tooling)
- Dev container gets kubectl + kubeconfig (dev-only cluster, accepted risk)
- `${HOST_GATEWAY_IP}` is podman/slirp4netns-specific — may differ on other container runtimes (mitigated by dynamic detection)
- Human browser access to Grafana requires `kubectl port-forward` or separate `127.0.0.1` port mapping (convenience gap, not a blocker)
- Orphaned Kind clusters possible on ungraceful exit (mitigated by startup scan)

### Neutral
- Kind as cluster tool preserved (no production drift from k3s or alternatives)
- All Kustomize manifests, Dockerfiles, service code unchanged
- Existing manual `setup.sh` workflow backward compatible
- Existing `kind-config.yaml` (static, for manual use) can keep `127.0.0.1` — only the template changes

## Participants

- **Infrastructure** (97%): Host-native helper, envsubst templating, registry-based port allocation. Verified named podman network + `--userns=keep-id` compatibility, container DNS, and `host.containers.internal` routing. Confirmed Kind passes `listenAddress` to podman `-p` flag.
- **Security** (95%): Compiled Rust binary eliminates injection class. All ADR-0025 isolation preserved. `--network=host` and podman socket access explicitly prohibited. Container can reach Kind ports but not arbitrary localhost services.
- **Test** (95%): `ClusterPorts::from_env()` only test code change. Zero test code changes for networking. Same feedback loop as running on host. Exit code classification for attempt budgets.
- **Observability** (93%): Flat JSON port map, `host.containers.internal`, `--skip-observability` for resource-constrained. Prometheus/Grafana/Loki directly reachable. Observability access documented as a requirement.
- **Operations** (95%): PID file lifecycle, multi-layer orphan cleanup, 200-stride port allocation. Gateway-IP binding is operationally clean. Dynamic detection, orphan cleanup with PID file and state directory scanning.

## Debate Reference

See: `docs/debates/2026-04-07-host-side-cluster-helper/debate.md`
Networking amendment: `docs/debates/2026-04-09-devloop-cluster-networking/debate.md`
Prior art: `docs/debates/2026-04-05-devloop-cluster-sidecar.md` (sidecar approach, PoC failed)

## References

- ADR-0025: Containerized Dev-Loop Execution
- ADR-0014: Environment Integration Tests
- ADR-0024: Agent Teams Workflow (validation pipeline)
- `infra/devloop/devloop.sh` — current wrapper script
- `infra/kind/scripts/setup.sh` — cluster setup script
- `crates/env-tests/` — environment integration test suite
