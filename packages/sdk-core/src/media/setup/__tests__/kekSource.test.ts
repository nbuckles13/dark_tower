// File: packages/sdk-core/src/media/setup/__tests__/kekSource.test.ts
//
// The KEK holder: current generation plus at most ONE retained previous, the
// previous kept for a window derived from W (story 2 R-14; ADR-0036 §4).
//
// Time is a FAKE SCHEDULER throughout — no wall clock, no real timers. Expiry
// has TWO mechanisms (the timer, and a lazy backstop for a timer that fires
// late) and each gets its own test, so either can be removed only by turning a
// test red.
//
// Expected windows are read back from `deriveKekRetention`, never restated.

import { describe, expect, it } from 'vitest';

import {
  deriveKekRetention,
  DEFAULT_CLIENT_CONFIG,
  transmitRewrapLatencyMs,
} from '../../../config/clientConfig.js';
import { takeMeetingKek } from '../../../signaling/kekIntake.js';
import { NOMINAL_DEBOUNCE_SECONDS } from '../../__tests__/helpers.js';
import { MEETING_KEK_BYTES } from '../../frame/sframe.js';
import {
  MAX_KEK_GENERATION,
  MeetingKekHolder,
  RetentionGuard,
  type HeldKek,
  type KekArrival,
  type KekHolderObserver,
} from '../kekSource.js';

const T = transmitRewrapLatencyMs(DEFAULT_CLIENT_CONFIG.media);
const NOMINAL_RETENTION_MS = deriveKekRetention(NOMINAL_DEBOUNCE_SECONDS, T).retentionMs;

function key(fill: number): Uint8Array {
  return new Uint8Array(MEETING_KEK_BYTES).fill(fill);
}

/** A deterministic clock plus a timer queue the test advances by hand. */
function fakeScheduler() {
  let now = 1_000;
  let nextId = 1;
  const timers = new Map<number, { at: number; fn: () => void }>();
  return {
    clock: () => now,
    setTimeout: (fn: () => void, ms: number) => {
      const id = nextId++;
      timers.set(id, { at: now + ms, fn });
      return id as unknown as ReturnType<typeof setTimeout>;
    },
    clearTimeout: (h: ReturnType<typeof setTimeout>) => {
      timers.delete(h as unknown as number);
    },
    /** Advance the clock, firing due timers. */
    advance(ms: number) {
      now += ms;
      for (const [id, t] of [...timers]) {
        if (t.at <= now) {
          timers.delete(id);
          t.fn();
        }
      }
    },
    /** Advance the clock WITHOUT firing anything: a throttled background tab. */
    advanceWithoutTimers(ms: number) {
      now += ms;
    },
    get pending() {
      return timers.size;
    },
  };
}

/** Records every report, and every WARN message verbatim. */
function recordingObserver() {
  const calls = {
    arrived: [] as KekArrival[],
    anomalies: [] as string[],
    refusals: [] as string[],
    retained: 0,
    violations: 0,
    warnings: [] as string[],
  };
  const observer: KekHolderObserver = {
    kekArrived: (s) => calls.arrived.push(s),
    retentionAnomaly: (o) => calls.anomalies.push(o),
    installRefused: (o) => calls.refusals.push(o),
    generationRetained: () => (calls.retained += 1),
    retentionViolation: () => (calls.violations += 1),
    warn: (m) => calls.warnings.push(m),
  };
  return { calls, observer };
}

function rig() {
  const sched = fakeScheduler();
  const { calls, observer } = recordingObserver();
  const holder = new MeetingKekHolder({
    rewrapLatencyMs: T,
    observer,
    clock: sched.clock,
    setTimeout: sched.setTimeout,
    clearTimeout: sched.clearTimeout,
  });
  const install = (fill: number, generation: number, w = NOMINAL_DEBOUNCE_SECONDS) =>
    holder.install(key(fill), generation, w, 'kek_update');
  return { holder, sched, calls, install };
}

describe('install: a NEWER generation demotes the current and retains exactly one previous', () => {
  it('keeps current + previous, and the previous still opens frames', () => {
    const { holder, install } = rig();
    expect(install(0x10, 0)).toBe('installed');
    expect(install(0x11, 1)).toBe('installed_retaining');
    expect(holder.kekForGeneration(1)?.every((b) => b === 0x11)).toBe(true);
    expect(holder.kekForGeneration(0)?.every((b) => b === 0x10)).toBe(true);
    expect(holder.currentForWrapping()?.generation).toBe(1);
  });

  it('zeroizes the previous it DROPS, by the buffer the caller once held', () => {
    const { holder, install } = rig();
    install(0x10, 0);
    install(0x11, 1);
    const gen0 = holder.kekForGeneration(0)!;
    install(0x12, 2);
    // Overwritten, not merely unreferenced.
    expect(gen0.every((b) => b === 0)).toBe(true);
    expect(holder.kekForGeneration(0)).toBeUndefined();
    expect(holder.kekForGeneration(1)).toBeDefined();
  });

  it('holds AT MOST ONE previous across many rotations, and the tripwire never fires', () => {
    const { holder, install, calls } = rig();
    for (let g = 0; g <= 5; g += 1) {
      install(0x20 + g, g);
      if (g >= 2)
        expect(
          holder.kekForGeneration(g - 2),
          `gen ${g - 2} after installing ${g}`,
        ).toBeUndefined();
    }
    expect(holder.kekForGeneration(5)).toBeDefined();
    expect(holder.kekForGeneration(4)).toBeDefined();
    expect(calls.violations).toBe(0);
  });

  it('announces the new generation only AFTER it is current (R-13 ordering)', () => {
    // A listener that rotates transmit keys must find the new KEK already
    // current, or its next mint would wrap under the KEK a leaver holds.
    const { holder, install } = rig();
    install(0x10, 0);
    const seen: (number | undefined)[] = [];
    holder.onCurrentGenerationChanged(() => seen.push(holder.currentForWrapping()?.generation));
    install(0x11, 1);
    expect(seen).toEqual([1]);
  });

  it('counts a RETAINING install, never the first one', () => {
    // The first install has nothing to retain. Counting it would make the
    // retained counter mean "installed", and a fleet that never retained would
    // read healthy.
    const { install, calls } = rig();
    install(0x10, 0);
    expect(calls.retained).toBe(0);
    install(0x11, 1);
    expect(calls.retained).toBe(1);
  });
});

describe('install: the SAME generation', () => {
  it('with the same bytes is a no-op: no demotion, no rotation, no retention count', () => {
    // A reconnect redelivering the current generation. Rotating here would make
    // every reconnect a transmit-key rotation — a storm lever.
    const { holder, install, calls } = rig();
    install(0x10, 0);
    install(0x11, 1);
    let rotations = 0;
    holder.onCurrentGenerationChanged(() => (rotations += 1));
    expect(install(0x11, 1)).toBe('already_held');
    expect(rotations).toBe(0);
    expect(calls.retained).toBe(1);
    expect(holder.kekForGeneration(0)).toBeDefined();
    expect(calls.refusals).toEqual([]);
  });

  it("does NOT re-arm or extend the previous generation's retention", () => {
    // A redelivery landing just before expiry must not buy the previous KEK
    // another full window.
    const { holder, install, sched } = rig();
    install(0x10, 0);
    install(0x11, 1);
    sched.advance(NOMINAL_RETENTION_MS - 1);
    install(0x11, 1);
    sched.advance(1);
    expect(holder.kekForGeneration(0)).toBeUndefined();
  });

  it('with DIFFERENT bytes is refused, counted and warned — the held key keeps working', () => {
    const { holder, install, calls } = rig();
    install(0x10, 0);
    expect(install(0x99, 0)).toBe('refused');
    expect(calls.refusals).toEqual(['conflicting_key']);
    expect(holder.kekForGeneration(0)?.every((b) => b === 0x10)).toBe(true);
    expect(calls.warnings).toHaveLength(1);
  });
});

describe('install: an OLDER generation never rolls the current back', () => {
  it.each([
    ['the retained previous', 1],
    ['a generation older than anything held', 0],
  ])('refuses %s and leaves every held generation as it was', (_label, generation) => {
    const { holder, install, calls } = rig();
    install(0x10, 0);
    install(0x11, 1);
    install(0x12, 2);
    expect(install(0x77, generation)).toBe('refused');
    expect(calls.refusals).toEqual(['older_generation']);
    expect(holder.currentForWrapping()?.generation).toBe(2);
    expect(holder.kekForGeneration(1)?.every((b) => b === 0x11)).toBe(true);
    expect(holder.kekForGeneration(2)?.every((b) => b === 0x12)).toBe(true);
  });
});

describe('install: malformed material is refused at the boundary, on both arrival paths', () => {
  it.each([
    ["a generation above the wire's u16 range", key(1), MAX_KEK_GENERATION + 1],
    ['a negative generation', key(1), -1],
    ['a non-integer generation', key(1), 1.5],
    ['an all-zero key', new Uint8Array(MEETING_KEK_BYTES), 0],
    ['a wrong-width key', new Uint8Array(16).fill(1), 0],
  ])('refuses %s', (_label, kek, generation) => {
    for (const source of ['join_response', 'kek_update'] as const) {
      const { holder, calls } = rig();
      expect(holder.install(kek, generation, NOMINAL_DEBOUNCE_SECONDS, source)).toBe('refused');
      expect(calls.refusals).toEqual(['malformed']);
      expect(holder.isProvisioned).toBe(false);
    }
  });

  it('accepts the top of the u16 range', () => {
    const { holder, install } = rig();
    expect(install(0x10, MAX_KEK_GENERATION)).toBe('installed');
    expect(holder.kekForGeneration(MAX_KEK_GENERATION)).toBeDefined();
  });
});

describe('expiry: two mechanisms, each tested alone', () => {
  it('the TIMER drops and zeroizes the previous at the derived window, not before', () => {
    const { holder, install, sched } = rig();
    install(0x10, 0);
    install(0x11, 1);
    const gen0 = holder.kekForGeneration(0)!;

    sched.advance(NOMINAL_RETENTION_MS - 1);
    expect(holder.kekForGeneration(0), 'just inside the window').toBeDefined();
    expect(gen0.every((b) => b === 0x10)).toBe(true);

    sched.advance(1);
    expect(sched.pending).toBe(0);
    expect(holder.kekForGeneration(0), 'at the window').toBeUndefined();
    expect(gen0.every((b) => b === 0)).toBe(true);
    // The current is untouched by the previous's expiry.
    expect(holder.kekForGeneration(1)).toBeDefined();
  });

  it('the LAZY BACKSTOP refuses an expired previous when the timer has not fired', () => {
    // A throttled background tab fires timers late. Without the backstop the
    // previous KEK would keep opening frames past its window until it did.
    const { holder, install, sched } = rig();
    install(0x10, 0);
    install(0x11, 1);
    const gen0 = holder.kekForGeneration(0)!;
    sched.advanceWithoutTimers(NOMINAL_RETENTION_MS);
    expect(sched.pending, 'the timer genuinely has not fired').toBe(1);
    expect(holder.kekForGeneration(0)).toBeUndefined();
    expect(gen0.every((b) => b === 0)).toBe(true);
    expect(holder.isOlderThanRetained(0)).toBe(true);
  });

  it('derives the window from the W on the message that CAUSED the demotion', () => {
    // A long-lived member must follow MC's current configuration, not the W it
    // learned at join.
    const { holder, install, sched } = rig();
    install(0x10, 0, 3_600);
    install(0x11, 1, 10);
    const expected = deriveKekRetention(10, T).retentionMs;
    sched.advance(expected - 1);
    expect(holder.kekForGeneration(0)).toBeDefined();
    sched.advance(1);
    expect(holder.kekForGeneration(0)).toBeUndefined();
  });

  it('clear() zeroizes BOTH generations and cancels the pending expiry', () => {
    const { holder, install, sched } = rig();
    install(0x10, 0);
    install(0x11, 1);
    const gen0 = holder.kekForGeneration(0)!;
    const gen1 = holder.kekForGeneration(1)!;
    holder.clear();
    expect(gen0.every((b) => b === 0)).toBe(true);
    expect(gen1.every((b) => b === 0)).toBe(true);
    expect(sched.pending).toBe(0);
    expect(holder.isProvisioned).toBe(false);
    expect(holder.currentForWrapping()).toBeUndefined();
  });
});

describe('isOlderThanRetained: the newer/older split for a generation not held', () => {
  it('is false with nothing held — every unheld generation is then the NEWER case', () => {
    const { holder } = rig();
    expect(holder.isOlderThanRetained(0)).toBe(false);
  });

  it('is false for a held generation and for anything newer than the current', () => {
    const { holder, install } = rig();
    install(0x10, 3);
    install(0x11, 4);
    expect(holder.isOlderThanRetained(3)).toBe(false);
    expect(holder.isOlderThanRetained(4)).toBe(false);
    expect(holder.isOlderThanRetained(9)).toBe(false);
  });

  it('is true below the retained previous, and for a gap between previous and current', () => {
    const { holder, install } = rig();
    install(0x10, 3);
    install(0x11, 6);
    expect(holder.isOlderThanRetained(2)).toBe(true);
    expect(holder.isOlderThanRetained(5), 'a skipped generation is not held either').toBe(true);
  });
});

describe('the send side can reach ONLY the current generation', () => {
  it('currentForWrapping never returns the retained previous', () => {
    // A sender wrapping under a retained previous KEK would keep wrapping under
    // the key a departed member still holds, defeating R-13.
    const { holder, install } = rig();
    install(0x10, 0);
    install(0x11, 1);
    const wrap = holder.currentForWrapping();
    expect(wrap?.generation).toBe(1);
    expect(wrap?.kek.every((b) => b === 0x11)).toBe(true);
    expect(Object.keys(MeetingKekHolder.prototype)).not.toContain('previousForWrapping');
  });
});

describe('W handling: never a hard failure, always counted and loud', () => {
  it('floor-substitutes a zero W, counts it, and warns with DURATIONS ONLY', () => {
    const { install, calls } = rig();
    const GENERATION = 4242;
    install(0x5a, GENERATION, 0);
    expect(calls.anomalies).toEqual(['floor_substituted']);
    expect(calls.warnings).toHaveLength(1);
    const [warning] = calls.warnings;
    // Never the generation, never key bytes (0x5a = 90).
    expect(warning).not.toContain(String(GENERATION));
    expect(warning).not.toMatch(/\b90\b|5a/i);
  });

  it('counts a ceiling clamp distinctly from a floor substitution', () => {
    const { install, calls } = rig();
    install(0x10, 0, 86_400);
    install(0x11, 1, 0);
    expect(calls.anomalies).toEqual(['ceiling_clamped', 'floor_substituted']);
  });

  it('names the below-T relationship without raising the window', () => {
    const sched = fakeScheduler();
    const { calls, observer } = recordingObserver();
    // A client whose re-wrap latency exceeds W/2 for a small W.
    const holder = new MeetingKekHolder({
      rewrapLatencyMs: 5_000,
      observer,
      clock: sched.clock,
      setTimeout: sched.setTimeout,
      clearTimeout: sched.clearTimeout,
    });
    holder.install(key(0x10), 0, 2, 'kek_update');
    expect(calls.anomalies).toEqual(['below_rewrap_latency']);
    expect(calls.warnings[0]).toMatch(/re-wrap latency/);
  });

  it('is silent at a nominal W', () => {
    const { install, calls } = rig();
    install(0x10, 0);
    install(0x11, 1);
    expect(calls.anomalies).toEqual([]);
    expect(calls.warnings).toEqual([]);
  });
});

describe('the retention tripwire', () => {
  it('fires on a FABRICATED over-retained list, fails safe, and zeroizes the excess', () => {
    // No production path may violate the invariant on purpose, so the
    // over-retained state is fabricated here and handed to the evaluator.
    const excessA = new Uint8Array(MEETING_KEK_BYTES).fill(3);
    const excessB = new Uint8Array(MEETING_KEK_BYTES).fill(4);
    const held: HeldKek[] = [
      { generation: 9, kek: key(1) },
      { generation: 8, kek: key(2), expiresAtMs: 0 },
      { generation: 7, kek: excessA, expiresAtMs: 0 },
      { generation: 6, kek: excessB, expiresAtMs: 0 },
    ];
    let violations = 0;
    new RetentionGuard(() => (violations += 1)).evaluate(held);
    expect(violations).toBe(1);
    expect(held.map((h) => h.generation)).toEqual([9, 8]);
    expect(excessA.every((b) => b === 0)).toBe(true);
    expect(excessB.every((b) => b === 0)).toBe(true);
  });

  it('does not fire at or under the bound', () => {
    let violations = 0;
    const guard = new RetentionGuard(() => (violations += 1));
    guard.evaluate([{ generation: 1, kek: key(1) }]);
    guard.evaluate([
      { generation: 2, kek: key(1) },
      { generation: 1, kek: key(2), expiresAtMs: 0 },
    ]);
    expect(violations).toBe(0);
    expect(guard.evaluations).toBe(2);
  });

  it('is evaluated on EVERY live install, through the real signaling intake', () => {
    // THE TEST THAT WATCHES THE TRIPWIRE'S CALL SITE. The refactor the counter
    // exists to catch is the one most likely to delete the evaluation, and a
    // tripwire armed only in tests is disarmed in the product. So this drives
    // the holder through `takeMeetingKek` — the production intake — and asserts
    // the bound was checked once per delivery on EVERY path: first install,
    // retaining install, idempotent redelivery, and each refusal.
    const { holder } = rig();
    const deliver = (fill: number, generation: number, source: KekArrival = 'kek_update') =>
      takeMeetingKek(
        {
          meetingKek: key(fill),
          kekGeneration: generation,
          kekRotationDebounceSeconds: NOMINAL_DEBOUNCE_SECONDS,
        },
        holder,
        source,
      ).result;

    const results = [
      deliver(0x10, 0, 'join_response'),
      deliver(0x11, 1),
      deliver(0x11, 1),
      deliver(0x99, 1),
      deliver(0x10, 0),
      deliver(0x00, 2),
    ];
    expect(results).toEqual([
      'installed',
      'installed_retaining',
      'already_held',
      'refused',
      'refused',
      'refused',
    ]);
    expect(holder.retentionChecks).toBe(results.length);
  });
});

describe('arrivals are counted by source', () => {
  it('reports every delivery that carried key material, by the source it came on', () => {
    const { holder, calls } = rig();
    holder.install(key(0x10), 0, NOMINAL_DEBOUNCE_SECONDS, 'join_response');
    holder.install(key(0x11), 1, NOMINAL_DEBOUNCE_SECONDS, 'kek_update');
    expect(calls.arrived).toEqual(['join_response', 'kek_update']);
  });
});
