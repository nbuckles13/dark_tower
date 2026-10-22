// File: packages/sdk-core/src/media/setup/seams.ts
//
// The injectable seams the audio pipeline is built from.
//
// ---------------------------------------------------------------------------
// WHY EVERY BROWSER MEDIA API SITS BEHIND ONE OF THESE
// ---------------------------------------------------------------------------
//
// ADR-0036 §10 makes the transport and measurement seams "prerequisites, not
// refinements", for a reason that generalises to capture and codecs here: the
// SDK's unit tier runs under `environment: 'node'`, where `getUserMedia`,
// `AudioEncoder`, `AudioDecoder`, `MediaStreamTrackProcessor` and `AudioContext`
// do not exist. Without seams the pipeline would be unit-testable only in a
// browser, which means the drop paths, the reject reasons and the conservation
// identity would be reachable only in an environment where asserting them is
// slow and flaky — and per ADR-0028 a flaky gate becomes a deleted gate.
//
// These are TYPE declarations only. The production implementations live in the
// sibling modules (`capture.ts`, `opus.ts`, `audioPlayback.ts`); the test doubles
// live in `@darktower/test-utils`.

/**
 * One encoded audio frame leaving the encoder.
 *
 * Deliberately a plain shape rather than `EncodedAudioChunk`: the chunk's bytes
 * are only reachable through `copyTo`, and the pipeline wants the bytes. Framing
 * the seam in terms of what the pipeline needs keeps the copy at the boundary
 * where it is unavoidable, rather than inside the frame path.
 */
export interface EncodedAudioFrame {
  /** Raw Opus packet bytes. */
  readonly data: Uint8Array;
  /** Presentation timestamp, in microseconds. */
  readonly timestampUs: number;
}

/** An audio encoder, as the egress path uses one. */
export interface AudioEncoderSeam {
  /** Submit a captured frame. Output arrives on the factory's `onOutput`. */
  encode(data: AudioData): void;
  /** Release the encoder. Idempotent. */
  close(): void;
}

/** An audio decoder, as the ingress path uses one. */
export interface AudioDecoderSeam {
  /** Submit an encoded frame. Output arrives on the factory's `onOutput`. */
  decode(frame: EncodedAudioFrame): void;
  /** Release the decoder. Idempotent. */
  close(): void;
}

/**
 * One sender's decoded audio, on its own timeline.
 *
 * `enqueue` is a per-frame method, so an implementation must not log — see
 * `pipeline/`'s layout rule, which applies to any code the frame path reaches
 * regardless of which directory it is declared in.
 */
export interface PlaybackLane {
  /**
   * Hand a decoded frame to playback. Takes OWNERSHIP: the implementation is
   * responsible for calling `close()` on the `AudioData`.
   */
  enqueue(data: AudioData): void;
  /** Release this lane only. Idempotent; other lanes keep playing. */
  close(): void;
  /**
   * This lane's own output node, PRE-mix and post-decode — where its audio
   * enters the shared output. Present on the `AudioContext` sink; absent on
   * implementations with no audio graph (test doubles).
   *
   * An OBSERVATION point only: an embedder may attach an analyser to it (the
   * E2E build's per-slot tone analysis, `packages/web-app/src/lib/e2eAnalysis.ts`)
   * but nothing in the SDK reads it, and connecting it anywhere else would
   * duplicate the lane's audio.
   */
  readonly output?: AudioNode;
}

/**
 * Where decoded audio goes: ONE output, MIXING any number of sender lanes.
 *
 * WHY LANES, AND NOT ONE `enqueue`: a single scheduled timeline places every
 * frame after the previous one. With N senders each producing 50 frames a second
 * into one 50-frames-a-second playhead, the senders play one AFTER another
 * rather than together, and the schedule runs further ahead of real time with
 * every frame — unbounded latency growth that reads as "audio getting later and
 * later". Each lane has its own timeline; the shared output node sums them,
 * which is the mix.
 */
export interface PlaybackSink {
  /**
   * Open a new lane on this output. One per actively decoded sender.
   *
   * `senderId` is the lane's verified key-id sender. The SDK's own sink ignores
   * it; it exists so an embedder-wrapped sink can tell lanes apart (the E2E
   * build keys its per-slot analysis by it). Never a metric label, never logged.
   */
  openLane(senderId: number): PlaybackLane;
  /** Release the sink and every lane on it. Idempotent. */
  close(): void;
}

/** Microphone capture, behind a device-selection seam. */
export interface CaptureSource {
  /**
   * Start capturing. `onFrame` receives each captured `AudioData` and takes
   * ownership of it.
   *
   * @param onEnded called if the underlying track ends (device unplugged, user
   * revoked permission, OS took the device). Never silently absorbed.
   */
  start(onFrame: (data: AudioData) => void, onEnded: () => void): Promise<void>;
  /**
   * Stop capturing and release the device.
   *
   * MUST be reached even from a partially-constructed pipeline: a leaked track
   * keeps the microphone hot and the browser's recording indicator lit, which is
   * a user-visible privacy failure rather than a memory nit.
   */
  stop(): void;
}

/** Construction options for an encoder. */
export interface AudioEncoderSeamOptions {
  readonly sampleRateHz: number;
  readonly channels: number;
  readonly bitrateBps: number;
  readonly frameDurationMs: number;
  readonly complexity: number;
  readonly application: string;
  readonly signal: string;
  readonly useDtx: boolean;
  readonly useInbandFec: boolean;
  readonly packetLossPerc: number;
  /** Called for each encoded frame. */
  readonly onOutput: (frame: EncodedAudioFrame) => void;
  /** Called on the codec's error callback. Bounded/event-once, never per frame. */
  readonly onError: (err: unknown) => void;
}

/** Construction options for a decoder. */
export interface AudioDecoderSeamOptions {
  readonly sampleRateHz: number;
  readonly channels: number;
  /** Called for each decoded frame. Takes ownership of the `AudioData`. */
  readonly onOutput: (data: AudioData) => void;
  /** Called on the codec's error callback. Bounded/event-once, never per frame. */
  readonly onError: (err: unknown) => void;
}

/** Builds an encoder. Injected so unit tests never touch WebCodecs. */
export type AudioEncoderFactory = (options: AudioEncoderSeamOptions) => Promise<AudioEncoderSeam>;

/** Builds a decoder. Injected so unit tests never touch WebCodecs. */
export type AudioDecoderFactory = (options: AudioDecoderSeamOptions) => Promise<AudioDecoderSeam>;

/** Builds a playback sink. Injected so unit tests never touch `AudioContext`. */
export type PlaybackSinkFactory = (options: {
  readonly sampleRateHz: number;
  readonly channels: number;
}) => Promise<PlaybackSink>;

/** Builds a capture source. Injected so unit tests never touch `getUserMedia`. */
export type CaptureSourceFactory = (options: {
  readonly sampleRateHz: number;
  readonly channels: number;
  /** `MediaDeviceInfo.deviceId` of the chosen microphone, or `undefined` for the default. */
  readonly deviceId: string | undefined;
}) => Promise<CaptureSource>;

// ---------------------------------------------------------------------------
// VIDEO (story 3 C-A2)
// ---------------------------------------------------------------------------

/**
 * MC's DIRECTED video encoding for one stream, validated
 * (`session/mediaWiring.ts:validateVideoEncoding`). Every field is required:
 * the SDK has no client default for any video encoding value.
 */
export interface DirectedVideoEncoding {
  /** The one video codec this story directs. */
  readonly codec: 'vp8';
  readonly width: number;
  readonly height: number;
  readonly frameRateHz: number;
  readonly maxBitrateBps: number;
  /** As directed. The publisher enforces max(this, the compiled floor). */
  readonly minKeyframeIntervalMs: number;
}

/** One encoded video frame leaving the encoder. */
export interface EncodedVideoFrame {
  /** Raw VP8 frame bytes. */
  readonly data: Uint8Array;
  /** Presentation timestamp, in microseconds. */
  readonly timestampUs: number;
  /** `true` for a key (independently decodable) chunk, solicited or not. */
  readonly key: boolean;
}

/** A video encoder, as the video egress path uses one. NO keyframe interval. */
export interface VideoEncoderSeam {
  /**
   * Submit a captured frame; `keyFrame: true` asks for a key chunk. The
   * encoder does NOT take ownership — the caller closes the frame.
   */
  encode(frame: VideoFrame, keyFrame: boolean): void;
  /** Frames handed to `encode` and not yet output (`VideoEncoder.encodeQueueSize`). */
  readonly encodeQueueSize: number;
  /** Release the encoder. Idempotent. */
  close(): void;
}

/** Construction options for a video encoder. */
export interface VideoEncoderSeamOptions {
  readonly encoding: DirectedVideoEncoding;
  /** Called for each encoded frame. */
  readonly onOutput: (frame: EncodedVideoFrame) => void;
  /** Called on the codec's error callback. Bounded/event-once, never per frame. */
  readonly onError: (err: unknown) => void;
}

/** Builds a video encoder (re-probing the directed config). Injected so unit tests never touch WebCodecs. */
export type VideoEncoderFactory = (options: VideoEncoderSeamOptions) => Promise<VideoEncoderSeam>;

/** Camera (or test-pattern) capture, behind the same seam shape as audio's. */
export interface VideoCaptureSource {
  /**
   * Start capturing. `onFrame` receives each captured `VideoFrame` and takes
   * ownership of it. `onEnded` fires if the track ends on its own (unplugged,
   * revoked). Rejects with a typed `MediaCaptureError` on acquisition failure.
   */
  start(onFrame: (frame: VideoFrame) => void, onEnded: () => void): Promise<void>;
  /** Stop capturing and release the device (camera light off). Idempotent. */
  stop(): void;
  /**
   * The device actually granted, once `start` resolved (`getSettings().deviceId`),
   * so a re-acquire reuses it rather than re-picking. `undefined` for a
   * synthetic source.
   */
  readonly grantedDeviceId: string | undefined;
}

/** Builds a video capture source at the DIRECTED geometry. */
export type VideoCaptureSourceFactory = (options: {
  readonly width: number;
  readonly height: number;
  readonly frameRateHz: number;
  /** The previously granted camera, or `undefined` for the platform default. */
  readonly deviceId: string | undefined;
}) => Promise<VideoCaptureSource>;

// ---------------------------------------------------------------------------
// VIDEO RECEIVE (story 3 C-B)
// ---------------------------------------------------------------------------

/**
 * One encoded video chunk entering a slot's decoder. `timestampUs` is the
 * slot's PLAYOUT timeline (receiver arrival time, re-based on a source switch —
 * lead ruling E1), never a sender value.
 */
export interface EncodedVideoChunkInput {
  /** Decrypted VP8 frame bytes. */
  readonly data: Uint8Array;
  readonly timestampUs: number;
  /** The SIGNED `independentlyDecodable` header flag. */
  readonly isKeyframe: boolean;
}

/** A video decoder, as one receive slot uses it for the slot's whole lifetime. */
export interface VideoDecoderSeam {
  /** Submit a chunk. Output arrives on the factory's `onOutput`. */
  decode(chunk: EncodedVideoChunkInput): void;
  /** Chunks submitted and not yet output (`VideoDecoder.decodeQueueSize`). */
  readonly decodeQueueSize: number;
  /** Release the decoder. Idempotent. */
  close(): void;
}

/** Construction options for a video decoder. */
export interface VideoDecoderSeamOptions {
  /**
   * The decoder configuration, built from the slot's CONFIGURED receive bounds
   * (`setup/vp8.ts:buildVp8DecoderConfig`), never from the bitstream or any
   * extension value (R-6).
   */
  readonly config: VideoDecoderConfig;
  /** Called for each decoded frame. Takes ownership of the `VideoFrame`. */
  readonly onOutput: (frame: VideoFrame) => void;
  /** Called on the codec's error callback. The slot closes and replaces this decoder. */
  readonly onError: (err: unknown) => void;
}

/**
 * Builds a slot's video decoder. SYNCHRONOUS (WebCodecs construction and
 * `configure` are; failures arrive on `onError`). Injected so unit tests never
 * touch WebCodecs — and so a test can COUNT constructions (R-6: a source switch
 * constructs none; exactly M exist).
 */
export type VideoDecoderFactory = (options: VideoDecoderSeamOptions) => VideoDecoderSeam;

/**
 * Where one video slot's frames are drawn, supplied by the embedder per slot
 * (`MeetingSession.videoReceive.setVideoSink`). Frame delivery NEVER passes through events
 * or reactive stores.
 *
 * BORROW CONTRACT: `drawFrame` borrows the frame for the duration of the call.
 * The SDK owns it and closes it exactly once (on replacement, avatar switch or
 * teardown). An embedder that must keep a frame past the call MUST
 * `frame.clone()` it and close its clone. A sink that throws does not change
 * ownership: the SDK still closes the frame.
 */
export interface VideoSink {
  drawFrame(frame: VideoFrame): void;
}
