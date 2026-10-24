# Environment Integration Tests

Environment-level integration tests for the Dark Tower local development environment. These tests validate that Kubernetes deployments, observability stack, and service flows work correctly against actual deployment artifacts.

## Prerequisites

Before running these tests, ensure:

1. **Kind cluster running**: Execute `./infra/kind/scripts/setup.sh` from the repository root
2. **Port-forwards active**: The setup script starts port-forwards for:
   - AC Service: `localhost:8082`
   - Prometheus: `localhost:9090`
   - Grafana: `localhost:3000`
   - Loki (optional): `localhost:3100`
3. **kubectl in PATH**: Required for NetworkPolicy diagnostics, secret exposure validation, and reading the deployed ConfigMaps
4. **MH gRPC forwards (for `29_mh_meeting_teardown`)**: Layer 7 (`scripts/layer7.sh`, step (i)) starts a per-POD `kubectl port-forward` to each MH pod's gRPC port and exports `ENV_TEST_MH_{0,1}_GRPC_URL` and `ENV_TEST_MH_{0,1}_POD_IP`. The setup script does NOT start these. Run that test through Layer 7, or export the four variables yourself; it fails loudly when they are unset.

## Test Categories

Tests are organized by features to allow flexible execution:

| Feature | Tests | Duration | Purpose |
|---------|-------|----------|---------|
| `smoke` | Cluster health, auth smoke, secret exposure | 30s | Fast validation that cluster is working |
| `flows` | Auth flows, cross-replica, JWKS validation | 2-3min | Core service functionality |
| `observability` | Metrics scraping, log aggregation | 2-3min | Observability stack validation (Loki optional) |
| `resilience` | Pod restarts, chaos scenarios | 5min+ | Pre-deploy resilience testing |
| `all` | All test categories | 8-10min | Complete validation suite |

## Usage

### From Repository Root

```bash
# Run 0 env-tests (no default features)
cargo test

# Smoke tests only (30s)
cargo test -p env-tests --features smoke

# Smoke + service flows (3min)
cargo test -p env-tests --features smoke,flows

# With observability stack (4min, Loki optional)
cargo test -p env-tests --features smoke,flows,observability

# Pre-deploy validation - full suite (8-10min)
cargo test -p env-tests --features all
```

### Pre-Deployment Validation

Before deploying to production, run the full test suite:

```bash
./infra/kind/scripts/setup.sh
cargo test -p env-tests --features all
```

## Test Structure

```
crates/env-tests/
├── src/
│   ├── cluster.rs          # ClusterConnection, health checks
│   ├── eventual.rs         # Retry helpers for eventual consistency
│   ├── fixtures/
│   │   ├── auth_client.rs  # Token issuance, JWKS operations
│   │   └── metrics.rs      # Prometheus queries
│   └── canary.rs           # NetworkPolicy test pods (placeholder)
└── tests/
    ├── 00_cluster_health.rs   # P0 smoke tests
    ├── 10_auth_smoke.rs       # P0 auth smoke tests
    ├── 20_auth_flows.rs       # P1 auth flow tests
    ├── 25_auth_security.rs    # P1 security tests
    ├── 30_observability.rs    # P1 observability tests
    ├── 40_resilience.rs       # P2 resilience tests (stubs)
    └── 90_runbook.rs          # P2 runbook validation (stubs)
```

## Pre-Seeded Test Credentials

The kind cluster has these credentials seeded for testing:

- **Client ID**: `test-client`
- **Client Secret**: `test-client-secret-dev-999`
- **Scopes**: `test:all`

`29_mh_meeting_teardown` also uses the seeded **`meeting-controller`** service client — reading its id from the live `mc-0` Deployment and its secret from the live `mc-service-secrets` Secret, never a copy — narrowed to `service.write.mh`, to call MH's `MediaHandlerService` directly: MC does not call `EndMeeting` yet, and a mismatched `mc_id` cannot be produced through MC at all. That relaxes the suite's credential convention only; ADR-0028's layering is unchanged — the MH client comes from `proto-gen`, and the suite links no service crate. The test registers only uuid-named meetings of its own and releases them on every exit path.

## Observability Stack

- **Prometheus**: Required (lightweight ~200MB, essential for health monitoring)
- **Loki**: Optional with auto-skip and warning (~600MB with Promtail)
  - Tests that require Loki will print a warning and skip if unavailable
  - CI runs with `--features all` to ensure full validation

## Design

For the full architecture and design decisions, see:

- **ADR-0014**: [docs/decisions/adr-0014-environment-integration-tests.md](../../docs/decisions/adr-0014-environment-integration-tests.md)
- **Design Debate**: [docs/debates/2025-12-16-environment-integration-tests.md](../../docs/debates/2025-12-16-environment-integration-tests.md)

### Evidence rule: public APIs, per-entity evidence

Suites run in parallel against shared pods, so:

- **Drive the public APIs** (GC/MC signalling, WebTransport clients, the browser). Call an internal API directly only to *observe* an effect the public surface cannot show, or to reach a case no public path can (for example an `EndMeeting` from the wrong owner).
- **Assert per-entity evidence** a concurrent suite cannot disturb: an RPC reply about your own meeting, frames on your own connection, your own client's counters, a generation or ownership check on your own meeting id.
- **Never assert the value of a shared pod-level gauge** (`mh_media_egress_edges`, `mh_media_registered_meetings`, ...): other suites move it. A config-published gauge (a limit, ceiling or threshold) is fine, and so is a presence check. A counter rising past your own baseline is weak evidence (another suite can cause the rise), so never the only proof. Exact gauge accounting belongs in the owning service's in-process tests, where nothing else runs.
- **Do not depend on shared capacity you do not need**: register a zero-edge policy when edges are irrelevant to what the test proves.

`tests/29_mh_meeting_teardown.rs` is the worked example.

### Media fixtures (ADR-0036; story 3)

Keyless, black-box clients for the media path. They build and parse frames only through the production `media-protocol` codec, hold no key material, and log nothing: GoP counters, stream and slot ids, key-id senders and lengths are returned values only (R-30).

- **Audio** — `src/fixtures/media.rs`. One frame-v3 datagram per frame, marker-only payload, uplink relay `stream_id` = `AUDIO_SEND_ID` (0). `gop_counter` equals `stream_sequence`, because every audio frame starts a group. The send-until-received loop draws its sequences from one process-wide, strictly increasing `next_audio_sequence()`, which is capped below `AUDIO_SEQUENCE_CEILING`, the lowest hand-rolled phase floor in tests 26 and 35. So a sender's counter never regresses across phases.
- **Video** — `src/fixtures/video/` (module docs carry the full contract). The video tests depend on MH-A1 (task 8), MH-A2 (task 9) and MH-B (task 10).
  - **`VideoPublisher`** sends one uni stream per group of pictures, written through `encode_frame`.
    - **Group start, `GroupStartSource`**: one knob, default `None`. It is also the never-keying source for MH-B's pending gauge.
      - `OnKeyframeRequest { min_interval }` answers `MhServerMessage.keyframe_request` read from the MH carrier stream. It acts only on the directed stream with a counter ≥ the current one, and defers one in-interval request rather than refusing it.
      - `Periodic(interval)` starts a group on tokio time. **It is fixture-only and never a production keyframe interval**: ADR-0036 as amended has none, and keyframes are on demand only.
    - **Independent knobs**: the uplink label (default: the directed send id `VIDEO_SEND_ID` = 1; override to relabel or to use an undeclared id) and the transport (uni-stream-per-group or one ≤MTU datagram per frame).
    - **Modes**: `KeepSendingWhileMuted` is the R-16 patched client, `StallMidFrame` the R-21 stall, and `ResetMidGroup` the R-12 abandonment.
    - Every request and group start goes into one ordered event log.
    - A compliant publisher finishes its group stream on withdrawal and starts a NEW group (`Resume`, next counter) when re-directed. MH's STOP_SENDING on a stream (a stream-path attribution drop) is an observable `StoppedByPeer` outcome, never a fixture panic.
  - **`VideoSubscriber`** reads every incoming uni stream with the chunk-fed `HeaderReader`, enforcing the maximum payload before any allocation. Per frame it reports the key-id sender, relay slot id, hop sequence, `gop_counter` and whether the frame is independently decodable. Per stream it reports first-frame-is-a-group-start, slot change within the stream, and finish / reset / partial. `next()` reports `Quiet` only while the connection is live, so an absence assertion cannot pass on a dead subscriber.
  - **`send_recovery_request`** writes `recovery_request { slot_id, sender_id, gop_counter }` with every field explicit.
  - MH's expected outcome table (`unmatched_stream`, `transport_mismatch`, `server_muted`, PAUSED forwarding, the recovery outcomes) is pinned in `src/fixtures/video/mod.rs` as what the consuming env-tests assert. The fixtures' own hermetic tests assert no MH behaviour.

### Per-file edge-sum header rule

Every env-test file states in its header the sum, over its **concurrently running** test fns, of **P × (N+M)**: participants, times the audio plus video receive slots each declares. `cargo test` runs one file's test fns in parallel and the files one after another, so that per-file sum is the load the Kind cluster carries at once.

EVERY file under `tests/` carries exactly one such line — `0` when it opens no media edges — so a new file cannot slip past the rule. The S9 exhaustion test is the one exception: it sizes its meeting from the live ceiling and deliberately exceeds it, so it carries the fixed exempt token instead of a number. The Kind egress window is sized against the PLANNED per-file cap `MAX_PER_FILE_EDGE_SUM` (`src/fixtures/egress_admission.rs`, beside the window's other constants). Each file's ACTUAL sum is the `Edge-sum` line in its own header, the one home for that number. The hermetic check `every_env_test_file_records_an_edge_sum_within_the_cap` (Layer 4) fails a missing, malformed or duplicate line, any sum above the cap, and any count of exemptions other than one. A change that pushes a file past the cap must, in the same change, either serialise that file's fns or raise the cap, which moves the window's lower side, so re-check the Kind budget.

## Future Work

- Resilience tests: pod restart recovery, network partition, DB connection loss (see TODO.md)
- Runbook validation tests: key rotation, graceful drain, scale-up procedures (see TODO.md)
- In-cluster test Job for CI (eliminate port-forward dependency)
