# Media Handler Navigation

## Architecture & Design
- SFU architecture, MH registration/load reports → ADR-0010 (Section 4a); MH assignment/selection → ADR-0023 (Section 5)
- Media flow: frame v2 (§2), forward path (§7), control plane (§8), multi-handler edges (§9), transport seam (§10), sender binding (§11) → ADR-0036
- Actor pattern → ADR-0001; service auth + gRPC scopes → ADR-0003; fuzzing → ADR-0006; deploy parity / config-key ordering → ADR-0038
- Observability: metrics facade → ADR-0011; dashboards → ADR-0029; service-owned dashboards/alerts → ADR-0031; metric testability → ADR-0032
- User stories: QUIC connection → `docs/user-stories/2026-04-12-mh-quic-connection.md`; hear-each-other (story 2) → `docs/user-stories/2026-09-21-hear-each-other.md`
- Open media-path obligations (cross-tenant guard, transport-contract mirrors) → `docs/TODO.md` §Media Path Obligations

## Service Skeleton & Config
- Entry point (process epoch sampled once, advisory WARNs) → `crates/mh-service/src/main.rs`; module roots + `per-frame-trace` gate → `src/lib.rs`
- Config: required env vars, transport constants, drain window, `NOMINAL_AUDIO_FRAME_BYTES` → `crates/mh-service/src/config.rs`
- Policy bounds + ceilings (incl. registered-meeting cap, muted-source bound) → `config.rs:PolicyLimits`, `config.rs:MAX_REGISTERED_MEETINGS_CEILING`, `config.rs:MAX_MUTED_SOURCES_PER_MEETING_CEILING`
- Egress-budget admission config (budget, per-kind costs, derived stream ceiling) → `config.rs:EgressAdmission`; boot refusals → `config.rs:ConfigError`
- Process-incarnation epoch → `crates/mh-service/src/process.rs`; errors → `src/errors.rs`

## Control Plane (MC → MH, ADR-0036 §8)
- `RegisterMeeting` (validate → upsert → apply → echo applied generation) + `EndMeeting` → `crates/mh-service/src/grpc/mh_service.rs:end_meeting`
- Session actor: lifecycle + config-apply mailboxes → `crates/mh-service/src/session/mod.rs`; registration + cap → `session/mod.rs:handle_register_meeting`; teardown → `session/mod.rs:handle_end_meeting`; `mc_id` ownership → `session/mod.rs:Ownership`
- Stream admission (budget enforcement, rejection-ratio window) → `crates/mh-service/src/session/admission.rs`, `session/mod.rs:handle_config_apply`
- Forwarding policy + ArcSwap routing snapshot; edge accounting kernel → `crates/mh-service/src/routing/mod.rs:edges_after_displacing`
- Server mute set → `routing/mod.rs:is_server_muted`; meeting-scoped sender lookup → `routing/mod.rs:for_each_source`
- Subscriber registry + `SenderBindings` → `crates/mh-service/src/session/mod.rs`
- GC client (registration, heartbeats, `max_streams`/`current_streams`) → `src/grpc/gc_client.rs`; MC client (connect/disconnect notify, returns `sender_id`) → `src/grpc/mc_client.rs`
- gRPC auth layer → `src/grpc/auth_interceptor.rs`; trace-context span layer → `src/grpc/span_layer.rs`; JWT validator → `src/auth/mod.rs`

## Data Plane (Client ↔ MH)
- WebTransport server + `quinn::TransportConfig` → `crates/mh-service/src/webtransport/server.rs:build_transport_config`
- Connection handler, sender-binding gate, media session start, per-connection `close` token → `webtransport/connection.rs:resolve_sender_binding()`, `connection.rs:start_media_session`
- Transport seam trait (ADR-0036 §10) → `crates/mh-service/src/transport/mod.rs`; real impl → `webtransport/media_transport.rs`
- Hot-path boundary (no telemetry macros under it) → `crates/mh-service/src/media/mod.rs`
- `forward_one` (mute check first, decode/rewrite, fan-out) → `media/forward.rs`; `HopSequence` + `ConnectionForwarder` → `media/forwarder.rs`
- Drop-oldest ring → `media/queue.rs`; pre-alloc caps + stream rate limiter → `media/caps.rs`; per-connection loops + `FramesRead` → `media/ingress.rs`; random sampler → `media/sampler.rs`

## Media Protocol (ADR-0036 §2)
- Frame v2 layout + size constants → `crates/media-protocol/src/frame.rs`; parser + entry points + `ALL_REJECT_REASONS` → `crates/media-protocol/src/codec.rs`
- Relay-region rewrite → `codec.rs:rewrite_relay_region()`; extension TLVs → `crates/media-protocol/src/extensions.rs`; fuzz → `crates/media-protocol/fuzz/fuzz_targets/`

## Proto Definitions
- MC↔MH (`RegisterMeeting`, `EndMeeting`, `MutedSource`), MH↔GC, MH→MC → `proto/dark_tower/internal/v1/internal.proto`; codegen → `crates/proto-gen/build.rs`
- Client signaling (`StreamAssignments.unreachable_sender_ids`, `SlotState`) → `proto/dark_tower/signaling/v1/signaling.proto`
- MC-side edge placement that drives MH policy (ADR-0036 §9) → `crates/mc-service/src/media_routing/`

## Observability
- Metrics: policy-apply → `src/observability/metrics.rs:PolicyApplyOutcome`; admission → `metrics.rs:publish_egress_admission`, `StreamAdmissionOutcome`; teardown → `metrics.rs:MeetingTeardownOutcome`; `GrpcMethod`; session handles → `metrics.rs:resolve_session_handles`; hot-path handles + `MediaDropReason` → `metrics.rs:resolve_media_handles`
- Health/readiness → `src/observability/health.rs`; shared `key_custody` label → `crates/common/src/observability/labels.rs`
- Catalog → `docs/observability/metrics/mh-service.md`; alert inventory → `docs/observability/alerts.md`; media-path label rules → `docs/observability/label-taxonomy.md`
- Alert rules (incl. egress headroom + budget exhausted) → `infra/docker/prometheus/rules/mh-alerts.yaml`
- Dashboards → `infra/grafana/dashboards/mh-overview.json`, `mh-media.json`, `mh-slos.json`, `mh-logs.json`

## Testing
- Rigs (JWKS/MC mocks, gRPC + WT rigs, token minters) → `crates/mh-service/tests/common/`; fixtures (policy builders, admission limits, transport shim) → `crates/mh-test-utils/src/`
- Control plane: `register_meeting_integration.rs`, `policy_apply_integration.rs`, `end_meeting_integration.rs`, `stream_admission_integration.rs`, `gc_integration.rs`, `mc_client_integration.rs`, `auth_layer_integration.rs` → `crates/mh-service/tests/`
- Data plane: `media_forward_integration.rs`, `media_backpressure_integration.rs`, `media_metrics_integration.rs`, `media_session_binding_integration.rs`, `media_server_mute_integration.rs`, `transport_real_impl.rs`, `transport_seam_reachability.rs`, `webtransport_*integration.rs` → `crates/mh-service/tests/`
- Trace propagation → `tests/otel_grpc_integration.rs`, `tests/otel_webtransport_integration.rs`
- Env-tests: config surface → `crates/env-tests/tests/01_mh_deployment_config.rs`; QUIC flow + server mute → `26_mh_quic.rs`; multi-handler forwarding → `27_mc_slot_placement.rs`; egress admission + alert joins → `28_mh_egress_admission.rs`; teardown → `29_mh_meeting_teardown.rs`; media metric hygiene → `32_media_metric_hygiene.rs`
- Env-test fixtures: MH gRPC as MC → `crates/env-tests/src/fixtures/mh_grpc.rs`; S9 sizing → `fixtures/egress_admission.rs`; frames → `fixtures/media.rs`; scrape settle → `fixtures/metrics.rs:service_job_scrape_settle`

## Infrastructure & Operations
- Deployments (mh-0/mh-1, all config keys as `configMapKeyRef`) → `infra/services/mh-service/mh-0-deployment.yaml`, `mh-1-deployment.yaml`; config → `infra/services/mh-service/config.env`
- Kind egress-budget overlay → `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml`
- ConfigMap key ↔ env-var guard → `crates/dt-guard/src/env_config.rs`
- Network policy → `infra/services/mh-service/network-policy.yaml`, `infra/services/mc-service/network-policy.yaml`
- Runbooks: deployment + config triage → `docs/runbooks/mh-deployment.md`; incidents (Scenario 18 egress budget, 19 server mute) → `docs/runbooks/mh-incident-response.md`
