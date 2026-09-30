// File: packages/web-app/src/lib/e2eBus.ts
//
// R-29: the `window.__darktower_test__` E2E event bus — the stable contract the
// test-owned Playwright specs (#18/#19) consume. It is a replay-buffered bus of
// BOUNDED, non-PII events projected from the SDK's session events.
//
// SECURITY (R-23 / @security / @semantic-guard):
//   - The ENTIRE active body is behind `if (__E2E_HOOKS__)` — a SEPARATE define
//     from __DEV_TRUST_FINGERPRINT__ that is `false` in prod, so the whole bus
//     (window attach + replay buffer) is dead-code-eliminated. A prod
//     bundle-content test asserts `__darktower_test__` is absent from the bundle.
//   - Every payload is an explicit WHITELIST projection — never a raw event or
//     Error. `joined` DROPS `bindingToken` (reconnection credential) and
//     `correlationId`; `senderId` is stringified, and is `undefined` until MC
//     assigns one (ADR-0036 §2 — absence is never coerced to 0, since 0 is
//     reserved-invalid). Errors cross via `SdkError.toJSON()`
//     (the R-23 non-secret allowlist), never a raw Error / `.cause` / `.stack`.

import { flushMetrics, getMetricsSink } from '@darktower/sdk-core';
import type {
  CaptureSourceInfo,
  MediaFrameCounts,
  MeetingSessionEventMap,
  ReceiveSlotsDiagnostics,
  RosterParticipant,
  SdkError,
} from '@darktower/sdk-core';
import type { E2EInstrumentation } from './e2eAnalysis.js';

/** The minimal surface installE2EHooks needs: subscribe to typed session events. */
export interface SessionEvents {
  on<K extends keyof MeetingSessionEventMap>(
    type: K,
    listener: (payload: MeetingSessionEventMap[K]) => void,
  ): () => void;
}

/**
 * What the bus reads to SAMPLE the frame counters.
 *
 * A getter, deliberately, not a subscription. `pipeline/egress.ts` is the hot
 * path under ADR-0036 §11, whose per-frame invariant is zero allocation and zero
 * registry lookup — a `bus.emit()` per frame is exactly the shape that rule
 * exists to stop, and it would put a test-only channel on the production forward
 * path. `MeetingSession` satisfies this structurally. The per-(slot, sender)
 * receive counters (story 2 R-30) keep the same rule from the other side: an
 * INJECTED recorder (`e2eAnalysis.ts`) the ingress calls only when present, and
 * which only a test build injects.
 */
export interface SessionMediaCounters {
  readonly media: { readonly frameCounts: MediaFrameCounts } | undefined;
  /** Effective N, its source and the server cap (story 2 R-23). Plain values. */
  readonly receiveSlots: ReceiveSlotsDiagnostics;
  /** What feeds the send path, once media has started (story 2 R-7). */
  readonly captureSource: CaptureSourceInfo | undefined;
  /** The current meeting-KEK generation (public: it is in every key id). Story 2 S6. */
  readonly currentKekGeneration: number | undefined;
}

/** What the bus needs to know about the app's build and config. Plain booleans. */
export interface E2EHookOptions {
  /** Whether SDK telemetry was configured (`VITE_TELEMETRY_ENDPOINT`). */
  readonly telemetryConfigured: boolean;
}

/** The surface {@link installE2EHooks} consumes. */
export type E2ESessionHandle = SessionEvents & SessionMediaCounters;

/**
 * How often the bus samples the frame counters, in ms.
 *
 * Fast enough that a ~2s muted window yields several samples (a flatness
 * assertion over ZERO samples would be vacuous, and the browser suite asserts
 * the sample count separately for exactly that reason), slow enough that the
 * replay buffer stays small over a whole run. Test builds only.
 */
export const E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS = 250;

type BusEvent = Readonly<Record<string, unknown>> & { readonly type: string };

interface InternalBus {
  events: BusEvent[];
  /**
   * Force a metric export NOW (the SDK's `flushMetrics`), for the browser suite's
   * telemetry credential-scan positive control and the R-27 read-back. REJECTS
   * with `telemetry_not_configured` when telemetry is off — a silent no-op would
   * let "flushed" mean "nothing happened". Returns nothing: no export body, header
   * or response ever reaches the bus.
   */
  flushMetrics(): Promise<void>;
  readonly listeners: Map<string, Set<(event: BusEvent) => void>>;
  on(type: string, listener: (event: BusEvent) => void): () => void;
  emit(event: BusEvent): void;
}

/**
 * Subscribe the `window.__darktower_test__` bus to `session`'s events. No-op (and
 * fully tree-shaken) in production builds. Safe to call once at startup.
 *
 * @returns a disposer that stops the frame-count sampler. Callers must invoke it
 * on unmount; in production it is an already-inert no-op.
 */
export function installE2EHooks(
  session: E2ESessionHandle,
  instrumentation?: E2EInstrumentation,
  options: E2EHookOptions = { telemetryConfigured: false },
): () => void {
  if (__E2E_HOOKS__) {
    const roster = (p: RosterParticipant): Record<string, string> => ({
      participantId: p.participantId,
      name: p.name,
      // Stringified like every sender id on the bus; omitted when MC set none.
      ...(p.senderId !== undefined ? { senderId: p.senderId.toString() } : {}),
    });

    const projectError = (err: SdkError): Record<string, unknown> => {
      // toJSON() is the R-23 redaction boundary — a fixed non-secret allowlist.
      const json = err.toJSON();
      const out: Record<string, unknown> = { code: json.code, message: json.message };
      if (json.status !== undefined) out['status'] = json.status;
      if (json.serverCode !== undefined) out['serverCode'] = json.serverCode;
      return out;
    };

    const existing = window.__darktower_test__ as InternalBus | undefined;
    const bus: InternalBus = existing ?? {
      events: [],
      async flushMetrics() {
        // The SDK's ACTUAL state, not the config's intent: sdk-core's
        // `flushMetrics()` resolves having flushed nothing when no provider is
        // configured, so a config that asked for telemetry but whose
        // configuration was skipped or failed must still refuse here.
        if (!options.telemetryConfigured || getMetricsSink() === undefined) {
          throw new Error('telemetry_not_configured');
        }
        await flushMetrics();
      },
      listeners: new Map<string, Set<(event: BusEvent) => void>>(),
      on(type, listener) {
        let set = this.listeners.get(type);
        if (set === undefined) {
          set = new Set();
          this.listeners.set(type, set);
        }
        set.add(listener);
        return () => {
          this.listeners.get(type)?.delete(listener);
        };
      },
      emit(event) {
        this.events.push(event);
        const set = this.listeners.get(event.type);
        if (set !== undefined) {
          for (const listener of [...set]) listener(event);
        }
      },
    };
    window.__darktower_test__ = bus;

    session.on('stateChange', (state) => bus.emit({ type: 'stateChange', state }));
    session.on('joined', (event) =>
      bus.emit({
        type: 'joined',
        participantId: event.participantId,
        senderId: event.senderId?.toString(),
        participants: event.existingParticipants.map(roster),
        mediaServers: [...event.mediaServers],
        // NOTE: bindingToken + correlationId are intentionally NOT projected (R-23).
      }),
    );
    session.on('participantJoined', (event) =>
      bus.emit({ type: 'participantJoined', ...roster(event.participant) }),
    );
    session.on('participantLeft', (event) =>
      bus.emit({
        type: 'participantLeft',
        participantId: event.participantId,
        reason: event.reason,
      }),
    );
    session.on('mediaConnected', (url) => bus.emit({ type: 'mediaConnected', mhUrl: url }));

    // ------------------------------------------------------------------
    // STORY 2 R-1 / R-7 / R-23 / R-30 — N, the capture source, the expected
    // sender per slot, and the three receive-verification layers
    // ------------------------------------------------------------------
    //
    // Same rules as below: whitelist projections of plain values, sender ids
    // STRINGIFIED (the `joined` convention), never a session or media object on
    // the bus, and nothing here reaches `MetricsSink`.

    // Effective N and the server cap, readable at runtime — the suite's check
    // that the build declares the N it was configured with (browser knobs are
    // outside `dt-guard env-config`). Emitted once the join has told us the cap.
    session.on('joined', () => {
      const slots = session.receiveSlots;
      bus.emit({
        type: 'receiveSlots',
        declared: slots.declared,
        source: slots.source,
        serverCapState: slots.serverCap.state,
        ...(slots.serverCap.state !== 'unknown' ? { serverCap: slots.serverCap.value } : {}),
      });
    });

    // The EXPECTED sender per slot, from MC's latest assignment. Replace, never
    // merge — each event is the complete current view.
    session.on('streamAssignments', (event) =>
      bus.emit({
        type: 'slotAssignments',
        assignments: event.assignments.map((a) => ({
          slotId: a.slotId,
          ...(a.senderId !== undefined ? { senderId: a.senderId.toString() } : {}),
          slotState: a.slotState,
        })),
        // Story 2 R-33: the per-subscriber unreachable set, from the same message.
        unreachableSenderIds: event.unreachableSenderIds.map((id) => id.toString()),
      }),
    );

    // Story 2 R-10/R-11: SERVER mute from the wire, and the host's relay of an
    // unmute request. Participant ids only (already bounded by the SDK).
    session.on('participantMuteChanged', (event) =>
      bus.emit({
        type: 'participantMute',
        participantId: event.participantId,
        audioServerMuted: event.audioServerMuted,
        ...(event.serverMutedBy !== undefined ? { serverMutedBy: event.serverMutedBy } : {}),
      }),
    );
    session.on('unmuteRequested', (event) =>
      bus.emit({ type: 'unmuteRequested', participantId: event.participantId }),
    );

    // The build knobs the suite asserts before any media assertion, so a dev
    // server reused without them fails as "reused server", not as "C never heard
    // A". Plain booleans; the same values the build baked in.
    bus.emit({
      type: 'buildKnobs',
      testLevers: __DT_TEST_LEVERS__,
      // Config intent AND the SDK's actual state, side by side: an endpoint that
      // was configured but never reached the SDK reads as the second being false.
      telemetryConfigured: options.telemetryConfigured,
      telemetrySinkActive: getMetricsSink() !== undefined,
    });
    session.on('error', (err) => bus.emit({ type: 'error', ...projectError(err) }));

    // ------------------------------------------------------------------
    // MEDIA (ADR-0036 §5/§10) — scalars only, and nothing else
    // ------------------------------------------------------------------
    //
    // None of these is a new signal: each is a projection of state that already
    // has a production home — `dt_client_time_to_first_media_frame_ms`,
    // `dt_client_media_mute_transitions_total{action}`, and
    // `dt_client_media_frames_sent_total` / `..._accepted_total`. The bus is a
    // TEST channel; no production-relevant signal may exist only here.
    //
    // What is deliberately NOT projected, and must never be: key ids, salts,
    // wrapped keys, frame bytes, decrypted plaintext, decoded samples, per-frame
    // SIZES (ADR-0036 §11 — the time-ordered sequence of sizes for one stream IS
    // the voice-activity trace), microphone labels, `deviceId`s, participant
    // names, the meeting code, and any token. `mediaFault` is also absent: it is
    // a DOM-rendered operator signal, not a test observable.
    //
    // THE ONE ANALYSED-AUDIO EXCEPTION (story 2 R-30, @security): `receiveAnalysis`
    // carries a per-lane, band-limited (`E2E_ANALYSIS_BAND_*_HZ`, at most
    // `E2E_ANALYSIS_MAX_BINS` bins — `e2eAnalysis.ts`) dB MAGNITUDE
    // spectrum plus a level, sampled on this tick — no phase, no time-domain
    // samples, no PCM. With a real microphone that is a coarse view of speech
    // content; its only readers are scripts already in the page, which could tap
    // the played-back AudioContext directly, so it adds no capability. It never
    // leaves the bus.

    // The latency §10 says is OBSERVED, NEVER GATED. This event carries the
    // number; nothing in the suite compares it against a threshold, and a
    // threshold appearing later would be the defect §10 describes, not a
    // tightening.
    session.on('firstMediaFrame', (elapsedMs) => bus.emit({ type: 'firstMediaFrame', elapsedMs }));
    // The SDK's client-mute state, echoed. The same value the UI indicator
    // renders, so a test can assert the DOM and the SDK agree rather than
    // assuming it.
    session.on('muteChanged', (snapshot) =>
      bus.emit({ type: 'muteState', audioMuted: snapshot.audioMuted }),
    );

    // SAMPLED counters, never per-frame emission. The structural mute assertion
    // ("egress stays flat while muted") needs a handful of reads across a
    // window, not a stream — and a stream would cost a bus event per frame at
    // 50 frames a second on the production hot path.
    // The capture source is announced ONCE, when media starts: the harness
    // ASSERTS tone mode from it rather than assuming the dev server was started
    // with `DT_TEST_TONE=1` (Playwright may reuse an existing server).
    let captureAnnounced = false;
    // Story 2 S6: the KEK generation, emitted on CHANGE (not every tick) — the
    // per-participant evidence that this client installed a rotation.
    let lastKekGeneration: number | undefined;
    const sampler = setInterval(() => {
      const generation = session.currentKekGeneration;
      if (generation !== undefined && generation !== lastKekGeneration) {
        lastKekGeneration = generation;
        bus.emit({ type: 'kekGeneration', generation, atMs: Date.now() });
      }
      const source = session.captureSource;
      if (!captureAnnounced && source !== undefined) {
        captureAnnounced = true;
        bus.emit({
          type: 'captureSource',
          mode: source.mode,
          ...(source.toneHz !== undefined ? { toneHz: source.toneHz } : {}),
        });
      }
      // Receive layers 1/2 (+ drops) and layer 3, SAMPLED on the same tick.
      // Cumulative snapshots, never per frame.
      if (instrumentation !== undefined && session.media !== undefined) {
        const verification = instrumentation.verification();
        bus.emit({
          type: 'receiveLayers',
          layers: verification.layers.map((l) => ({
            slot: l.slot,
            senderId: l.senderId.toString(),
            keyed: l.keyed,
            verified: l.verified,
          })),
          drops: verification.drops.map((d) => ({
            slot: d.slot,
            senderId: d.senderId === null ? null : d.senderId.toString(),
            reason: d.reason,
            count: d.count,
          })),
          overflow: verification.overflow,
          atMs: Date.now(),
        });
        bus.emit({
          type: 'receiveAnalysis',
          // Keyed by the lane's sender; the harness joins it to the slot via
          // the latest `slotAssignments`.
          lanes: instrumentation.analysis().map((a) => ({
            senderId: a.senderId.toString(),
            levelDbfs: a.levelDbfs,
            ready: a.ready,
            sampleRateHz: a.sampleRateHz,
            fftSize: a.fftSize,
            smoothingTimeConstant: a.smoothingTimeConstant,
            binHz: a.binHz,
            bandStartHz: a.bandStartHz,
            bandEndHz: a.bandEndHz,
            spectrumDb: [...a.spectrumDb],
          })),
          atMs: Date.now(),
        });
      }
      const counts = session.media?.frameCounts;
      // Only while a pipeline exists. Emitting zeroes before `startMedia()`
      // would put samples inside a window where "flat" means nothing.
      if (counts === undefined) return;
      // THE COMPLETE RECEIVE IDENTITY, not just `accepted`.
      // `received = accepted + sum(drops by reason)`. Projecting `accepted`
      // alone makes "nothing is arriving" and "arriving and being rejected"
      // indistinguishable on the bus — two states with opposite remediations,
      // and the pair that produced a live misdiagnosis on this pipeline's first
      // end-to-end run. `lastDropReason` is bounded by construction (the frozen
      // reject vocabulary) and carries no participant, key or payload data.
      bus.emit({
        type: 'mediaFrameCounts',
        framesSent: counts.framesSent,
        framesReceived: counts.framesReceived,
        framesAccepted: counts.framesAccepted,
        framesDropped: counts.framesDropped,
        // Omitted rather than emitted as `undefined`, so the field's PRESENCE
        // means a drop has happened.
        ...(counts.lastDropReason !== undefined ? { lastDropReason: counts.lastDropReason } : {}),
        atMs: Date.now(),
      });
    }, E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
    return () => clearInterval(sampler);
  }
  return () => {};
}
