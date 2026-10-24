// File: packages/sdk-core/src/media/lifecycle/VideoReceiver.ts
//
// The session-scoped VIDEO RECEIVE orchestrator (story 3 C-B): start, stop,
// assignments, sinks, mute edges, evidence. A SIBLING of the hot path (ADR-0036
// §11): logging is permitted here and forbidden in `../pipeline/**`.
//
//   * EXACTLY M slot pipelines (`pipeline/videoSlotPipeline.ts`), one per
//     declared VIDEO slot, each with ONE decoder for the slot's lifetime, built
//     at start — decode resources bounded by M, never by sender count (R-6).
//   * ONE incoming-stream acceptor per handler transport
//     (`receiveTransports.ts:PerHandlerReadLoops`, the loop registry audio uses),
//     with a GLOBAL concurrency cap: a stream arriving at
//     `media.videoReceive.maxIncomingStreams` is cancelled unread and counted,
//     never queued. Each accepted stream is read by `pipeline/videoIngress.ts`.
//   * The video kind's OWN `TransmitKeyCache` + `ReplayWindow`, scoped per (KEK
//     generation, sender, stream) exactly as audio's, bound to the KEK holder
//     and teardown by the ONE helper both kinds use (`receiverState.ts`); the
//     roster rebind purge reaches it through `purgeTransmitKeys`.
//   * Recovery requests through `recoveryRequestQueue.ts` (one queued per slot,
//     one in flight per handler, no timer, no retry).
//   * The unfulfilled-slot series (`unfulfilledSlots.ts`) and the per-interval
//     decode-activity counter, on one export-cadence timer.

import {
  type ConfigValueSource,
  type MediaConfig,
  videoReceiveConfigLogLine,
} from '../../config/clientConfig.js';
import type { SlotStateToken } from '../../signaling/events.js';
import { ReplayWindow, TransmitKeyCache, type ReceiverKeys } from '../frame/receivePath.js';
import { FrameAccounting, type FrameCounts } from '../pipeline/frameAccounting.js';
import type { ReceiveVerificationRecorder } from '../pipeline/receiveVerification.js';
import type { IdentityKeyResolver } from '../pipeline/receivePolicy.js';
import type { VideoRenderState } from '../pipeline/renderDecision.js';
import type { FrameAllocator } from '../pipeline/streamReader.js';
import { VideoIngress } from '../pipeline/videoIngress.js';
import { VideoSlotPipeline } from '../pipeline/videoSlotPipeline.js';
import type { MhRecoveryRequest } from '../events.js';
import { FirstMediaObserver } from '../setup/measurement.js';
import { MEDIA_RECOVERY_SOURCES, type MediaKindMetrics } from '../setup/mediaMetrics.js';
import type { VideoDecoderFactory, VideoSink } from '../setup/seams.js';
import { TeardownRegistry, teardownFailureMessage } from '../teardown/teardown.js';
import { MediaFaultStage, type MediaFault } from './AudioPipeline.js';
import { FaultOnce } from './faultOnce.js';
import { RecoveryRequestQueue } from './recoveryRequestQueue.js';
import { PerHandlerReadLoops } from './receiveTransports.js';
import { bindReceiverState, type GenerationDroppedSource } from './receiverState.js';
import type { LocalReceiveSlotStats } from './localStats.js';
import { UnfulfilledSlots, type UnfulfilledSlotState } from './unfulfilledSlots.js';

/** Fixed fault texts (event-once dedupe keys; never a platform string). */
const FAULT_DECODER = "one participant's video decoder failed; it is being replaced";
const FAULT_RECOVERY_SEND = 'a video recovery request could not be sent to the media handler';
const FAULT_SINK =
  "the application's video sink failed to draw a frame; that slot is not rendering";

/** One video slot's assignment, as MC stated it. */
export interface VideoSlotAssignment {
  readonly slotId: number;
  readonly slotState: SlotStateToken;
  /** The avatar's participant (sender id); `undefined` for live and sourceless slots. */
  readonly senderId: number | undefined;
  readonly mediaHandlerUrl: string;
}

/**
 * One video slot's R-32 evidence: counts, closed tokens and booleans, plus the
 * two identity values the security ruling allows (`slotId`, the VERIFIED
 * `lastRenderedSender`). NO durations, GoP values, stream numbers or sizes.
 * `framesDecoded` is a cumulative count (story 3 C-C1, the bus's R-32 field).
 */
export interface VideoSlotEvidence {
  readonly slotId: number;
  /** MC's state for the slot; `undefined` before any assignment named it. */
  readonly mcState: SlotStateToken | undefined;
  /** What the slot actually renders. */
  readonly render: VideoRenderState['kind'];
  readonly lastRenderedSender: number | undefined;
  readonly unfulfilled: boolean;
  /** Frames this slot's decoder produced within bounds (cumulative count). */
  readonly framesDecoded: number;
}

/**
 * The NARROW surface an embedder gets from a session (security F4): attach a
 * slot's sink and read the evidence and local overlay snapshots. Nothing here
 * can change MC's slot state, request recovery, purge keys or stop the path.
 */
export interface VideoReceiveView {
  setVideoSink(slotId: number, sink: VideoSink | undefined): void;
  evidence(): readonly VideoSlotEvidence[];
  unfulfilledSnapshot(): readonly UnfulfilledSlotState[];
  /** Per-slot LOCAL R-35 statistics (read-only, never exported; `localStats.ts`). */
  localStats(): readonly LocalReceiveSlotStats[];
}

/** Construction options. */
export interface VideoReceiverOptions {
  readonly config: MediaConfig;
  readonly configSource: ConfigValueSource;
  /** The VIDEO per-kind handle. */
  readonly metrics: MediaKindMetrics;
  readonly kekSource: ReceiverKeys & GenerationDroppedSource;
  readonly roster: IdentityKeyResolver;
  /** This client's own sender id. */
  readonly selfSenderId: number;
  /** A verified sender's video server-mute state (latest `ParticipantMuteUpdate`). */
  readonly videoMuteState: (senderId: number) => 'muted' | 'unmuted' | 'unmapped';
  /** The VIDEO slot ids this client declared. */
  readonly declaredSlotIds: readonly number[];
  /** Incoming group streams of an ALREADY-CONNECTED handler, or `undefined`. A selector, never a dialer. */
  readonly incomingStreamsFor: (
    mediaHandlerUrl: string,
  ) => ReadableStream<ReadableStream<Uint8Array>> | undefined;
  /** The per-handler control send (`MediaTransport.sendRecoveryRequest`). */
  readonly sendRecoveryRequest: (
    mediaHandlerUrl: string,
    request: MhRecoveryRequest,
  ) => Promise<void>;
  readonly decoderFactory: VideoDecoderFactory;
  readonly metricExportIntervalMs: number;
  readonly fault: (fault: MediaFault) => void;
  readonly onRenderChanged: (slotId: number, state: VideoRenderState) => void;
  /** Test-only receive verification (story 2 R-30). Absent in production. */
  readonly verification?: ReceiveVerificationRecorder;
  /** The stream reader's allocation seam. Tests only. */
  readonly alloc?: FrameAllocator;
  readonly info?: (line: string) => void;
  readonly clock?: () => number;
  readonly setInterval?: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
  readonly clearInterval?: (handle: ReturnType<typeof setInterval>) => void;
}

/** See the module header. */
export class VideoReceiver {
  readonly #o: VideoReceiverOptions;
  readonly #clock: () => number;
  readonly #teardown = new TeardownRegistry();
  readonly #faults = new FaultOnce();
  readonly #accounting: FrameAccounting;
  readonly #cache: TransmitKeyCache;
  readonly #replay: ReplayWindow;
  readonly #unfulfilled: UnfulfilledSlots;
  readonly #slots = new Map<number, VideoSlotPipeline>();
  readonly #firstMedia: FirstMediaObserver;
  readonly #recovery: RecoveryRequestQueue;
  #ingress: VideoIngress | undefined;
  #loops: PerHandlerReadLoops<ReadableStream<Uint8Array>, string> | undefined;
  /** Verified senders that DECODED at least one frame this export interval. */
  readonly #decodedThisInterval = new Set<number>();
  #mediaStartedAtMs: number | undefined;
  #firstRenderObserved = false;
  #pendingAssignments: readonly VideoSlotAssignment[] = [];
  #started = false;
  #stopped = false;

  constructor(options: VideoReceiverOptions) {
    this.#o = options;
    this.#clock = options.clock ?? Date.now;
    const { config, metrics } = options;
    this.#accounting = new FrameAccounting(metrics, options.verification, options.declaredSlotIds);
    this.#cache = new TransmitKeyCache(config.receiverState.maxTransmitKeysPerSender);
    this.#replay = new ReplayWindow(
      config.receiverState.maxReplayContextsPerSender,
      config.receiverState.replayWindowBits,
    );
    this.#unfulfilled = new UnfulfilledSlots(metrics, this.#clock);
    this.#firstMedia = new FirstMediaObserver(metrics, this.#clock);
    this.#recovery = new RecoveryRequestQueue({
      send: (url, request) => options.sendRecoveryRequest(url, request),
      sent: (reason) => metrics.recoveryRequestSent(reason),
      failed: () => this.#fault(MediaFaultStage.Transport, FAULT_RECOVERY_SEND),
    });
  }

  /** Monotone frame counters (the parse-boundary identity). Sampled, never per frame. */
  get frameCounts(): FrameCounts {
    const a = this.#accounting;
    return {
      framesReceived: a.framesReceived,
      framesAccepted: a.framesAccepted,
      framesDropped: a.framesDropped,
      lastDropReason: a.lastDropReason,
    };
  }

  /** Start: build the M slot pipelines and bind the receive state. Idempotent. */
  start(): void {
    if (this.#started || this.#stopped) return;
    this.#started = true;
    const o = this.#o;
    const { metrics } = o;
    (o.info ?? ((m) => console.info('[dt-media-video-receive-config]', m)))(
      videoReceiveConfigLogLine(o.config, o.configSource),
    );
    // Zero-initialised, so `rate()` has a series before the first event.
    for (const reason of Object.values(MEDIA_RECOVERY_SOURCES)) {
      metrics.recoveryRequestSent(reason, 0);
    }
    metrics.incomingStreamRefused(0);
    metrics.activeSourceIntervals(0);

    // Bound before any stream can reach the ingress (C-3).
    bindReceiverState(
      { cache: this.#cache, replay: this.#replay },
      o.kekSource,
      (name, dispose) => this.#teardown.register(name, dispose),
      'video-',
    );

    const guards = {
      selfSenderId: o.selfSenderId,
      videoMuteState: (senderId: number) => o.videoMuteState(senderId),
    };
    for (const slotId of o.declaredSlotIds) {
      this.#slots.set(
        slotId,
        new VideoSlotPipeline({
          slotId,
          config: o.config.videoReceive,
          decoderRestartBackoffMs: o.config.ingress.decoderRestartBackoffMs,
          decoderFactory: o.decoderFactory,
          metrics,
          accounting: this.#accounting,
          guards,
          clock: this.#clock,
          unfulfilled: this.#unfulfilled,
          onRenderChanged: (id, state) => o.onRenderChanged(id, state),
          onLoss: (event) =>
            this.#recovery.request(
              event.url,
              { slotId: event.slotId, senderId: event.senderId, gopCounter: event.gopCounter },
              event.reason,
            ),
          onDecoded: (senderId) => this.#decodedThisInterval.add(senderId),
          onDrawn: (activeForMs) => this.#onDrawn(activeForMs),
          onDecoderFault: () => this.#fault(MediaFaultStage.Decoder, FAULT_DECODER),
          onSinkError: () => this.#fault(MediaFaultStage.Playback, FAULT_SINK),
        }),
      );
    }
    this.#teardown.register('video-slots', () => {
      for (const slot of this.#slots.values()) slot.stop();
    });

    const ingress = new VideoIngress({
      metrics,
      accounting: this.#accounting,
      ctx: { roster: o.roster, keys: o.kekSource, cache: this.#cache, replay: this.#replay },
      gate: { kind: 'video', guards },
      slotFor: (slotId) => this.#slots.get(slotId),
      firstMedia: this.#firstMedia,
      isStopped: () => this.#stopped,
      ...(o.verification ? { verification: o.verification } : {}),
      ...(o.alloc ? { alloc: o.alloc } : {}),
    });
    this.#ingress = ingress;
    this.#teardown.register('video-streams', () => ingress.cancelAll());

    const cap = o.config.videoReceive.maxIncomingStreams;
    this.#loops = new PerHandlerReadLoops<ReadableStream<Uint8Array>, string>({
      readableFor: (url) => o.incomingStreamsFor(url),
      contextFor: (url) => url,
      onItem: (stream, url) => {
        if (ingress.openStreams >= cap) {
          // REFUSED, never queued: a refusal costs nothing but the cancel.
          metrics.incomingStreamRefused();
          void stream.cancel().catch(() => {});
          return;
        }
        // NOT awaited: streams are read concurrently; the acceptor keeps accepting.
        void ingress.readStream(stream, url).catch(() => {
          this.#fault(MediaFaultStage.Crypto, 'a video frame could not be processed');
        });
      },
      isStopped: () => this.#stopped,
      register: (name, dispose) => this.#teardown.register(name, dispose),
      transportFault: (message) => this.#fault(MediaFaultStage.Transport, message),
      frameFault: (message) => this.#fault(MediaFaultStage.Crypto, message),
      texts: {
        disposerName: 'video-stream-acceptor',
        ended: 'the inbound video stream acceptor ended while the session was still active',
        failed: 'the video stream acceptor failed',
        itemDefect: 'an incoming video stream could not be accepted',
      },
    });

    this.#mediaStartedAtMs = this.#clock();
    this.#firstMedia.start();
    this.#teardown.register('video-receive-bookkeeping', () => {
      this.#unfulfilled.stop();
      this.#recovery.stop();
      this.#firstMedia.clear();
      this.#decodedThisInterval.clear();
    });

    const setI = o.setInterval ?? ((fn, ms) => setInterval(fn, ms));
    const clearI = o.clearInterval ?? ((h) => clearInterval(h));
    const timer = setI(() => this.#tick(), o.metricExportIntervalMs);
    this.#teardown.register('video-interval-timer', () => clearI(timer));

    this.setAssignments(this.#pendingAssignments);
  }

  /**
   * Apply MC's latest VIDEO slot assignments. REPLACES the view. A declared slot
   * the snapshot omits loses its MC state (fail closed); the handlers named are
   * where incoming group streams are accepted from.
   */
  setAssignments(assignments: readonly VideoSlotAssignment[]): void {
    if (this.#stopped) return;
    this.#pendingAssignments = assignments;
    if (!this.#started) return;
    for (const [slotId, slot] of this.#slots) {
      const a = assignments.find((x) => x.slotId === slotId);
      slot.setMcState(a?.slotState, a?.senderId);
    }
    const urls = [
      ...new Set(
        assignments
          .filter((a) => this.#slots.has(a.slotId))
          .map((a) => a.mediaHandlerUrl)
          .filter((url) => url !== ''),
      ),
    ];
    if (urls.length > 0) this.#loops?.open(urls);
  }

  /** `senderId`'s video server mute was lifted: the unmute-edge repair (`server_mute_lifted`). */
  videoServerMuteLifted(senderId: number): void {
    for (const slot of this.#slots.values()) slot.serverMuteLifted(senderId);
  }

  /** Attach (or detach) the embedder's sink for one declared video slot. */
  setVideoSink(slotId: number, sink: VideoSink | undefined): void {
    this.#slots.get(slotId)?.setSink(sink);
  }

  /** Story 2 R-18 for video: drop `senderId`'s cached transmit keys (every scope). Replay kept. */
  purgeTransmitKeys(senderId: number): void {
    this.#cache.purgeSender(BigInt(senderId));
  }

  /** The narrow embedder view (frozen closures over this receiver). */
  view(): VideoReceiveView {
    return Object.freeze({
      setVideoSink: (slotId: number, sink: VideoSink | undefined) =>
        this.setVideoSink(slotId, sink),
      evidence: () => this.evidence(),
      unfulfilledSnapshot: () => this.unfulfilledSnapshot(),
      localStats: () => this.localStats(),
    });
  }

  /** Per-slot R-32 evidence. Allocates; sample it, never per frame. */
  evidence(): readonly VideoSlotEvidence[] {
    return [...this.#slots.values()].map((slot) => ({
      slotId: slot.slotId,
      mcState: slot.mcState,
      render: slot.renderState.kind,
      lastRenderedSender: slot.lastRenderedSender,
      unfulfilled: this.#unfulfilled.isUnfulfilled(slot.slotId),
      framesDecoded: slot.framesDecoded,
    }));
  }

  /** Per-slot unfulfilled time-in-state, for the LOCAL R-35 overlay. Never exported. */
  unfulfilledSnapshot(): readonly UnfulfilledSlotState[] {
    return this.#unfulfilled.snapshot([...this.#slots.keys()]);
  }

  /**
   * Per declared VIDEO slot, the LOCAL R-35 statistics (`localStats.ts`).
   * Allocates a fresh snapshot; sample it on a timer, never per frame. Never
   * exported: not evidence, not a metric, not the bus.
   */
  localStats(): readonly LocalReceiveSlotStats[] {
    const unfulfilled = new Map(
      this.unfulfilledSnapshot().map((u) => [u.slotId, u.sinceMs] as const),
    );
    const rows: LocalReceiveSlotStats[] = [];
    for (const slot of this.#slots.values()) {
      const tally = this.#accounting.slotTally(slot.slotId);
      if (tally === undefined) continue;
      rows.push({
        kind: 'video',
        slotId: slot.slotId,
        packets: tally.packets,
        bytes: tally.bytes,
        framesDecoded: slot.framesDecoded,
        queueDepth: slot.jitterDepth,
        drops: tally.drops,
        localDrops: slot.localDrops,
        keyframesSeen: slot.keyframesSeen,
        unfulfilledMs: unfulfilled.get(slot.slotId) ?? 0,
      });
    }
    return rows;
  }

  /** Slot id -> the verified sender a LIVE slot shows (the pairing view's live half). */
  liveSenders(): ReadonlyMap<number, number> {
    const live = new Map<number, number>();
    for (const slot of this.#slots.values()) {
      const sender = slot.liveSender;
      if (sender !== undefined) live.set(slot.slotId, sender);
    }
    return live;
  }

  /**
   * Resolves once every incoming group stream accepted so far has ENDED and its
   * frames were processed. A test seam for deterministic assertions (no
   * wall-clock wait); a stream still open keeps it pending.
   */
  async settled(): Promise<void> {
    await this.#ingress?.settled();
  }

  /** Stop everything. Idempotent. Every unfulfilled entry exits (observed); the gauge reads 0. */
  async stop(): Promise<void> {
    if (this.#stopped) return;
    this.#stopped = true;
    // SYNCHRONOUSLY, before any await: the session's disconnect starts the final
    // metric flush right after it calls this, and these are exactly the
    // observations that flush exists to rescue — every unfulfilled slot's exit
    // (a slot stuck all session FIRST appears here) and the gauge's 0. Both
    // stops are idempotent; the teardown disposer repeats them harmlessly.
    this.#unfulfilled.stop();
    this.#recovery.stop();
    const failures = await this.#teardown.dispose();
    for (const failure of failures) {
      this.#fault(MediaFaultStage.Transport, teardownFailureMessage(failure.name));
    }
  }

  #onDrawn(activeForMs: number | undefined): void {
    if (!this.#firstRenderObserved && this.#mediaStartedAtMs !== undefined) {
      this.#firstRenderObserved = true;
      this.#o.metrics.timeToFirstRenderMs(this.#clock() - this.#mediaStartedAtMs);
    }
    if (activeForMs !== undefined) this.#o.metrics.slotActiveToFirstRenderMs(activeForMs);
  }

  /** The export-cadence tick: decode activity (observability O1) and the gauge re-set. */
  #tick(): void {
    if (this.#stopped) return;
    const sources = this.#decodedThisInterval.size;
    this.#decodedThisInterval.clear();
    if (sources > 0) this.#o.metrics.activeSourceIntervals(sources);
    this.#unfulfilled.reassert();
  }

  #fault(stage: MediaFault['stage'], message: string): void {
    if (!this.#faults.report(stage, message)) return;
    this.#o.fault({ stage, message, fatal: false });
  }
}
