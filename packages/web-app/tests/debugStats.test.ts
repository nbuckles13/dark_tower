// File: packages/web-app/tests/debugStats.test.ts
//
// The R-35 overlay's ONE rate computation and its row projection, exactly:
// rates from the MEASURED gap between successive cumulative snapshots, with no
// rate (never NaN, Infinity or a negative) for a first sample, a zero or
// negative gap, a counter reset, or a slot that appears or disappears.

import { describe, expect, it } from 'vitest';
import type { LocalMediaStats, LocalReceiveSlotStats } from '@darktower/sdk-core';

import { formatRate, rates, statsView, type StatsSample } from '../src/lib/debugStats.js';

const NO_DROPS = {} as LocalReceiveSlotStats['drops'];

function rx(slotId: number, packets: number, bytes: number): LocalReceiveSlotStats {
  return {
    kind: 'audio',
    slotId,
    packets,
    bytes,
    framesDecoded: 0,
    queueDepth: 0,
    drops: NO_DROPS,
    localDrops: 0,
    keyframesSeen: undefined,
    unfulfilledMs: undefined,
  };
}

function sample(atMs: number, receive: LocalReceiveSlotStats[], sendBytes = 0): StatsSample {
  const stats: LocalMediaStats = {
    receive,
    send: [
      {
        kind: 'audio',
        packets: sendBytes / 100,
        bytes: sendBytes,
        keyframes: undefined,
        groupsAbandoned: undefined,
      },
    ],
  };
  return { atMs, stats };
}

describe('rates()', () => {
  it('is exact over three snapshots with UNEVEN gaps (1000 ms, then 500 ms)', () => {
    const a = { packets: 0, bytes: 0 };
    const b = { packets: 50, bytes: 10_000 };
    const c = { packets: 80, bytes: 16_000 };
    expect(rates(a, b, 1_000)).toEqual({ packetsPerSec: 50, bitsPerSec: 80_000 });
    expect(rates(b, c, 500)).toEqual({ packetsPerSec: 60, bitsPerSec: 96_000 });
  });

  it('has no rate for a first sample, dt <= 0 or a reset', () => {
    const none = { packetsPerSec: undefined, bitsPerSec: undefined };
    expect(rates(undefined, { packets: 5, bytes: 5 }, 1_000)).toEqual(none);
    expect(rates({ packets: 0, bytes: 0 }, { packets: 5, bytes: 5 }, 0)).toEqual(none);
    expect(rates({ packets: 0, bytes: 0 }, { packets: 5, bytes: 5 }, -10)).toEqual(none);
    expect(rates({ packets: 0, bytes: 0 }, { packets: 5, bytes: 5 }, Number.NaN)).toEqual(none);
    expect(rates({ packets: 9, bytes: 900 }, { packets: 5, bytes: 1_000 }, 1_000)).toEqual(none);
    expect(rates({ packets: 1, bytes: 900 }, { packets: 5, bytes: 100 }, 1_000)).toEqual(none);
  });
});

describe('statsView()', () => {
  it('rates each slot against ITS previous reading; a new slot has none; a vanished slot is gone', () => {
    const first = sample(0, [rx(0, 0, 0), rx(1, 10, 1_000)], 0);
    const second = sample(2_000, [rx(1, 30, 5_000), rx(2, 7, 700)], 4_000);
    const view = statsView(first, second);
    expect(view.receive.map((r) => r.key)).toEqual(['audio:1', 'audio:2']);
    expect(view.receive[0]).toMatchObject({ packetsPerSec: 10, bitsPerSec: 16_000 });
    expect(view.receive[1]).toMatchObject({ packetsPerSec: undefined, bitsPerSec: undefined });
    expect(view.send[0]).toMatchObject({ key: 'audio', packetsPerSec: 20, bitsPerSec: 16_000 });
  });

  it('a first sample has no rates; drops show only non-zero closed tokens', () => {
    const drops = {
      ...NO_DROPS,
      replay_detected: 0,
      truncated: 3,
    } as LocalReceiveSlotStats['drops'];
    const view = statsView(undefined, sample(0, [{ ...rx(4, 1, 1), drops }]));
    expect(view.receive[0]).toMatchObject({ packetsPerSec: undefined, bitsPerSec: undefined });
    expect(view.receive[0]!.drops).toEqual([{ reason: 'truncated', count: 3 }]);
  });
});

describe('formatRate()', () => {
  it('renders "—" for no rate and never NaN or Infinity', () => {
    expect(formatRate(undefined, 'pkt/s')).toBe('—');
    expect(formatRate(Number.NaN, 'pkt/s')).toBe('—');
    expect(formatRate(Number.POSITIVE_INFINITY, 'kbit/s')).toBe('—');
    expect(formatRate(50, 'pkt/s')).toBe('50.0 pkt/s');
    expect(formatRate(96_000, 'kbit/s')).toBe('96.0 kbit/s');
  });
});
