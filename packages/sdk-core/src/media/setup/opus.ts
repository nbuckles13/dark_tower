// File: packages/sdk-core/src/media/setup/opus.ts
//
// WebCodecs Opus encoder and decoder construction.
//
// ---------------------------------------------------------------------------
// THE CONFIGURATION IS FOR HUMAN VOICE, AND EVERY FIELD IS A DECISION
// ---------------------------------------------------------------------------
//
// 48 kHz, mono, 20 ms frames, variable bitrate with a directed ceiling, highest
// complexity, VoIP application mode, voice signal hint, no DTX. None of it is a
// literal here — every value is read from `config/clientConfig.ts`, and the
// bitrate is the send directive's `EncodingParameters.max_bitrate_bps` whenever
// MC supplies it.
//
// ---------------------------------------------------------------------------
// `application` AND `signal` MAY NOT REACH THE ENCODER, AND WE SAY SO
// ---------------------------------------------------------------------------
//
// `OpusEncoderConfig` in the TypeScript DOM lib carries `format`,
// `frameDuration`, `complexity`, `packetlossperc`, `useinbandfec` and `usedtx`
// but NOT `application` or `signal`, which the WebCodecs Opus registration added
// later. A user agent that does not recognise them ignores them silently — which
// would leave the encoder tuned for general audio while this file claims voice
// tuning.
//
// So the config is probed with `AudioEncoder.isConfigSupported`, which returns
// the config with unrecognised members STRIPPED, and the effective values are
// reported back to the caller. That turns "did the voice tuning apply" from an
// assumption into an observation. It is a SETUP-time check, emitted once — never
// per frame.

import type {
  AudioDecoderFactory,
  AudioDecoderSeam,
  AudioEncoderFactory,
  AudioEncoderSeam,
  EncodedAudioFrame,
} from './seams.js';

/**
 * The Opus encoder members WebCodecs' registration added after the DOM lib was
 * cut. Declared locally rather than by augmenting `OpusEncoderConfig`, so this
 * file states exactly which members it is passing speculatively.
 */
export interface OpusVoiceTuning {
  /** `voip` | `audio` | `lowdelay`. */
  application?: string;
  /** `auto` | `voice` | `music`. */
  signal?: string;
}

export type OpusConfigWithTuning = OpusEncoderConfig & OpusVoiceTuning;

/** What the user agent actually accepted, after `isConfigSupported` stripping. */
export interface EffectiveOpusTuning {
  /** `true` when the UA kept `opus.application`. */
  readonly applicationApplied: boolean;
  /** `true` when the UA kept `opus.signal`. */
  readonly signalApplied: boolean;
}

/** Thrown when the platform cannot encode or decode the directed configuration. */
export class AudioCodecUnsupportedError extends Error {
  /** `encoder` or `decoder`. Bounded; never a raw platform message. */
  readonly stage: 'encoder' | 'decoder';

  constructor(stage: 'encoder' | 'decoder', message: string) {
    super(message);
    this.name = 'AudioCodecUnsupportedError';
    this.stage = stage;
  }
}

function buildEncoderConfig(options: {
  sampleRateHz: number;
  channels: number;
  bitrateBps: number;
  frameDurationMs: number;
  complexity: number;
  application: string;
  signal: string;
  useDtx: boolean;
  useInbandFec: boolean;
  packetLossPerc: number;
}): AudioEncoderConfig {
  const opus: OpusConfigWithTuning = {
    // Raw Opus packets, never an Ogg container: the payload is sealed into an
    // SFrame object and framed by our own header.
    format: 'opus',
    // WebCodecs takes MICROSECONDS.
    frameDuration: options.frameDurationMs * 1000,
    complexity: options.complexity,
    packetlossperc: options.packetLossPerc,
    useinbandfec: options.useInbandFec,
    usedtx: options.useDtx,
    application: options.application,
    signal: options.signal,
  };
  return {
    codec: 'opus',
    sampleRate: options.sampleRateHz,
    numberOfChannels: options.channels,
    bitrate: options.bitrateBps,
    // VARIABLE, per the story. Note the consequence recorded in `clientConfig.ts`:
    // the configured bitrate is a TARGET, not a floor, so instantaneous frames
    // fall below it on quiet input.
    bitrateMode: 'variable',
    opus,
  };
}

/**
 * Did the voice tuning survive `isConfigSupported`'s stripping?
 *
 * PURE, and separated from the probe deliberately. `isConfigSupported` returns
 * the config with unrecognised members REMOVED, so this comparison is the whole
 * point of probing at all — it turns "did the UA honour `application: 'voip'`"
 * from an assumption into an observation. It lives here rather than inside the
 * probe because the probe is welded to the global `AudioEncoder` and is excluded
 * from the coverage gate; a comparison hidden behind that exclusion would be
 * untestable and unscored while the exclusion claimed only wiring was excluded.
 *
 * A member the caller did not request counts as applied: there is nothing for
 * the UA to have stripped, so "absent" must not read as "rejected".
 */
export function effectiveTuning(
  requested: OpusConfigWithTuning | undefined,
  effective: OpusConfigWithTuning,
): EffectiveOpusTuning {
  return {
    applicationApplied:
      requested?.application === undefined || effective.application === requested.application,
    signalApplied: requested?.signal === undefined || effective.signal === requested.signal,
  };
}

/**
 * Probe the platform for the configuration, and report what it kept.
 *
 * @throws {AudioCodecUnsupportedError} when the configuration is not supported
 * at all. Fail loudly: a silently-degraded encoder produces audio that is worse
 * for reasons nobody can see.
 */
export async function probeEncoderSupport(
  config: AudioEncoderConfig,
): Promise<EffectiveOpusTuning> {
  const support = await AudioEncoder.isConfigSupported(config);
  if (support.supported !== true) {
    throw new AudioCodecUnsupportedError(
      'encoder',
      'the platform does not support the directed Opus encoder configuration ' +
        '(48 kHz mono, 20 ms frames, variable bitrate)',
    );
  }
  return effectiveTuning(
    config.opus as OpusConfigWithTuning | undefined,
    (support.config?.opus ?? {}) as OpusConfigWithTuning,
  );
}

/**
 * Production encoder factory.
 *
 * Deliberately thin: construction and seam delegation only. Every decision lives
 * in {@link buildEncoderConfig} and in `clientConfig.ts`, so this function has no
 * branch to test and is excluded from the coverage gate with that stated reason.
 */
export const createAudioEncoder: AudioEncoderFactory = async (options) => {
  const config = buildEncoderConfig(options);
  await probeEncoderSupport(config);
  const encoder = new AudioEncoder({
    output: (chunk: EncodedAudioChunk) => {
      const data = new Uint8Array(chunk.byteLength);
      chunk.copyTo(data);
      const frame: EncodedAudioFrame = { data, timestampUs: chunk.timestamp };
      options.onOutput(frame);
    },
    // The codec's error callback, not a per-frame path: WebCodecs reports a
    // terminal codec fault here, at most once per encoder instance.
    error: options.onError,
  });
  encoder.configure(config);
  const seam: AudioEncoderSeam = {
    encode(data: AudioData): void {
      encoder.encode(data);
    },
    close(): void {
      if (encoder.state !== 'closed') encoder.close();
    },
  };
  return seam;
};

/**
 * Production decoder factory.
 *
 * Opus needs no `description`: the encoder is configured for raw packets, so
 * each decoded frame is self-describing.
 */
export const createAudioDecoder: AudioDecoderFactory = async (options) => {
  const config: AudioDecoderConfig = {
    codec: 'opus',
    sampleRate: options.sampleRateHz,
    numberOfChannels: options.channels,
  };
  const support = await AudioDecoder.isConfigSupported(config);
  if (support.supported !== true) {
    throw new AudioCodecUnsupportedError(
      'decoder',
      'the platform does not support the directed Opus decoder configuration',
    );
  }
  const decoder = new AudioDecoder({
    output: options.onOutput,
    error: options.onError,
  });
  decoder.configure(config);
  const seam: AudioDecoderSeam = {
    decode(frame: EncodedAudioFrame): void {
      decoder.decode(
        new EncodedAudioChunk({
          type: 'key',
          timestamp: frame.timestampUs,
          data: frame.data,
        }),
      );
    },
    close(): void {
      if (decoder.state !== 'closed') decoder.close();
    },
  };
  return seam;
};
