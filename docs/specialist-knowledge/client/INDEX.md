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
- Frame codec + SFrame crypto → `media/frame/`; audio pipeline → `media/{pipeline,lifecycle,setup,teardown}/`; layout rule (hot path vs siblings, ADR-0036 §11) → `media/__tests__/hotPathLayout.test.ts`
- Multi-handler (ADR-0036 §9): send seals ONCE then fans out to per-target lanes (own queue/`hop_sequence`/buffer; `TransmitKeyManager` identity is the nonce boundary); `framesSent` counts DATAGRAMS; `not_connected` = directed target with no transport; receive = one read loop + hop monitor PER TRANSPORT → `media/pipeline/{egress,ingress}.ts`, `media/lifecycle/{AudioPipeline,transmitKeys,receiveTransports}.ts`, `media/pipeline/__tests__/egress.multiTarget.test.ts`
- N-sender receive (story 2 R-5): slot-edge gate = verify -> gate -> open, authority is MC's `StreamAssignments` (declared slots only; never observed traffic), drop `sender_not_assigned`; one decoder + playback LANE per assigned sender (lanes MIX; one shared playhead would serialise N senders); fault replaces one sender's decoder, rate-bounded; pending-queue eviction is POST-ACCEPT (`decode_queue_dropped_total`, never a drop reason) → `media/pipeline/{receiveLanes,ingress,playbackSink}.ts`, `media/lifecycle/__tests__/audioPipeline.multiSender.test.ts`
- Media metric handles + allow-list labels (only `dt_client_` names under `media/**`) → `media/setup/mediaMetrics.ts`; join-bag spread only by the client.md grandfathered roster (behavioural + static) → `session/__tests__/meeting-session.test.ts` ("join label bag"), `__tests__/joinLabelSpread.test.ts`, `__tests__/joinLabelRoster.ts`; one comment stripper for source scans → `__tests__/stripComments.ts`
- Story 2 N + test tone: N is a REQUEST bounded by MC's advertised `JoinResponse.max_receive_slots` (known/unknown/invalid), refused LOCALLY above it, never shrunk; strict parser; ceiling `maxDecodeLanes`. Tone: opt-in build define `__DT_TEST_TONE__` (THROW in prod, one predicate `packages/web-app/vite/testTone.ts`) read at ONE site; frequency from sender id only; mode is a TYPE so prod cannot spell `test_tone` → `config/clientConfig.ts:parseReceiveSlots`, `session/mediaSelection.ts`, `media/setup/{testTone,capture}.ts`; send-directive listener (empty = withdraw, forward secrecy) → `session/mediaWiring.ts:sendDirectiveListener`
- Per-export-interval signals (capture-source presence gauge + receive-source deficit, pure decision with grace rule; interval SSoT `telemetry/telemetryConfig.ts:getMetricExportIntervalMs`) → `media/lifecycle/{intervalSignals,receiveSourceDeficit}.ts`; R-30 test-only INJECTED bounded recorder (layer 1 observed stream_id + key-id sender, layer 2 verified+decrypted, drops) → `media/pipeline/{receiveVerification,ingress}.ts`
- Sampled frame counters (one increment beside each metric call) + live mic swap (`setCaptureDevice`) → `media/pipeline/{egress,ingress}.ts`, `media/lifecycle/AudioPipeline.ts`
- The four media absence-signals on the session facade (mute / first-media / slot state / fault) → `session/events.ts:MeetingSessionEventMap`; `unreachable_sender_ids` (§9) carried to `MediaStore`, replace-never-merge, rendering owed by story-2 task 15 → `signaling/events.ts:StreamAssignmentsEvent`
- KEK holder (R-14): current + at most ONE previous, retained `min(W/2, ceiling)` from the W on the demoting message (rule's home = `JoinResponse.kek_rotation_debounce_seconds`; the ONE computation = `config/clientConfig.ts:deriveKekRetention`); send side reaches CURRENT only; `RetentionGuard` count is FUSED with its check (split them and the live-path test goes vacuous); stale vs newer split → `kek_generation_stale` / `no_kek_for_generation` → `media/setup/kekSource.ts`, `media/setup/__tests__/kekSource.test.ts`; TS-side KEK sink control (scrub at decode) → `signaling/kekIntake.ts`, `__tests__/serverMessageSinkScan.test.ts`
- Key material seams (KEK, roster keys, identity, transmit keys); **own key self-seeded at join — MC's roster excludes self**; R-13 sender rotation on KEK receipt (install first, THEN rotate; never on equal gen); R-18 rebind purge (transmit keys only, NEVER replay state; decision synchronous, import sequence-guarded; leave/eviction also purge) → `media/setup/{kekSource,rosterKeys,identity}.ts`, `signaling/rosterKeyFeed.ts`, `session/{MeetingSession,mediaWiring}.ts`
- WebTransport connection mgmt + datagram I/O → `transport/`, `media/MediaTransport.ts`
- HTTP/3 meeting API client; protobuf-es generated types; wire-`Codec`→public `SignalingCodec` oracle (one table, both directions; generated enums never reach `.d.ts`) → `http/`, `proto/dark_tower/`, `signaling/codecMap.ts`
- Error taxonomy (`SdkErrorCode`, HTTP→meeting mapping); input validation SSoT (`SUBDOMAIN_REGEX`, caps) → `errors/`, `validation/limits.ts`

## Svelte Adapter (`packages/sdk-svelte/src/`)
- Roster/participant store (seeded from `existingParticipants`, excludes self) → `stores/MeetingStore.svelte.ts`
- Media/mute/slot/fault store (one `$state` cell per field; hung off `MeetingStore.media` as a NON-reactive field so mute does not invalidate the roster) → `stores/MediaStore.svelte.ts`
- Session-to-store binding (one subscription, one aggregate unsubscribe, roster + media alike) → `stores/bindMeetingSession.ts`
- Re-render granularity proof (counts `$derived` recomputations, with positive control); unreachable-set replace-not-merge (no-latch) → `__tests__/MediaStore.test.ts`

## Web Application (`packages/web-app/src/`)
- Roster DOM (`participant-list` / `participant-${id}` testids) → `views/JoinMeeting.svelte`
- In-meeting view: mute control + indicator, mic picker, slot rows (`in-meeting` / `mute-toggle` / `mute-state` / `mic-select` / `slot-${id}` testids) → `views/InMeeting.svelte`
- §6 slot-state → user-facing text SSoT (exhaustive by type; `awaiting-assignment` is NOT a wire token) → `lib/slotState.ts`
- Auth/meeting views → `views/{SignIn,SignUp,CreateMeeting}.svelte`
- E2E event bus (whitelist projection of SDK events; story 2: `receiveSlots`, `slotAssignments`, `captureSource`, `receiveLayers`, `receiveAnalysis`) → `lib/e2eBus.ts`; layer-3 per-lane AnalyserNode + recorder, all under `__E2E_HOOKS__` → `lib/e2eAnalysis.ts`
- Session wiring, config, error text → `lib/{session,config,errorText}.ts`
- Vite proxy + E2E-hook / cert-fingerprint / test-tone plumbing → `packages/web-app/vite.config.ts`, `packages/web-app/vite/{fingerprints,testTone}.ts`; prod-absence + positive-control bundle scan → `packages/web-app/tests/bundle-content.test.ts`

## Client Telemetry (`packages/sdk-core/src/telemetry/`)
- OTel sinks / trace propagation / close-reason; structured logger + name redaction guard → `{OtelMetricsSink,NoopMetricsSink,tracePropagation,closeReason,logger,nameGuard}.ts`

## Browser E2E Harness (`packages/web-app/e2e/`)
- Fixtures (auth, roster asserts, recovery, shared-user memo, media loopback helpers) + MC metric assertions → `fixtures.ts`, `mcMetrics.ts`
- Specs: happy-path, negative, recovery tails; media loopback (first-media pass/fail, latency OBSERVED-never-gated, structural mute) → `*.spec.ts`
- Env contract (required `E2E_ORG_SUBDOMAIN`, derived `E2E_BASE_URL`); budgets & assertion catalog → `env.ts`, `global-setup.ts`, `README.md`
- Playwright config (retries=0, workers=1) → `packages/web-app/playwright.config.ts`
- Layer 7 lane + per-run org provisioning (source of `E2E_ORG_SUBDOMAIN`) → `scripts/layer7.sh` (Phase 1h), `infra/kind/scripts/setup.sh:provision_run_org()`
- Node-tier vitest specs (env contract, bundle content) → `packages/web-app/tests/`
- Subdomain-pattern drift guard (e2e mirror ↔ SDK SSoT) → `scripts/guards/simple/validate-subdomain-regex-sync.sh`

## Client Test Utilities (`packages/test-utils/src/`)
- Mock transport (datagrams, injected drops, back-pressure, queue knobs); audio seam doubles (capture, codecs, playback) — never frame crypto → `MockWebTransport.ts`, `media/index.ts`
- Test tokens; metrics sink contract + mocks; deterministic ids → `TestTokenBuilder.ts`, `token-claims.ts`, `contracts/MetricsSink.ts`, `InMemoryMetricsSink.ts`, `MockOTLPExporter.ts`, `deterministic-ids.ts`

## Protocol Integration
- Signaling proto (client-server) → `proto/dark_tower/signaling/v1/signaling.proto`
- Media frame binary format (publisher/relay header split, per-frame Ed25519-signed) → `crates/media-protocol/`; cross-language test vectors → `proto/test-vectors/`
- Meeting-token `display_name` claim (roster names) → `crates/common/src/jwt.rs:MeetingTokenClaims`

## Observability
- Metrics catalog + frozen grandfathered label roster → `docs/observability/metrics/client.md`
- Media-path label rules (R1/R2/R3; `mode` row) → `docs/observability/label-taxonomy.md`; client alert rules & dashboards → `client-alerts.yaml`, `client-overview.json`, `client-slo.json`, `client-synthetic.json`, `client-media.json`

## Toolchain & Build
- Node version SSoT (`.nvmrc`; enforced via `.npmrc` engine-strict, `package.json` engines.node, `infra/devloop/Dockerfile`) → `.nvmrc`
- pnpm transitive-security overrides → `package.json:pnpm.overrides`
- Dev-web preflight (rolldown-binding probe) → `scripts/dev-web.sh`
- Secure-context facts (frozen anchor `#secure-context-and-media-setup`; prose pinned from `scripts/dev-web.test.sh`) → `docs/runbooks/client-dev-local.md`
- Client CI workflow → `.github/workflows/ci-client.yml`
