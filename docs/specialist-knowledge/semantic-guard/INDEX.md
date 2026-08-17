# Semantic Guard Navigation

## Architecture & Design
- Guard methodology → ADR-0015 | Agent Teams pipeline → ADR-0024 | Polyglot layers → ADR-0033
- Story runner → ADR-0035 | Cluster helper (+ cannot-self-validate corollary) → ADR-0030
- Semantic check definitions → `scripts/guards/semantic/checks.md` | Shell utils → `scripts/guards/common.sh`
- Reviewer-panel slot (Gate 2 unicast loop, Step 7 dedup) → `.claude/skills/devloop/SKILL.md`

## Story Runner (`scripts/workflow/`)
- Test seams (`STORY_REPO_ROOT`, `DT_STORY`) + containment predicate → `run-story.sh` seam block (after `set -euo pipefail`)
- Failure lanes → `run-story.sh:canary_probe()`, `canary_classify()`, `git_error_lane()`, `newest_devloop_output()`
- Hermetic suite → `scripts/workflow/run-story.test.sh` (wired in `scripts/layer3.sh`)
- Preflight → `preflight-story.sh` | Stop hook (fail-closed) → `devloop-stop-hook.sh`
- Planning / closing skills → `.claude/skills/user-story/SKILL.md`, `.claude/skills/close-story/SKILL.md`

## dt-story Manifest (`crates/dt-story/src/`)
- Schema, `Slug` newtype, `SLUG_PATTERN`, fence-safe emit → `manifest.rs:Slug`, `Manifest::to_block_body()`
- Block extraction (unterminated fence + orphan `- id:` scan) → `markdown.rs:find_manifest_block()`
- Task selection / completion (status + slug write) → `engine.rs` | CLI verbs → `main.rs`
- Manifest guard → `scripts/guards/simple/validate-story-manifest.sh`

## Drift Guards & Shell Test Helpers
- Slug class sync → `scripts/guards/simple/validate-slug-class-sync.sh` | Org-subdomain regex sync → `validate-subdomain-regex-sync.sh`
- Shared assertions (`assert_absent`/`assert_marker`/`assert_no_marker`) → `scripts/lang/_test_helpers.sh`

## Layer-7 Per-Run Org Provisioning
- Phase 1h generation + precondition lanes → `scripts/layer7.sh:__generate_org_subdomain()`
- SQL provisioning (stopgap for an AC org API) → `infra/kind/scripts/setup.sh:provision_run_org()`, `dt_psql()`
- Rust suite org resolution → `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`
- Browser env contract → `packages/web-app/e2e/env.ts` | Hermetic coverage → `scripts/layer7.test.sh`, `scripts/setup.test.sh`

## Authentication Seams
- Common JWT (types, JWKS, validator, HasIat) → `crates/common/src/jwt.rs` | Token refresh → `common/src/token_manager.rs`
- GC JWT validation → `crates/gc-service/src/auth/jwt.rs` | JWKS → `auth/jwks.rs` | Middleware → `middleware/auth.rs`
- MC JWT validation (McJwtValidator) → `crates/mc-service/src/auth/mod.rs` | JWKS config → `mc-service/src/config.rs:ac_jwks_url`
- MC WebTransport JWT check (pre-actor) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- MH gRPC auth interceptor → `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor` | JWKS → `infra/services/mh-service/configmap.yaml:AC_JWKS_URL`
- JwtError → service error mapping → `crates/gc-service/src/errors.rs`, `crates/mc-service/src/errors.rs`

## MC Actors & WebTransport (`crates/mc-service/src/`)
- Actors (controller, meeting, participant, messages, metrics) → `actors/*.rs`
- Server (accept loop, TLS, capacity gate) → `webtransport/server.rs:WebTransportServer`
- Connection handler (join flow, bridge loop) → `webtransport/connection.rs:handle_connection()`
- RegisterMeeting trigger (first-participant, async spawn) → `connection.rs:register_meeting_with_handlers()`
- MhRegistrationClient trait (testable RPC abstraction) → `grpc/mh_client.rs:MhRegistrationClient`
- Protobuf framing utilities → `webtransport/handler.rs:encode_participant_update()`

## GC Handlers, Repositories & MH Selection
- Create/Join/Guest/Settings handlers → `crates/gc-service/src/handlers/meetings.rs`
- Meeting-refusal outcomes → `repositories/meetings.rs:CreateMeetingOutcome`, `MeetingRefusal::metric_label()`
- Insert-error classification (SQLSTATE, not prose) → `repositories/meetings.rs:classify_insert_error()`
- Refusal → HTTP + `error.code` + `error_type` → `errors.rs:GcError` (`OrgMeetingLimitExceeded`, `OrgInactive`, `OrgNotProvisioned`)
- Routes → `routes/mod.rs:build_routes()` | Participants → `repositories/participants.rs` | Models → `models/mod.rs`
- MH selection (weighted random) → `services/mh_selection.rs:MhSelectionService` | Assignment → `services/mc_assignment.rs:AssignmentWithMh`

## Service Test Harnesses
- GC meeting tests → `crates/gc-service/tests/meeting_tests.rs`, `meeting_create_tests.rs`, `mc_assignment_rpc_tests.rs`
- MC TestKeypair + JWKS mock → `crates/mc-test-utils/src/jwt_test.rs` | Join tests → `crates/mc-service/tests/join_tests.rs`

## Observability
- GC metrics → `crates/gc-service/src/observability/metrics.rs` | MC → `crates/mc-service/src/observability/metrics.rs`
- Metric catalogs (one per service) → `docs/observability/metrics/` | Dashboards → `infra/grafana/dashboards/gc-overview.json`, `mc-overview.json`
- Alerts → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml` | Docs → `docs/observability/alerts.md`, `dashboards.md`

## E2E Env-Tests (`crates/env-tests/`)
- Cluster infra → `src/cluster.rs:ClusterConnection`, `ClusterPorts::from_env()`, `parse_host_port()`
- Auth/GC fixtures → `src/fixtures/auth_client.rs`, `gc_client.rs` | Join flow → `tests/24_join_flow.rs`

## Network Policies & Kind
- Per-service policies → `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- Kind overlay → `infra/kubernetes/overlays/kind/` | Setup → `infra/kind/scripts/setup.sh` | Helper → `crates/devloop-helper/src/commands.rs`
