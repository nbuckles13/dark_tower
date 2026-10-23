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
import type {
  PublishKind,
  SendDirectiveEvent,
  SendStreamDirective,
  SendStreamEncoding,
} from '../signaling/events.js';
import { SignalingCodec } from '../signaling/codecMap.js';
import { KEYFRAME_MIN_INTERVAL_CEILING_MS } from '../config/clientConfig.js';
import type { DirectedVideoEncoding } from '../media/setup/seams.js';
import type { VideoSendDirective } from '../media/lifecycle/VideoPublisher.js';
import type { VideoReceiver } from '../media/lifecycle/VideoReceiver.js';
import type { MhControlSink } from '../media/events.js';
import type { MediaStateListeners } from './signalingMediaState.js';
import type { MediaKindToken } from '../media/mediaKind.js';
import {
  MEDIA_KEYFRAME_REQUEST_OUTCOMES,
  MEDIA_KEYFRAME_UNATTRIBUTED_OUTCOMES,
  MEDIA_MH_CONTROL_CHANNEL_ENDINGS,
  MEDIA_ROSTER_KEY_CHANGES,
  type MediaKeyframePerKindOutcome,
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

/**
 * The operation a roster invalidation drives. The session's media
 * (`mediaStart.ts:SessionMedia`) implements it over EVERY kind's receive key
 * cache — audio's and video's — so the purge covers video (story 3 C-B).
 */
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

/**
 * The session's client-mute report to MC — ONE kind per request, the other
 * kind's field absent (= unchanged at MC; `signaling/muteRequest.ts`).
 * Deliberately BEST-EFFORT: ADR-0036 §5 requires client mute to hold WITHOUT
 * the server honouring it, so a failed report must not undo the local
 * suppression; nothing is actionable and nothing is safe to log about it.
 */
export function bestEffortMuteReport(signaling: {
  sendMuteRequest(kind: MediaKindToken, muted: boolean): Promise<unknown>;
}): (kind: MediaKindToken, muted: boolean) => void {
  return (kind, muted) => {
    void signaling.sendMuteRequest(kind, muted).catch(() => {});
  };
}

/** The pipeline operation a directive drives. */
export type DirectiveTarget = Pick<AudioPipeline, 'setSendDirective'>;

/** The fault text for a published kind missing from a non-empty directive. A fixed literal. */
export const DIRECTIVE_KIND_MISSING_FAULT =
  "the controller's send directive omitted a media kind this client publishes; " +
  'nothing of that kind is sent until a directive lists it again';

/** The fault text for a refused video encoding. A fixed literal (event-once dedupe key). */
export const VIDEO_ENCODING_REFUSED_FAULT =
  "the controller's video send directive has a missing or invalid required encoding field; " +
  'no video is sent until a complete directive arrives';

/** The required video encoding fields, by their wire names (the WARN names one). */
export const VIDEO_ENCODING_FIELDS = [
  'codec',
  'width',
  'height',
  'frame_rate',
  'max_bitrate_bps',
  'min_keyframe_interval_ms',
] as const;

/** One of {@link VIDEO_ENCODING_FIELDS}. */
export type VideoEncodingField = (typeof VIDEO_ENCODING_FIELDS)[number];

/** The result of {@link validateVideoEncoding}. */
export type VideoEncodingValidation =
  | { readonly ok: true; readonly encoding: DirectedVideoEncoding }
  | { readonly ok: false; readonly field: VideoEncodingField };

/**
 * Validate a VIDEO stream's directed encoding (story 3 R-8, R-11, R-23). PURE.
 *
 * EVERY field is required, `min_keyframe_interval_ms` included, and there is no
 * client default for any of them: an absent or 0 plain scalar, a codec other
 * than VP8, or an absent `min_keyframe_interval_ms` refuses the stream (MC's
 * `VideoEncoding` always sets all six). A PRESENT `min_keyframe_interval_ms`
 * of 0 is refused too; a present value below the compiled floor is not refused
 * here — the keyframe gate clamps and counts it. Video-only: audio leaves
 * these unset.
 */
export function validateVideoEncoding(
  encoding: SendStreamEncoding | undefined,
): VideoEncodingValidation {
  if (!encoding || encoding.codec !== SignalingCodec.Vp8) return { ok: false, field: 'codec' };
  const positive = (n: number | undefined): n is number =>
    n !== undefined && Number.isInteger(n) && n > 0;
  if (!positive(encoding.width)) return { ok: false, field: 'width' };
  if (!positive(encoding.height)) return { ok: false, field: 'height' };
  if (!positive(encoding.frameRate)) return { ok: false, field: 'frame_rate' };
  if (!positive(encoding.maxBitrateBps)) return { ok: false, field: 'max_bitrate_bps' };
  const minInterval = encoding.minKeyframeIntervalMs;
  if (!positive(minInterval) || minInterval > KEYFRAME_MIN_INTERVAL_CEILING_MS) {
    return { ok: false, field: 'min_keyframe_interval_ms' };
  }
  return {
    ok: true,
    encoding: {
      codec: 'vp8',
      width: encoding.width,
      height: encoding.height,
      frameRateHz: encoding.frameRate,
      maxBitrateBps: encoding.maxBitrateBps,
      minKeyframeIntervalMs: minInterval,
    },
  };
}

/** The video publisher operation a directive drives. */
export interface VideoDirectiveTarget {
  setDirective(directive: VideoSendDirective | undefined): void;
}

/** Construction options for {@link sendDirectiveListener}. */
export interface SendDirectiveWiring {
  /** The frozen publish set. Every published kind must be listed by MC. */
  readonly publish: readonly PublishKind[];
  readonly pipeline: DirectiveTarget;
  /** The video publisher, when video is published and media started. */
  readonly video?: VideoDirectiveTarget;
  /** Lifecycle WARN sink for a refused video encoding (names the field only). */
  readonly warn?: (message: string) => void;
  /** The configured audio bitrate, applied only when MC directs none. */
  readonly defaultBitrateBps: number;
  readonly metrics: MediaMetrics;
  /** Raise a bounded, SDK-authored media fault (the session's `mediaFault`). */
  readonly fault: (fault: MediaFault) => void;
  /**
   * The R-18 feed: whether `kind` is EFFECTIVELY being sent after validation
   * (`serverMuteResume.ts`). A refused header version, a missing kind and a
   * refused video encoding all report `false`.
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
 *     MC lists EVERY published kind on every directive (withdrawal is EMPTY
 *     TARGETS, never removal), so an empty `streams` list from MC is the same
 *     fault for each published kind — and no fault at all for a receive-only
 *     participant, which publishes nothing.
 *   * A VIDEO stream missing any encoding field is refused (counted, a typed
 *     fault, a WARN naming the field) and video fails closed.
 *   * Client mute never changes the directive (ADR-0036 §5); audio withdrawal
 *     keeps the mic OPEN and sends nothing (no silence, no DTX).
 */
export function sendDirectiveListener(
  wiring: SendDirectiveWiring,
): (directive: SendDirectiveEvent) => void {
  let lastAudio: AudioSendDirective | undefined;
  let missingFaultRaised = false;
  let encodingFaultRaised = false;
  const warn = wiring.warn ?? ((message: string) => console.warn('[dt-media-video]', message));

  /**
   * Apply video's instruction, or withdraw it. Fail closed on a refused header
   * version or a refused encoding: the publisher sees a withdrawal (camera off).
   * Returns whether video is EFFECTIVELY being sent (the R-18 feed).
   */
  const applyVideo = (
    stream: SendStreamDirective | undefined,
    headerOk: boolean,
    headerVersion: number,
  ): boolean => {
    if (!stream || !headerOk) {
      wiring.video?.setDirective(undefined);
      return false;
    }
    const valid = validateVideoEncoding(stream.encoding);
    if (!valid.ok) {
      // Counted per directive receipt; the fault is event-once per episode.
      // Counted per directive receipt; the WARN and the fault once per episode
      // (MC re-sends directives on every composition change).
      wiring.metrics.video.sendDirectiveEncodingRefused();
      if (!encodingFaultRaised) {
        encodingFaultRaised = true;
        warn(
          `refused a video send directive: required encoding field ${valid.field} is missing or invalid`,
        );
        wiring.fault({
          stage: MediaFaultStage.Config,
          message: VIDEO_ENCODING_REFUSED_FAULT,
          fatal: false,
        });
      }
      wiring.video?.setDirective(undefined);
      return false;
    }
    encodingFaultRaised = false;
    wiring.video?.setDirective({
      streamNumber: stream.streamNumber,
      encoding: valid.encoding,
      targets: stream.targets,
      headerVersion,
    });
    return stream.targets.length > 0;
  };

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
    if (kind === 'video') {
      wiring.effectiveTargets(kind, applyVideo(stream, headerOk, headerVersion));
      return;
    }
    applyAudio(stream, headerVersion);
    wiring.effectiveTargets(kind, headerOk && stream !== undefined && stream.targets.length > 0);
  };

  return (directive) => {
    const headerOk = isSupportedHeaderVersion(directive.headerVersion);
    if (!headerOk) wiring.metrics.session.sendDirectiveHeaderVersionRefused();

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
 * The per-kind handle for a PUBLISHED kind. Exhaustive: a published kind with
 * no handle is an internal invariant violation and THROWS, never a silently
 * uncounted fault.
 */
export function kindMetrics(metrics: MediaMetrics, kind: PublishKind): MediaKindMetrics {
  switch (kind) {
    case 'audio':
      return metrics.audio;
    case 'video':
      return metrics.video;
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
  /** The video publisher once media started (`undefined` before, or not publishing video). */
  readonly video: () => VideoKeyframeTarget | undefined;
}

/** The video publisher operation a keyframe request drives. */
export interface VideoKeyframeTarget {
  /** Classify, count (the gate counts) and key if honoured. */
  keyframeRequest(url: string, gopCounter: number): MediaKeyframePerKindOutcome;
}

/**
 * The session's MH control-channel sink (story 3 R-11).
 *
 * A `KeyframeRequest` is UNTRUSTED input. Only a request naming the directed
 * `stream_number` of the PUBLISHED video stream reaches the keyframe gate
 * (`lifecycle/keyframePolicy.ts`), with `url` = the handler connection the
 * request was READ on (bound by `MediaTransport`'s per-connection read loop,
 * never taken from the payload); the gate classifies and counts it. Every
 * other request is refused BEFORE any gate state is touched: one naming the
 * AUDIO stream (audio has no keyframes) counts `not_sending` on the audio
 * handle; one naming no directed stream counts kind-free, as does malformed
 * input. Neither the stream number nor the GoP counter reaches a label, log or
 * event.
 */
export function mhControlSink(wiring: MhControlWiring): MhControlSink {
  return {
    keyframeRequest: (url, request) => {
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
      const video = kind === 'video' && handle ? wiring.video() : undefined;
      if (video) video.keyframeRequest(url, request.gopCounter);
      else if (handle) handle.keyframeRequestReceived(MEDIA_KEYFRAME_REQUEST_OUTCOMES.NotSending);
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
 * series for PUBLISHED kinds only (video's `kind_missing` when video is
 * published; video's keyframe and abandonment series wait for first direction).
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
 * The pipeline's two signalling listeners (story 3 C-A1, C-B):
 *
 *   * SENDING — {@link sendDirectiveListener}, per published kind; its effective
 *     per-kind send state feeds R-18.
 *   * RECEIVING — the slot assignments, independently of the send directive: a
 *     participant nobody holds gets an empty target set and must still receive
 *     the slots it holds. MEDIA KIND IS TAKEN FROM THE SLOT, never the frame:
 *     audio slots go to the audio pipeline (slot -> sender pairs are its
 *     slot-edge gate), video slots to the video receiver (MC's flat state per
 *     slot; a live video slot names no sender). The handler URLs pick the
 *     receive transports for each.
 */
export function pipelineListeners(
  wiring: SendDirectiveWiring & {
    readonly pipeline: DirectiveTarget & Pick<AudioPipeline, 'setReceiveAssignments'>;
    /** The video receiver, when this client declared video slots. */
    readonly videoReceiver?: Pick<VideoReceiver, 'setAssignments'>;
  },
): MediaStateListeners {
  const { pipeline, videoReceiver } = wiring;
  return {
    onDirective: sendDirectiveListener(wiring),
    onAssignments: (event) => {
      pipeline.setReceiveAssignments(
        event.assignments
          .filter((a) => a.mediaKind === 'audio')
          .map((a) => ({
            slotId: a.slotId,
            senderId: a.senderId,
            mediaHandlerUrl: a.mediaHandlerUrl,
            active: a.slotState === 'active',
          })),
      );
      videoReceiver?.setAssignments(
        event.assignments
          .filter((a) => a.mediaKind === 'video')
          .map((a) => ({
            slotId: a.slotId,
            slotState: a.slotState,
            senderId: a.senderId,
            mediaHandlerUrl: a.mediaHandlerUrl,
          })),
      );
    },
  };
}
