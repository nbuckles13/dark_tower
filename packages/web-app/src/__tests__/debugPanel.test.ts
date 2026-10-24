// File: packages/web-app/src/__tests__/debugPanel.test.ts
//
// The local debug panel (story 3 R-7, R-35) as a component, in real Chromium:
// hidden by default, the one toggle and its fail-closed persistence, and the
// statistics overlay's rates computed from SYNTHETIC snapshot sequences on an
// INJECTED clock and timer (no real timers anywhere). The sampler exists only
// while the panel is open: after hide or unmount the snapshot getter is never
// read again.

import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import type { LocalMediaStats, LocalReceiveSlotStats } from '@darktower/sdk-core';
import DebugPanel from '../views/DebugPanel.svelte';
import { DEBUG_PANEL_STORAGE_KEY, type ToggleStorage } from '../lib/debugToggle.js';
import { DEBUG_STATS_SAMPLE_INTERVAL_MS } from '../lib/debugStats.js';

/** An in-memory storage double. */
function memoryStorage(initial: Record<string, string> = {}): ToggleStorage & {
  readonly data: Record<string, string>;
} {
  const data = { ...initial };
  return {
    data,
    getItem: (k) => data[k] ?? null,
    setItem: (k, v) => {
      data[k] = v;
    },
  };
}

/** A storage whose every access throws (blocked storage, privacy mode). */
const throwingStorage: ToggleStorage = {
  getItem: () => {
    throw new Error('storage blocked');
  },
  setItem: () => {
    throw new Error('storage blocked');
  },
};

/** A manual timer: `tick()` runs whatever is currently scheduled. */
function manualTimer() {
  const timers = new Map<number, { fn: () => void; ms: number }>();
  let next = 1;
  return {
    setInterval: (fn: () => void, ms: number) => {
      const id = next++;
      timers.set(id, { fn, ms });
      return id as unknown as ReturnType<typeof setInterval>;
    },
    clearInterval: (h: ReturnType<typeof setInterval>) => {
      timers.delete(h as unknown as number);
    },
    tick: () => {
      for (const { fn } of [...timers.values()]) fn();
    },
    get active() {
      return timers.size;
    },
    periods: () => [...timers.values()].map((t) => t.ms),
  };
}

const NO_DROPS = Object.freeze({}) as LocalReceiveSlotStats['drops'];

function slot(packets: number, bytes: number, extra: Partial<LocalReceiveSlotStats> = {}) {
  return {
    kind: 'video' as const,
    slotId: 3,
    packets,
    bytes,
    framesDecoded: packets,
    queueDepth: 2,
    drops: NO_DROPS,
    localDrops: 0,
    keyframesSeen: 1,
    unfulfilledMs: 0,
    ...extra,
  };
}

function snapshot(packets: number, bytes: number): LocalMediaStats {
  return {
    receive: [slot(packets, bytes)],
    send: [
      {
        kind: 'video',
        packets: packets * 2,
        bytes: bytes * 2,
        keyframes: { directive_start: 1, request: 2, abandonment: 0 },
        groupsAbandoned: 3,
      },
    ],
  };
}

test('hidden by default with empty storage; the toggle shows, hides and persists only the boolean', async () => {
  const storage = memoryStorage();
  const screen = await render(DebugPanel, { storage });
  await expect.element(screen.getByTestId('debug-panel')).not.toBeInTheDocument();
  await screen.getByTestId('debug-panel-toggle').click();
  await expect.element(screen.getByTestId('debug-panel')).toBeInTheDocument();
  expect(storage.data).toEqual({ [DEBUG_PANEL_STORAGE_KEY]: '1' });
  await screen.getByTestId('debug-panel-toggle').click();
  await expect.element(screen.getByTestId('debug-panel')).not.toBeInTheDocument();
  expect(storage.data).toEqual({ [DEBUG_PANEL_STORAGE_KEY]: '0' });
  screen.unmount();
});

test('reads fail CLOSED: only exactly "1" opens it', async () => {
  for (const value of ['0', 'true', '1 ', 'yes', '']) {
    const screen = await render(DebugPanel, {
      storage: memoryStorage({ [DEBUG_PANEL_STORAGE_KEY]: value }),
    });
    await expect.element(screen.getByTestId('debug-panel')).not.toBeInTheDocument();
    screen.unmount();
  }
  const open = await render(DebugPanel, {
    storage: memoryStorage({ [DEBUG_PANEL_STORAGE_KEY]: '1' }),
  });
  await expect.element(open.getByTestId('debug-panel')).toBeInTheDocument();
  // Persisted open never starts the self-check.
  await expect.element(open.getByTestId('self-check-status')).toHaveTextContent('idle');
  open.unmount();
});

test('a throwing storage (getItem AND setItem) breaks nothing: hidden, and the toggle still works', async () => {
  const screen = await render(DebugPanel, { storage: throwingStorage });
  await expect.element(screen.getByTestId('debug-panel')).not.toBeInTheDocument();
  await screen.getByTestId('debug-panel-toggle').click();
  await expect.element(screen.getByTestId('debug-panel')).toBeInTheDocument();
  await screen.getByTestId('debug-panel-toggle').click();
  await expect.element(screen.getByTestId('debug-panel')).not.toBeInTheDocument();
  screen.unmount();
});

test('rates come from successive snapshots on the injected clock, using the MEASURED gap', async () => {
  const timer = manualTimer();
  let now = 0;
  const sequence = [snapshot(0, 0), snapshot(50, 10_000), snapshot(80, 16_000)];
  let calls = 0;
  const screen = await render(DebugPanel, {
    storage: memoryStorage(),
    stats: () => sequence[Math.min(calls++, sequence.length - 1)],
    clock: () => now,
    setInterval: timer.setInterval,
    clearInterval: timer.clearInterval,
  });
  // Hidden: no sampler, no read.
  expect(timer.active).toBe(0);
  expect(calls).toBe(0);
  await screen.getByTestId('debug-panel-toggle').click();
  expect(timer.periods()).toEqual([DEBUG_STATS_SAMPLE_INTERVAL_MS]);
  // First sample (t=0): no previous, so no rate.
  const row = screen.getByTestId('debug-stats-slot-video:3');
  await expect.element(row.getByTestId('pkt-rate')).toHaveTextContent('—');
  await expect.element(row.getByTestId('bit-rate')).toHaveTextContent('—');
  await expect.element(row.getByTestId('queue-depth')).toHaveTextContent('2');
  await expect.element(row.getByTestId('keyframes-seen')).toHaveTextContent('1');
  // t=1000: +50 packets, +10 000 bytes over 1000 ms.
  now = 1_000;
  timer.tick();
  await expect.element(row.getByTestId('pkt-rate')).toHaveTextContent('50.0 pkt/s');
  await expect.element(row.getByTestId('bit-rate')).toHaveTextContent('80.0 kbit/s');
  // t=1500 (an UNEVEN 500 ms gap): +30 packets, +6 000 bytes.
  now = 1_500;
  timer.tick();
  await expect.element(row.getByTestId('pkt-rate')).toHaveTextContent('60.0 pkt/s');
  await expect.element(row.getByTestId('bit-rate')).toHaveTextContent('96.0 kbit/s');
  const send = screen.getByTestId('debug-stats-send-video');
  await expect.element(send.getByTestId('pkt-rate')).toHaveTextContent('120.0 pkt/s');
  await expect.element(send.getByTestId('bit-rate')).toHaveTextContent('192.0 kbit/s');
  await expect
    .element(send.getByTestId('keyframes'))
    .toHaveTextContent('directive_start: 1 request: 2 abandonment: 0');
  await expect.element(send.getByTestId('groups-abandoned')).toHaveTextContent('3');

  // HIDE: the sampler is cleared and the getter is never read again.
  await screen.getByTestId('debug-panel-toggle').click();
  expect(timer.active).toBe(0);
  const readsAtHide = calls;
  timer.tick();
  expect(calls).toBe(readsAtHide);
  screen.unmount();
});

test('unmount clears the sampler; a zero time gap and a counter reset render "—" (each after a real rate), never NaN or Infinity', async () => {
  const timer = manualTimer();
  let now = 0;
  const sequence = [
    snapshot(10, 1_000),
    snapshot(20, 2_000),
    snapshot(30, 3_000),
    snapshot(40, 4_000),
    snapshot(5, 500),
  ];
  let calls = 0;
  const screen = await render(DebugPanel, {
    storage: memoryStorage({ [DEBUG_PANEL_STORAGE_KEY]: '1' }),
    stats: () => sequence[Math.min(calls++, sequence.length - 1)],
    clock: () => now,
    setInterval: timer.setInterval,
    clearInterval: timer.clearInterval,
  });
  const rate = screen.getByTestId('debug-stats-slot-video:3').getByTestId('pkt-rate');
  now = 1_000;
  timer.tick();
  await expect.element(rate).toHaveTextContent('10.0 pkt/s');
  // Same instant (dt = 0) after a real rate.
  timer.tick();
  await expect.element(rate).toHaveTextContent('—');
  now = 2_000;
  timer.tick();
  await expect.element(rate).toHaveTextContent('10.0 pkt/s');
  // A reset (counters went backwards) after a real rate.
  now = 3_000;
  timer.tick();
  await expect.element(rate).toHaveTextContent('—');
  await expect.element(screen.getByTestId('debug-panel')).not.toHaveTextContent('NaN');
  await expect.element(screen.getByTestId('debug-panel')).not.toHaveTextContent('Infinity');
  screen.unmount();
  expect(timer.active).toBe(0);
  const readsAtUnmount = calls;
  timer.tick();
  expect(calls).toBe(readsAtUnmount);
});

test('a THROWING stats getter is "statistics unavailable", distinct from "media not started"', async () => {
  const timer = manualTimer();
  const screen = await render(DebugPanel, {
    storage: memoryStorage({ [DEBUG_PANEL_STORAGE_KEY]: '1' }),
    stats: () => {
      throw new Error('sdk defect');
    },
    setInterval: timer.setInterval,
    clearInterval: timer.clearInterval,
  });
  await expect
    .element(screen.getByTestId('debug-stats-error'))
    .toHaveTextContent('statistics unavailable');
  await expect.element(screen.getByTestId('debug-stats-empty')).not.toBeInTheDocument();
  screen.unmount();
});

test('before media starts the overlay says so; drops render by closed token', async () => {
  const timer = manualTimer();
  const current: { value?: LocalMediaStats } = {};
  const screen = await render(DebugPanel, {
    storage: memoryStorage({ [DEBUG_PANEL_STORAGE_KEY]: '1' }),
    stats: () => current.value,
    setInterval: timer.setInterval,
    clearInterval: timer.clearInterval,
  });
  await expect
    .element(screen.getByTestId('debug-stats-empty'))
    .toHaveTextContent('media not started');
  current.value = {
    receive: [
      slot(1, 1, {
        drops: { ...NO_DROPS, replay_detected: 2 } as LocalReceiveSlotStats['drops'],
        localDrops: 4,
      }),
    ],
    send: [],
  };
  timer.tick();
  const drops = screen.getByTestId('debug-stats-slot-video:3').getByTestId('drops');
  await expect.element(drops).toHaveTextContent('replay_detected: 2');
  await expect.element(drops).toHaveTextContent('local: 4');
  screen.unmount();
});
