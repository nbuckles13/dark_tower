// File: packages/sdk-core/src/media/setup/vp8.ts
//
// WebCodecs VP8 capability probe and encoder construction (story 3 R-8).
// Mirrors `./opus.ts`.
//
// ---------------------------------------------------------------------------
// FAIL LOUDLY, NEVER DEGRADE TO AUDIO-ONLY
// ---------------------------------------------------------------------------
//
// A client whose browser cannot encode OR decode VP8 at the directed profile
// fails media start with a typed `VideoCodecUnsupportedError`. It never quietly
// publishes or receives audio only — that would be a participant silently
// missing from everyone's grid, the failure mode this story exists to make
// loud. `CODEC_VP8` is declared in `supported_codecs` only after BOTH
// `VideoEncoder.isConfigSupported` AND `VideoDecoder.isConfigSupported` say yes.
//
// INDEPENDENT OF THE TEST SUBSTRATE PROBE. `packages/web-app/e2e/vp8Probe.ts`
// (task T-A) is deliberately separate and does not import this: an SDK
// capability-check defect must not pass or fail in lockstep with the test
// substrate's own check.
//
// ---------------------------------------------------------------------------
// NO KEYFRAME INTERVAL
// ---------------------------------------------------------------------------
//
// WebCodecs' `VideoEncoderConfig` has no keyframe-interval member, and none is
// invented here. A key chunk is produced when the caller passes
// `keyFrame: true` — which the publisher does only through its keyframe gate
// (`lifecycle/keyframePolicy.ts`). An UNSOLICITED key chunk (the platform may
// emit one, e.g. after configure) is reported faithfully as `key: true` and the
// gate stamps its interval clock from it.

import type { DirectedVideoEncoding } from './seams.js';

/** The probe's view of a profile: the directed values, or the capability-probe values. */
export interface Vp8Profile {
  readonly width: number;
  readonly height: number;
  readonly frameRateHz: number;
  readonly bitrateBps: number;
}

/** Thrown when the platform cannot encode or decode VP8 at the profile. Bounded. */
export class VideoCodecUnsupportedError extends Error {
  /** `encoder`, `decoder`, or `platform` (no WebCodecs at all). */
  readonly stage: 'encoder' | 'decoder' | 'platform';

  constructor(stage: 'encoder' | 'decoder' | 'platform', message: string) {
    super(message);
    this.name = 'VideoCodecUnsupportedError';
    this.stage = stage;
  }
}

/** The WebCodecs statics the probe uses. Injected so the node tier can exercise every branch. */
export interface Vp8ProbeApi {
  readonly VideoEncoder?: Pick<typeof VideoEncoder, 'isConfigSupported'>;
  readonly VideoDecoder?: Pick<typeof VideoDecoder, 'isConfigSupported'>;
}

/** The VP8 encoder configuration for a profile. Pure: no keyframe interval, realtime latency. */
export function buildVp8EncoderConfig(profile: Vp8Profile): VideoEncoderConfig {
  return {
    codec: 'vp8',
    width: profile.width,
    height: profile.height,
    framerate: profile.frameRateHz,
    bitrate: profile.bitrateBps,
    latencyMode: 'realtime',
  };
}

/**
 * The VP8 decoder configuration for a profile — the probe's AND every receive
 * slot's (story 3 C-B, built from the slot's configured bounds, never the
 * bitstream). `optimizeForLatency`: a call, not a playback buffer.
 *
 * `codedWidth`/`codedHeight` are HINTS to WebCodecs, not enforcement: the
 * receive slot judges every decoded frame against its bounds itself.
 */
export function buildVp8DecoderConfig(
  profile: Pick<Vp8Profile, 'width' | 'height'>,
): VideoDecoderConfig {
  return {
    codec: 'vp8',
    codedWidth: profile.width,
    codedHeight: profile.height,
    optimizeForLatency: true,
  };
}

/**
 * The bytes of a VP8 key frame's uncompressed header (RFC 6386 §9.1): the
 * 3-byte frame tag, the 3-byte start code, then 2 bytes each of width and
 * height (14-bit dimension, 2-bit scale).
 */
const VP8_KEYFRAME_HEADER_BYTES = 10;
const VP8_START_CODE = [0x9d, 0x01, 0x2a] as const;
const VP8_DIMENSION_MASK = 0x3fff;

/** What a VP8 key frame's header declares, or why it could not be read. */
export type Vp8KeyframeDimensions =
  { readonly ok: true; readonly width: number; readonly height: number } | { readonly ok: false };

/**
 * Whether a VP8 frame's bitstream says it is a KEY frame (RFC 6386 §9.1: frame
 * tag bit 0 is `!key_frame`). Bounds-checked: an empty buffer is not one. Used
 * to refuse a frame whose SIGNED header flag and bitstream disagree.
 */
export function vp8IsKeyFrame(plaintext: Uint8Array): boolean {
  return plaintext.length > 0 && ((plaintext[0] as number) & 0x01) === 0;
}

/**
 * Read the coded dimensions a VP8 KEY frame declares (RFC 6386 §9.1), from
 * DECRYPTED plaintext, before the native decoder sees it — so an over-bound
 * key frame never reaches the platform decoder (story 3 C-B, security).
 *
 * Bounds-checked: a buffer shorter than the header, a frame whose tag says it
 * is NOT a key frame, or a wrong start code is `{ok: false}` — a typed result,
 * never an exception and never a read past the end. Call on key frames only.
 * PURE; VP8 bitstream knowledge lives in this module and nowhere else.
 */
export function vp8KeyframeDimensions(plaintext: Uint8Array): Vp8KeyframeDimensions {
  if (plaintext.length < VP8_KEYFRAME_HEADER_BYTES) return { ok: false };
  // Frame tag bit 0 is `!key_frame` (0 = key frame).
  if (((plaintext[0] as number) & 0x01) !== 0) return { ok: false };
  if (
    plaintext[3] !== VP8_START_CODE[0] ||
    plaintext[4] !== VP8_START_CODE[1] ||
    plaintext[5] !== VP8_START_CODE[2]
  ) {
    return { ok: false };
  }
  const width = ((plaintext[6] as number) | ((plaintext[7] as number) << 8)) & VP8_DIMENSION_MASK;
  const height = ((plaintext[8] as number) | ((plaintext[9] as number) << 8)) & VP8_DIMENSION_MASK;
  return { ok: true, width, height };
}

/** The probe profile of a directed encoding. */
export function vp8ProfileOf(encoding: DirectedVideoEncoding): Vp8Profile {
  return {
    width: encoding.width,
    height: encoding.height,
    frameRateHz: encoding.frameRateHz,
    bitrateBps: encoding.maxBitrateBps,
  };
}

/**
 * Probe the platform for VP8 encode AND decode at `profile`.
 *
 * @throws {VideoCodecUnsupportedError} — `platform` when WebCodecs is absent,
 *   `encoder` / `decoder` when that half says no. Never a platform string.
 */
export async function probeVp8Support(
  profile: Vp8Profile,
  api: Vp8ProbeApi = globalThis as Vp8ProbeApi,
): Promise<void> {
  const Encoder = api.VideoEncoder;
  const Decoder = api.VideoDecoder;
  if (!Encoder || !Decoder) {
    throw new VideoCodecUnsupportedError(
      'platform',
      'this platform does not expose WebCodecs VideoEncoder/VideoDecoder, so it cannot send or ' +
        'receive VP8 video',
    );
  }
  const encoderOk = await Encoder.isConfigSupported(buildVp8EncoderConfig(profile)).then(
    (support) => support.supported === true,
    () => false,
  );
  if (!encoderOk) {
    throw new VideoCodecUnsupportedError(
      'encoder',
      'the platform cannot encode VP8 at the directed video profile',
    );
  }
  const decoderOk = await Decoder.isConfigSupported(buildVp8DecoderConfig(profile)).then(
    (support) => support.supported === true,
    () => false,
  );
  if (!decoderOk) {
    throw new VideoCodecUnsupportedError(
      'decoder',
      'the platform cannot decode VP8 at the directed video profile',
    );
  }
}
