// File: packages/sdk-core/src/media/lifecycle/VideoPublisher.ts
//
// The video PUBLISH lifecycle for one session (story 3 C-A2): camera (or the
// build-gated test pattern), the VP8 encoder, the stream-per-group egress, the
// keyframe gate and the video rotation timer. A SIBLING of the hot path
// (ADR-0036 §11), so it may log; the per-frame work is `pipeline/videoEgress.ts`.
//
// It wires three pure decision modules and owns no policy of its own:
//
//   * `cameraGate.ts`      — WHEN the camera is lit, and the R-19 latch;
//   * `keyframePolicy.ts`  — WHEN a keyframe may be produced (interval, one
//                            deferral, request acceptance);
//   * `videoEgress.ts`     — HOW frames reach targets (groups, abandonment, hop).
//
// THE TRANSMIT-KEY MANAGER IS THE SESSION'S (one per sender, the nonce
// boundary). This class never constructs one, never clears it (the session
// does, at teardown), and does not subscribe to KEK rotation (the session's one
// subscription rotates the shared generation). It adds only the video-only
// triggers: every T (`videoRotationPeriodMs`) here, and every group start in
// the egress.

import type { ConfigValueSource, MediaConfig } from '../../config/clientConfig.js';
import { videoConfigLogLine } from '../../config/clientConfig.js';
import { uplinkRelayStreamId } from '../pipeline/sendId.js';
import { VideoEgress, type GroupStreamOpener } from '../pipeline/videoEgress.js';
import { classifyCaptureError, MediaCaptureError } from '../setup/captureFailure.js';
import type { MeetingIdentity } from '../setup/identity.js';
import type { KekWrapSource } from '../setup/kekSource.js';
import {
  MEDIA_CAPTURE_FAILURE_OUTCOMES,
  MEDIA_KEYFRAME_SOURCES,
  MEDIA_MUTE_ACTIONS,
  MEDIA_SEND_GROUP_STOP_OUTCOMES,
  type MediaCaptureFailureOutcome,
  type MediaKeyframePerKindOutcome,
  type MediaKindMetrics,
} from '../setup/mediaMetrics.js';
import type {
  DirectedVideoEncoding,
  EncodedVideoFrame,
  VideoCaptureSource,
  VideoCaptureSourceFactory,
  VideoEncoderFactory,
  VideoEncoderSeam,
} from '../setup/seams.js';
import { initialCameraGate, stepCameraGate, type CameraGateEvent } from './cameraGate.js';
import { KeyframeGate } from './keyframePolicy.js';
import { FaultOnce } from './faultOnce.js';
import { MediaFaultStage, type MediaFault } from './AudioPipeline.js';
import type { MuteSnapshot, MuteState } from './muteState.js';
import type { MediaKindToken } from '../mediaKind.js';
import type { TransmitKeyManager } from './transmitKeys.js';

/** MC's validated instruction for this client's video stream. */
export interface VideoSendDirective {
  /** MC's send id (`SendStream.stream_number`; video 1). */
  readonly streamNumber: number;
  /** Validated; every field present (`session/mediaWiring.ts:validateVideoEncoding`). */
  readonly encoding: DirectedVideoEncoding;
  /** EMPTY MEANS SEND NOTHING: unwatched, or server-muted (both withdraw). */
  readonly targets: readonly string[];
  /** MC's meeting-wide header version. */
  readonly headerVersion: number;
}

/** Construction options. */
export interface VideoPublisherOptions {
  readonly config: MediaConfig;
  /** Provenance of `config` (embedder-supplied or the SDK default), for the R-23 log line. */
  readonly configSource: ConfigValueSource;
  /** The VIDEO per-kind handle. */
  readonly metrics: MediaKindMetrics;
  /** The SESSION's single transmit-key manager. */
  readonly transmitKeys: TransmitKeyManager;
  readonly kek: KekWrapSource;
  readonly identity: MeetingIdentity;
  /** The session's shared client-mute state (both kinds). */
  readonly mute: MuteState;
  /** Report ONE kind's client mute to MC (`signaling/muteRequest.ts`). Best-effort. */
  readonly reportMute: (kind: MediaKindToken, muted: boolean) => void;
  /** The presented mute changed (drives the session's `muteChanged`). */
  readonly onMuteChanged: (state: MuteSnapshot) => void;
  /** The one capture-source selection's factory (camera or gated test pattern). */
  readonly captureFactory: VideoCaptureSourceFactory;
  readonly encoderFactory: VideoEncoderFactory;
  /** Opens a group stream to a CONNECTED handler (`MediaTransport.openUniStream`). */
  readonly openStream: GroupStreamOpener;
  /** A bounded, SDK-authored fault (the session's `mediaFault`). */
  readonly fault: (fault: MediaFault) => void;
  readonly clock?: () => number;
  readonly setInterval?: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
  readonly clearInterval?: (handle: ReturnType<typeof setInterval>) => void;
  readonly setTimeout?: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  readonly clearTimeout?: (handle: ReturnType<typeof setTimeout>) => void;
  /** Lifecycle log sinks. Fixed text and config values only. */
  readonly warn?: (message: string) => void;
  readonly info?: (message: string) => void;
}

const FAULT_ENCODER = 'the video encoder failed; video publishing has stopped';
const FAULT_CAPTURE = 'the camera could not be used; you are shown as video-muted';
const FAULT_BUILD = 'a video frame could not be built or signed; video publishing has stopped';
const FAULT_NOT_CONNECTED =
  'the controller directed video to a media handler this client is not connected to';
const FAULT_UNSTAMPABLE =
  'the controller directed a video stream number this client cannot stamp; nothing is sent';

/** See the module header. */
export class VideoPublisher {
  readonly #o: VideoPublisherOptions;
  readonly #clock: () => number;
  readonly #setInterval: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
  readonly #clearInterval: (handle: ReturnType<typeof setInterval>) => void;
  readonly #gate: KeyframeGate;
  /** Event-once fault dedupe (`./faultOnce.ts`). */
  readonly #faults = new FaultOnce();
  #camera = initialCameraGate();
  #egress: VideoEgress | undefined;
  #directive: VideoSendDirective | undefined;
  #encoder: VideoEncoderSeam | undefined;
  #encoderEncoding: DirectedVideoEncoding | undefined;
  #capture: VideoCaptureSource | undefined;
  /** The camera the platform granted, reused by every re-acquire (no device re-pick). */
  #grantedDeviceId: string | undefined;
  /** Monotone acquire attempt id: a stale attempt's result is ignored. */
  #attempt = 0;
  #cameraLive = false;
  #keyNext = false;
  /** Encoder timestamp -> egress epoch at ENCODE time (security S5). */
  readonly #pendingEpochs = new Map<number, number>();
  #rotationTimer: ReturnType<typeof setInterval> | undefined;
  #started = false;
  #stopped = false;
  #everDirected = false;

  constructor(options: VideoPublisherOptions) {
    this.#o = options;
    this.#clock = options.clock ?? Date.now;
    this.#setInterval = options.setInterval ?? ((fn, ms) => setInterval(fn, ms));
    this.#clearInterval = options.clearInterval ?? ((h) => clearInterval(h));
    this.#camera = initialCameraGate(options.mute.videoMuted);
    const setT = options.setTimeout ?? ((fn, ms) => setTimeout(fn, ms));
    const clearT = options.clearTimeout ?? ((h) => clearTimeout(h));
    this.#gate = new KeyframeGate({
      metrics: options.metrics,
      clock: this.#clock,
      setTimeout: setT,
      clearTimeout: clearT,
      currentGop: () =>
        this.#directive === undefined
          ? undefined
          : options.transmitKeys.currentGopCounter(this.#directive.streamNumber),
      targetsAwaitingGroup: () => this.#egress?.targetsAwaitingGroup() ?? 0,
      produceKeyframe: () => {
        this.#keyNext = true;
      },
      warn: options.warn ?? ((m) => console.warn('[dt-media-video]', m)),
    });
  }

  /** Camera light state: a live, acquired capture track. For the test bus and UI. */
  get cameraTrackLive(): boolean {
    return this.#cameraLive;
  }

  /** The egress, for sampled assertion (frame counts, lane states). */
  get egress(): VideoEgress | undefined {
    return this.#egress;
  }

  /** The keyframe gate, for assertion. */
  get keyframeGate(): KeyframeGate {
    return this.#gate;
  }

  /** Start the rotation timer and apply anything already directed. */
  start(): void {
    if (this.#started || this.#stopped) return;
    this.#started = true;
    (this.#o.info ?? ((m) => console.info('[dt-media-video-config]', m)))(
      videoConfigLogLine(this.#o.config, this.#o.configSource),
    );
    // §4 / R-14: video rotates the session generation every T, its own period.
    this.#rotationTimer = this.#setInterval(
      () => this.#o.transmitKeys.rotate(),
      this.#o.config.keys.videoRotationPeriodMs,
    );
    // A directive that arrived before start() (the E3 replay of MC's
    // join-carried directive) is applied as a transition FROM NOTHING, so its
    // targets count as added and the directed start keys through the gate.
    const pending = this.#directive;
    this.#directive = undefined;
    if (pending) this.setDirective(pending);
  }

  /**
   * Apply MC's video instruction. `undefined` (or empty targets) is a
   * withdrawal: the camera stops, groups end, the deferral is discarded.
   * Self-keying follows the DIFF of target URLs only — a re-sent directive with
   * the same video targets keys nothing and touches no camera.
   */
  setDirective(directive: VideoSendDirective | undefined): void {
    if (this.#stopped) return;
    const previous = this.#directive;
    if (directive === undefined) {
      if (!previous) return;
      this.#directive = { ...previous, targets: [] };
    } else {
      this.#directive = directive;
    }
    if (!this.#started) return;
    const current = this.#directive;
    if (!current) return;

    const streamId = uplinkRelayStreamId(current);
    const targets = streamId === undefined ? [] : current.targets.filter((u) => u !== '');
    if (streamId === undefined && current.targets.length > 0) {
      this.#fault(MediaFaultStage.Config, FAULT_UNSTAMPABLE, false);
    }
    const nonEmpty = targets.length > 0;
    if (nonEmpty && !this.#everDirected) {
      this.#everDirected = true;
      // Zero-initialised when video is FIRST DIRECTED, never at join: no video
      // series exists before video flows (C-B's read-back must not pass vacuously).
      for (const source of Object.values(MEDIA_KEYFRAME_SOURCES)) {
        this.#o.metrics.keyframeProduced(source, 0);
      }
      this.#o.metrics.sendGroupAbandoned(0);
      for (const outcome of Object.values(MEDIA_SEND_GROUP_STOP_OUTCOMES)) {
        this.#o.metrics.sendGroupStopped(outcome, 0);
      }
      this.#o.metrics.encodeQueueDropped(0);
    }
    if (!this.#egress && nonEmpty && streamId !== undefined) {
      this.#egress = this.#buildEgress(current.streamNumber, streamId);
    }
    const before = new Set(
      previous && this.#egress ? previous.targets.filter((u) => u !== '') : [],
    );
    const egress = this.#egress;
    egress?.setTargets({ targets, headerVersion: current.headerVersion });
    this.#gate.setDirective(targets, current.encoding.minKeyframeIntervalMs);

    // A changed directed encoding rebuilds the encoder and restarts capture at
    // the new geometry; every target then awaits a new (gated) group.
    const encodingChanged =
      this.#encoderEncoding !== undefined && !sameEncoding(this.#encoderEncoding, current.encoding);
    if (encodingChanged) {
      this.#closeEncoder();
      egress?.endGroups();
      this.#restartCapture();
    }
    if (!nonEmpty) egress?.endGroups();

    this.#stepCamera({ type: 'directive', nonEmpty });
    const added = targets.some((u) => !before.has(u));
    if (nonEmpty && (added || encodingChanged)) {
      this.#gate.selfKey(MEDIA_KEYFRAME_SOURCES.DirectiveStart);
    }
  }

  /**
   * USER client video mute (R-15, R-17). Local first: the camera stops and
   * groups end before MC is told; unmute resumes with a keyed group. A user
   * mute clears an R-19 latch; a user unmute is one of the two retry edges.
   */
  setMuted(muted: boolean): void {
    if (this.#stopped) return;
    const changed = this.#o.mute.setVideoMuted(muted);
    if (changed) {
      this.#o.metrics.muteTransition(muted ? MEDIA_MUTE_ACTIONS.Mute : MEDIA_MUTE_ACTIONS.Unmute);
    }
    this.#gate.setMuted(muted);
    if (muted) this.#egress?.endGroups();
    this.#stepCamera({ type: 'userMute', muted });
    if (changed) this.#announceMute();
    if (!muted) this.#gate.selfKey(MEDIA_KEYFRAME_SOURCES.DirectiveStart);
  }

  /** An MH `KeyframeRequest` for the directed video stream, read on `url`'s connection. */
  keyframeRequest(url: string, gopCounter: number): MediaKeyframePerKindOutcome {
    return this.#gate.request(url, gopCounter);
  }

  /**
   * Stop everything this publisher owns. Idempotent. Does NOT clear the
   * session's keys — the returned promise settles once any in-flight video seal
   * has finished, so the session clears them after it.
   */
  stop(): Promise<void> {
    if (this.#stopped) return this.#egress?.drained ?? Promise.resolve();
    this.#stopped = true;
    this.#gate.close();
    if (this.#rotationTimer !== undefined) this.#clearInterval(this.#rotationTimer);
    this.#rotationTimer = undefined;
    this.#egress?.stop();
    this.#stopCapture();
    this.#closeEncoder();
    return this.#egress?.drained ?? Promise.resolve();
  }

  // ------------------------------------------------------------------------

  #buildEgress(streamNumber: number, streamId: number): VideoEgress {
    const { egress, keys } = this.#o.config;
    return new VideoEgress({
      metrics: this.#o.metrics,
      transmitKeys: this.#o.transmitKeys,
      kek: this.#o.kek,
      identity: this.#o.identity,
      streamNumber,
      streamId,
      keyBearingFrames: keys.videoKeyBearingFrames,
      maxBacklogFrames: egress.videoMaxBacklogFrames,
      maxBacklogBytes: egress.videoMaxBacklogBytes,
      openStream: this.#o.openStream,
      onSelfAbandoned: () => this.#gate.selfKey(MEDIA_KEYFRAME_SOURCES.Abandonment),
      onKeyChunk: () => this.#gate.keyChunkEmitted(),
      onTargetNotConnected: () =>
        this.#fault(MediaFaultStage.Transport, FAULT_NOT_CONNECTED, false),
      onBuildFailed: () => {
        this.#fault(MediaFaultStage.Crypto, FAULT_BUILD, true);
        this.stop();
      },
    });
  }

  #stepCamera(event: CameraGateEvent): void {
    const { state, actions } = stepCameraGate(this.#camera, event);
    this.#camera = state;
    if (actions.stop) this.#stopCapture();
    if (actions.acquire) void this.#acquire();
    if (actions.presentMuted !== undefined && this.#o.mute.setVideoMuted(actions.presentMuted)) {
      // A failure-mute (R-19) or the lift after a successful restore retry.
      this.#gate.setMuted(actions.presentMuted);
      if (actions.presentMuted) this.#egress?.endGroups();
      this.#announceMute();
      if (!actions.presentMuted) this.#gate.selfKey(MEDIA_KEYFRAME_SOURCES.DirectiveStart);
    }
  }

  #announceMute(): void {
    const snapshot = this.#o.mute.snapshot();
    this.#o.onMuteChanged(snapshot);
    // VIDEO only: the request names the kind it changed; audio's field stays
    // absent, so MC leaves audio exactly as it is.
    this.#o.reportMute('video', snapshot.videoMuted);
  }

  #restartCapture(): void {
    if (!this.#camera.live) return;
    this.#stopCapture();
    this.#camera = { ...this.#camera, live: false };
  }

  /** Acquire the camera (and the encoder) for the current directive. Never retries itself. */
  async #acquire(): Promise<void> {
    const attempt = ++this.#attempt;
    const directive = this.#directive;
    if (!directive) return;
    const encoding = directive.encoding;
    try {
      if (!this.#encoder) {
        this.#encoder = await this.#o.encoderFactory({
          encoding,
          onOutput: (chunk) => this.#onEncoded(chunk),
          onError: () => this.#onEncoderError(),
        });
        this.#encoderEncoding = encoding;
      }
    } catch {
      this.#onEncoderError();
      return;
    }
    if (attempt !== this.#attempt || this.#stopped) return;
    let capture: VideoCaptureSource | undefined;
    try {
      capture = await this.#o.captureFactory({
        width: encoding.width,
        height: encoding.height,
        frameRateHz: encoding.frameRateHz,
        deviceId: this.#grantedDeviceId,
      });
      // Tracked ONLY once this attempt is known current: a stale attempt that
      // resolves late must never overwrite (and so orphan, light still on) the
      // capture a newer attempt owns. A stale capture is stopped here, locally.
      if (attempt !== this.#attempt || this.#stopped) {
        capture.stop();
        return;
      }
      this.#capture = capture;
      const started = capture;
      await capture.start(
        (frame) => this.#onCaptured(frame),
        () => this.#onCaptureEnded(started),
      );
    } catch (err) {
      // Release THIS attempt's capture (never another attempt's).
      capture?.stop();
      if (capture !== undefined && this.#capture === capture) this.#capture = undefined;
      if (attempt !== this.#attempt || this.#stopped) return;
      const outcome = err instanceof MediaCaptureError ? err.reason : classifyCaptureError(err);
      this.#captureFailed(outcome);
      return;
    }
    if (attempt !== this.#attempt || this.#stopped) {
      capture.stop();
      return;
    }
    this.#grantedDeviceId = capture.grantedDeviceId ?? this.#grantedDeviceId;
    this.#cameraLive = true;
    this.#stepCamera({ type: 'acquired' });
  }

  #onCaptureEnded(capture: VideoCaptureSource): void {
    if (this.#capture !== capture) return;
    this.#capture = undefined;
    capture.stop();
    this.#cameraLive = false;
    this.#captureFailed(MEDIA_CAPTURE_FAILURE_OUTCOMES.Ended);
  }

  /** R-19: counted, presented as video-muted, NO automatic retry. */
  #captureFailed(outcome: MediaCaptureFailureOutcome): void {
    this.#o.metrics.captureFailure(outcome);
    // One fault per failure EDGE (each is bounded by a user or directive edge),
    // so not deduped for the session.
    this.#o.fault({ stage: MediaFaultStage.Capture, message: FAULT_CAPTURE, fatal: false });
    this.#egress?.endGroups();
    this.#stepCamera({ type: 'failed', outcome });
  }

  #stopCapture(): void {
    this.#attempt += 1;
    this.#capture?.stop();
    this.#capture = undefined;
    this.#cameraLive = false;
  }

  #closeEncoder(): void {
    this.#encoder?.close();
    this.#encoder = undefined;
    this.#encoderEncoding = undefined;
    this.#pendingEpochs.clear();
  }

  /** Captured frame -> encoder. Mute is enforced by the camera being OFF; this is the backstop. */
  #onCaptured(frame: VideoFrame): void {
    const egress = this.#egress;
    const encoder = this.#encoder;
    if (
      this.#stopped ||
      !encoder ||
      !egress ||
      this.#o.mute.videoMuted ||
      !egress.hasActiveTarget
    ) {
      frame.close();
      return;
    }
    // Pre-encode bound (ops F3): a starved encoder sheds the CAPTURED frame
    // rather than queueing without bound. A pending keyframe is NOT consumed —
    // it rides the next frame that is actually encoded.
    if (encoder.encodeQueueSize > this.#o.config.egress.videoMaxEncodeQueue) {
      frame.close();
      this.#o.metrics.encodeQueueDropped();
      return;
    }
    const key = this.#keyNext;
    this.#keyNext = false;
    this.#pendingEpochs.set(frame.timestamp, egress.epoch);
    try {
      encoder.encode(frame, key);
    } finally {
      frame.close();
    }
  }

  #onEncoded(chunk: EncodedVideoFrame): void {
    const epoch = this.#pendingEpochs.get(chunk.timestampUs);
    // Entries at or before this timestamp are done (an encoder may drop frames).
    for (const ts of this.#pendingEpochs.keys()) {
      if (ts <= chunk.timestampUs) this.#pendingEpochs.delete(ts);
    }
    if (epoch === undefined || !this.#egress) return;
    void this.#egress.submit(chunk, epoch);
  }

  #onEncoderError(): void {
    this.#o.metrics.encoderError();
    this.#fault(MediaFaultStage.Encoder, FAULT_ENCODER, true);
    this.stop();
  }

  #fault(stage: MediaFaultStage, message: string, fatal: boolean): void {
    if (!this.#faults.report(stage, message)) return;
    this.#o.fault({ stage, message, fatal });
  }
}

function sameEncoding(a: DirectedVideoEncoding, b: DirectedVideoEncoding): boolean {
  return (
    a.codec === b.codec &&
    a.width === b.width &&
    a.height === b.height &&
    a.frameRateHz === b.frameRateHz &&
    a.maxBitrateBps === b.maxBitrateBps
  );
}
