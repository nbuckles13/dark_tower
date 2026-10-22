// File: packages/sdk-core/src/session/mediaWiring.ts
//
// The pieces of session-level wiring between the key-material seams and the
// media pipeline, in one module so the session and its integration tests share
// the SAME code. A test that re-implemented this wiring would prove its own copy
// rather than the product's.

import type { KekHolderObserver } from '../media/setup/kekSource.js';
import {
  MediaFaultStage,
  type AudioPipeline,
  type AudioSendDirective,
  type MediaFault,
} from '../media/lifecycle/AudioPipeline.js';
import { isSupportedHeaderVersion } from '../media/frame/frameCodec.js';
import type { PublishKind, SendDirectiveEvent, SendStreamDirective } from '../signaling/events.js';
import type { MhControlSink } from '../media/events.js';
import type { MediaStateListeners } from './signalingMediaState.js';
import {
  MEDIA_KEYFRAME_REQUEST_OUTCOMES,
  MEDIA_KEYFRAME_UNATTRIBUTED_OUTCOMES,
  MEDIA_MH_CONTROL_CHANNEL_ENDINGS,
  MEDIA_ROSTER_KEY_CHANGES,
  type MediaKindMetrics,
  type MediaMetrics,
  type MediaSessionMetrics,
} from '../media/setup/mediaMetrics.js';
import type { TransmitKeyInvalidationListener } from '../media/setup/rosterKeys.js';

/**
 * Route the KEK holder's reports to the media metric handles and the console.
 *
 * The WARN line carries DURATIONS ONLY — `kekSource.ts` builds every message
 * from the received W, the derived retention and T, never from key bytes, a
 * generation or a sender id. It reaches only the browser console; the counters
 * are what an operator can see, which is why every warned condition is also
 * counted.
 */
export function kekObserver(
  metrics: MediaSessionMetrics,
  warn: (message: string) => void = (message) => console.warn('[dt-media-kek]', message),
): KekHolderObserver {
  return {
    kekArrived: (source) => metrics.kekUpdate(source),
    retentionAnomaly: (outcome) => metrics.kekRetentionAnomaly(outcome),
    installRefused: (outcome) => metrics.kekInstallRefused(outcome),
    generationRetained: () => metrics.kekGenerationRetained(),
    retentionViolation: () => metrics.kekRetentionViolation(),
    warn,
  };
}

/** The pipeline operation a roster invalidation drives. */
export interface TransmitKeyPurger {
  purgeTransmitKeys(senderId: number): void;
}

/**
 * Story 2 R-18: a roster change to a sender's key purges ITS cached transmit
 * keys — never its replay state, which no path here can reach.
 *
 * A REBIND or a DOWNGRADE is counted (separately); a FORGET (a leave, or LRU
 * eviction) is memory hygiene and is not. Both getters are read at event time rather than captured, because the
 * roster is wired at signaling setup, before media starts: a rebind before then
 * is still counted, and simply has no transmit-key cache to purge.
 */
export function rosterInvalidationListener(
  metrics: () => MediaSessionMetrics | undefined,
  pipeline: () => TransmitKeyPurger | undefined,
): TransmitKeyInvalidationListener {
  return (senderId, cause) => {
    if (cause === 'rebind') metrics()?.rosterKeyRebind(MEDIA_ROSTER_KEY_CHANGES.Rebind);
    if (cause === 'downgrade') metrics()?.rosterKeyRebind(MEDIA_ROSTER_KEY_CHANGES.Downgrade);
    pipeline()?.purgeTransmitKeys(senderId);
  };
}

/** The pipeline operation a directive drives. */
export type DirectiveTarget = Pick<AudioPipeline, 'setSendDirective'>;

/** The fault text for a published kind missing from a non-empty directive. A fixed literal. */
export const DIRECTIVE_KIND_MISSING_FAULT =
  "the controller's send directive omitted a media kind this client publishes; " +
  'nothing of that kind is sent until a directive lists it again';

/** Construction options for {@link sendDirectiveListener}. */
export interface SendDirectiveWiring {
  /** The frozen publish set. Every published kind must be listed by MC. */
  readonly publish: readonly PublishKind[];
  readonly pipeline: DirectiveTarget;
  /** The configured audio bitrate, applied only when MC directs none. */
  readonly defaultBitrateBps: number;
  readonly metrics: MediaMetrics;
  /** Raise a bounded, SDK-authored media fault (the session's `mediaFault`). */
  readonly fault: (fault: MediaFault) => void;
  /**
   * The R-18 feed: whether `kind` is EFFECTIVELY being sent after validation
   * (`serverMuteResume.ts`). A refused header version, a missing kind and shim
   * (c) all report `false`.
   */
  readonly effectiveTargets: (kind: PublishKind, nonEmpty: boolean) => void;
}

/**
 * The session's `sendDirective` listener: MC's instruction for what this client
 * SENDS, applied PER PUBLISHED KIND (story 3 R-16, wire freeze).
 *
 *   * HEADER VERSION. Not this SDK's (`isSupportedHeaderVersion`) ⇒ the whole
 *     directive is refused and counted ONCE here
 *     (`send_directive_header_version_refused`); the pipeline's own fail-closed
 *     path (send nothing + its fault) runs as before. Never counted twice.
 *   * A PUBLISHED KIND MISSING from a NON-EMPTY `streams` is a protocol fault:
 *     counted per kind, a typed `mediaFault`, and that kind FAILS CLOSED (empty
 *     targets) — never read as a quiet withdrawal, and never left on its stale
 *     target set (forward secrecy; see {@link applyAudio}).
 *   * An EMPTY `streams` list is the pre-MC-A withdrawal spelling — TRANSITIONAL
 *     shim (c), see {@link legacyEmptyStreamsWithdrawal}.
 *   * Client mute never changes the directive (ADR-0036 §5); audio withdrawal
 *     keeps the mic OPEN and sends nothing (no silence, no DTX).
 */
export function sendDirectiveListener(
  wiring: SendDirectiveWiring,
): (directive: SendDirectiveEvent) => void {
  let lastAudio: AudioSendDirective | undefined;
  let missingFaultRaised = false;

  /**
   * Apply audio's instruction, or its WITHDRAWAL (`undefined`).
   *
   * Withdrawal must NOT be ignored, and the reason is FORWARD SECRECY, not tidy
   * bookkeeping. Keeping the STALE instruction would mean the pipeline's target
   * set never reached zero. `AudioPipeline.setSendDirective` rotates the
   * transmit key on the empty -> non-empty EDGE (one of only four `rotate()`
   * call sites, `lifecycle/transmitKeys.ts`); never seeing "empty" made a
   * publisher whose last holder left resume under the SAME transmit key
   * generation the departed holder still has. Applied as an empty target set,
   * which is exactly §5's "send nothing". With no earlier instruction there is
   * nothing to withdraw: egress never started, and no placeholder stream
   * number or bitrate is invented.
   */
  const applyAudio = (stream: SendStreamDirective | undefined, headerVersion: number): void => {
    if (!stream) {
      if (lastAudio && lastAudio.targets.length > 0) {
        lastAudio = { ...lastAudio, targets: [] };
        wiring.pipeline.setSendDirective(lastAudio);
      }
      return;
    }
    lastAudio = {
      streamNumber: stream.streamNumber,
      // MC's directed value when present; the configured DEFAULT otherwise.
      // Never a local ceiling applied on top — see `clientConfig.ts`.
      bitrateBps: stream.maxBitrateBps ?? wiring.defaultBitrateBps,
      targets: stream.targets,
      // MC's meeting-wide version, passed through verbatim. The pipeline refuses
      // one it cannot send; nothing here defaults or coerces it.
      headerVersion,
    };
    wiring.pipeline.setSendDirective(lastAudio);
  };

  const apply = (
    kind: PublishKind,
    stream: SendStreamDirective | undefined,
    headerOk: boolean,
    headerVersion: number,
  ): void => {
    // Video has no send path until story 3 C-A2, and `validatePublishSet`
    // refuses to publish it, so only audio reaches a pipeline.
    if (kind === 'audio') applyAudio(stream, headerVersion);
    wiring.effectiveTargets(kind, headerOk && stream !== undefined && stream.targets.length > 0);
  };

  return (directive) => {
    const headerOk = isSupportedHeaderVersion(directive.headerVersion);
    if (!headerOk) wiring.metrics.session.sendDirectiveHeaderVersionRefused();

    if (directive.streams.length === 0) {
      legacyEmptyStreamsWithdrawal(wiring.publish, (kind) =>
        apply(kind, undefined, headerOk, directive.headerVersion),
      );
      return;
    }

    let anyMissing = false;
    for (const kind of wiring.publish) {
      const stream = directive.streams.find((s) => s.mediaKind === kind);
      if (!stream) {
        anyMissing = true;
        kindMetrics(wiring.metrics, kind).sendDirectiveKindMissing();
      }
      apply(kind, stream, headerOk, directive.headerVersion);
    }
    // Event-once per EPISODE: re-armed by a directive listing every kind.
    if (anyMissing && !missingFaultRaised) {
      missingFaultRaised = true;
      wiring.fault({
        stage: MediaFaultStage.Config,
        message: DIRECTIVE_KIND_MISSING_FAULT,
        fatal: false,
      });
    } else if (!anyMissing) {
      missingFaultRaised = false;
    }
  };
}

/**
 * TRANSITIONAL (C-A1 shim c) — deleted by C-A2 (story 3 task 13).
 *
 * The controller in service before MC-A (task 12) spells "nobody watches you"
 * as a directive with NO streams at all (`build_send_directive` omits unwatched
 * streams; a solo participant gets exactly this). MC-A lists every published
 * kind and withdraws with EMPTY TARGETS instead. Until then an EMPTY `streams`
 * list is read as a withdrawal of EVERY published kind — no fault, no counter —
 * while a NON-empty list missing a published kind stays a counted fault. Either
 * way the kind fails closed (sends nothing).
 */
export function legacyEmptyStreamsWithdrawal(
  publish: readonly PublishKind[],
  withdraw: (kind: PublishKind) => void,
): void {
  for (const kind of publish) withdraw(kind);
}

/**
 * The per-kind handle for a PUBLISHED kind. Exhaustive: a published kind with
 * no handle is an internal invariant violation and THROWS, never a silently
 * uncounted fault. `validatePublishSet` refuses video until story 3 C-A2 (task
 * 13), which adds `MediaMetrics.video` and this arm's handle.
 */
export function kindMetrics(metrics: MediaMetrics, kind: PublishKind): MediaKindMetrics {
  switch (kind) {
    case 'audio':
      return metrics.audio;
    case 'video':
      // C-A2 (task 13): add MediaMetrics.video and return it here.
      throw new Error('internal invariant: video is published but has no metric handle');
    default: {
      const exhaustive: never = kind;
      return exhaustive;
    }
  }
}

/** Construction options for {@link mhControlSink}. */
export interface MhControlWiring {
  readonly metrics: MediaMetrics;
  readonly publish: readonly PublishKind[];
  /** MC's latest send directive (the session's cache), read at request time. */
  readonly latestDirective: () => SendDirectiveEvent | undefined;
}

/**
 * The session's MH control-channel sink (story 3 R-11).
 *
 * A `KeyframeRequest` is UNTRUSTED input. Until story 3 C-A2 adds a video
 * stream there is nothing this client can key, so EVERY request is refused
 * `not_sending` — none is honoured, and none reaches capture or an encoder.
 * Attribution: a request naming the directed `stream_number` of a PUBLISHED
 * kind counts on that kind's handle; any other stream number has no kind and
 * counts kind-free, as does malformed input. Neither the stream number nor the
 * GoP counter reaches a label, log or event.
 */
export function mhControlSink(wiring: MhControlWiring): MhControlSink {
  return {
    keyframeRequest: (_url, request) => {
      const stream = wiring
        .latestDirective()
        ?.streams.find((s) => s.streamNumber === request.streamNumber);
      const kind =
        stream && (stream.mediaKind === 'audio' || stream.mediaKind === 'video')
          ? stream.mediaKind
          : undefined;
      const handle =
        kind !== undefined && wiring.publish.includes(kind)
          ? kindMetrics(wiring.metrics, kind)
          : undefined;
      if (handle) handle.keyframeRequestReceived(MEDIA_KEYFRAME_REQUEST_OUTCOMES.NotSending);
      else
        wiring.metrics.session.keyframeRequestUnattributed(
          MEDIA_KEYFRAME_REQUEST_OUTCOMES.NotSending,
        );
    },
    malformed: () => {
      wiring.metrics.session.keyframeRequestUnattributed(MEDIA_KEYFRAME_REQUEST_OUTCOMES.Malformed);
    },
    ended: (_url, cause) => {
      wiring.metrics.session.mhControlChannelEnded(cause);
    },
  };
}

/**
 * Zero-initialise, at join, the directive and keyframe series that should read
 * zero forever, so an alert later has a series to read (`client.md`). Per-kind
 * series for PUBLISHED kinds only — no video series before video flows.
 */
export function zeroInitDirectiveSeries(
  metrics: MediaMetrics,
  publish: readonly PublishKind[],
): void {
  metrics.session.sendDirectiveHeaderVersionRefused(0);
  for (const outcome of MEDIA_KEYFRAME_UNATTRIBUTED_OUTCOMES) {
    metrics.session.keyframeRequestUnattributed(outcome, 0);
  }
  for (const kind of publish) kindMetrics(metrics, kind).sendDirectiveKindMissing(0);
  for (const outcome of Object.values(MEDIA_MH_CONTROL_CHANNEL_ENDINGS)) {
    metrics.session.mhControlChannelEnded(outcome, 0);
  }
}

/**
 * The pipeline's two signalling listeners (story 3 C-A1):
 *
 *   * SENDING — {@link sendDirectiveListener}, per published kind; its effective
 *     per-kind send state feeds R-18.
 *   * RECEIVING — the AUDIO slot assignments, independently of the send
 *     directive: a participant nobody holds gets an empty target set and must
 *     still hear the slots it holds. Media kind is taken from the slot, never
 *     the frame. Slot, sender and handler travel together: the handler URLs pick
 *     the receive transports and the slot -> sender pairs are the slot-edge gate.
 */
export function pipelineListeners(
  wiring: SendDirectiveWiring & {
    readonly pipeline: DirectiveTarget & Pick<AudioPipeline, 'setReceiveAssignments'>;
  },
): MediaStateListeners {
  const { pipeline } = wiring;
  return {
    onDirective: sendDirectiveListener(wiring),
    onAssignments: (event) =>
      pipeline.setReceiveAssignments(
        event.assignments
          .filter((a) => a.mediaKind === 'audio')
          .map((a) => ({
            slotId: a.slotId,
            senderId: a.senderId,
            mediaHandlerUrl: a.mediaHandlerUrl,
            active: a.slotState === 'active',
          })),
      ),
  };
}
