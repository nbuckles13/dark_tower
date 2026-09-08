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
  CaptureSourceFactory,
  PlaybackSinkFactory,
} from '../setup/seams.js';
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
  /** Inbound datagrams for the assigned media handler. */
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
   * Whether the inbound read loop is running.
   *
   * MC re-issues a send directive whenever meeting state changes, so
   * `#applyDirective` runs many times per session. A `ReadableStream` can be
   * locked by only ONE reader, so a second `getReader()` throws — and it would
   * throw from inside a directive handler, where the failure would present as
   * "media stopped after someone joined" rather than as what it is.
   */
  #readLoopStarted = false;
  #directive: AudioSendDirective | undefined;
  #started = false;
  #stopped = false;
  /** Event-once flags: a repeating fault must not become per-frame telemetry. */
  readonly #reportedFaults = new Set<MediaFaultStage>();

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
    this.#hopMonitor = new HopSequenceMonitor(options.declaredSlotIds);
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
    const capture = await this.#options.captureFactory({
      sampleRateHz: audio.sampleRateHz,
      channels: audio.channels,
      deviceId: options.deviceId,
    });
    // REGISTERED BEFORE THE NEXT AWAIT: a leaked capture keeps the microphone
    // hot and the browser's recording indicator lit.
    this.#teardown.register('capture', () => capture.stop());

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
      () => {
        this.#fault(
          MediaFaultStage.Capture,
          'the microphone stopped; the device may have been unplugged or access revoked',
          true,
        );
      },
    );

    this.#rotationTimer = this.#setInterval(() => {
      // §4: senders rotate every T for audio, because rotation is O(1),
      // involves nobody else, and needs no signalling.
      this.#transmitKeys.rotate();
    }, this.#config.keys.audioRotationPeriodMs);
    const timer = this.#rotationTimer;
    this.#teardown.register('rotation-timer', () => this.#clearInterval(timer));

    this.#applyDirective();
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

  /** Attach the egress pipeline and the read loop to MC's assigned handler. */
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

    // Started ONCE. See `#readLoopStarted`.
    if (!this.#readLoopStarted) {
      const readable = this.#options.readableFor(target);
      if (readable) {
        this.#readLoopStarted = true;
        this.#startReadLoop(readable);
      }
    }
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

  /** Emit a fault at most once per stage. Bounded, never per frame. */
  #fault(stage: MediaFaultStage, message: string, fatal: boolean): void {
    if (this.#reportedFaults.has(stage)) return;
    this.#reportedFaults.add(stage);
    this.emit('fault', { stage, message, fatal });
    if (fatal) void this.stop();
  }
}
