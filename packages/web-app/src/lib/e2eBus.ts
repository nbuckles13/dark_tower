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

import type {
  MediaFrameCounts,
  MeetingSessionEventMap,
  RosterParticipant,
  SdkError,
} from '@darktower/sdk-core';

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
 * path. `MeetingSession` satisfies this structurally, so nothing was added to
 * the SDK for the bus's benefit.
 */
export interface SessionMediaCounters {
  readonly media: { readonly frameCounts: MediaFrameCounts } | undefined;
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
export function installE2EHooks(session: E2ESessionHandle): () => void {
  if (__E2E_HOOKS__) {
    const roster = (p: RosterParticipant): Record<string, string> => ({
      participantId: p.participantId,
      name: p.name,
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
      bus.emit({
        type: 'participantJoined',
        participantId: event.participant.participantId,
        name: event.participant.name,
      }),
    );
    session.on('participantLeft', (event) =>
      bus.emit({
        type: 'participantLeft',
        participantId: event.participantId,
        reason: event.reason,
      }),
    );
    session.on('mediaConnected', (url) => bus.emit({ type: 'mediaConnected', mhUrl: url }));
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
    const sampler = setInterval(() => {
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
