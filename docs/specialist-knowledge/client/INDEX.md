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
- Signaling client / meeting session (single-use per join) → `signaling/SignalingClient.ts`, `session/MeetingSession.ts`
- WebTransport length-prefix framing (MC wire contract) → `framing/{length-prefix,sendFramed}.ts`
- Client config SSoT (Opus, rotation T, queue bounds, metric cadence) → `config/clientConfig.ts`
- Frame codec + SFrame crypto → `media/frame/`; audio pipeline → `media/{pipeline,lifecycle,setup,teardown}/`
- Media layout rule (hot path vs siblings, ADR-0036 §11) → `media/__tests__/hotPathLayout.test.ts`
- Media metric handles + allow-list labels (only `dt_client_` names under `media/**`) → `media/setup/mediaMetrics.ts`
- Key material seams (meeting KEK, roster keys, identity, transmit keys) → `media/setup/{kekSource,rosterKeys,identity}.ts`, `media/lifecycle/transmitKeys.ts`
- TS-side KEK sink control → `signaling/kekIntake.ts`, `__tests__/serverMessageSinkScan.test.ts`
- WebTransport connection mgmt + datagram I/O → `transport/`, `media/MediaTransport.ts`
- HTTP/3 meeting API client → `http/`
- Error taxonomy (`SdkErrorCode`); HTTP status → meeting error mapping → `errors/`, `errors/MeetingError.ts:fromResponse()`
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
- OTel metrics sinks / trace propagation / close-reason → `{OtelMetricsSink,NoopMetricsSink,tracePropagation,closeReason}.ts`
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
- Mock transport (datagrams, injected drops, back-pressure, queue knobs) → `MockWebTransport.ts`
- Audio seam doubles (capture, codecs, playback) — never frame crypto → `media/index.ts`
- Test token builder → `TestTokenBuilder.ts`, `token-claims.ts`
- Metrics sink contract + mocks; deterministic ids → `contracts/MetricsSink.ts`, `InMemoryMetricsSink.ts`, `MockOTLPExporter.ts`, `deterministic-ids.ts`

## Protocol Integration
- Signaling proto (client-server) → `proto/dark_tower/signaling/v1/signaling.proto`
- Media frame binary format (publisher/relay header split, per-frame Ed25519-signed) → `crates/media-protocol/`
- Cross-language test vectors → `proto/test-vectors/`
- Meeting-token `display_name` claim (roster names) → `crates/common/src/jwt.rs:MeetingTokenClaims`

## Observability
- Metrics catalog + frozen grandfathered label roster → `docs/observability/metrics/client.md`
- Media-path label rules (R1/R2/R3) → `docs/observability/label-taxonomy.md`
- Client alert rules & dashboards → `client-alerts.yaml`, `client-overview.json`, `client-slo.json`, `client-synthetic.json`

## Toolchain & Build
- Node version SSoT (`.nvmrc`; enforced via `.npmrc` engine-strict, `package.json` engines.node, `infra/devloop/Dockerfile`) → `.nvmrc`
- pnpm transitive-security overrides → `package.json:pnpm.overrides`
- Dev-web preflight (rolldown-binding probe) → `scripts/dev-web.sh`
- Client CI workflow → `.github/workflows/ci-client.yml`
