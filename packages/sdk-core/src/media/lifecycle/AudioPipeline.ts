// File: packages/sdk-core/src/media/lifecycle/AudioPipeline.ts
//
// The session-scoped orchestrator: start, stop, mute, rotate, and the ingress
// read loop.
//
// SIBLING of the hot path, not a child. ADR-0036 §11 requires lifecycle, setup
// and teardown to be siblings of the media directory "so the directory boundary
// and the hot-path boundary are the same boundary". Logging is therefore
// permitted here by design and forbidden in `../pipeline/**`.
//
// ---------------------------------------------------------------------------
// A GREEN LOOPBACK IS NOT PROOF OF CRYPTO CORRECTNESS
// ---------------------------------------------------------------------------
//
// This story is deliberately a loopback: one client encrypts, signs, sends,
// receives, verifies, decrypts and plays its OWN audio. That proves WIRING —
// capture, encode, frame, queue, transport, decode, verify, decrypt, playback —
// and it proves nothing about the cryptography, because a single client
// encrypting and decrypting with its own keys is SELF-CONSISTENT under a swapped
// associated data, an inverted nonce, a truncated key id, or a signature taken
// over the wrong range. Every one of those produces a working demo.
//
// The only evidence of crypto correctness is the codec task's externally
// anchored vectors — `proto/test-vectors/frame-v2.vectors.json`, the vendored
// sframe-wg key-schedule vectors, and the conformance suite that runs both. This
// module does not weaken, regenerate, or route around them. If a vector row ever
// appears to need changing to accommodate this pipeline, that is a finding to
// raise, not a fix to apply.
//
// ---------------------------------------------------------------------------
// A MEDIA FAULT MUST NOT TAKE THE MEETING WITH IT
// ---------------------------------------------------------------------------
//
// Starting is EXPLICIT — the application calls it — and every failure surfaces
// as a typed event rather than by rejecting the join. Degraded audio beats a
// dropped meeting. There is deliberately no "disable the media path" flag; the
// explicit start is the only in-story lever and that is recorded rather than
// papered over.

import { TypedEventEmitter } from '../../events/TypedEventEmitter.js';
import { assertEd25519Available } from '../frame/ed25519.js';
import { ReplayWindow, TransmitKeyCache } from '../frame/receivePath.js';
import type { MediaConfig } from '../../config/clientConfig.js';
import { validateMediaConfig } from '../../config/clientConfig.js';
import { EgressPipeline, type DatagramSender } from '../pipeline/egress.js';
import { IngressPipeline, type AcceptedFrame } from '../pipeline/ingress.js';
import { HopSequenceMonitor } from '../pipeline/hopSequenceMonitor.js';
import { FirstMediaObserver } from '../setup/measurement.js';
import { MEDIA_MUTE_ACTIONS, type MediaMetrics } from '../setup/mediaMetrics.js';
import type { MeetingKekSource } from '../setup/kekSource.js';
import type { MeetingIdentity } from '../setup/identity.js';
import type { RosterIdentityKeys } from '../setup/rosterKeys.js';
import type {
  AudioDecoderFactory,
  AudioEncoderFactory,
  AudioEncoderSeam,
  CaptureSource,
  CaptureSourceFactory,
  PlaybackSinkFactory,
} from '../setup/seams.js';
import type { RejectReason } from '../frame/rejectReason.js';
import { TeardownRegistry } from '../teardown/teardown.js';
import { MuteState, type MuteSnapshot } from './muteState.js';
import { TransmitKeyManager } from './transmitKeys.js';

/** Why the pipeline reported a fault. Bounded; never a raw platform message. */
export const MediaFaultStage = {
  Capture: 'capture',
  Encoder: 'encoder',
  Decoder: 'decoder',
  Playback: 'playback',
  Crypto: 'crypto',
  Transport: 'transport',
  Config: 'config',
} as const;

/** One of {@link MediaFaultStage}. */
export type MediaFaultStage = (typeof MediaFaultStage)[keyof typeof MediaFaultStage];

/** A media-path fault, surfaced rather than absorbed. */
export interface MediaFault {
  readonly stage: MediaFaultStage;
  /** SDK-authored, bounded. Never a platform string and never key material. */
  readonly message: string;
  /** True when the pipeline stopped as a result. */
  readonly fatal: boolean;
}

/**
 * A sampled snapshot of the pipeline's monotone frame counters.
 *
 * Deliberately a SNAPSHOT rather than a live view: it is read by an embedder on
 * a timer (the browser demo samples it a few times a second for its E2E bus),
 * never on the forward path. See `AudioPipeline.frameCounts`.
 */
export interface MediaFrameCounts {
  /** Frames this client actually put on the wire. */
  readonly framesSent: number;
  /** Datagrams that arrived, counted at the wire before any parse. */
  readonly framesReceived: number;
  /** Frames that verified, decrypted and reached the decoder. */
  readonly framesAccepted: number;
  /** Frames rejected for a wire reason. */
  readonly framesDropped: number;
  /**
   * The most recent reject token, or `undefined` before the first drop.
   *
   * Bounded by TYPE, not by convention: `RejectReason` is the frozen
   * sixteen-token union whose SSoT is `proto/test-vectors/frame-v2.vectors.json`,
   * so no participant, key or payload string can reach this field and compile.
   */
  readonly lastDropReason: RejectReason | undefined;
}

/** Events the pipeline emits. */
export interface AudioPipelineEventMap {
  /** Client-mute state changed. Drives UI; never inferred from frame absence. */
  muteChanged: MuteSnapshot;
  /** The first media frame arrived at the wire, in ms since media start. */
  firstMediaFrame: number;
  /** A frame completed the receive path, attributed from its own key id. */
  frameAccepted: AcceptedFrame;
  /** Something went wrong. Bounded; event-once per condition, never per frame. */
  fault: MediaFault;
}

/** What MC directed this client to produce. Distilled from `SendDirective`. */
export interface AudioSendDirective {
  /** The publisher's stream index (8-bit semantics in the key id). */
  readonly streamNumber: number;
  /** Encoder bitrate. From `EncodingParameters.max_bitrate_bps` when MC set it. */
  readonly bitrateBps: number;
  /**
   * Where to send. EMPTY MEANS SEND NOTHING (ADR-0036 §5), and resuming from
   * empty rotates the transmit key.
   */
  readonly targets: readonly string[];
}

/** Construction options. */
export interface AudioPipelineOptions {
  readonly config: MediaConfig;
  readonly metrics: MediaMetrics;
  readonly kekSource: MeetingKekSource;
  readonly roster: RosterIdentityKeys;
  /** MC's per-meeting sender id for this client, from the join response. */
  readonly senderId: number;
  /** The KEK generation to announce on every wrapped block. */
  readonly kekGeneration: number;
  /**
   * The meeting identity holder.
   *
   * Passed as the holder, never as a bare `CryptoKey` — see
   * `EgressPipelineOptions.identity` for why: a copied capability outlives
   * `MeetingIdentity.clear()` and teardown then fails to revoke it.
   */
  readonly identity: MeetingIdentity;
  /** Slot ids this client declared in its `ReceiveCapability`. */
  readonly declaredSlotIds: readonly number[];
  /** Resolves a datagram channel for a media-handler URL, or `undefined`. */
  readonly senderFor: (mediaHandlerUrl: string) => DatagramSender | undefined;
  /**
   * Inbound datagrams for an ALREADY-CONNECTED media handler, or `undefined`.
   *
   * A selector, never a dialer: see {@link AudioPipeline.setReceiveHandlers}.
   */
  readonly readableFor: (mediaHandlerUrl: string) => ReadableStream<Uint8Array> | undefined;
  /** Reports client mute to MC. Informational; the local state changes first. */
  readonly reportMute: (audioMuted: boolean) => void;
  readonly captureFactory: CaptureSourceFactory;
  readonly encoderFactory: AudioEncoderFactory;
  readonly decoderFactory: AudioDecoderFactory;
  readonly playbackFactory: PlaybackSinkFactory;
  readonly clock?: () => number;
  /** Timer seam, so rotation is testable without wall-clock waits. */
  readonly setInterval?: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
  readonly clearInterval?: (handle: ReturnType<typeof setInterval>) => void;
}

/** Options for {@link AudioPipeline.start}. */
export interface AudioPipelineStartOptions {
  /** `MediaDeviceInfo.deviceId` of the chosen microphone; default when absent. */
  readonly deviceId?: string;
}

export class AudioPipeline extends TypedEventEmitter<AudioPipelineEventMap> {
  readonly #options: AudioPipelineOptions;
  readonly #config: MediaConfig;
  readonly #metrics: MediaMetrics;
  readonly #mute = new MuteState();
  readonly #teardown = new TeardownRegistry();
  readonly #transmitKeys: TransmitKeyManager;
  readonly #cache: TransmitKeyCache;
  readonly #replay: ReplayWindow;
  readonly #hopMonitor: HopSequenceMonitor;
  readonly #firstMedia: FirstMediaObserver;
  readonly #clock: () => number;
  readonly #setInterval: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
  readonly #clearInterval: (handle: ReturnType<typeof setInterval>) => void;

  // Only the ENCODER is held on the instance, because only it is used per frame
  // (the capture callback feeds it). Capture, the decoder and the playback sink
  // are reachable solely through the teardown registry, which is where their
  // lifetime actually lives — holding a second reference would be a second place
  // to forget to release them.
  #encoder: AudioEncoderSeam | undefined;
  #egress: EgressPipeline | undefined;
  #ingress: IngressPipeline | undefined;
  #rotationTimer: ReturnType<typeof setInterval> | undefined;
  /**
   * The handler the inbound read loop runs on, once started; `undefined` before.
   *
   * Started at most ONCE. MC re-emits `StreamAssignments` on every structural
   * change in the meeting (every join, leave, declaration and mute), so
   * `#applyReceive` runs many times per session. A `ReadableStream` can be
   * locked by only ONE reader, so a second `getReader()` throws — and it would
   * throw from inside an assignment handler, where the failure would present as
   * "media stopped after someone joined" rather than as what it is. A re-emit
   * naming the same handler is therefore a no-op: nothing on this path touches
   * the transport or any receiver state, so the replay window survives every
   * unrelated roster event.
   */
  #readLoopUrl: string | undefined;
  /** The single handler the latest `StreamAssignments` names, awaiting `start()`. */
  #receiveUrl: string | undefined;
  #directive: AudioSendDirective | undefined;
  #started = false;
  #stopped = false;
  /**
   * Event-once flags: a repeating fault must not become per-frame telemetry.
   *
   * Keyed on (stage, message), NOT stage alone. Several faults share a stage
   * with genuinely different remedies — on `transport`, "not connected to this
   * handler" and "assignments moved to a different handler mid-session" (an MC
   * placement defect) — and a stage-only key let whichever fired first silence
   * the rest for the session. Still bounded by construction: every message is
   * a string literal at its call site, except teardown's, which interpolates
   * only a `TeardownRegistry` name — itself a literal from a fixed set.
   */
  readonly #reportedFaults = new Set<string>();

  /**
   * The live capture, held as a FIELD rather than captured by the teardown
   * closure, so `setCaptureDevice` can swap it while keeping exactly ONE
   * registration. `TeardownRegistry` has no unregister, so a closure over the
   * local would pin the first capture forever and a device swap would leak the
   * old microphone — the lit-indicator privacy failure `setup/capture.ts` calls
   * out, one layer up.
   */
  #capture: CaptureSource | undefined;
  /** The requested microphone. Applied at `start()`, or immediately if running. */
  #deviceId: string | undefined;

  constructor(options: AudioPipelineOptions) {
    super();
    validateMediaConfig(options.config);
    this.#options = options;
    this.#config = options.config;
    this.#metrics = options.metrics;
    this.#clock = options.clock ?? Date.now;
    this.#setInterval = options.setInterval ?? ((fn, ms) => setInterval(fn, ms));
    this.#clearInterval = options.clearInterval ?? ((h) => clearInterval(h));
    this.#transmitKeys = new TransmitKeyManager(options.senderId);
    this.#cache = new TransmitKeyCache(options.config.receiverState.maxTransmitKeysPerSender);
    this.#replay = new ReplayWindow(
      options.config.receiverState.maxReplayContextsPerSender,
      options.config.receiverState.replayWindowBits,
    );
    this.#hopMonitor = new HopSequenceMonitor(
      options.declaredSlotIds,
      options.config.ingress.hopRestartBackwardJumpFrames,
    );
    this.#firstMedia = new FirstMediaObserver(options.metrics, this.#clock, (ms) => {
      this.emit('firstMediaFrame', ms);
    });
  }

  /** Current client-mute state. */
  get muteState(): MuteSnapshot {
    return this.#mute.snapshot();
  }

  /** The current transmit-key generation. For assertion; never a metric label. */
  get transmitGeneration(): bigint {
    return this.#transmitKeys.generation;
  }

  /**
   * Monotone frame counters, for an embedder to SAMPLE.
   *
   * Allocates one object PER READ, which is why reads must stay sampled and
   * must never happen per frame. Nothing on the forward path calls this.
   *
   * Every number is a read of bookkeeping the pipelines already maintain beside
   * their metric counters (`dt_client_media_frames_{sent,received,accepted,dropped}_total`),
   * so a projection of these cannot disagree with the metrics. Zero before
   * `start()`, because neither pipeline exists yet — which is a real state a
   * caller can observe, not a missing value.
   *
   * THE RECEIVE SIDE IS REPORTED AS A COMPLETE IDENTITY —
   * `received = accepted + sum(drops by reason)` — and not as `accepted` alone,
   * because `accepted` on its own cannot distinguish **nothing is arriving**
   * from **arriving and being rejected**. Those have opposite remediations (a
   * relay that stopped forwarding vs a client that cannot attribute the sender),
   * and collapsing them cost a live misdiagnosis during this pipeline's first
   * end-to-end run. `lastDropReason` turns the second case from "something
   * rejected them" into the actual token.
   */
  get frameCounts(): MediaFrameCounts {
    const ingress = this.#ingress;
    return {
      framesSent: this.#egress?.framesSent ?? 0,
      framesReceived: ingress?.framesReceived ?? 0,
      framesAccepted: ingress?.framesAccepted ?? 0,
      framesDropped: ingress?.framesDropped ?? 0,
      lastDropReason: ingress?.lastDropReason,
    };
  }

  /** The microphone currently requested, or `undefined` for the platform default. */
  get captureDeviceId(): string | undefined {
    return this.#deviceId;
  }

  /**
   * Start capture, encode, transport I/O and playback.
   *
   * EVERY resource is registered for teardown at acquisition, before the next
   * await, so a throw part-way through still stops the microphone.
   */
  async start(options: AudioPipelineStartOptions = {}): Promise<void> {
    if (this.#started) return;
    this.#started = true;

    // Gates BOTH directions. `docs/TODO.md` scoped this to the receive loop, but
    // that entry predates the send path: if Ed25519 is unavailable the client
    // must fail closed on egress too, or it would emit frames whose signature
    // step failed. Nothing here degrades to an unsigned or zero-signature frame.
    await assertEd25519Available();

    const audio = this.#config.audio;
    if (options.deviceId !== undefined) this.#deviceId = options.deviceId;
    const capture = await this.#options.captureFactory({
      sampleRateHz: audio.sampleRateHz,
      channels: audio.channels,
      deviceId: this.#deviceId,
    });
    this.#capture = capture;
    // REGISTERED BEFORE THE NEXT AWAIT: a leaked capture keeps the microphone
    // hot and the browser's recording indicator lit.
    //
    // The disposer reads the FIELD, not this local, so `setCaptureDevice` can
    // replace the capture without a second registration — see `#capture`.
    this.#teardown.register('capture', () => this.#capture?.stop());

    const playback = await this.#options.playbackFactory({
      sampleRateHz: audio.sampleRateHz,
      channels: audio.channels,
    });
    this.#teardown.register('playback', () => playback.close());

    const decoder = await this.#options.decoderFactory({
      sampleRateHz: audio.sampleRateHz,
      channels: audio.channels,
      onOutput: (data) => playback.enqueue(data),
      onError: () => {
        // The codec's terminal error callback, at most once per instance — not a
        // per-frame path. Counted AND evented; never logged per frame.
        this.#metrics.decoderError();
        this.#fault(
          MediaFaultStage.Decoder,
          'the audio decoder failed; playback has stopped',
          true,
        );
      },
    });
    this.#teardown.register('decoder', () => decoder.close());

    const encoder = await this.#options.encoderFactory({
      sampleRateHz: audio.sampleRateHz,
      channels: audio.channels,
      // MC's directed bitrate when it supplied one; the configured DEFAULT
      // otherwise. Never a local ceiling applied on top: the directive's
      // `max_bitrate_bps` is the wire SSoT, and MC validates it at config load
      // against a code-owned band it refuses to start without satisfying.
      bitrateBps: this.#directive?.bitrateBps ?? audio.defaultBitrateBps,
      frameDurationMs: audio.frameDurationMs,
      complexity: audio.opusComplexity,
      application: audio.opusApplication,
      signal: audio.opusSignal,
      useDtx: audio.opusUseDtx,
      useInbandFec: audio.opusUseInbandFec,
      packetLossPerc: audio.opusPacketLossPerc,
      onOutput: (frame) => {
        void this.#egress?.submit(frame).catch(() => {
          this.#fault(MediaFaultStage.Crypto, 'a media frame could not be built or signed', false);
        });
      },
      onError: () => {
        this.#fault(MediaFaultStage.Encoder, 'the audio encoder failed; capture has stopped', true);
      },
    });
    this.#encoder = encoder;
    this.#teardown.register('encoder', () => encoder.close());

    this.#ingress = new IngressPipeline({
      metrics: this.#metrics,
      roster: this.#options.roster,
      keys: this.#options.kekSource,
      cache: this.#cache,
      replay: this.#replay,
      hopMonitor: this.#hopMonitor,
      firstMedia: this.#firstMedia,
      onAccepted: (frame) => this.emit('frameAccepted', frame),
    });
    this.#ingress.setDecoder(decoder);

    // Receive-side state is cleared at teardown: ADR-0028 §5's explicit cleanup,
    // whose wiring the codec task deliberately left to this task.
    this.#teardown.register('receiver-state', () => {
      this.#cache.clear();
      this.#replay.clear();
      this.#hopMonitor.clear();
      this.#firstMedia.clear();
    });
    this.#teardown.register('transmit-keys', () => this.#transmitKeys.clear());

    // Sampled FROM THE START, before any datagram can arrive, so the measurement
    // exists for every session rather than only for lucky ones.
    this.#firstMedia.start();

    await capture.start(
      (data) => this.#onCapturedFrame(data),
      () => this.#onCaptureEnded(),
    );

    this.#rotationTimer = this.#setInterval(() => {
      // §4: senders rotate every T for audio, because rotation is O(1),
      // involves nobody else, and needs no signalling.
      this.#transmitKeys.rotate();
    }, this.#config.keys.audioRotationPeriodMs);
    const timer = this.#rotationTimer;
    this.#teardown.register('rotation-timer', () => this.#clearInterval(timer));

    this.#applyDirective();
    this.#applyReceive();
  }

  /**
   * Apply a send directive from MC.
   *
   * Resuming from an EMPTY target set rotates the transmit key (ADR-0036 §4).
   */
  setSendDirective(directive: AudioSendDirective): void {
    const wasEmpty = (this.#directive?.targets.length ?? 0) === 0;
    const isEmpty = directive.targets.length === 0;
    this.#directive = directive;
    if (wasEmpty && !isEmpty) this.#transmitKeys.rotate();
    this.#applyDirective();
  }

  /**
   * Apply the handler URLs from MC's latest `StreamAssignments` — one per
   * declared slot, EMPTY for a slot with no source (the proto's "Empty when no
   * source is assigned": every `fewer_sources` slot, and every slot of a solo
   * participant). This, not the send directive, is where receiving comes from.
   *
   * ---------------------------------------------------------------------------
   * A SELECTOR OVER CONNECTED TRANSPORTS, NEVER A DIAL TARGET
   * ---------------------------------------------------------------------------
   *
   * `readableFor` resolves through `MediaTransport.getDatagramChannel`, which
   * only looks up transports `connectAll` already connected and returns
   * `undefined` for anything else. `connectAll(joined.mediaServers)` is the SOLE
   * dial site in the SDK (`#connectedTransports.set` is reached only from its
   * fan-out), and THAT is what makes MC's scoping of `JoinResponse.media_servers`
   * the single control on what this client ever connects to. An assignment URL
   * can pick among those transports; it can never create one. Any future write
   * to `#connectedTransports` outside the `connectAll` path breaks this property
   * and reopens the active/active fan-out MC's scoping exists to prevent.
   *
   * MORE THAN ONE DISTINCT HANDLER is a fault, and nothing is opened for it.
   * MC places each participant on exactly one handler this story (multi-handler
   * send is story 6), so two URLs mean MC broke its own scoping invariant. This
   * is the far-end DETECTOR for that server-side break — not a guard against
   * dialing, which is structurally impossible here — so do not remove it on the
   * grounds that nothing could be dialed anyway.
   */
  setReceiveHandlers(mediaHandlerUrls: readonly string[]): void {
    if (this.#stopped) return;
    const distinct = new Set(mediaHandlerUrls.filter((url) => url !== ''));
    if (distinct.size > 1) {
      this.#fault(
        MediaFaultStage.Transport,
        'slot assignments name more than one media handler',
        false,
      );
      return;
    }
    const [url] = distinct;
    // No source in any slot: nothing to receive yet. A running loop stays up,
    // so a later refill needs no reconnect.
    if (url === undefined) return;
    this.#receiveUrl = url;
    this.#applyReceive();
  }

  /**
   * Set client mute.
   *
   * Local first, always: the state changes before MC is told, and unmute resumes
   * capture with NO server round trip. MC keeps the send directive active
   * throughout, so "MC has not asked you to send" and "you have muted yourself"
   * stay distinguishable.
   */
  setAudioMuted(muted: boolean): void {
    if (!this.#mute.setAudioMuted(muted)) return;
    this.#metrics.muteTransition(muted ? MEDIA_MUTE_ACTIONS.Mute : MEDIA_MUTE_ACTIONS.Unmute);
    // Resuming from silence is a resume-from-empty for this stream: bump the
    // generation, which bounds a leaked transmit key across the muted window.
    if (!muted) this.#transmitKeys.rotate();
    this.emit('muteChanged', this.#mute.snapshot());
    // Informational, and deliberately last. A failure to report must not undo
    // the local suppression: ADR-0036 §5 requires client mute to hold without
    // the server honouring it.
    this.#options.reportMute(muted);
  }

  /**
   * Choose the microphone, before or during a session.
   *
   * Before `start()` this records the choice, which `start()` then applies.
   * While running it performs a REAL swap — stop the old capture, acquire the
   * new one, resume feeding the same encoder. A device picker whose selection
   * silently did nothing after start would be a masked failure wearing a UI
   * disguise, which is the thing this story's whole failure mode is made of.
   *
   * What deliberately does NOT change across a swap: the encoder, decoder and
   * playback sink (same sample rate and channel count, so rebuilding them would
   * only add a gap), the mute state (checked per frame, so it survives), the
   * transmit keys, and the first-media measurement epoch — a device change is
   * not a new session and must not restart a measurement that means "time to
   * first media after media start".
   *
   * @throws {MediaCaptureError} if the new device cannot be acquired. The old
   * capture is already stopped by then, so the caller is told loudly rather than
   * left believing a dead pipeline is live; a fault is also raised so a UI that
   * ignores the rejection still sees it.
   */
  async setCaptureDevice(deviceId: string | undefined): Promise<void> {
    if (this.#deviceId === deviceId) return;
    this.#deviceId = deviceId;
    if (!this.#started || this.#stopped) return;

    const audio = this.#config.audio;
    // Stop FIRST. Two live captures means two lit microphone indicators and two
    // frame sources feeding one encoder, which reorders the encoder's input.
    this.#capture?.stop();
    this.#capture = undefined;
    let capture: CaptureSource;
    try {
      capture = await this.#options.captureFactory({
        sampleRateHz: audio.sampleRateHz,
        channels: audio.channels,
        deviceId,
      });
    } catch (err) {
      this.#fault(
        MediaFaultStage.Capture,
        'the selected microphone could not be opened; capture has stopped',
        true,
      );
      throw err;
    }
    // Assigned BEFORE the next await, same rule as `start()`: the single
    // registered disposer reads this field, so an acquisition that completes
    // after teardown is still released.
    this.#capture = capture;
    if (this.#stopped) {
      capture.stop();
      this.#capture = undefined;
      return;
    }
    await capture.start(
      (data) => this.#onCapturedFrame(data),
      () => this.#onCaptureEnded(),
    );
  }

  /** Stop everything. Idempotent, and safe to call during `start()`. */
  async stop(): Promise<void> {
    if (this.#stopped) return;
    this.#stopped = true;
    this.#egress?.stop();
    this.#ingress?.setDecoder(undefined);
    const failures = await this.#teardown.dispose();
    for (const failure of failures) {
      // Surfaced, not swallowed. The NAME is a static string chosen at
      // registration; the error itself is deliberately not interpolated,
      // because a platform message on this path could carry anything.
      this.#fault(
        MediaFaultStage.Transport,
        `media teardown step '${failure.name}' failed; the resource may not have been released`,
        false,
      );
    }
  }

  // ------------------------------------------------------------------------

  /**
   * CLIENT MUTE IS ENFORCED HERE, AT CAPTURE.
   *
   * While muted the frame is released and never reaches the encoder, so no
   * encoded audio exists to leave the device — enforcement does not depend on
   * the server honouring anything, and it stops within one frame because the
   * check happens before the encoder sees the sample. Unmute resumes on the very
   * next captured frame with no round trip.
   */
  /**
   * The capture track ended on its own. Shared by `start()` and
   * `setCaptureDevice()` so a device acquired by the swap reports an unplug
   * exactly as the original does — a second, subtly different handler is how a
   * swapped device ends up failing silently.
   */
  #onCaptureEnded(): void {
    this.#fault(
      MediaFaultStage.Capture,
      'the microphone stopped; the device may have been unplugged or access revoked',
      true,
    );
  }

  #onCapturedFrame(data: AudioData): void {
    if (this.#mute.audioMuted || this.#stopped) {
      // `AudioData` holds a platform-side buffer; a suppressed frame must still
      // be released or a long mute leaks at 50 frames a second.
      data.close();
      return;
    }
    try {
      this.#encoder?.encode(data);
    } finally {
      data.close();
    }
  }

  /**
   * Attach the egress pipeline to MC's directed handler.
   *
   * SEND ONLY. The read loop is NOT started from here: a participant can hold
   * filled receive slots while being in nobody's slot (static fill, N=1, three
   * participants: C's slot holds A, nobody's holds C), and MC then correctly
   * directs an EMPTY target set (§5 "send nothing"). Receiving keyed off the send
   * target would leave C deaf behind an ACTIVE slot. See `setReceiveHandlers`.
   */
  #applyDirective(): void {
    const directive = this.#directive;
    if (!directive || this.#stopped) return;
    const [target] = directive.targets;
    if (target === undefined) {
      // "A target set may be empty. That means send nothing." (§5)
      this.#egress?.setSender(undefined);
      return;
    }

    if (!this.#egress) {
      this.#egress = new EgressPipeline({
        metrics: this.#metrics,
        transmitKeys: this.#transmitKeys,
        kek: { source: this.#options.kekSource, generation: this.#options.kekGeneration },
        identity: this.#options.identity,
        maxQueueFrames: this.#config.egress.maxQueueFrames,
        streamNumber: directive.streamNumber,
        // The relay `stream_id` this client stamps on its uplink. The declared
        // slot id, so the value space matches what MH routes on.
        streamId: this.#options.declaredSlotIds[0] ?? 0,
      });
      this.#teardown.register('egress', () => this.#egress?.stop());
    }
    this.#egress.setSender(this.#options.senderFor(target));
  }

  /** Start the read loop on the assigned handler, once, after `start()`. */
  #applyReceive(): void {
    const url = this.#receiveUrl;
    // Before `start()` there is no ingress to hand frames to; `start()` calls
    // this again. Never read and discard.
    if (url === undefined || !this.#ingress || this.#stopped) return;
    if (this.#readLoopUrl !== undefined) {
      if (url !== this.#readLoopUrl) {
        // MC's placement is frozen per participant (reconnect keeps the
        // handler), so a DIFFERENT handler mid-session is an MC defect. Say so;
        // do not tear down a working loop to follow it.
        this.#fault(
          MediaFaultStage.Transport,
          'slot assignments moved to a different media handler mid-session',
          false,
        );
      }
      return;
    }
    const readable = this.#options.readableFor(url);
    if (!readable) {
      this.#fault(
        MediaFaultStage.Transport,
        'slot assignments name a media handler this client is not connected to',
        false,
      );
      return;
    }
    this.#readLoopUrl = url;
    this.#startReadLoop(readable);
  }

  #startReadLoop(readable: ReadableStream<Uint8Array>): void {
    const reader = readable.getReader();
    this.#teardown.register('datagram-reader', () => {
      void reader.cancel().catch(() => {
        // Already cancelled or errored; the transport close is what matters.
      });
    });
    void this.#readLoop(reader);
  }

  async #readLoop(reader: ReadableStreamDefaultReader<Uint8Array>): Promise<void> {
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) {
          if (!this.#stopped) {
            // Absence of frames is not a signal (ADR-0036 §6): a datagram stream
            // that ends while the session continues is a real condition and is
            // surfaced rather than read as silence.
            this.#fault(
              MediaFaultStage.Transport,
              'the inbound media datagram stream ended while the session was still active',
              false,
            );
          }
          return;
        }
        if (value === undefined) continue;
        try {
          await this.#ingress?.accept(value);
        } catch {
          // `accept` returns normally for every WIRE condition; reaching here
          // means a defect. Surfaced ONCE — a repeating defect must not become
          // per-frame telemetry — and the loop continues, because one bad frame
          // must not end media for the session.
          this.#fault(MediaFaultStage.Crypto, 'a media frame could not be processed', false);
        }
      }
    } catch {
      if (this.#stopped) return;
      this.#fault(MediaFaultStage.Transport, 'the media datagram reader failed', false);
    } finally {
      try {
        reader.releaseLock();
      } catch {
        // Already released during teardown.
      }
    }
  }

  /**
   * Emit a fault at most once per (stage, message). Bounded, never per frame.
   *
   * `message` is part of the dedupe key, so it MUST come from a fixed set: a
   * literal, or interpolation of static names only. Never a platform error
   * string or any per-frame value — that would make a repeating fault unbounded.
   */
  #fault(stage: MediaFaultStage, message: string, fatal: boolean): void {
    const key = `${stage}\u0000${message}`;
    if (this.#reportedFaults.has(key)) return;
    this.#reportedFaults.add(key);
    this.emit('fault', { stage, message, fatal });
    if (fatal) void this.stop();
  }
}
