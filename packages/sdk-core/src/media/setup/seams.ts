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
 * Where decoded audio goes.
 *
 * `enqueue` is a per-frame method, so an implementation must not log — see
 * `pipeline/`'s layout rule, which applies to any code the frame path reaches
 * regardless of which directory it is declared in.
 */
export interface PlaybackSink {
  /**
   * Hand a decoded frame to playback. Takes OWNERSHIP: the implementation is
   * responsible for calling `close()` on the `AudioData`.
   */
  enqueue(data: AudioData): void;
  /** Release the sink. Idempotent. */
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
