# Code Reviewer Navigation

## Architecture & Design (ADRs)

- Actor handle/task separation → ADR-0001; Error handling + service-layer wrapping → ADR-0003; Cross-service duplication → ADR-0019
- No-panic policy, `#[expect]` over `#[allow]` → ADR-0002; workspace clippy deny list → `Cargo.toml`
- Observability naming + SLOs → ADR-0011; Dashboards → ADR-0029; Alert conventions → ADR-0031; Metric testability → ADR-0032
- Guard methodology → ADR-0015; Guard pipeline as a Rust binary → ADR-0034; Polyglot validation pipeline → ADR-0033
- Agent Teams devloop + cross-boundary ownership → ADR-0024; Containerized devloop → ADR-0025; Local dev → ADR-0013; Host cluster helper → ADR-0030
- Deterministic story runner → ADR-0035
- User auth + three-tier tokens → ADR-0020; Infrastructure architecture → ADR-0012; Browser E2E harness → ADR-0028
- Cross-boundary classification vocabulary → ADR-0024, `.claude/skills/devloop/SKILL.md`, `.claude/skills/devloop/review-protocol.md`

## Story Runner & Orchestration (ADR-0035)

- Serial loop, canary classification, gate invocation, auto-remediation → `scripts/workflow/run-story.sh`
- Container guard, settings patch, substrate probe → `scripts/workflow/preflight-story.sh`
- Headless completion enforcement → `scripts/workflow/devloop-stop-hook.sh`
- Manifest engine, markdown parsing, CLI verbs → `crates/dt-story/src/engine.rs`, `crates/dt-story/src/markdown.rs`, `crates/dt-story/src/manifest.rs`, `crates/dt-story/src/main.rs`
- Manifest CLI tests + fixtures → `crates/dt-story/tests/cli.rs`
- Story manifest guard → `scripts/guards/simple/validate-story-manifest.sh`
- Devloop workflow + review protocol → `.claude/skills/devloop/SKILL.md`, `.claude/skills/devloop/review-protocol.md`

## Validation Pipeline (ADR-0033)

- Pipeline entry + per-layer wrappers → `scripts/layer-all.sh`, `scripts/lang/_dispatch.sh`, `scripts/lang/_common.sh`
- Gate-2 binding: producer, validator, signature, exclusion set → `scripts/lang/_gate2_binding.sh`
- Layer 7 env-tests + browser E2E → `scripts/layer7.sh`; failure-mode triage → `docs/runbooks/devloop-validation.md`
- Bash test convention → `scripts/layer-all.test.sh`, `scripts/layer7.test.sh`, `scripts/lang/_common.test.sh`, `scripts/lang/_dispatch.test.sh`

## Guards & Suppression Surfaces (ADR-0034)

- Guard runner → `scripts/guards/run-guards.sh`; subcommand crate → `crates/dt-guard/src/main.rs`, `crates/dt-guard/src/lib.rs`
- Semantic lens checklist; mechanical-guard boundary precedent → `scripts/guards/semantic/checks.md`
- Full-tree standing invariant (client credential lifetime) → `crates/dt-guard/src/ts_retained_credentials.rs`
- `guard:ignore` marker + lazy-reason parsing → `crates/dt-guard/src/ignore.rs`
- Audit suppression SSoT + drift check → `audit-suppressions.toml`, `scripts/guards/simple/audit-suppressions.sh`, `scripts/audit-suppressions-check.test.sh`
- Knowledge-index policy (pointer resolution, ADR refs, 75-line cap) → `crates/dt-guard/src/knowledge_index.rs`
- Cross-boundary classification, scope drift, GSA sync → `crates/dt-guard/src/cross_boundary_classification.rs`, `crates/dt-guard/src/cross_boundary_scope.rs`, `crates/dt-guard/src/gsa_sync.rs`
- Structural-duplication ban (`Regex::new` outside canonical homes) → `clippy.toml`
- Metrics, labels, coverage, histograms → `crates/dt-guard/src/application_metrics.rs`, `crates/dt-guard/src/metric_labels.rs`, `crates/dt-guard/src/metric_coverage.rs`, `crates/dt-guard/src/histogram_buckets.rs`
- Test coverage, registration, rigidity → `crates/dt-guard/src/test_coverage.rs`, `crates/dt-guard/src/test_registration.rs`, `crates/dt-guard/src/test_rigidity.rs`
- Secrets + PII vocabularies → `crates/dt-guard/src/secret_patterns.rs`, `crates/dt-guard/src/rust_pii.rs`, `crates/dt-guard/src/ts_pii.rs`, `crates/dt-guard/src/common/pii_vocabulary.rs`
- Kustomize + alert rules → `scripts/guards/simple/validate-kustomize.sh`, `scripts/guards/simple/validate-alert-rules.sh`

## Code Locations — AC Service

- Config → `crates/ac-service/src/config.rs`; Error type + `ErrorCategory` → `crates/ac-service/src/errors.rs`, `crates/ac-service/src/observability/mod.rs`
- Crypto (EdDSA, AES-256-GCM, bcrypt) → `crates/ac-service/src/crypto/mod.rs`
- Handlers + routes → `crates/ac-service/src/handlers/auth_handler.rs`, `crates/ac-service/src/routes/mod.rs`
- Meeting-token display-name issuance → `crates/ac-service/src/handlers/internal_tokens.rs`, `crates/ac-service/src/repositories/users.rs`
- Repository + service layers → `crates/ac-service/src/repositories/signing_keys.rs`, `crates/ac-service/src/services/key_management_service.rs`
- Metrics → `crates/ac-service/src/observability/metrics.rs`; shared test fixtures → `crates/ac-service/tests/common/test_state.rs`
- K8s wiring → `infra/services/ac-service/configmap.yaml`, `infra/services/ac-service/statefulset.yaml`

## Code Locations — GC / MC / MH

- GC error type, auth, meeting handlers → `crates/gc-service/src/errors.rs`, `crates/gc-service/src/middleware/auth.rs`, `crates/gc-service/src/handlers/meetings.rs`; repositories, AC client, MH selection → `crates/gc-service/src/repositories/meetings.rs`, `crates/gc-service/src/services/ac_client.rs`, `crates/gc-service/src/services/mh_selection.rs`
- MC error type, auth, config → `crates/mc-service/src/errors.rs`, `crates/mc-service/src/auth/mod.rs`, `crates/mc-service/src/config.rs`
- MC WebTransport join + disconnect → `crates/mc-service/src/webtransport/server.rs`, `crates/mc-service/src/webtransport/connection.rs`, `crates/mc-service/src/actors/meeting.rs`; gRPC client, registry, Redis → `crates/mc-service/src/grpc/mh_client.rs`, `crates/mc-service/src/mh_connection_registry.rs`, `crates/mc-service/src/redis/client.rs`
- MH config, error type, auth, sessions → `crates/mh-service/src/config.rs`, `crates/mh-service/src/errors.rs`, `crates/mh-service/src/auth/mod.rs`, `crates/mh-service/src/session/mod.rs`; WebTransport + gRPC → `crates/mh-service/src/webtransport/connection.rs`, `crates/mh-service/src/grpc/mh_service.rs`, `crates/mh-service/src/grpc/mc_client.rs`
- Per-service metrics → `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`, `crates/mh-service/src/observability/metrics.rs`

## Code Locations — Common

- JWT, claims, JWKS, validator → `crates/common/src/jwt.rs`; meeting-token shared types → `crates/common/src/meeting_token.rs`
- Secret wrappers + token manager → `crates/common/src/secret.rs`, `crates/common/src/token_manager.rs`
- `MetricAssertion` test harness (ADR-0032, `test-utils` feature) → `crates/common/src/observability/testing.rs`

## Testing & Infrastructure

- Env-tests cluster module → `crates/env-tests/src/cluster.rs`; Kind cluster setup → `infra/kind/scripts/setup.sh`
- Browser E2E harness → `packages/web-app/e2e/`; dev certs → `scripts/generate-dev-certs.sh`
- Devloop helper + container tooling → `crates/devloop-helper/src/commands.rs`, `infra/devloop/devloop.sh`
- Deferred work + decay tracking → `docs/TODO.md`
