# Semantic Guard Navigation

## Architecture & Design
- Media path / key custody / frame-v2 → ADR-0036 | Guard methodology → ADR-0015 | Agent Teams pipeline → ADR-0024
- Polyglot layers → ADR-0033 | Story runner → ADR-0035 | Cluster helper (+ cannot-self-validate) → ADR-0030
- Semantic check definitions → `scripts/guards/semantic/checks.md` | Shell utils → `scripts/guards/common.sh`
- Reviewer-panel slot (Gate 2 unicast loop, Step 7 dedup) → `.claude/skills/devloop/SKILL.md`

## Media Path — Key Custody (ADR-0036, credential-leak items 11-13)
- MC key custody (KEK, identity key, sender-id, binding outcome; §4 floor in `mod.rs`) → `crates/mc-service/src/media_admission/`
- Frame-v2 + wrapped-key redacting Debug → `crates/media-protocol/src/frame.rs`
- Redaction control (`skip_debug` set + `RedactedLen` hand-Debug) → `crates/proto-gen/build.rs`, `crates/proto-gen/src/lib.rs`
- Signaling contract (MediaKind/Codec/SlotState, `JoinResponse.meeting_kek`) → `proto/dark_tower/signaling/v1/signaling.proto`
- MC→MH internal contract (RegisterMeeting, sole RPC) → `proto/dark_tower/internal/v1/internal.proto`

## MC Media Routing & MH Coordination
- Routing control plane (assignment / generation / confirm) → `crates/mc-service/src/media_routing/`
- MH programming call → `crates/mc-service/src/grpc/mh_client.rs` | sender-binding resolve → `grpc/media_coordination.rs`

## MH Media Hot Path (telemetry-free by construction)
- Forward path (no tracing/metrics macros) → `crates/mh-service/src/media/`
- Transport seam → `crates/mh-service/src/transport/mod.rs`, `webtransport/media_transport.rs`
- Sender binding (`SenderBindings`, `bind()`, `start_media_session`) → `crates/mh-service/src/session/mod.rs`

## Client SDK Media (credential-leak items 5-10 + credential lifetime)
- Hot path (no console/logger/metric names) → `packages/sdk-core/src/media/pipeline/`
- Custody siblings (KEK, roster keys, allow-list metric projection) → `packages/sdk-core/src/media/setup/`
- Layout-deny test → `packages/sdk-core/src/media/__tests__/hotPathLayout.test.ts` | whitelist-projection SAFE example → `packages/web-app/src/lib/e2eBus.ts`
- Media store → `packages/sdk-svelte/src/stores/MediaStore.svelte.ts`

## Media Telemetry Deny & Key-Custody Fixtures
- Deny guard → `crates/dt-guard/src/media_telemetry_deny.rs` | config → `scripts/guards/simple/media-telemetry-deny.yaml`
- Macro-vocab SSoT → `crates/dt-guard/src/telemetry_macros.rs`, `metric_macros.rs` | scope liveness → `crates/dt-guard/src/common/scope.rs`
- Key-custody fixtures + harness (fixture-verification runs) → `crates/dt-guard/tests/credential_leak_key_custody_fixtures.rs`

## MC Actors & WebTransport (`crates/mc-service/src/`)
- Actors (controller, meeting, participant, messages, metrics) → `actors/*.rs`
- Server (accept loop, TLS, capacity gate) → `webtransport/server.rs:WebTransportServer`
- Connection handler (join flow, post-join media dispatch, bridge loop) → `webtransport/connection.rs:handle_connection()`
- RegisterMeeting trigger (first-participant, async spawn) → `connection.rs:register_meeting_with_handlers()`
- MhRegistrationClient trait (testable RPC abstraction) → `grpc/mh_client.rs:MhRegistrationClient`

## Authentication Seams
- Common JWT (types, JWKS, validator, HasIat) → `crates/common/src/jwt.rs` | Token refresh → `common/src/token_manager.rs`
- GC JWT → `crates/gc-service/src/auth/jwt.rs` | MC JWT (McJwtValidator) → `crates/mc-service/src/auth/mod.rs`
- MC WebTransport JWT check (pre-actor) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- MH gRPC auth interceptor → `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor`
- JwtError → service error mapping → `crates/gc-service/src/errors.rs`, `crates/mc-service/src/errors.rs`

## GC Handlers, Repositories & MH Selection
- Create/Join/Guest/Settings handlers → `crates/gc-service/src/handlers/meetings.rs` | routes → `routes/mod.rs:build_routes()`
- Insert-error classification (SQLSTATE, not prose) → `repositories/meetings.rs:classify_insert_error()`
- Refusal → HTTP + `error.code` → `errors.rs:GcError` | MH selection (weighted random) → `services/mh_selection.rs:MhSelectionService`

## Devloop Tooling (`scripts/workflow/`, `crates/dt-story/`)
- Story runner seams + failure lanes → `run-story.sh` | Stop hook (fail-closed) → `devloop-stop-hook.sh`
- Manifest schema, `Slug`, fence-safe emit → `crates/dt-story/src/manifest.rs`; block extraction → `markdown.rs:find_manifest_block()`
- Manifest guard → `scripts/guards/simple/validate-story-manifest.sh` | Slug-class sync → `validate-slug-class-sync.sh`
- Planning / closing skills → `.claude/skills/user-story/SKILL.md`, `.claude/skills/close-story/SKILL.md` | shell assertions → `scripts/lang/_test_helpers.sh`

## Layer-7 Per-Run Org Provisioning
- Phase 1h generation + precondition lanes → `scripts/layer7.sh:__generate_org_subdomain()`
- SQL provisioning → `infra/kind/scripts/setup.sh:provision_run_org()` | Rust org resolution → `crates/env-tests/src/fixtures/auth_client.rs` | browser env → `packages/web-app/e2e/env.ts`

## Observability
- GC metrics → `crates/gc-service/src/observability/metrics.rs` | MC → `crates/mc-service/src/observability/metrics.rs`
- MH media metrics → `crates/mh-service/src/observability/metrics.rs` | catalogs → `docs/observability/metrics/`
- Alerts → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml` | docs → `docs/observability/alerts.md`, `dashboards.md`

## E2E Env-Tests (`crates/env-tests/`)
- Cluster infra → `src/cluster.rs:ClusterConnection` | Auth/GC fixtures → `src/fixtures/auth_client.rs`, `gc_client.rs`
- Join → `tests/24_join_flow.rs` | MH QUIC media loopback → `tests/26_mh_quic.rs` | web-app media loopback → `packages/web-app/e2e/media-loopback.spec.ts`

## Network Policies & Kind
- Per-service policies → `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml` | Kind setup → `infra/kind/scripts/setup.sh`
