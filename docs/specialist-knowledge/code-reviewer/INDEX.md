# Code Reviewer Navigation

## Architecture & Design (ADRs)

- Media flow (frame-v2, signaling/internal contracts, §4 security floor, §8 control plane, §10 transport seam, §11 telemetry/layout) → ADR-0036; approved crypto (SFrame, AES-256-GCM, EdDSA) → ADR-0027
- Actor handle/task separation → ADR-0001; Error handling + service-layer wrapping → ADR-0003; Cross-service duplication → ADR-0019
- No-panic policy, `#[expect]` over `#[allow]` → ADR-0002; workspace clippy deny list → `Cargo.toml`
- Observability naming + SLOs → ADR-0011 (targets now authoritative in `docs/observability/slos.md`); Dashboards → ADR-0029; Alert conventions → ADR-0031; Metric testability → ADR-0032
- Guard methodology → ADR-0015; Guard pipeline as a Rust binary → ADR-0034; Polyglot validation pipeline → ADR-0033
- Agent Teams devloop + cross-boundary ownership → ADR-0024; Containerized devloop → ADR-0025; Local dev → ADR-0013; Host cluster helper (+ helper-cannot-self-validate corollary) → ADR-0030
- Story runner + manifest per-task state → ADR-0035; User auth + three-tier tokens → ADR-0020; Infrastructure architecture → ADR-0012; Browser E2E harness → ADR-0028
- Cross-boundary classification vocabulary → ADR-0024, `.claude/skills/devloop/SKILL.md`, `.claude/skills/devloop/review-protocol.md`

## Media Path (story: hear-yourself-through-handler)

- Frame-v2 codec (one `parse_layout()`, four entry points, TLV) → `crates/media-protocol/src/codec.rs`, `crates/media-protocol/src/frame.rs`, `crates/media-protocol/src/extensions.rs`; frozen cross-language vectors + generator → `proto/test-vectors/frame-v2.vectors.json`, `crates/media-vector-gen/`
- Wire contract (media vocabulary, `sender_id`, `meeting_kek`) → `proto/dark_tower/signaling/v1/signaling.proto`; MC→MH single-RPC control plane → `proto/dark_tower/internal/v1/internal.proto`; redacting `Debug` (`skip_debug` + `RedactedLen`) → `crates/proto-gen/src/lib.rs`, `crates/proto-gen/build.rs`; Rule R (reserve vacated tag+name) → `docs/protocol/CONVENTIONS.md`
- MC admission (KEK, identity key, sender-id, binding outcome) → `crates/mc-service/src/media_admission/`; routing plane (assignment / generation / confirm) → `crates/mc-service/src/media_routing/`; client signaling (capability / directive / assignments / outcome) → `crates/mc-service/src/media_signaling/`
- MC steering + post-join dispatch → `crates/mc-service/src/webtransport/connection.rs`; policy push `MeetingProgramming` → `crates/mc-service/src/grpc/mh_client.rs`; sender-binding resolve → `crates/mc-service/src/grpc/media_coordination.rs`
- MH transport seam (§10) + real wtransport impl → `crates/mh-service/src/transport/mod.rs`, `crates/mh-service/src/webtransport/media_transport.rs`; QUIC params + derived drain → `crates/mh-service/src/config.rs`
- MH hot path (ingress/forward/egress, drop-oldest ring, telemetry-macro-free) → `crates/mh-service/src/media/`; per-frame routing `for_each_source` → `crates/mh-service/src/routing/mod.rs`; policy-apply mailbox + `SenderBindings` → `crates/mh-service/src/session/mod.rs`; transport/policy shims → `crates/mh-test-utils/`
- Client frame codec/SFrame/receive path → `packages/sdk-core/src/media/frame/`; audio pipeline + setup → `packages/sdk-core/src/media/pipeline/`, `packages/sdk-core/src/media/setup/`; in-meeting store/view/slot map → `packages/sdk-svelte/src/stores/MediaStore.svelte.ts`, `packages/web-app/src/views/InMeeting.svelte`, `packages/web-app/src/lib/slotState.ts`
- Shared label vocabulary (`key_custody`) → `crates/common/src/observability/labels.rs`; media-telemetry-deny guard + scope machinery → `crates/dt-guard/src/media_telemetry_deny.rs`, `crates/dt-guard/src/telemetry_macros.rs`, `crates/dt-guard/src/common/scope.rs`

## Story Runner & Orchestration (ADR-0035)

- Story runner (serial loop, gate rc lanes, slug derivation, `DEVLOOP_TEST` seams) → `scripts/workflow/run-story.sh`; hermetic suite → `scripts/workflow/run-story.test.sh`; preflight → `scripts/workflow/preflight-story.sh`; completion hook → `scripts/workflow/devloop-stop-hook.sh`
- Manifest schema, `Slug` newtype, fence-safe emit → `crates/dt-story/src/manifest.rs:to_block_body()`; block discovery → `crates/dt-story/src/markdown.rs:find_manifest_block()`; engine/CLI → `crates/dt-story/src/engine.rs`, `crates/dt-story/src/main.rs`; tests + fixtures → `crates/dt-story/tests/cli.rs`, `crates/dt-story/tests/fixtures/`
- Manifest emit (planning) / read (closing) → `.claude/skills/user-story/SKILL.md`, `.claude/skills/close-story/SKILL.md`; manifest + slug-class guards → `scripts/guards/simple/validate-story-manifest.sh`, `scripts/guards/simple/validate-slug-class-sync.sh`
- Devloop workflow + review protocol → `.claude/skills/devloop/SKILL.md`, `.claude/skills/devloop/review-protocol.md`

## Validation Pipeline (ADR-0033)

- Pipeline entry + per-layer wrappers → `scripts/layer-all.sh`, `scripts/lang/_dispatch.sh`, `scripts/lang/_common.sh`; Gate-2 binding → `scripts/lang/_gate2_binding.sh`
- Layer 7 env-tests, browser E2E, per-run org provisioning → `scripts/layer7.sh`; failure-mode triage → `docs/runbooks/devloop-validation.md`
- Bash test convention + shared assertions → `scripts/lang/_test_helpers.sh`, `scripts/layer-all.test.sh`, `scripts/layer7.test.sh`, `scripts/setup.test.sh`, `scripts/lang/_common.test.sh`, `scripts/lang/_dispatch.test.sh`

## Guards & Suppression Surfaces (ADR-0034)

- Guard runner → `scripts/guards/run-guards.sh`; subcommand crate → `crates/dt-guard/src/main.rs`, `crates/dt-guard/src/lib.rs`; semantic lens checklist → `scripts/guards/semantic/checks.md`
- Full-tree standing invariant (client credential lifetime) → `crates/dt-guard/src/ts_retained_credentials.rs`
- `guard:ignore` marker + lazy-reason parsing → `crates/dt-guard/src/ignore.rs`; knowledge-index policy (pointer resolution, ADR refs, 75-line cap) → `crates/dt-guard/src/knowledge_index.rs`
- Audit suppression SSoT + drift check → `audit-suppressions.toml`, `scripts/guards/simple/audit-suppressions.sh`, `scripts/audit-suppressions-check.test.sh`
- Cross-boundary classification, scope drift, GSA sync → `crates/dt-guard/src/cross_boundary_classification.rs`, `crates/dt-guard/src/cross_boundary_scope.rs`, `crates/dt-guard/src/gsa_sync.rs`
- Structural-duplication ban (`Regex::new` outside canonical homes) → `clippy.toml`; release-build-profile / insecure-browser-flags / env-config guards → `crates/dt-guard/src/release_build_profile.rs`, `crates/dt-guard/src/no_insecure_browser_flags.rs`, `crates/dt-guard/src/env_config.rs`
- Metrics, labels, coverage, histograms → `crates/dt-guard/src/application_metrics.rs`, `crates/dt-guard/src/metric_labels.rs`, `crates/dt-guard/src/metric_coverage.rs`, `crates/dt-guard/src/histogram_buckets.rs`; macro vocabulary home → `crates/dt-guard/src/metric_macros.rs`
- Test coverage, registration, rigidity → `crates/dt-guard/src/test_coverage.rs`, `crates/dt-guard/src/test_registration.rs`, `crates/dt-guard/src/test_rigidity.rs`
- Secrets + PII vocabularies → `crates/dt-guard/src/secret_patterns.rs`, `crates/dt-guard/src/rust_pii.rs`, `crates/dt-guard/src/ts_pii.rs`, `crates/dt-guard/src/common/pii_vocabulary.rs`
- Kustomize, alert rules, org-subdomain pattern sync → `scripts/guards/simple/validate-kustomize.sh`, `scripts/guards/simple/validate-alert-rules.sh`, `scripts/guards/simple/validate-subdomain-regex-sync.sh`

## Code Locations — AC / GC / MC / MH / Common

- AC: config → `crates/ac-service/src/config.rs`; error + `ErrorCategory` → `crates/ac-service/src/errors.rs`, `crates/ac-service/src/observability/mod.rs`; crypto → `crates/ac-service/src/crypto/mod.rs`; handlers/routes → `crates/ac-service/src/handlers/auth_handler.rs`, `crates/ac-service/src/routes/mod.rs`
- AC: meeting-token display-name issuance → `crates/ac-service/src/handlers/internal_tokens.rs`, `crates/ac-service/src/repositories/users.rs`; repo/service layers → `crates/ac-service/src/repositories/signing_keys.rs`, `crates/ac-service/src/services/key_management_service.rs`; metrics → `crates/ac-service/src/observability/metrics.rs`
- GC error type, auth, meeting handlers → `crates/gc-service/src/errors.rs`, `crates/gc-service/src/middleware/auth.rs`, `crates/gc-service/src/handlers/meetings.rs`; refusal outcomes → `crates/gc-service/src/repositories/meetings.rs:CreateMeetingOutcome`; AC client, MH selection → `crates/gc-service/src/services/ac_client.rs`, `crates/gc-service/src/services/mh_selection.rs`
- MC error/auth/config → `crates/mc-service/src/errors.rs`, `crates/mc-service/src/auth/mod.rs`, `crates/mc-service/src/config.rs`; WebTransport join/disconnect → `crates/mc-service/src/webtransport/server.rs`, `crates/mc-service/src/actors/meeting.rs`; gRPC/registry/Redis → `crates/mc-service/src/grpc/mh_client.rs`, `crates/mc-service/src/mh_connection_registry.rs`, `crates/mc-service/src/redis/client.rs`
- MH config/error/auth/session → `crates/mh-service/src/config.rs`, `crates/mh-service/src/errors.rs`, `crates/mh-service/src/auth/mod.rs`, `crates/mh-service/src/session/mod.rs`; WebTransport + gRPC → `crates/mh-service/src/webtransport/connection.rs`, `crates/mh-service/src/grpc/mh_service.rs`, `crates/mh-service/src/grpc/mc_client.rs`
- Per-service metrics → `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`, `crates/mh-service/src/observability/metrics.rs`
- Common: JWT/claims/JWKS → `crates/common/src/jwt.rs`; meeting-token types → `crates/common/src/meeting_token.rs`; secret wrappers + token manager → `crates/common/src/secret.rs`, `crates/common/src/token_manager.rs`; `MetricAssertion` harness → `crates/common/src/observability/testing.rs`

## Testing & Infrastructure

- Env-tests cluster + per-run org resolution → `crates/env-tests/src/cluster.rs`, `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`; Kind setup + org provisioning → `infra/kind/scripts/setup.sh:provision_run_org()`
- Media env-tests: metric-hygiene kernel → `crates/env-tests/src/fixtures/metric_hygiene.rs`, `crates/env-tests/tests/32_media_metric_hygiene.rs`; release feature-gate self-test → `scripts/release-feature-gate.test.sh`
- Browser E2E harness + required org env → `packages/web-app/e2e/`, `packages/web-app/e2e/env.ts`; dev certs → `scripts/generate-dev-certs.sh`
- Devloop helper + container tooling → `crates/devloop-helper/src/commands.rs`, `infra/devloop/devloop.sh`; deferred work + decay → `docs/TODO.md`
