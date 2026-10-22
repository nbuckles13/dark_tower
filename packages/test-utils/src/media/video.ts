// File: packages/test-utils/src/media/video.ts
//
// Test doubles for the SDK's VIDEO RECEIVE seams (story 3 C-B): a VP8 decoder
// factory and `VideoFrame`s with a close counter.
//
// DETERMINISTIC BY CONSTRUCTION. Nothing here runs on a timer or an unawaited
// microtask: a decoder produces an output ONLY when the test calls
// `emitOutput()`, and fails ONLY when the test calls `emitError()`. So
// attribution-through-the-decoder, decoder-error fields and fault isolation are
// tested at a FIXED interleaving the test chooses.
//
// Like the audio doubles, these replace the MEDIA CODEC only and sit strictly
// outside the frame boundary: they consume the plaintext `openSframe` produced.
// Nothing here fakes frame crypto.

/** One chunk handed to a fake decoder. */
export interface FakeVideoChunk {
  readonly data: Uint8Array;
  readonly timestampUs: number;
  readonly isKeyframe: boolean;
}

/** A structural `VideoFrame` stand-in that counts `close()` calls. */
export class FakeVideoFrame {
  readonly timestamp: number;
  readonly codedWidth: number;
  readonly codedHeight: number;
  closeCount = 0;

  constructor(timestamp: number, codedWidth: number, codedHeight: number) {
    this.timestamp = timestamp;
    this.codedWidth = codedWidth;
    this.codedHeight = codedHeight;
  }

  close(): void {
    this.closeCount += 1;
  }
}

/**
 * Every fake frame ever created, so a test (or an `afterEach`) can assert that
 * EACH was closed exactly once — including frames the test never looked at.
 */
export class FakeVideoFrameRegistry {
  readonly frames: FakeVideoFrame[] = [];

  make(timestamp: number, codedWidth: number, codedHeight: number): FakeVideoFrame {
    const frame = new FakeVideoFrame(timestamp, codedWidth, codedHeight);
    this.frames.push(frame);
    return frame;
  }

  /** Frames whose close count is not exactly 1. Empty = every frame closed once. */
  notClosedExactlyOnce(): FakeVideoFrame[] {
    return this.frames.filter((f) => f.closeCount !== 1);
  }
}

/** One fake decoder instance. */
export interface FakeVideoDecoderRecord {
  /** The configuration the SDK built (from the slot's bounds). */
  readonly config: unknown;
  /** Chunks handed to THIS decoder, in order. */
  readonly decoded: FakeVideoChunk[];
  /** How many of `decoded` have produced an output (or been skipped). */
  delivered: number;
  closed: boolean;
  /**
   * When set, overrides the reported `decodeQueueSize` (default: chunks decoded
   * and not yet delivered).
   */
  queueSizeOverride: number | undefined;
  /**
   * Output the NEXT undelivered chunk as a decoded frame (or, with `timestampUs`,
   * a frame carrying that timestamp — to test unmatched / out-of-order output).
   * Returns the frame handed to the SDK.
   */
  emitOutput(options?: {
    readonly timestampUs?: number;
    readonly width?: number;
    readonly height?: number;
  }): FakeVideoFrame;
  /** Fire THIS decoder's error callback. */
  emitError(err?: unknown): void;
  /**
   * When set, the NEXT `decode()` throws this synchronously (WebCodecs'
   * InvalidStateError / DataError), then clears.
   */
  throwOnNextDecode: unknown;
}

/** The factory double. `constructions` is the R-6 test seam (exactly M; a switch builds none). */
export class FakeVideoDecoders {
  readonly decoders: FakeVideoDecoderRecord[] = [];
  readonly registry: FakeVideoFrameRegistry;
  /** Default decoded geometry. */
  width: number;
  height: number;
  /** When set, the NEXT factory call throws this (a decoder that cannot be built). */
  failNextConstruction: unknown = undefined;

  constructor(
    options: { width?: number; height?: number; registry?: FakeVideoFrameRegistry } = {},
  ) {
    this.width = options.width ?? 640;
    this.height = options.height ?? 360;
    this.registry = options.registry ?? new FakeVideoFrameRegistry();
  }

  /** Decoder constructions so far (including failed ones). */
  get constructions(): number {
    return this.decoders.length + this.#failedConstructions;
  }

  #failedConstructions = 0;

  /** The most recently constructed decoder. */
  get last(): FakeVideoDecoderRecord | undefined {
    return this.decoders[this.decoders.length - 1];
  }

  factory = (options: {
    readonly config: unknown;
    readonly onOutput: (frame: never) => void;
    readonly onError: (err: unknown) => void;
  }): { decode(chunk: FakeVideoChunk): void; readonly decodeQueueSize: number; close(): void } => {
    if (this.failNextConstruction !== undefined) {
      const err = this.failNextConstruction;
      this.failNextConstruction = undefined;
      this.#failedConstructions += 1;
      throw err;
    }
    const onOutput = options.onOutput as unknown as (frame: FakeVideoFrame) => void;
    const record: FakeVideoDecoderRecord = {
      config: options.config,
      decoded: [],
      delivered: 0,
      closed: false,
      queueSizeOverride: undefined,
      throwOnNextDecode: undefined,
      emitOutput: (o = {}) => {
        const chunk = record.decoded[record.delivered];
        const timestamp = o.timestampUs ?? chunk?.timestampUs ?? -1;
        if (o.timestampUs === undefined && chunk) record.delivered += 1;
        const frame = this.registry.make(timestamp, o.width ?? this.width, o.height ?? this.height);
        onOutput(frame);
        return frame;
      },
      emitError: (err: unknown = new Error('decoder failed')) => options.onError(err),
    };
    this.decoders.push(record);
    return {
      decode: (chunk: FakeVideoChunk) => {
        if (record.closed) throw new Error('decode on a closed decoder');
        if (record.throwOnNextDecode !== undefined) {
          const err = record.throwOnNextDecode;
          record.throwOnNextDecode = undefined;
          throw err;
        }
        record.decoded.push({
          data: Uint8Array.from(chunk.data),
          timestampUs: chunk.timestampUs,
          isKeyframe: chunk.isKeyframe,
        });
      },
      get decodeQueueSize(): number {
        return record.queueSizeOverride ?? record.decoded.length - record.delivered;
      },
      close: () => {
        record.closed = true;
      },
    };
  };
}
