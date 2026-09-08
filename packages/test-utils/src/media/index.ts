// File: packages/test-utils/src/media/index.ts
//
// Test doubles for the SDK's audio media seams.
//
// ---------------------------------------------------------------------------
// WHAT THESE MAY AND MAY NOT REPLACE
// ---------------------------------------------------------------------------
//
// `FakeAudioCodecs` replaces the OPUS MEDIA CODEC and nothing else. It sits
// strictly OUTSIDE the frame boundary: it produces the plaintext that goes into
// `sealSframe` and consumes the plaintext that comes out of `openSframe`. It is
// never reachable from a verify path.
//
// **Nothing here fakes frame crypto.** Ed25519 verification, the KEK unwrap and
// the SFrame decrypt are the real primitives over WebCrypto in every test. A
// test whose verify path could return `true` without real signature math would
// be worse than no test: the loopback this story ships is SELF-CONSISTENT under
// a swapped associated data, an inverted nonce, a truncated key id, or a
// signature over the wrong range, so a faked verifier would make a green suite
// evidence of nothing.
//
// These doubles exist because the SDK's unit tier runs under
// `environment: 'node'`, where `getUserMedia`, `AudioEncoder`, `AudioDecoder`,
// `MediaStreamTrackProcessor` and `AudioContext` do not exist.

/** Minimal structural mirror of WebCodecs' `AudioData`, sufficient for the seams. */
export interface FakeAudioData {
  readonly format: 'f32-planar';
  readonly sampleRate: number;
  readonly numberOfFrames: number;
  readonly numberOfChannels: number;
  readonly duration: number;
  readonly timestamp: number;
  copyTo(destination: Float32Array, options: { planeIndex: number; format?: string }): void;
  close(): void;
}

/** One encoded frame, mirroring the SDK's `EncodedAudioFrame`. */
export interface FakeEncodedFrame {
  readonly data: Uint8Array;
  readonly timestampUs: number;
}

/** Build a synthetic `AudioData`-shaped frame carrying `samples`. */
export function makeAudioData(options: {
  samples: Float32Array;
  sampleRate?: number;
  channels?: number;
  timestamp?: number;
  onClose?: () => void;
}): FakeAudioData {
  const sampleRate = options.sampleRate ?? 48_000;
  const channels = options.channels ?? 1;
  let closed = false;
  return {
    format: 'f32-planar',
    sampleRate,
    numberOfChannels: channels,
    numberOfFrames: options.samples.length,
    duration: (options.samples.length / sampleRate) * 1e6,
    timestamp: options.timestamp ?? 0,
    copyTo(destination: Float32Array): void {
      destination.set(options.samples.subarray(0, destination.length));
    },
    close(): void {
      // Tracked so a test can assert the pipeline RELEASES every frame — a
      // leaked `AudioData` holds a platform buffer, and at 50 frames a second a
      // leak on the mute path is a real resource fault rather than a nit.
      if (closed) return;
      closed = true;
      options.onClose?.();
    },
  };
}

/** A capture source a test drives by hand. */
export class FakeCaptureSource {
  #onFrame: ((data: FakeAudioData) => void) | undefined;
  #onEnded: (() => void) | undefined;
  /** How many times `stop()` was called. Teardown is asserted on this. */
  stopCount = 0;
  /** Whether `start()` has run. */
  started = false;

  async start(onFrame: (data: never) => void, onEnded: () => void): Promise<void> {
    this.started = true;
    this.#onFrame = onFrame as (data: FakeAudioData) => void;
    this.#onEnded = onEnded;
  }

  stop(): void {
    this.stopCount += 1;
    this.#onFrame = undefined;
  }

  /** Deliver one captured frame. Returns `false` if capture is not running. */
  emit(data: FakeAudioData): boolean {
    if (!this.#onFrame) return false;
    this.#onFrame(data);
    return true;
  }

  /** Simulate the device disappearing (unplugged, revoked, taken by the OS). */
  endTrack(): void {
    this.#onEnded?.();
  }
}

/**
 * A deterministic "codec" whose encode is the identity function.
 *
 * Identity encoding is what lets a loopback test assert an EXACT plaintext round
 * trip — the bytes that go in are the bytes that must come out — without the
 * assertion depending on a real codec's tolerances. It replaces Opus only; every
 * byte between encode and decode still travels through the real frame codec and
 * the real crypto.
 */
export class FakeAudioCodecs {
  /** Frames the encoder emitted, in order. */
  readonly encoded: FakeEncodedFrame[] = [];
  /** Frames handed to the decoder, in order. */
  readonly decoded: FakeEncodedFrame[] = [];
  #onEncoded: ((frame: FakeEncodedFrame) => void) | undefined;
  #onDecoded: ((data: FakeAudioData) => void) | undefined;
  #encoderError: ((err: unknown) => void) | undefined;
  #decoderError: ((err: unknown) => void) | undefined;
  encoderClosed = 0;
  decoderClosed = 0;

  encoderFactory = async (options: {
    onOutput: (frame: FakeEncodedFrame) => void;
    onError: (err: unknown) => void;
  }): Promise<{ encode(data: never): void; close(): void }> => {
    this.#onEncoded = options.onOutput;
    this.#encoderError = options.onError;
    return {
      encode: (data: never) => {
        const audio = data as unknown as FakeAudioData;
        const bytes = new Uint8Array(audio.numberOfFrames);
        const scratch = new Float32Array(audio.numberOfFrames);
        audio.copyTo(scratch, { planeIndex: 0 });
        for (let i = 0; i < scratch.length; i += 1) {
          bytes[i] = (scratch[i] ?? 0) & 0xff;
        }
        const frame: FakeEncodedFrame = { data: bytes, timestampUs: audio.timestamp };
        this.encoded.push(frame);
        this.#onEncoded?.(frame);
      },
      close: () => {
        this.encoderClosed += 1;
      },
    };
  };

  decoderFactory = async (options: {
    sampleRateHz: number;
    channels: number;
    onOutput: (data: never) => void;
    onError: (err: unknown) => void;
  }): Promise<{ decode(frame: FakeEncodedFrame): void; close(): void }> => {
    this.#onDecoded = options.onOutput as unknown as (data: FakeAudioData) => void;
    this.#decoderError = options.onError;
    return {
      decode: (frame: FakeEncodedFrame) => {
        this.decoded.push({ data: Uint8Array.from(frame.data), timestampUs: frame.timestampUs });
        const samples = new Float32Array(frame.data.length);
        for (let i = 0; i < frame.data.length; i += 1) samples[i] = frame.data[i] ?? 0;
        this.#onDecoded?.(
          makeAudioData({
            samples,
            sampleRate: options.sampleRateHz,
            channels: options.channels,
            timestamp: frame.timestampUs,
          }),
        );
      },
      close: () => {
        this.decoderClosed += 1;
      },
    };
  };

  /** Fire the encoder's terminal error callback. */
  failEncoder(err: unknown = new Error('encoder failed')): void {
    this.#encoderError?.(err);
  }

  /** Fire the decoder's terminal error callback. */
  failDecoder(err: unknown = new Error('decoder failed')): void {
    this.#decoderError?.(err);
  }
}

/** A playback sink that records what it was handed and whether it released it. */
export class RecordingPlaybackSink {
  readonly played: Uint8Array[] = [];
  /** Frames handed to `enqueue` that were NOT `close()`d. Must stay empty. */
  leaked = 0;
  closeCount = 0;

  enqueue(data: never): void {
    const audio = data as unknown as FakeAudioData;
    const samples = new Float32Array(audio.numberOfFrames);
    audio.copyTo(samples, { planeIndex: 0 });
    const bytes = new Uint8Array(samples.length);
    for (let i = 0; i < samples.length; i += 1) bytes[i] = (samples[i] ?? 0) & 0xff;
    this.played.push(bytes);
    this.leaked += 1;
    audio.close();
    this.leaked -= 1;
  }

  close(): void {
    this.closeCount += 1;
  }

  factory = async (): Promise<{ enqueue(data: never): void; close(): void }> => this;
}
