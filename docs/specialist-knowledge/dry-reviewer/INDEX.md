# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)
- Cross-Boundary Ownership Model (three-tier classification, GSA, Paired flag) -> ADR-0024 §6
- GSA quartet + cross-boundary guards -> ADR-0024 §6.4; mirrors `.claude/skills/devloop/SKILL.md:116`, `.claude/skills/devloop/review-protocol.md:16`, `scripts/guards/simple/cross-boundary-ownership.yaml` (manifest; anchor-of-truth grep on "Mirror of ADR-0024 §6.4" / "Source of truth for GSA enumeration"); guards `scripts/guards/simple/validate-cross-boundary-scope.sh` + `scripts/guards/simple/validate-cross-boundary-classification.sh` share `scripts/guards/common.sh:parse_cross_boundary_table` (Pattern C precedent)
- Spin-out as third fix-or-defer path + Ownership-lens verdict field -> `.claude/skills/devloop/review-protocol.md:68-94`, `:113-119`
- Guard pipeline as Rust binary `dt-guard` (single crate, subcommand-per-policy, structural duplication impossible by construction) -> ADR-0034 (`docs/decisions/adr-0034-guard-pipeline-as-rust-binary.md`). Debate of record -> `docs/debates/2026-05-17-guard-toolchain-supersede/debate.md`. Per-guard vendor-coverage matrix (canonical (D)-complement input) -> `docs/debates/2026-05-14-python-guard-pipeline-strategy/guard-vendor-coverage-matrix.md`; predecessor debate -> `docs/debates/2026-05-14-python-guard-pipeline-strategy/debate.md`
- Story runner + manifest contract (manifest is the single home for per-task status/slug) -> ADR-0035 (`docs/decisions/adr-0035-story-runner.md`), §4; "no devloop can validate a change to `crates/devloop-helper/` within its own run" corollary -> ADR-0030 (`docs/decisions/adr-0030-host-side-cluster-helper.md`)

## Shared Helper SPOTs (single-source-of-truth across guards, scripts and tests)
- **Bash helpers** -> `scripts/guards/common.sh` (Pattern C precedent: `parse_cross_boundary_table` two-consumer extraction; `doc_citation_in_scope_files`, `path_matches_glob`). Bash guards share via `source common.sh`.
- **Bash test-assertion helpers** -> `scripts/lang/_test_helpers.sh:assert_absent()`/`assert_marker()`/`assert_no_marker()` (promoted out of `layer-all.test.sh`; the only definitions in-tree — new `*.test.sh` files source these rather than redefining)
- **Rust canonical-home kernels** (ADR-0034 §1 + §6) -> `crates/dt-guard/src/`: `ignore.rs::LAZY_REASON_RE` + `IGNORE_MARKER_RE`; `secret_patterns.rs::HYGIENE_PATTERNS` (consumed by `dt-guard alert-rules-policy` + `dt-guard secret-scan`); `common/path_safety.rs::resolve_cited_path` (dual-consumer with `dt-guard cite-extract` and `alert-rules-policy::validate_runbook_url`); `metric_macros.rs` (counter!/gauge!/histogram! trio).
- **Structural enforcement** (ADR-0034 §6): one `pub static Lazy<Regex>` per canonical module; workspace `clippy.toml disallowed-methods` blocks `regex::Regex::new` outside canonical module initializers (scoped `#[allow]` is the documented escape).
- **Module-naming discipline** (ADR-0034 §Neutral): catch-all names (`util`, `helpers`, `common`, `validate`) rejected at code-review. **Sprawl threshold** (ADR-0034 §When-to-Revisit): ≥10 subcommands triggers re-debate; Layer-1 cold-cache budget excess triggers a `dt-guard-core` lib/bin split.
- **Cross-stack duplication collapse** (ADR-0034 §6 + §10 Wave 3 Day 11): the 7-pattern set split across `validate-alert-rules.sh` heredoc ↔ `no-hardcoded-secrets.sh` regex literals collapses to `dt_guard::secret_patterns::HYGIENE_PATTERNS`.
- **Cross-encoding constant anchors** — each mirror site carries an `ANCHOR (DRY):` comment naming its source of truth (grep `ANCHOR (DRY)` to enumerate). Org-subdomain class -> `packages/sdk-core/src/validation/limits.ts:SUBDOMAIN_REGEX`, guarded across 7 encodings by `scripts/guards/simple/validate-subdomain-regex-sync.sh` (+ self-test `scripts/guards/validate-subdomain-regex-sync.test.sh`). Devloop-output slug class -> `crates/dt-story/src/manifest.rs:SLUG_PATTERN` (enforced in the `Slug` newtype deserializer, so every CLI verb, preflight, the Layer-3 guard and `/close-story` inherit one floor), guarded by `scripts/guards/simple/validate-slug-class-sync.sh` (+ `scripts/guards/validate-slug-class-sync.test.sh`).

## Story Workflow (run-story, dt-story, skills)
- Manifest schema + per-task state (`status`, `slug`, `commit`, `escalation`; no `branch`) -> `crates/dt-story/src/manifest.rs` (`Manifest`, `Task`, `Slug`, `to_block_body()`, `save()` round-trip assert); block discovery + orphan/unterminated-fence detection -> `crates/dt-story/src/markdown.rs:find_manifest_block()`; selection/completion engine -> `crates/dt-story/src/engine.rs`; CLI verbs (`next`, `complete --slug`, `escalate`, `validate`, `list-tasks`, `add-task --deps --tag`) -> `crates/dt-story/src/main.rs`
- Runner -> `scripts/workflow/run-story.sh`: `DEVLOOP_TEST`-gated seams `STORY_REPO_ROOT`/`DT_STORY` + containment predicate (`:50-150`), `canary_classify`/`canary_probe`, `newest_devloop_output`, git-lane helpers (`git_head`/`tree_dirty`/`git_error_lane`), out-of-band prompt passing. Hermetic suite -> `scripts/workflow/run-story.test.sh` (wired at `scripts/layer3.sh`); preflight -> `scripts/workflow/preflight-story.sh`; stop hook -> `scripts/workflow/devloop-stop-hook.sh`
- Manifest producers/consumers (one home each) -> `.claude/skills/user-story/SKILL.md` (Steps 10.4/10.5 emit + verify), `.claude/skills/close-story/SKILL.md` (reads status + slug), `scripts/guards/simple/validate-story-manifest.sh`; template -> `docs/user-stories/_template.md`

## JWT Validation (Common + Thin Wrappers)
- Common JWT code (all shared logic) -> `crates/common/src/jwt.rs`; internal token request types (GC->AC contract) -> `crates/common/src/meeting_token.rs`
- GC thin wrapper -> `crates/gc-service/src/auth/jwt.rs` (ServiceClaims, UserClaims); MC thin wrapper -> `crates/mc-service/src/auth/mod.rs` (MeetingTokenClaims, GuestTokenClaims); MH JWKS config -> `infra/services/mh-service/configmap.yaml:AC_JWKS_URL`
- Display-name join carrier (AC resolves -> shared claim -> MC consumes) -> `crates/ac-service/src/handlers/internal_tokens.rs:resolve_meeting_display_name`, `crates/common/src/jwt.rs:MeetingTokenClaims` (`display_name`, GSA); AC-local serialize-only copy lockstep -> `docs/TODO.md` #53

## Per-Service Observability (Metrics, Dashboards, Health)
- AC/GC/MC/MH metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication); paired MC↔MH notification counters — different sender/receiver perspectives, not duplication
- Alert rules -> `infra/docker/prometheus/rules/{mc,gc,mh}-alerts.yaml`; guard + conventions -> `scripts/guards/simple/validate-alert-rules.sh`, `docs/observability/alert-conventions.md` (ADR-0031); dashboards -> ADR-0029, `infra/grafana/dashboards/`
- Runbooks (per-service incident-response + deployment) -> `docs/runbooks/`; canonical post-deploy checklist owned in `docs/runbooks/mh-deployment.md`, MC addendum cross-pointers to it
- Health endpoints -> `crates/mc-service/src/observability/health.rs:health_router()` | `crates/mh-service/src/observability/health.rs` (duplicated, see TODO.md) | `crates/gc-service/src/routes/mod.rs:64-65`

## Integration Test Coverage
- MC tests -> `crates/mc-service/tests/`; idempotent MH-retry invariant -> `crates/mc-service/src/grpc/media_coordination.rs:tests::test_coordination_flow_connect_disconnect_round_trip`; GC -> `crates/gc-service/tests/meeting_tests.rs`, `crates/gc-service/tests/meeting_create_tests.rs`
- MC shared scaffolding (MockMhAssignmentStore, MockMhRegistrationClient, TestStackHandles, build_test_stack, seed_meeting_with_mh) -> `crates/mc-service/tests/common/mod.rs`, `crates/mc-service/tests/join_tests.rs`
- MC accept-loop rig -> `crates/mc-service/tests/common/accept_loop_rig.rs:AcceptLoopRig` (near-clone of MH's; extraction candidate per ADR-0032 §Step 6 + TODO.md)
- MH tests -> `crates/mh-service/tests/`; MH shared rigs (TestKeypair, mock_mc, jwks_rig, grpc_rig, accept_loop_rig, wt_client, tokens) -> `crates/mh-service/tests/common/`
- env-tests (cluster integration) -> `crates/env-tests/tests/`; MH QUIC E2E -> `:26_mh_quic.rs`; join flow -> `:24_join_flow.rs`; per-run org subdomain resolution (no fallback const) -> `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`
- AC tests -> `crates/ac-service/tests/*_integration.rs` + `crates/ac-service/tests/common/`; GC scaffolding -> `crates/gc-service/tests/common/jwt_fixtures.rs`; shared fixtures -> `crates/{ac,gc,mc}-test-utils/src/`; MetricAssertion -> `crates/common/src/observability/testing.rs`
- Browser E2E harness (Playwright) -> `packages/web-app/e2e/`: topology/config `env.ts:describeEnv`/`orgSubdomain` (required `E2E_ORG_SUBDOMAIN`, base URL derived from it); auth+join fixtures `fixtures.ts`; single MC-PromQL home `mcMetrics.ts:pollUntilSumAbove`; division of responsibility vs `crates/env-tests/tests/` -> `packages/web-app/e2e/README.md` (ADR-0028); node-tier env contract -> `packages/web-app/tests/e2e-env.test.ts`
- Script self-tests -> `scripts/**/*.test.sh` (wired in `scripts/layer3.sh`); shared assertions -> `scripts/lang/_test_helpers.sh`

## Per-Service Config Parsing
- AC/GC/MC/MH config -> `crates/*/src/config.rs:Config::from_vars()` (per-service); ordinal parsing -> `crates/common/src/config.rs:parse_statefulset_ordinal()`; extraction candidate: `generate_instance_id(prefix)` -> 4-line pattern in GC + MC + MH config

## gRPC Auth & Clients (Cross-Service)
- MC/MH auth layers (async JWKS; near-identical tower Layer/Service — extraction candidate in TODO.md; shared `common::jwt::MAX_JWT_SIZE_BYTES`) -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer`, `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- MC GcClient (bounded retries, fast/comprehensive heartbeats) -> `crates/mc-service/src/grpc/gc_client.rs`; MC MhClient (per-call channels, no retries) -> `crates/mc-service/src/grpc/mh_client.rs`; MH GcClient (unbounded retries, load reports) -> `crates/mh-service/src/grpc/gc_client.rs`; MH McClient (channel-per-call, exp backoff) -> `crates/mh-service/src/grpc/mc_client.rs`; shared `add_auth` (~10 lines, 4 call sites — extraction candidate per TODO.md)

## MC / MH Service Internals
- MC assignment (GC→MC) -> `crates/mc-service/src/grpc/mc_service.rs:McAssignmentService`; media coordination (MH→MC) -> `crates/mc-service/src/grpc/media_coordination.rs`; MH connection registry (single MAX_ID_LENGTH source) -> `crates/mc-service/src/mh_connection_registry.rs`; MhRegistrationClient trait + async RegisterMeeting trigger -> `crates/mc-service/src/grpc/mh_client.rs`, `crates/mc-service/src/webtransport/connection.rs:register_meeting_with_handlers()`
- MC Redis: MhAssignmentStore trait (testability seam) + FencedRedisClient -> `crates/mc-service/src/redis/client.rs`
- MH WebTransport stack -> `crates/mh-service/src/webtransport/` (`connection.rs:await_meeting_registration()`); MH JWT validator + SessionManager + gRPC clients -> `crates/mh-service/src/auth/mod.rs:MhJwtValidator`, `crates/mh-service/src/session/mod.rs:SessionManager`, `crates/mh-service/src/grpc/`; MH selection -> `crates/gc-service/src/services/mh_selection.rs:MhSelection`

## GC Meeting Creation (Refusal Causes)
- Typed outcome + cause discriminant -> `crates/gc-service/src/repositories/meetings.rs` (`CreateMeetingOutcome`, `MeetingRefusal::metric_label()`/`from_discriminant()`, `classify_insert_error()`); single cause→wire mapping site -> `crates/gc-service/src/errors.rs:impl From<MeetingRefusal> for GcError`; handler match + per-cause logs/labels -> `crates/gc-service/src/handlers/meetings.rs`; wire contract -> `docs/API_CONTRACTS.md`
- Sibling org exists-and-active check (different owner, TODO-tracked) -> `crates/ac-service/src/repositories/organizations.rs:get_by_subdomain`

## Per-Service Infrastructure (K8s, Docker, Kind)
- Kustomize bases -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`; MC/MH per-pod Services + ConfigMaps (port: `base + ordinal*2`) -> `infra/services/{mc,mh}-service/`; network policies -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`; Kind setup/teardown (ADR-0030) -> `infra/kind/scripts/{setup,teardown}.sh` (single psql entry point `dt_psql()`, `ORG_MAX_CONCURRENT_MEETINGS`, `provision_run_org()` behind `--provision-org`; self-test `scripts/setup.test.sh`); devloop -> `infra/devloop/devloop.sh`
- Layer-7 per-run org provisioning (Phase 1h: subdomain generator, `PRECONDITION_FAILURE` exit-2 lanes, `ENV_TEST_ORG_SUBDOMAIN`/`E2E_ORG_SUBDOMAIN` export) -> `scripts/layer7.sh`; tests -> `scripts/layer7.test.sh`

## False Positive Boundaries
- Per-service error mapping (GcError/McError/MhError); MC vs MH GcClient (different RPCs/retry); AC vs GC rate limiting (different mechanisms); `common::jwt` vs `common::meeting_token` (JWT enums narrower)
- AC test-side `Claims { ... }` literal repetition -> false-positive on the literal; surrounding decrypt-and-sign IS extracted to `crates/ac-service/tests/common/jwt_fixtures.rs`

## Tech Debt / Common Crates
- Active cross-service duplication -> `docs/TODO.md`; common crate + per-service test-utils -> `crates/common/src/`, `crates/{ac,gc,mc}-test-utils/`
