// Node-tier SELF-TEST of the VP8 substrate probe (`e2e/vp8Probe.ts`, story 3
// R-28). Fake WebCodecs constructors are installed on `globalThis` — the probe
// body's one seam — so every case runs the REAL `runVp8ProbeInPage` (not a
// copy), and `classifyVp8Probe` maps what it observed. The step bound is driven
// on vitest FAKE timers: a callback that never fires resolves to a `*_timeout`
// verdict, distinct from `*_failed`, and nothing here waits on wall time.
//
// Also pinned: the Layer-7 mirror of the token and the sub-check list
// (`scripts/layer7.sh`) cannot drift from this module, and the probe imports
// nothing from the SDK or the app.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { VideoCodecProfile } from '../e2e/configEnv.js';
import {
  assertProbeProfile,
  classifyVp8Probe,
  runVp8ProbeInPage,
  vp8VerdictFileBody,
  VP8_PROBE_SUBCHECKS,
  VP8_UNSUPPORTED_REASON,
  type Vp8ProbeVerdict,
} from '../e2e/vp8Probe.js';

const PROFILE: VideoCodecProfile = {
  codec: 'vp8',
  width: 320,
  height: 180,
  frameRateHz: 15,
  maxBitrateBps: 250_000,
};
/** This test's own bound — the probe takes it as an argument. */
const BOUND_MS = 1_000;

// ---------------------------------------------------------------------------
// Fake WebCodecs, scripted per case
// ---------------------------------------------------------------------------

type Flush = 'ok' | 'hang' | 'reject';
interface Script {
  secure: boolean;
  present: boolean;
  encoderSupported: boolean | 'hang' | 'throw';
  decoderSupported: boolean | 'hang';
  encodeThrows: boolean;
  encodeErrors: boolean;
  encoderOutputs: boolean;
  encoderFlush: Flush;
  decoderOutputs: boolean;
  decoderFlush: Flush;
  decodedSize: readonly [number, number];
}
const baseline = (): Script => ({
  secure: true,
  present: true,
  encoderSupported: true,
  decoderSupported: true,
  encodeThrows: false,
  encodeErrors: false,
  encoderOutputs: true,
  encoderFlush: 'ok',
  decoderOutputs: true,
  decoderFlush: 'ok',
  decodedSize: [PROFILE.width, PROFILE.height],
});

const closed: string[] = [];
const never = <T>(): Promise<T> => new Promise<T>(() => {});
const flushWith = (mode: Flush): Promise<void> =>
  mode === 'ok' ? Promise.resolve() : mode === 'hang' ? never() : Promise.reject(new Error('x'));

function install(script: Script): void {
  const g = globalThis as Record<string, unknown>;
  g['isSecureContext'] = script.secure;
  if (!script.present) {
    for (const name of ['VideoEncoder', 'VideoDecoder', 'VideoFrame', 'OffscreenCanvas']) {
      delete g[name];
    }
    return;
  }
  class FakeEncoder {
    static isConfigSupported(): Promise<{ supported: boolean }> {
      if (script.encoderSupported === 'hang') return never();
      if (script.encoderSupported === 'throw') return Promise.reject(new TypeError('bad config'));
      return Promise.resolve({ supported: script.encoderSupported });
    }
    state = 'unconfigured';
    constructor(
      private readonly init: {
        output: (c: unknown, m?: unknown) => void;
        error: (e: unknown) => void;
      },
    ) {}
    configure(): void {
      this.state = 'configured';
    }
    encode(): void {
      if (script.encodeThrows) throw new Error('encode threw');
      if (script.encodeErrors) this.init.error(new Error('encoder error'));
      else if (script.encoderOutputs) {
        this.init.output({ chunk: 1 }, { decoderConfig: { codec: 'vp8' } });
      }
    }
    flush(): Promise<void> {
      return flushWith(script.encoderFlush);
    }
    close(): void {
      this.state = 'closed';
      closed.push('encoder');
    }
  }
  class FakeDecoder {
    static isConfigSupported(): Promise<{ supported: boolean }> {
      if (script.decoderSupported === 'hang') return never();
      return Promise.resolve({ supported: script.decoderSupported });
    }
    state = 'unconfigured';
    constructor(
      private readonly init: {
        output: (f: { displayWidth: number; displayHeight: number; close(): void }) => void;
      },
    ) {}
    configure(): void {
      this.state = 'configured';
    }
    decode(): void {
      if (script.decoderOutputs) {
        const [displayWidth, displayHeight] = script.decodedSize;
        this.init.output({ displayWidth, displayHeight, close: () => closed.push('decodedFrame') });
      }
    }
    flush(): Promise<void> {
      return flushWith(script.decoderFlush);
    }
    close(): void {
      this.state = 'closed';
      closed.push('decoder');
    }
  }
  g['VideoEncoder'] = FakeEncoder;
  g['VideoDecoder'] = FakeDecoder;
  g['VideoFrame'] = class {
    close(): void {
      closed.push('frame');
    }
  };
  g['OffscreenCanvas'] = class {
    getContext(): { fillStyle: string; fillRect(): void } {
      return { fillStyle: '', fillRect: () => {} };
    }
  };
}

/** Run the REAL probe body under fake timers, advancing past the bound. */
async function probe(script: Script): Promise<Vp8ProbeVerdict> {
  install(script);
  const pending = runVp8ProbeInPage({ profile: PROFILE, stepBoundMs: BOUND_MS });
  // Drive every step's bound (at most four bounded steps) — on the FAKE clock.
  for (let i = 0; i < 4; i += 1) await vi.advanceTimersByTimeAsync(BOUND_MS);
  return classifyVp8Probe(await pending, PROFILE);
}

const saved: Record<string, unknown> = {};
const GLOBALS = [
  'isSecureContext',
  'VideoEncoder',
  'VideoDecoder',
  'VideoFrame',
  'OffscreenCanvas',
];
beforeEach(() => {
  vi.useFakeTimers();
  closed.length = 0;
  for (const n of GLOBALS) saved[n] = (globalThis as Record<string, unknown>)[n];
});
afterEach(() => {
  vi.useRealTimers();
  for (const n of GLOBALS) {
    if (saved[n] === undefined) delete (globalThis as Record<string, unknown>)[n];
    else (globalThis as Record<string, unknown>)[n] = saved[n];
  }
});

describe('runVp8ProbeInPage + classifyVp8Probe: the verdict mapping', () => {
  it('a full round trip at the profile size is a PASS (positive control)', async () => {
    expect(await probe(baseline())).toBe('pass');
    // Everything it opened is closed: the source frame and both codecs.
    expect(closed).toEqual(expect.arrayContaining(['frame', 'encoder', 'decoder', 'decodedFrame']));
  });

  it.each<[string, Partial<Script>, Vp8ProbeVerdict]>([
    ['not a secure context', { secure: false }, 'webcodecs_unavailable'],
    ['no WebCodecs constructors', { present: false }, 'webcodecs_unavailable'],
    ['unsupported encoder', { encoderSupported: false }, 'encoder_config_unsupported'],
    [
      'an encoder config the platform refuses to evaluate',
      { encoderSupported: 'throw' },
      'encoder_config_unsupported',
    ],
    ['unsupported decoder', { decoderSupported: false }, 'decoder_config_unsupported'],
    ['encode throws', { encodeThrows: true }, 'encode_failed'],
    ['the encoder reports an error', { encodeErrors: true }, 'encode_failed'],
    ['the encoder flush rejects', { encoderFlush: 'reject' }, 'encode_failed'],
    ['the encoder produces no chunk', { encoderOutputs: false }, 'encode_failed'],
    ['the decoder produces no frame', { decoderOutputs: false }, 'decode_failed'],
    ['the decoder flush rejects', { decoderFlush: 'reject' }, 'decode_failed'],
    [
      'decoded width differs from the profile',
      { decodedSize: [PROFILE.width + 2, PROFILE.height] },
      'dimension_mismatch',
    ],
    [
      'decoded height differs from the profile',
      { decodedSize: [PROFILE.width, PROFILE.height - 2] },
      'dimension_mismatch',
    ],
  ])('%s → %s', async (_why, override, expected) => {
    expect(await probe({ ...baseline(), ...override })).toBe(expected);
  });

  it.each<[string, Partial<Script>, Vp8ProbeVerdict]>([
    [
      'the encoder flush never settles (output never fires)',
      { encoderFlush: 'hang' },
      'encode_timeout',
    ],
    ['the decoder flush never settles', { decoderFlush: 'hang' }, 'decode_timeout'],
    ['the encoder config query never settles', { encoderSupported: 'hang' }, 'encode_timeout'],
    ['the decoder config query never settles', { decoderSupported: 'hang' }, 'decode_timeout'],
  ])('a HANG is a timeout, never a codec error: %s → %s', async (_why, override, expected) => {
    expect(await probe({ ...baseline(), ...override })).toBe(expected);
  });

  it('a hang resolves only once the bound has elapsed on the injected clock', async () => {
    install({ ...baseline(), encoderFlush: 'hang' });
    let settled = false;
    const pending = runVp8ProbeInPage({ profile: PROFILE, stepBoundMs: BOUND_MS }).then((o) => {
      settled = true;
      return o;
    });
    await vi.advanceTimersByTimeAsync(BOUND_MS - 1);
    expect(settled).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(classifyVp8Probe(await pending, PROFILE)).toBe('encode_timeout');
  });
});

describe('assertProbeProfile: a malformed profile is a HARNESS error, never a verdict', () => {
  it('accepts a valid profile', () => {
    expect(() => assertProbeProfile(PROFILE)).not.toThrow();
  });
  it.each<[string, unknown]>([
    ['missing', undefined],
    ['null', null],
    ['another codec', { ...PROFILE, codec: 'h264' }],
    ['zero width', { ...PROFILE, width: 0 }],
    ['fractional height', { ...PROFILE, height: 1.5 }],
    ['string frame rate', { ...PROFILE, frameRateHz: '15' }],
    ['missing bitrate', { ...PROFILE, maxBitrateBps: undefined }],
  ])('%s throws', (_why, bad) => {
    expect(() => assertProbeProfile(bad)).toThrow(/VP8 probe/);
  });
});

describe('the verdict file body', () => {
  it('is the token on line 1 and the sub-check on line 2', () => {
    expect(vp8VerdictFileBody('decode_timeout')).toBe(
      `${VP8_UNSUPPORTED_REASON}\ndecode_timeout\n`,
    );
  });
});

// ---------------------------------------------------------------------------
// Drift: scripts/layer7.sh mirrors the token and the sub-check list
// ---------------------------------------------------------------------------

const LAYER7 = fileURLToPath(new URL('../../../scripts/layer7.sh', import.meta.url));

/** A distinct error for "could not read the mirror" — never reported as a mismatch. */
class ExtractionError extends Error {}

function extractBashArray(text: string, name: string): string[] {
  const re = new RegExp(`^readonly ${name}=\\(([^)]*)\\)`, 'gm');
  const hits = [...text.matchAll(re)];
  if (hits.length !== 1) {
    throw new ExtractionError(
      `extraction failed: scripts/layer7.sh declares \`readonly ${name}=(...)\` ${hits.length} times (need exactly 1)`,
    );
  }
  const members = (hits[0]![1] ?? '').split(/\s+/).filter((m) => m !== '');
  if (members.length === 0) {
    throw new ExtractionError(`extraction failed: scripts/layer7.sh ${name} is empty`);
  }
  return members;
}

describe('scripts/layer7.sh mirrors the probe vocabulary (no drift)', () => {
  const text = readFileSync(LAYER7, 'utf8');

  it('VP8_PROBE_SUBCHECKS equals the TS list, in order', () => {
    expect(extractBashArray(text, 'VP8_PROBE_SUBCHECKS')).toEqual([...VP8_PROBE_SUBCHECKS]);
  });

  it('the REASON token is routed under exactly the TS spelling', () => {
    expect(extractBashArray(text, 'VP8_VERDICT_TOKEN_LIST')).toEqual([VP8_UNSUPPORTED_REASON]);
    // ...and the route emits the matched list member, so the list IS the routed vocabulary.
    expect(text).toContain('precondition_fail "$token"');
  });

  it('the extractor is not vacuous: a missing, duplicated or empty array is an EXTRACTION error', () => {
    expect(() => extractBashArray('', 'VP8_PROBE_SUBCHECKS')).toThrow(ExtractionError);
    expect(() => extractBashArray('readonly X=(a)\nreadonly X=(a)\n', 'X')).toThrow(
      ExtractionError,
    );
    expect(() => extractBashArray('readonly X=()\n', 'X')).toThrow(ExtractionError);
    expect(extractBashArray('readonly X=(a b)\n', 'X')).toEqual(['a', 'b']);
  });
});

describe('import graph: the probe is independent of the SDK and the app', () => {
  it('imports nothing but types from ./configEnv and playwright', () => {
    const src = readFileSync(fileURLToPath(new URL('../e2e/vp8Probe.ts', import.meta.url)), 'utf8');
    const imports = [...src.matchAll(/^import\s+(type\s+)?[^;]*?from\s+'([^']+)'/gm)].map((m) => ({
      typeOnly: m[1] !== undefined,
      from: m[2],
    }));
    // Positive control: the scan sees the two known imports.
    expect(imports.map((i) => i.from).sort()).toEqual(['./configEnv.js', 'playwright/test']);
    expect(imports.every((i) => i.typeOnly)).toBe(true);
    // And no code line (comments may NAME what it must not use) reaches for them.
    const code = src
      .split('\n')
      .filter((line) => !/^\s*(\/\/|\*|\/\*)/.test(line))
      .join('\n');
    expect(code).not.toMatch(/sdk-core|\.\.\/src\/|probeVp8Support/);
  });
});
