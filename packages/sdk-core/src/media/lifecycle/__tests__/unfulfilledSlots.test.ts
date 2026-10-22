// File: packages/sdk-core/src/media/lifecycle/__tests__/unfulfilledSlots.test.ts
//
// The unfulfilled-slot series (story 3 R-30): every entry and exit, teardown
// included; exact durations on a manual clock; the gauge always the absolute
// count and 0 after teardown; no identity or slot id on any label.

import { describe, expect, it } from 'vitest';

import { manualClock, videoMetrics } from '../../pipeline/__tests__/videoFixtures.js';
import { UnfulfilledSlots } from '../unfulfilledSlots.js';

function harness() {
  const { metrics, sink } = videoMetrics();
  const clock = manualClock(0);
  const tracker = new UnfulfilledSlots(metrics, clock.now);
  const gauge = () =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_slots_unfulfilled')
      .map((m) => m.value);
  const durations = () =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_slot_unfulfilled_duration_ms')
      .map((m) => m.value);
  return { tracker, clock, sink, gauge, durations };
}

describe('UnfulfilledSlots', () => {
  it('enter is idempotent: two losses while unfulfilled are one entry and one observation', () => {
    const h = harness();
    h.tracker.enter(1);
    h.clock.advance(100);
    h.tracker.enter(1);
    h.clock.advance(150);
    h.tracker.exit(1);
    expect(h.durations()).toEqual([250]);
    expect(h.gauge()).toEqual([1, 0]);
  });

  it('an exit without an entry observes nothing', () => {
    const h = harness();
    h.tracker.exit(4);
    expect(h.durations()).toEqual([]);
    expect(h.gauge()).toEqual([]);
  });

  it('the gauge is the ABSOLUTE count on every transition', () => {
    const h = harness();
    h.tracker.enter(1);
    h.tracker.enter(2);
    h.tracker.exit(1);
    expect(h.gauge()).toEqual([1, 2, 1]);
  });

  it('teardown with several slots unfulfilled: N observations, exact durations, gauge 0', () => {
    const h = harness();
    h.tracker.enter(1);
    h.clock.advance(1_000);
    h.tracker.enter(2);
    h.clock.advance(120_000);
    h.tracker.stop();
    expect(h.durations().sort((a, b) => a - b)).toEqual([120_000, 121_000]);
    expect(h.gauge().at(-1)).toBe(0);
    // Idempotent; nothing after stop.
    h.tracker.stop();
    h.tracker.enter(3);
    expect(h.tracker.isUnfulfilled(3)).toBe(false);
  });

  it('reassert re-sets the current count (export-tick cadence)', () => {
    const h = harness();
    h.tracker.enter(1);
    h.tracker.reassert();
    expect(h.gauge()).toEqual([1, 1]);
  });

  it('the local snapshot carries time-in-state; it is never a metric', () => {
    const h = harness();
    h.tracker.enter(2);
    h.clock.advance(42);
    expect(h.tracker.snapshot([1, 2])).toEqual([
      { slotId: 1, unfulfilled: false, sinceMs: 0 },
      { slotId: 2, unfulfilled: true, sinceMs: 42 },
    ]);
  });

  it('labels are exactly the base set + media_kind: no identity, no slot id', () => {
    const h = harness();
    h.tracker.enter(9);
    h.tracker.exit(9);
    for (const m of h.sink.getRecordedMetrics()) {
      expect(Object.keys(m.labels).sort()).toEqual(
        ['client_version', 'key_custody', 'media_kind', 'org_id'].sort(),
      );
      expect(m.labels['media_kind']).toBe('video');
    }
  });
});
