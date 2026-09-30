// Node-tier unit tests for the observation-window stop rule and the same-page
// mover rule (`e2e/windowSampling.ts`) — the fix for the task-17 runner-gate
// flake: a fixed wall-clock window yielded fewer samples than the anti-vacuity
// floor when probe reads were slow. The loop must stop only when the window has
// elapsed AND the floor is met, and must fail LOUDLY at a hard bound — including
// when a single read is slow or never returns. Driven with a fake clock.

import { describe, expect, test } from 'vitest';

import { sampleWindow, unpairedObservers, windowStep } from '../e2e/windowSampling.js';

const WINDOW = 2_500;
const MIN = 4;
const MAX = 10_000;

const step = (samples: number, elapsedMs: number) =>
  windowStep({ samples, elapsedMs, windowMs: WINDOW, minSamples: MIN, maxMs: MAX });

describe('windowStep — the stop condition', () => {
  test('window elapsed but floor NOT met: keeps sampling (the flake: 3 samples used to stop here)', () => {
    expect(step(3, WINDOW)).toBe('continue');
    expect(step(3, WINDOW + 1_000)).toBe('continue');
  });

  test('floor met but window NOT elapsed: keeps sampling (never a shortened window)', () => {
    expect(step(MIN, WINDOW - 1)).toBe('continue');
    expect(step(50, 0)).toBe('continue');
  });

  test('both met: done, exactly at both boundaries', () => {
    expect(step(MIN, WINDOW)).toBe('done');
    expect(step(MIN + 5, WINDOW + 3_000)).toBe('done');
  });

  test('hard bound reached with the floor unmet: overrun', () => {
    expect(step(MIN - 1, MAX)).toBe('overrun');
    expect(step(0, MAX + 1)).toBe('overrun');
    expect(step(MIN - 1, MAX - 1)).toBe('continue');
  });

  test('done wins over overrun when both conditions are met at the bound', () => {
    expect(step(MIN, MAX)).toBe('done');
  });
});

/**
 * A fake clock. Each read costs `readMs`; a read that would run past the raced
 * timer's deadline (or any read, when `hang`) never resolves, and the clock
 * jumps to the deadline where the timer fires — the shape of a real slow read.
 */
function fakeLoop(readMs: number, { hang = false }: { hang?: boolean } = {}) {
  let t = 1_000_000;
  let n = 0;
  let deadline = Infinity;
  let fire: (() => void) | undefined;
  return {
    start: t,
    now: () => t,
    sleep: async (ms: number) => {
      t += ms;
    },
    timer: (ms: number) => {
      deadline = t + ms;
      let cancelled = false;
      const fired = new Promise<void>((resolve) => {
        fire = () => {
          if (!cancelled) resolve();
        };
      });
      return {
        fired,
        cancel: () => {
          cancelled = true;
        },
      };
    },
    sample: (): Promise<number> => {
      if (hang || t + readMs > deadline) {
        t = Math.max(t, deadline);
        fire?.();
        return new Promise<number>(() => {});
      }
      t += readMs;
      n += 1;
      return Promise.resolve(n);
    },
    intervalMs: 250,
  };
}

const opts = (f: ReturnType<typeof fakeLoop>, why = 't') => ({
  sample: f.sample,
  now: f.now,
  sleep: f.sleep,
  timer: f.timer,
  intervalMs: f.intervalMs,
  windowMs: WINDOW,
  minSamples: MIN,
  maxMs: MAX,
  why,
});

describe('sampleWindow — the loop', () => {
  test('fast reads: stops at the first sample after the window elapses (floor already met)', async () => {
    const f = fakeLoop(0);
    const out = await sampleWindow(opts(f));
    // Samples at t=0,250,...,2500 -> 11 samples; the 11th is the first with elapsed >= window.
    expect(out).toEqual(Array.from({ length: 11 }, (_, i) => i + 1));
  });

  test('slow reads under load: sampling continues past the window until the floor is met', async () => {
    // Each read costs 900ms: after the window (2500ms) only 3 samples exist; the
    // fixed-window loop stopped there and failed the vacuity guard.
    const f = fakeLoop(900);
    const out = await sampleWindow(opts(f));
    expect(out.length).toBe(MIN);
  });

  test('reads too slow to reach the floor: a loud HARNESS error AT the bound, not after the in-flight read', async () => {
    const f = fakeLoop(4_000);
    await expect(sampleWindow(opts(f, 'S3: B muted'))).rejects.toThrow(
      /^HARNESS: observation window \(S3: B muted\) reached its hard bound of 10000ms with 2 sample\(s\) \(need >= 4\)/,
    );
    // The third read would have ended at 12 250ms; the race cut it off at the bound.
    expect(f.now() - f.start).toBe(MAX);
  });

  test('a read that NEVER returns fails at the bound as the same HARNESS error', async () => {
    const f = fakeLoop(0, { hang: true });
    await expect(sampleWindow(opts(f, 'hung'))).rejects.toThrow(
      /^HARNESS: observation window \(hung\) reached its hard bound of 10000ms with 0 sample\(s\)/,
    );
    expect(f.now() - f.start).toBe(MAX);
  });

  test('a hard bound not above the window is refused up front', async () => {
    const f = fakeLoop(0);
    await expect(sampleWindow({ ...opts(f), maxMs: WINDOW })).rejects.toThrow(
      /must exceed the window/,
    );
  });
});

describe('unpairedObservers — the same-page mover rule', () => {
  test('no advance probe at all: refused as a wholly vacuous window', () => {
    expect(
      unpairedObservers([
        { observer: 'A', expect: 'flat' },
        { observer: 'B', expect: 'zero' },
      ]),
    ).toBeNull();
  });

  test('a flat/zero page whose only mover is on ANOTHER page is named', () => {
    expect(
      unpairedObservers([
        { observer: 'X', expect: 'flat' },
        { observer: 'X', expect: 'zero' },
        { observer: 'Y', expect: 'advance' },
      ]),
    ).toEqual(['X']);
  });

  test('every flat page has its own mover: well formed', () => {
    expect(
      unpairedObservers([
        { observer: 'X', expect: 'flat' },
        { observer: 'X', expect: 'advance' },
        { observer: 'Y', expect: 'zero' },
        { observer: 'Y', expect: 'advance' },
      ]),
    ).toEqual([]);
  });

  test('an advance-only page is never reported', () => {
    expect(
      unpairedObservers([
        { observer: 'X', expect: 'advance' },
        { observer: 'Y', expect: 'flat' },
        { observer: 'Y', expect: 'advance' },
      ]),
    ).toEqual([]);
  });
});
