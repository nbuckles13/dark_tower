# Client Navigation

## Architecture & Design
- Client architecture (SDK, testing, deployment, E2EE) → ADR-0028; debate → `docs/debates/2026-02-28-client-architecture/debate.md`
- Media flow (multi-handler §9, slot model §6, hot-path layout §11, KEK lifecycle) → ADR-0036
- Crypto algorithms → ADR-0027; auth/meeting access → ADR-0020; observability → ADR-0011, ADR-0031; guards → ADR-0015; tests → ADR-0005
- Story 2 (hear each other) + manual test plan → `docs/user-stories/2026-09-21-hear-each-other.md`, `docs/user-stories/2026-09-21-hear-each-other-manual-test-plan.md`

## SDK Core: Session & Signaling (`packages/sdk-core/src/`)
- Signaling client / meeting session (single-use per join) → `signaling/SignalingClient.ts`, `session/MeetingSession.ts`
- Session event surface (mute / first-media / slot / fault) → `session/events.ts:MeetingSessionEventMap`; `StreamAssignments` + unreachable senders → `signaling/events.ts:StreamAssignmentsEvent`
- Session↔media wiring (rebind purge, KEK observer, per-kind send directives, video encoding validation, MH control sink → keyframe gate) → `session/mediaWiring.ts:sendDirectiveListener`, `validateVideoEncoding`, `mhControlSink`
- Signalling media state (E3 cache, mute) + R-18 tracker (S1); publish set + slot plan + declaration + capture source (audio + video/test pattern) → `session/{signalingMediaState,serverMuteResume,slotPlan,mediaSelection}.ts`; session-owned transmit keys + KEK subscription + VP8 join probe → `MeetingSession.ts:startMedia`
- Flat slot-state map (exhaustive `Record<SlotState, …>`) + pairing view + single-kind mute builder (other kind absent = unchanged) → `signaling/{slotStateMap,pairing,muteRequest}.ts`
- Host role hint → `session/meetingRole.ts`; base64url → `encoding/base64url.ts`; error codes (closing vs refusal) → `signaling/errorCodeMap.ts:closesConnection()`, `errors/`
- WebTransport length-prefix framing → `framing/{length-prefix,sendFramed}.ts`; connection mgmt, datagrams, uni streams, MH control channel → `transport/`, `media/MediaTransport.ts:sendRecoveryRequest`
- HTTP/3 meeting API; generated protobuf-es types (gitignored); wire→public codec oracle; validation SSoT → `http/`, `proto/dark_tower/`, `signaling/codecMap.ts`, `validation/limits.ts`

## SDK Core: Config
- Client config SSoT → `config/clientConfig.ts` (`parseReceiveSlots()`, `deriveKekRetention()`, video knobs + `videoConfigLogLine()`, video receive bounds `videoReceive` + `videoReceiveConfigLogLine()`, keyframe floor `KEYFRAME_MIN_INTERVAL_FLOOR_MS`); export interval → `telemetry/telemetryConfig.ts:getMetricExportIntervalMs()`; MC↔SDK floor guard → `scripts/guards/simple/validate-keyframe-floor-sync.sh`

## SDK Core: Media Pipeline (`packages/sdk-core/src/media/`)
- Frame codec + signing → `frame/{frameCodec,ed25519,wireConstants}.ts`; header v3 + sendable/supported versions → `frameCodec.ts:HEADER_VERSION_V3`, `__tests__/frameCodec.v3.test.ts`; hot-path layout rule → `__tests__/hotPathLayout.test.ts`
- Egress (seal once, per-target fan-out) + uplink stamp = MC send id → `pipeline/{egress,egressQueue,sendId,frameSeal}.ts`; proof → `pipeline/__tests__/egress.multiTarget.test.ts`
- Video egress (group streams, backlog reset, hop at write) → `pipeline/videoEgress.ts`; publish lifecycle (camera gate + R-19, keyframe gate) → `lifecycle/{VideoPublisher,cameraGate,keyframePolicy}.ts`; nonce property → `pipeline/__tests__/nonceUniqueness.property.test.ts`
- Ingress + slot-edge gate + per-sender receive lanes → `pipeline/{ingress,receiveLanes,playbackSink}.ts`; proof → `lifecycle/__tests__/audioPipeline.multiSender.test.ts`
- Receive verification recorder (test-injected); hop monitor; ONE receive policy by kind + parse-boundary accounting (branch table `__tests__/receivePolicy.branchTable.test.ts`) → `pipeline/{receiveVerification,hopSequenceMonitor,receivePolicy,frameAccounting}.ts`
- Video receive: uni-stream reader (extent via `frameCodec.ts:peekFrameExtent`) + ingress + M slot pipelines (jitter, decoder, group gate, re-base) + render rules → `pipeline/{streamReader,videoIngress,videoSlotPipeline,renderDecision}.ts`; recovery mapping `videoIngress.ts:RECOVERY_REASON_FOR`; reader corpus → `pipeline/__tests__/streamReader.test.ts`
- Video receive orchestrator (sinks, evidence, pairing live senders), recovery queue, unfulfilled series, per-kind receiver-state binding, per-handler read loops → `lifecycle/{VideoReceiver,recoveryRequestQueue,unfulfilledSlots,receiverState,receiveTransports}.ts`
- Pipeline lifecycle (per-transport receive, transmit keys, mute) + interval signals (capture presence, receive-source deficit) → `lifecycle/{AudioPipeline,receiveTransports,transmitKeys,muteState,intervalSignals,receiveSourceDeficit}.ts`
- Capture (mic, tone, camera, test pattern — spec home `web-app/e2e/README.md`) / playback / Opus / VP8 (decoder config + key-frame header dims) → `setup/{capture,testTone,videoCapture,videoCaptureBrowser,testPattern,meetingSenderId,audioPlayback,opus,vp8,vp8Encoder,vp8Decoder}.ts`; VideoSink / VideoDecoder seams → `setup/seams.ts`
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
- Fixtures + MC/MH metric read-backs; multi-party cohort (N+1 registered in global setup) → `fixtures.ts`, `mcMetrics.ts`, `instanceCounters.ts`, `s1Diagnostic.ts`, `cohort.ts`, `cohortContexts.ts`, `global-setup.ts`
- Receive evidence + tone detection + window stop rule → `receiveEvidence.ts`, `toneDetector.ts`, `windowSampling.ts`
- Credential scan, shared client metric names, config.env reader → `{credentialScan,clientMetricNames,configEnv}.ts`
- Specs → `*.spec.ts` (multi-party-hear, partial-connectivity, server-mute, kek-rotation, over-subscription, solo-participant, ...)
- Env contract + catalog → `env.ts`, `README.md`; Playwright config → `packages/web-app/playwright.config.ts`; node-tier specs → `packages/web-app/tests/`
- Layer 7 lane + per-run org provisioning → `scripts/layer7.sh`, `infra/kind/scripts/setup.sh:provision_run_org()`
- Subdomain-pattern drift guard → `scripts/guards/simple/validate-subdomain-regex-sync.sh`

## Client Test Utilities (`packages/test-utils/src/`)
- Mock transport (uni streams, STOP_SENDING/reset, write gates → `mockUniStreams.ts`); audio seam doubles; video decoder + VideoFrame close-registry doubles → `media/video.ts`; tokens, metrics sink contract + mocks, ids → `MockWebTransport.ts`, `media/index.ts`, `TestTokenBuilder.ts`, `token-claims.ts`, `contracts/MetricsSink.ts`, `InMemoryMetricsSink.ts`, `MockOTLPExporter.ts`, `deterministic-ids.ts`

## Protocol Integration
- Signaling proto (incl. per-kind `max_{audio,video}_receive_slots`, `kek_rotation_debounce_seconds`) → `proto/dark_tower/signaling/v1/signaling.proto`
- Media frame format → `crates/media-protocol/`; vectors → `proto/test-vectors/`; meeting-token claims → `crates/common/src/jwt.rs:MeetingTokenClaims`

## Observability
- Metrics catalog + grandfathered label roster → `docs/observability/metrics/client.md`; label rules → `docs/observability/label-taxonomy.md`; alerts / dashboard → `infra/docker/prometheus/rules/client-alerts.yaml`, `infra/grafana/dashboards/client-media.json`

## Toolchain & Build
- Node version SSoT → `.nvmrc`; pnpm overrides and settings → `pnpm-workspace.yaml`; dev-web preflight → `scripts/dev-web.sh`
- Secure-context runbook → `docs/runbooks/client-dev-local.md`; client CI → `.github/workflows/ci-client.yml`
