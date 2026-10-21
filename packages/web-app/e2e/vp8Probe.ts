// File: packages/web-app/e2e/vp8Probe.ts
//
// Story 3 R-28: the VP8 SUBSTRATE PROBE — is the browser this suite runs in
// able to ENCODE and DECODE VP8 at the profile the cluster directs? If not, the
// browser layer fails LOUDLY; it never skips and never degrades to audio-only.
//
// ---------------------------------------------------------------------------
// INDEPENDENT OF THE SDK (lead-ruled)
// ---------------------------------------------------------------------------
//
// Nothing here imports `@darktower/sdk-core` or its own VP8 support check: this
// is an independent check of the SUBSTRATE, and the SDK's own fail-loud path is
// tested in the client's task. A shared bug would cancel out.
// `tests/vp8Probe.test.ts` enforces the import rule.
//
// ---------------------------------------------------------------------------
// WHAT IT CHECKS, AND WHY isConfigSupported IS NOT ENOUGH
// ---------------------------------------------------------------------------
//
// `VideoEncoder.isConfigSupported` / `VideoDecoder.isConfigSupported` can
// answer `supported: true` for a configuration the platform then fails to run,
// so the probe also performs a REAL one-frame round trip: a synthetic
// OffscreenCanvas frame (no getUserMedia, no camera, no launch flag), encoded as
// a keyframe, decoded with the encoder's own emitted decoder config, and the
// decoded picture's display size compared with the profile.
//
// ---------------------------------------------------------------------------
// TWO CALLERS, ONE BODY
// ---------------------------------------------------------------------------
//
//   * Layer 7: `global-setup.ts` → {@link runVp8ProbeInBrowser}, with the
//     DEPLOYED profile; an unsupported verdict is written to the Layer-7 verdict
//     file ({@link VP8_UNSUPPORTED_REASON} + the sub-check) and
//     `scripts/layer7.sh` routes it to PRECONDITION_FAILURE.
//   * CI: `src/__tests__/vp8Probe.test.ts` (vitest browser mode, the
//     `test:component` step), with the repo's BASE profile.
//
// {@link runVp8ProbeInPage} is FULLY SELF-CONTAINED — it references nothing
// outside its own body, because Playwright serialises its source into the page.
// It reads the WebCodecs constructors off `globalThis`, which is also the
// node-tier test's seam (fakes on `globalThis` drive the REAL body). Each step is
// raced against a bound passed IN (a liveness bound, never a latency gate), on
// `setTimeout`, so the node test drives it with fake timers.
//
// The observation carries booleans, numbers and enum outcomes only — no browser
// free text — so nothing the browser says can reach a REASON token or a status
// line. No identity, no key material: the frame is a flat synthetic picture.

import type { Browser } from 'playwright/test';
import type { VideoCodecProfile } from './configEnv.js';

/** The ONE spelling of the Layer-7 REASON token (mirrored by `scripts/layer7.sh`, drift-tested). */
export const VP8_UNSUPPORTED_REASON = 'browser-vp8-unsupported';

/**
 * The sub-checks an UNSUPPORTED verdict names — a FIXED list, mirrored by
 * `scripts/layer7.sh:VP8_PROBE_SUBCHECKS` (drift-tested), which validates line 2
 * of the verdict file against it. `*_timeout` = a WebCodecs step on that side
 * (config query, flush) did not settle within the step bound: a HANG, kept
 * apart from `*_failed` (an error, or no output) so it is never triaged as a
 * codec error.
 */
export const VP8_PROBE_SUBCHECKS = [
  'webcodecs_unavailable',
  'encoder_config_unsupported',
  'decoder_config_unsupported',
  'encode_failed',
  'encode_timeout',
  'decode_failed',
  'decode_timeout',
  'dimension_mismatch',
] as const;
export type Vp8ProbeSubcheck = (typeof VP8_PROBE_SUBCHECKS)[number];

/** The probe's verdict: `pass`, or the one sub-check that failed. */
export type Vp8ProbeVerdict = 'pass' | Vp8ProbeSubcheck;

/**
 * Bound on each WebCodecs step (config query, flush). A LIVENESS bound — a hung
 * codec would otherwise park the suite until its outer timeout — never a
 * latency assertion: a step that finishes in 9 s passes.
 */
export const PROBE_STEP_BOUND_MS = 10_000;

/**
 * How many bounded steps one probe run can take in sequence: the encoder and
 * decoder config queries, the encoder flush and the decoder flush. Keep in step
 * with the `bounded(...)` uses in {@link runVp8ProbeInPage}.
 */
export const PROBE_BOUNDED_STEPS = 4;

/**
 * The longest one probe run can take before every step's bound has fired. A
 * caller's own timeout (e.g. the component test's) must exceed this, or it
 * kills the run before the probe can report WHICH step hung.
 */
export const PROBE_TOTAL_BOUND_MS = PROBE_STEP_BOUND_MS * PROBE_BOUNDED_STEPS;

/** Outcome of the encode or decode step. */
export type Vp8StepOutcome = 'not_run' | 'ok' | 'error' | 'no_output' | 'timeout';
/** Outcome of an isConfigSupported query. */
export type Vp8ConfigOutcome = 'not_run' | 'supported' | 'unsupported' | 'timeout';

/** What the in-page body observed. Booleans, numbers and enums only. */
export interface Vp8ProbeObservation {
  readonly secureContext: boolean;
  readonly webCodecsPresent: boolean;
  readonly encoderConfig: Vp8ConfigOutcome;
  readonly decoderConfig: Vp8ConfigOutcome;
  readonly encode: Vp8StepOutcome;
  readonly decode: Vp8StepOutcome;
  readonly decodedWidth: number | null;
  readonly decodedHeight: number | null;
}

/**
 * The codecs the probe can check — the ONE home of the allowlist (configEnv's
 * `readVideoProfile` re-exports and enforces it). An allowlist, not a free
 * string: each value is also its WebCodecs codec string.
 */
export const SUPPORTED_PROBE_CODECS = ['vp8'] as const;
export type ProbeCodec = (typeof SUPPORTED_PROBE_CODECS)[number];

/** The structured argument of {@link runVp8ProbeInPage}. */
export interface Vp8ProbeArgs {
  readonly profile: VideoCodecProfile;
  readonly stepBoundMs: number;
}

/**
 * Validate a profile BEFORE it reaches a page. Throws (a harness error, never a
 * verdict) unless the codec is `vp8` and every number is a positive integer.
 */
export function assertProbeProfile(profile: unknown): asserts profile is VideoCodecProfile {
  const p = profile as Record<string, unknown> | null;
  if (p === null || typeof p !== 'object') {
    throw new Error('VP8 probe: the codec profile is missing (expected an object)');
  }
  if (!(SUPPORTED_PROBE_CODECS as readonly unknown[]).includes(p['codec'])) {
    throw new Error(
      `VP8 probe: profile codec must be one of ${SUPPORTED_PROBE_CODECS.join(', ')} ` +
        `(got ${JSON.stringify(p['codec'])})`,
    );
  }
  for (const key of ['width', 'height', 'frameRateHz', 'maxBitrateBps'] as const) {
    const v = p[key];
    if (typeof v !== 'number' || !Number.isInteger(v) || v < 1) {
      throw new Error(
        `VP8 probe: profile ${key} must be a positive integer (got ${JSON.stringify(v)})`,
      );
    }
  }
}

/**
 * THE in-page probe body. SELF-CONTAINED (Playwright serialises it): no
 * reference to anything outside this function.
 */
export async function runVp8ProbeInPage(args: Vp8ProbeArgs): Promise<Vp8ProbeObservation> {
  const { profile, stepBoundMs } = args;
  type Ctor = new (...a: never[]) => unknown;
  const g = globalThis as unknown as Record<string, unknown> & { isSecureContext?: boolean };
  const obs: {
    -readonly [K in keyof Vp8ProbeObservation]: Vp8ProbeObservation[K];
  } = {
    secureContext: g.isSecureContext === true,
    webCodecsPresent: ['VideoEncoder', 'VideoDecoder', 'VideoFrame', 'OffscreenCanvas'].every(
      (name) => typeof g[name] === 'function',
    ),
    encoderConfig: 'not_run',
    decoderConfig: 'not_run',
    encode: 'not_run',
    decode: 'not_run',
    decodedWidth: null,
    decodedHeight: null,
  };
  if (!obs.secureContext || !obs.webCodecsPresent) return obs;

  const TIMED_OUT = { timedOut: true } as const;
  const bounded = <T>(work: Promise<T>): Promise<T | typeof TIMED_OUT> => {
    let handle: ReturnType<typeof setTimeout> | undefined;
    const timer = new Promise<typeof TIMED_OUT>((resolve) => {
      handle = setTimeout(() => resolve(TIMED_OUT), stepBoundMs);
    });
    return Promise.race([work, timer]).finally(() => clearTimeout(handle));
  };

  interface Codec {
    configure(c: unknown): void;
    flush(): Promise<void>;
    close(): void;
    readonly state: string;
  }
  interface Encoder extends Codec {
    encode(frame: unknown, opts: { keyFrame: boolean }): void;
  }
  interface Decoder extends Codec {
    decode(chunk: unknown): void;
  }
  interface CodecCtor extends Ctor {
    isConfigSupported(c: unknown): Promise<{ supported?: boolean }>;
  }
  const VideoEncoderCtor = g['VideoEncoder'] as CodecCtor;
  const VideoDecoderCtor = g['VideoDecoder'] as CodecCtor;
  const VideoFrameCtor = g['VideoFrame'] as unknown as new (
    src: unknown,
    init: { timestamp: number },
  ) => { close(): void };
  const OffscreenCanvasCtor = g['OffscreenCanvas'] as unknown as new (
    w: number,
    h: number,
  ) => {
    getContext(kind: '2d'): {
      fillStyle: string;
      fillRect(x: number, y: number, w: number, h: number): void;
    } | null;
  };

  // Every allowlisted profile codec IS its WebCodecs codec string, and the
  // caller ran `assertProbeProfile` first. Used as given — never coerced (this
  // body is serialised into the page, so it cannot reference the allowlist).
  const codec: string = profile.codec;
  const encoderConfig = {
    codec,
    width: profile.width,
    height: profile.height,
    bitrate: profile.maxBitrateBps,
    framerate: profile.frameRateHz,
  };
  const decoderQuery = { codec, codedWidth: profile.width, codedHeight: profile.height };

  const query = async (ctor: CodecCtor, config: unknown): Promise<Vp8ConfigOutcome> => {
    try {
      const r = await bounded(ctor.isConfigSupported(config));
      if (r === TIMED_OUT) return 'timeout';
      return (r as { supported?: boolean }).supported === true ? 'supported' : 'unsupported';
    } catch {
      // A config the platform refuses to even evaluate is not supported.
      return 'unsupported';
    }
  };
  obs.encoderConfig = await query(VideoEncoderCtor, encoderConfig);
  if (obs.encoderConfig !== 'supported') return obs;
  obs.decoderConfig = await query(VideoDecoderCtor, decoderQuery);
  if (obs.decoderConfig !== 'supported') return obs;

  let encoder: Encoder | undefined;
  let decoder: Decoder | undefined;
  let frame: { close(): void } | undefined;
  try {
    // --- encode one synthetic keyframe ---
    const chunks: unknown[] = [];
    let emittedDecoderConfig: unknown;
    let encodeErrored = false;
    try {
      encoder = new (VideoEncoderCtor as unknown as new (init: unknown) => Encoder)({
        output: (chunk: unknown, meta?: { decoderConfig?: unknown }) => {
          chunks.push(chunk);
          if (meta?.decoderConfig !== undefined && emittedDecoderConfig === undefined) {
            emittedDecoderConfig = meta.decoderConfig;
          }
        },
        error: () => {
          encodeErrored = true;
        },
      });
      encoder.configure(encoderConfig);
      const canvas = new OffscreenCanvasCtor(profile.width, profile.height);
      const ctx = canvas.getContext('2d');
      if (ctx !== null) {
        ctx.fillStyle = '#000';
        ctx.fillRect(0, 0, profile.width, profile.height);
        ctx.fillStyle = '#fff';
        ctx.fillRect(0, 0, Math.max(1, Math.floor(profile.width / 2)), profile.height);
      }
      frame = new VideoFrameCtor(canvas, { timestamp: 0 });
      encoder.encode(frame, { keyFrame: true });
      frame.close();
      frame = undefined;
      const flushed = await bounded(encoder.flush());
      if (flushed === TIMED_OUT) obs.encode = 'timeout';
      else if (encodeErrored) obs.encode = 'error';
      else obs.encode = chunks.length > 0 ? 'ok' : 'no_output';
    } catch {
      obs.encode = 'error';
    }
    if (obs.encode !== 'ok') return obs;

    // --- decode it with the encoder's own decoder config ---
    let decodeErrored = false;
    let frames = 0;
    try {
      decoder = new (VideoDecoderCtor as unknown as new (init: unknown) => Decoder)({
        output: (f: { displayWidth: number; displayHeight: number; close(): void }) => {
          frames += 1;
          if (obs.decodedWidth === null) {
            obs.decodedWidth = f.displayWidth;
            obs.decodedHeight = f.displayHeight;
          }
          f.close();
        },
        error: () => {
          decodeErrored = true;
        },
      });
      decoder.configure(emittedDecoderConfig ?? decoderQuery);
      decoder.decode(chunks[0]);
      const flushed = await bounded(decoder.flush());
      if (flushed === TIMED_OUT) obs.decode = 'timeout';
      else if (decodeErrored) obs.decode = 'error';
      else obs.decode = frames > 0 ? 'ok' : 'no_output';
    } catch {
      obs.decode = 'error';
    }
    return obs;
  } finally {
    try {
      frame?.close();
    } catch {
      /* already closed */
    }
    for (const c of [encoder, decoder]) {
      try {
        if (c !== undefined && c.state !== 'closed') c.close();
      } catch {
        /* already closed */
      }
    }
  }
}

/**
 * Map an observation to the verdict. Pure. The order is the order the body runs
 * its steps in, so the FIRST failing step is the one named.
 */
export function classifyVp8Probe(
  obs: Vp8ProbeObservation,
  profile: VideoCodecProfile,
): Vp8ProbeVerdict {
  if (!obs.secureContext || !obs.webCodecsPresent) return 'webcodecs_unavailable';
  if (obs.encoderConfig === 'timeout') return 'encode_timeout';
  if (obs.encoderConfig !== 'supported') return 'encoder_config_unsupported';
  if (obs.decoderConfig === 'timeout') return 'decode_timeout';
  if (obs.decoderConfig !== 'supported') return 'decoder_config_unsupported';
  if (obs.encode === 'timeout') return 'encode_timeout';
  if (obs.encode !== 'ok') return 'encode_failed';
  if (obs.decode === 'timeout') return 'decode_timeout';
  if (obs.decode !== 'ok') return 'decode_failed';
  if (obs.decodedWidth !== profile.width || obs.decodedHeight !== profile.height) {
    return 'dimension_mismatch';
  }
  return 'pass';
}

/** The probe page's origin: HTTPS (WebCodecs is [SecureContext]), answered by `page.route`, never resolved. */
export const VP8_PROBE_ORIGIN = 'https://vp8-probe.invalid';

/**
 * Layer-7 runner: a fresh context on a routed secure origin (no server, no
 * launch flag), the self-contained body, the verdict. The context is closed on
 * every path.
 */
export async function runVp8ProbeInBrowser(
  browser: Browser,
  profile: VideoCodecProfile,
): Promise<{ readonly verdict: Vp8ProbeVerdict; readonly observation: Vp8ProbeObservation }> {
  assertProbeProfile(profile);
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    await page.route(`${VP8_PROBE_ORIGIN}/**`, (route) =>
      route.fulfill({
        status: 200,
        contentType: 'text/html',
        body: '<!doctype html><title>vp8 substrate probe</title>',
      }),
    );
    await page.goto(`${VP8_PROBE_ORIGIN}/`);
    const observation = await page.evaluate(runVp8ProbeInPage, {
      profile,
      stepBoundMs: PROBE_STEP_BOUND_MS,
    });
    return { verdict: classifyVp8Probe(observation, profile), observation };
  } finally {
    await context.close();
  }
}

/** The verdict-file body for an unsupported verdict: token, then the sub-check. */
export function vp8VerdictFileBody(subcheck: Vp8ProbeSubcheck): string {
  return `${VP8_UNSUPPORTED_REASON}\n${subcheck}\n`;
}
