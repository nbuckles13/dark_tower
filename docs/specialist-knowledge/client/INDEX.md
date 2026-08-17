# Client Navigation

## Architecture & Design
- Client architecture (SDK, testing, deployment, E2EE) → ADR-0028
- Approved cryptographic algorithms → ADR-0027
- User auth and meeting access flows → ADR-0020
- Observability standards → ADR-0011
- Guards methodology → ADR-0015
- Test strategy → ADR-0005
- Debate record → `docs/debates/2026-02-28-client-architecture/debate.md`

## SDK Core (`packages/sdk-core/src/`)
- Signaling client (proto ServerMessage → events) → `signaling/SignalingClient.ts`
- Meeting session (event bridge, single-use per join) → `session/MeetingSession.ts`
- Binary media frame codec → `framing/`
- WebCodecs encode/decode → `media/`
- WebTransport connection mgmt → `transport/`
- HTTP/3 meeting API client → `http/`
- Error taxonomy (`SdkErrorCode`) → `errors/`
- HTTP status → meeting error mapping (cap vs org-inactive 403s) → `errors/MeetingError.ts:fromResponse()`
- Input validation SSoT (`SUBDOMAIN_REGEX`, length caps, validators) → `validation/limits.ts`
- Protobuf-es generated types → `proto/dark_tower/`

## Svelte Adapter (`packages/sdk-svelte/src/`)
- Roster/participant store (seeded from `existingParticipants`, excludes self) → `stores/MeetingStore.svelte.ts`
- Session-to-store binding → `stores/bindMeetingSession.ts`

## Web Application (`packages/web-app/src/`)
- Roster DOM (`participant-list` / `participant-${id}` testids) → `views/JoinMeeting.svelte`
- Auth/meeting views → `views/{SignIn,SignUp,CreateMeeting}.svelte`
- E2E event bus (whitelist projection of SDK events) → `lib/e2eBus.ts`
- Session wiring, config, error text → `lib/{session,config,errorText}.ts`
- Vite proxy + E2E-hook / cert-fingerprint plumbing → `packages/web-app/vite.config.ts`, `packages/web-app/vite/fingerprints.ts`

## Client Telemetry (`packages/sdk-core/src/telemetry/`)
- OTel metrics sink / noop → `OtelMetricsSink.ts`, `NoopMetricsSink.ts`
- Trace propagation → `tracePropagation.ts`
- Close-reason classification → `closeReason.ts`
- Structured logger + name redaction guard → `logger.ts`, `nameGuard.ts`

## Browser E2E Harness (`packages/web-app/e2e/`)
- Shared fixtures/helpers (auth, roster asserts, recovery, shared-user memo) → `fixtures.ts`
- MC metric assertions (baseline-delta, `pollUntilSumAbove`) → `mcMetrics.ts`
- Specs (happy-path, negative, recovery tails) → `*.spec.ts`
- Env contract (required `E2E_ORG_SUBDOMAIN`, derived `E2E_BASE_URL`) → `env.ts`, `global-setup.ts`
- Budgets & assertion catalog → `packages/web-app/e2e/README.md`
- Playwright config (retries=0, workers=1) → `packages/web-app/playwright.config.ts`
- Browser E2E pipeline lane (Layer 7) → `scripts/layer7.sh`
- Per-run org provisioning (source of `E2E_ORG_SUBDOMAIN`) → `scripts/layer7.sh` Phase 1h, `infra/kind/scripts/setup.sh:provision_run_org()`
- Node-tier vitest specs (env contract, bundle content) → `packages/web-app/tests/`
- Subdomain-pattern drift guard (e2e mirror ↔ SDK SSoT) → `scripts/guards/simple/validate-subdomain-regex-sync.sh`

## Client Test Utilities (`packages/test-utils/src/`)
- Mock transport → `MockWebTransport.ts`
- Test token builder → `TestTokenBuilder.ts`, `token-claims.ts`
- Metrics sink contract + in-memory / OTLP mocks → `contracts/MetricsSink.ts`, `InMemoryMetricsSink.ts`, `MockOTLPExporter.ts`
- Deterministic id generator → `deterministic-ids.ts`

## Protocol Integration
- Signaling proto (client-server) → `proto/dark_tower/signaling/v1/signaling.proto`
- 42-byte binary frame format → `crates/media-protocol/src/frame.rs`
- Cross-language test vectors → `proto/test-vectors/`
- Meeting-token `display_name` claim (roster names) → `crates/common/src/jwt.rs:MeetingTokenClaims`

## Observability
- Client alert rules & dashboards → `client-alerts.yaml`, `client-overview.json`, `client-slo.json`, `client-synthetic.json`

## Toolchain & Build
- Node version SSoT (`.nvmrc`; enforced via `.npmrc` engine-strict, `package.json` engines.node, `infra/devloop/Dockerfile`) → `.nvmrc`
- pnpm transitive-security overrides → `package.json:pnpm.overrides`
- Dev-web preflight (rolldown-binding probe) → `scripts/dev-web.sh`
- Client CI workflow → `.github/workflows/ci-client.yml`
