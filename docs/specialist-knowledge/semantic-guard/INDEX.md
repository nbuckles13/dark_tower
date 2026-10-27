# Semantic Guard Navigation

## Architecture & Design
- Media path / key custody / frame-v3 → ADR-0036 | Guard methodology → ADR-0015 | Agent Teams pipeline → ADR-0024
- Polyglot layers → ADR-0033 | Story runner → ADR-0035 | Cluster helper (+ cannot-self-validate) → ADR-0030
- Semantic check definitions → `scripts/guards/semantic/checks.md` | Shell utils → `scripts/guards/common.sh`
- Reviewer-panel slot (Gate 2 unicast loop, Step 7 dedup) → `.claude/skills/devloop/SKILL.md`

## Media Path — Key Custody (ADR-0036 §4/§11, credential-leak items 11-13)
- MC key custody (KEK, identity key, sender-id, binding outcome; §4 floor in `mod.rs`) → `crates/mc-service/src/media_admission/`
- KEK emission sites (named in checks.md §Credential Leak) → `media_admission/kek.rs:MeetingKeyState::rotate()`, `epoch.rs:AdmissionEpoch::admit()`
- KEK push path (typed `KekPush`, never `SignalingPayload::Raw`) → `actors/messages.rs:ParticipantMessage::KekUpdate` → `actors/participant.rs:handle_kek_update()`
- Rotation lifecycle log (`mc.kek.lifecycle`) → `media_admission/rotation.rs:collect_push_outcomes()`, `actors/meeting.rs:rotation_failed()`
- Frame-v3 + wrapped-key redacting Debug → `crates/media-protocol/src/frame.rs` | redaction control (`skip_debug`, `RedactedLen`) → `crates/proto-gen/build.rs`, `src/lib.rs`
- Signaling contract (`JoinResponse.meeting_kek`, `MeetingKekUpdate`, `unreachable_sender_ids`) → `proto/dark_tower/signaling/v1/signaling.proto`
- MC→MH internal contract (RegisterMeeting + EndMeeting; no-key-material header rule) → `proto/dark_tower/internal/v1/internal.proto`

## MC Media Routing & MH Coordination (`crates/mc-service/src/`)
- Routing control plane (assignment / generation / slots / placement / edges / connectivity) → `media_routing/`
- Per-kind vocabulary (`MediaKindLabel`, `served_kind()`, `PublishSet`) → `media_kind.rs` | send directive + outcome counting → `media_signaling/directive.rs:build_send_directive()`, `outcome.rs`
- Per-kind mute fan-out + client work bound → `actors/client_mute_fanout.rs`, `client_work_limiter.rs` | reconnect counting → `actors/meeting.rs:handle_reconnect()`
- RegisterMeeting push worker → `media_routing/pusher.rs:HandlerPusher`; published by `actors/meeting_media.rs:render_and_publish()`
- EndMeeting teardown → `media_routing/teardown.rs` | MH client trait → `grpc/mh_client.rs:MhRegistrationClient` | binding resolve → `grpc/media_coordination.rs`

## MH Media Hot Path (telemetry-free by construction)
- Forward path (no tracing/metrics macros) → `crates/mh-service/src/media/`
- Stream attribution (per-frame, never cached; exhaustive rejection) → `routing/mod.rs:RoutingSnapshot::attribute()`, `PolicyRejection::outcome()`
- Stream egress (non-awaiting push; sole abandon counting site) → `media/downlink.rs:Downlink::abandon()` | stream ingress → `media/stream_ingress.rs`
- On-demand keyframes (slot map + Notify writer) → `media/keyframe.rs:PublisherKeyframes`
- Transport seam → `crates/mh-service/src/transport/mod.rs`, `webtransport/media_transport.rs`
- Sender binding (`SenderBindings`, `bind()`, `start_media_session`) → `crates/mh-service/src/session/mod.rs`
- Egress-budget admission → `session/admission.rs` | EndMeeting handler → `grpc/mh_service.rs:end_meeting()`

## Client SDK Media (credential-leak items 5-10 + credential lifetime)
- Hot path (no console/logger/metric names) → `packages/sdk-core/src/media/pipeline/`
- Custody siblings (KEK, roster keys, allow-list metric projection) → `packages/sdk-core/src/media/setup/`
- TS KEK sink control (generation-scoped intake) → `packages/sdk-core/src/signaling/kekIntake.ts` | receive verification → `media/pipeline/receiveVerification.ts`
- Layout-deny test → `packages/sdk-core/src/media/__tests__/hotPathLayout.test.ts` | window-exposed sinks → `packages/web-app/src/lib/e2eBus.ts` (whitelist SAFE example), `debugStats.ts`
- Video publish key lifetime (session-owned, cleared on every exit) → `media/lifecycle/transmitKeys.ts:TransmitKeyManager` | seal → `media/pipeline/frameSeal.ts`
- Video publish/receive lifecycles → `media/lifecycle/VideoPublisher.ts`, `VideoReceiver.ts` | egress/ingress → `media/pipeline/videoEgress.ts`, `videoIngress.ts`
- Media store → `packages/sdk-svelte/src/stores/MediaStore.svelte.ts`

## Media Telemetry Deny & Key-Custody Fixtures
- Deny guard → `crates/dt-guard/src/media_telemetry_deny.rs` | config → `scripts/guards/simple/media-telemetry-deny.yaml`
- Macro-vocab SSoT → `crates/dt-guard/src/telemetry_macros.rs`, `metric_macros.rs` | scope liveness → `crates/dt-guard/src/common/scope.rs`
- Key-custody fixtures + harness (fixture-verification runs) → `crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs`
- Client metrics export drift guard (allow-list ↔ collector ↔ catalog) → `crates/dt-guard/src/client_metrics_export.rs` | collector → `infra/services/otel-collector/collector.yaml`
- Kind-label vocabulary (metric labels) → `crates/dt-guard/src/common/kind_labels.rs` | per-kind proto fields → `crates/dt-guard/src/proto_per_kind_fields.rs`

## MC Actors & WebTransport (`crates/mc-service/src/`)
- Actors (controller, meeting, participant, messages, metrics) → `actors/*.rs`
- Server (accept, TLS, capacity gate) → `webtransport/server.rs:WebTransportServer` | join/media dispatch/bridge → `webtransport/connection.rs:handle_connection()`

## Authentication Seams
- Common JWT (types, JWKS, validator, HasIat) → `crates/common/src/jwt.rs` | Token refresh → `common/src/token_manager.rs`
- GC JWT → `crates/gc-service/src/auth/jwt.rs` | MC JWT (McJwtValidator, checked pre-actor in `handle_connection()`) → `crates/mc-service/src/auth/mod.rs`
- MH gRPC auth → `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor` | JwtError mapping → `crates/gc-service/src/errors.rs`, `crates/mc-service/src/errors.rs`

## GC Handlers, Repositories & MH Selection
- Handlers → `crates/gc-service/src/handlers/meetings.rs` | routes → `routes/mod.rs:build_routes()` | insert-error (SQLSTATE) → `repositories/meetings.rs:classify_insert_error()`
- Refusal → HTTP + `error.code` → `errors.rs:GcError` | MH selection (weighted random) → `services/mh_selection.rs:MhSelectionService`

## Devloop Tooling (`scripts/workflow/`, `crates/dt-story/`)
- Story runner → `run-story.sh` | Stop hook (fail-closed) → `devloop-stop-hook.sh` | manifest schema → `crates/dt-story/src/manifest.rs`
- Manifest guard → `scripts/guards/simple/validate-story-manifest.sh` | Planning / closing → `.claude/skills/{user-story,close-story}/SKILL.md`

## Observability
- Service metrics (`observability/metrics.rs`) → `crates/gc-service/src/`, `crates/mc-service/src/`, `crates/mh-service/src/` | catalogs → `docs/observability/metrics/`
- Alerts → `infra/docker/prometheus/rules/{gc,mc,mh,client}-alerts.yaml` | docs → `docs/observability/alerts.md`, `dashboards.md`

## E2E Env-Tests, Cluster & Network (`crates/env-tests/`)
- Cluster infra → `src/cluster.rs:ClusterConnection` | Auth/GC fixtures → `src/fixtures/auth_client.rs`, `gc_client.rs`
- Join → `tests/24_join_flow.rs` | MH QUIC connect/auth → `tests/26_mh_quic.rs` | multi-party MH datagram forwarding → `tests/27_mc_slot_placement.rs` | web-app solo (R-3) → `packages/web-app/e2e/solo-participant.spec.ts`; multi-party S1/S2/S3/S6/S10a → `packages/web-app/e2e/{multi-party-hear,over-subscription,server-mute,kek-rotation,partial-connectivity}.spec.ts`
- Per-run org provisioning → `scripts/layer7.sh:__generate_org_subdomain()`, `infra/kind/scripts/setup.sh:provision_run_org()` | browser env → `packages/web-app/e2e/env.ts`
- Network policies → `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
