# Client Navigation

## Architecture & Design
- Client architecture (SDK, testing, deployment, E2EE) → ADR-0028; debate → `docs/debates/2026-02-28-client-architecture/debate.md`
- Media flow (multi-handler §9, slot model §6, hot-path layout §11, KEK lifecycle) → ADR-0036
- Crypto algorithms → ADR-0027; auth/meeting access → ADR-0020; observability → ADR-0011, ADR-0031; guards → ADR-0015; tests → ADR-0005
- Story 2 (hear each other) + manual test plan → `docs/user-stories/2026-09-21-hear-each-other.md`, `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md`

## SDK Core: Session & Signaling (`packages/sdk-core/src/`)
- Signaling client / meeting session (single-use per join) → `signaling/SignalingClient.ts`, `session/MeetingSession.ts`
- Session event surface (mute / first-media / slot / fault) → `session/events.ts:MeetingSessionEventMap`; `StreamAssignments` + unreachable senders → `signaling/events.ts:StreamAssignmentsEvent`
- Session↔media wiring (rebind purge, KEK observer, send directives) → `session/mediaWiring.ts:sendDirectiveListener`
- Capture-source + receive-slot selection → `session/mediaSelection.ts:selectCaptureSource`
- Host role hint → `session/meetingRole.ts`; base64url (one decoder) → `encoding/base64url.ts`
- Error codes, connection-closing vs request-refusal → `signaling/errorCodeMap.ts:closesConnection()`; SDK error taxonomy → `errors/`
- WebTransport length-prefix framing → `framing/{length-prefix,sendFramed}.ts`; connection mgmt + datagrams → `transport/`, `media/MediaTransport.ts`
- HTTP/3 meeting API; generated protobuf-es types (gitignored); wire→public codec oracle; validation SSoT → `http/`, `proto/dark_tower/`, `signaling/codecMap.ts`, `validation/limits.ts`

## SDK Core: Config
- Client config SSoT → `config/clientConfig.ts` (`parseReceiveSlots()`, `deriveKekRetention()`); export interval → `telemetry/telemetryConfig.ts:getMetricExportIntervalMs()`

## SDK Core: Media Pipeline (`packages/sdk-core/src/media/`)
- Frame codec + signing → `frame/{frameCodec,ed25519,wireConstants}.ts`; hot-path layout rule → `__tests__/hotPathLayout.test.ts`
- Egress (seal once, per-target fan-out) → `pipeline/{egress,egressQueue}.ts`; proof → `pipeline/__tests__/egress.multiTarget.test.ts`
- Ingress + slot-edge gate + per-sender receive lanes → `pipeline/{ingress,receiveLanes,playbackSink}.ts`; proof → `lifecycle/__tests__/audioPipeline.multiSender.test.ts`
- Receive verification recorder (test-injected); hop monitor → `pipeline/{receiveVerification,hopSequenceMonitor}.ts`
- Pipeline lifecycle (per-transport receive, transmit keys, mute) → `lifecycle/{AudioPipeline,receiveTransports,transmitKeys,muteState}.ts`
- Interval signals (capture presence, receive-source deficit) → `lifecycle/{intervalSignals,receiveSourceDeficit}.ts`
- Capture (mic + test tone) / playback / Opus → `setup/{capture,testTone,audioPlayback,opus}.ts`
- Media metric handles + allow-listed labels → `setup/mediaMetrics.ts`

## SDK Core: Key Material
- KEK holder (current + one previous) → `media/setup/kekSource.ts`; KEK intake/scrub → `signaling/kekIntake.ts`, `__tests__/serverMessageSinkScan.test.ts`
- Roster keys / identity / transmit keys → `media/setup/{rosterKeys,identity}.ts`, `signaling/rosterKeyFeed.ts`, `media/lifecycle/transmitKeys.ts`
- Join-label roster guards → `__tests__/{joinLabelSpread,joinLabelRoster,stripComments}.ts`

## Svelte Adapter (`packages/sdk-svelte/src/`)
- Roster store → `stores/MeetingStore.svelte.ts`; media/mute/slot/fault store → `stores/MediaStore.svelte.ts`
- Session-to-store binding → `stores/bindMeetingSession.ts`; re-render granularity proof → `__tests__/MediaStore.test.ts`

## Web Application (`packages/web-app/src/`)
- In-meeting view + slot grid → `views/InMeeting.svelte`; roster row + copy → `views/ParticipantRow.svelte`, `lib/participantState.ts`; roster DOM → `views/JoinMeeting.svelte`
- Slot-state user text → `lib/slotState.ts`; auth/meeting views → `views/{SignIn,SignUp,CreateMeeting}.svelte`
- Session wiring, config, error text → `lib/{session,config,errorText}.ts`
- Test-only build defines (one predicate) → `vite/testDefines.ts`; test levers → `lib/testLevers.ts`
- E2E bus + per-lane analysis (under `__E2E_HOOKS__`) → `lib/{e2eBus,e2eAnalysis}.ts`
- Vite proxy / cert fingerprints → `vite.config.ts`, `vite/fingerprints.ts`; prod-absence bundle scan → `tests/bundle-content.test.ts`

## Client Telemetry (`packages/sdk-core/src/telemetry/`)
- OTel sinks / trace propagation / close-reason / logger / name guard → `{OtelMetricsSink,NoopMetricsSink,tracePropagation,closeReason,logger,nameGuard}.ts`

## Browser E2E Harness (`packages/web-app/e2e/`)
- Fixtures + MC/MH metric read-backs → `fixtures.ts`, `mcMetrics.ts`, `instanceCounters.ts`, `s1Diagnostic.ts`
- Multi-party cohort (N+1 registered in global setup) → `cohort.ts`, `cohortContexts.ts`, `global-setup.ts`
- Receive evidence + tone detection + window stop rule → `receiveEvidence.ts`, `toneDetector.ts`, `windowSampling.ts`
- Credential scan, shared client metric names, config.env reader → `{credentialScan,clientMetricNames,configEnv}.ts`
- Specs → `*.spec.ts` (multi-party-hear, partial-connectivity, server-mute, kek-rotation, over-subscription, solo-participant, ...)
- Env contract + catalog → `env.ts`, `README.md`; Playwright config → `packages/web-app/playwright.config.ts`; node-tier specs → `packages/web-app/tests/`
- Layer 7 lane + per-run org provisioning → `scripts/layer7.sh`, `infra/kind/scripts/setup.sh:provision_run_org()`
- Subdomain-pattern drift guard → `scripts/guards/simple/validate-subdomain-regex-sync.sh`

## Client Test Utilities (`packages/test-utils/src/`)
- Mock transport; audio seam doubles; tokens, metrics sink contract + mocks, ids → `MockWebTransport.ts`, `media/index.ts`, `TestTokenBuilder.ts`, `token-claims.ts`, `contracts/MetricsSink.ts`, `InMemoryMetricsSink.ts`, `MockOTLPExporter.ts`, `deterministic-ids.ts`

## Protocol Integration
- Signaling proto (incl. `max_receive_slots`, `kek_rotation_debounce_seconds`) → `proto/dark_tower/signaling/v1/signaling.proto`
- Media frame format → `crates/media-protocol/`; vectors → `proto/test-vectors/`; meeting-token claims → `crates/common/src/jwt.rs:MeetingTokenClaims`

## Observability
- Metrics catalog + grandfathered label roster → `docs/observability/metrics/client.md`; label rules → `docs/observability/label-taxonomy.md`
- Alerts / dashboard → `infra/docker/prometheus/rules/client-alerts.yaml`, `infra/grafana/dashboards/client-media.json`

## Toolchain & Build
- Node version SSoT → `.nvmrc`; pnpm overrides and settings → `pnpm-workspace.yaml`; dev-web preflight → `scripts/dev-web.sh`
- Secure-context runbook → `docs/runbooks/client-dev-local.md`; client CI → `.github/workflows/ci-client.yml`
