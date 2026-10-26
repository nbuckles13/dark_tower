// File: packages/sdk-core/src/media/lifecycle/__tests__/keyframePolicy.test.ts
//
// Story 3 C-A2 (R-10, R-11, R-12, amendment 4): the keyframe gate. Injected
// clock and timer only — no wall time anywhere.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { KEYFRAME_MIN_INTERVAL_FLOOR_MS } from '../../../config/clientConfig.js';
import { MediaKindMetrics } from '../../setup/mediaMetrics.js';
import { MediaKind } from '../../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { KeyframeGate } from '../keyframePolicy.js';

const A = 'https://mh-a';
const B = 'https://mh-b';

/** A manual timer queue driven by an injected clock. */
function manualTime() {
  let now = 0;
  let nextId = 1;
  const timers = new Map<number, { at: number; fn: () => void }>();
  return {
    now: () => now,
    setTimeout: (fn: () => void, ms: number) => {
      const id = nextId++;
      timers.set(id, { at: now + ms, fn });
      return id as unknown as ReturnType<typeof setTimeout>;
    },
    clearTimeout: (h: ReturnType<typeof setTimeout>) => {
      timers.delete(h as unknown as number);
    },
    pending: () => timers.size,
    advance(ms: number) {
      const until = now + ms;
      for (;;) {
        const due = [...timers.entries()]
          .filter(([, t]) => t.at <= until)
          .sort((x, y) => x[1].at - y[1].at)[0];
        if (!due) break;
        timers.delete(due[0]);
        now = due[1].at;
        due[1].fn();
      }
      now = until;
    },
  };
}

function rig(opts: { awaiting?: number } = {}) {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaKindMetrics(
    { clientVersion: 'v', orgId: 'o' },
    MediaKind.VIDEO_CAMERA,
    sink,
  );
  const time = manualTime();
  let gop: number | undefined;
  let awaiting = opts.awaiting ?? 0;
  let produced = 0;
  const warnings: string[] = [];
  const gate = new KeyframeGate({
    metrics,
    clock: time.now,
    setTimeout: time.setTimeout,
    clearTimeout: time.clearTimeout,
    currentGop: () => gop,
    targetsAwaitingGroup: () => awaiting,
    produceKeyframe: () => {
      produced += 1;
    },
    warn: (m) => warnings.push(m),
  });
  const count = (name: string, labels: Record<string, string> = {}): number =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === name && Object.entries(labels).every(([k, v]) => m.labels[k] === v))
      .reduce((n, m) => n + m.value, 0);
  return {
    gate,
    time,
    count,
    warnings,
    produced: () => produced,
    setGop: (g: number | undefined) => {
      gop = g;
    },
    setAwaiting: (n: number) => {
      awaiting = n;
    },
    /** Simulate the requested key actually encoding: the GoP advances. */
    emitKey: () => {
      gop = (gop ?? -1) + 1;
      gate.keyChunkEmitted();
    },
  };
}

const RECEIVED = 'dt_client_media_keyframe_requests_received_total';
const PRODUCED = 'dt_client_media_keyframes_produced_total';
const CLAMPED = 'dt_client_media_keyframe_min_interval_clamped_total';

describe('KeyframeGate — request acceptance (untrusted input)', () => {
  it('honours a request at the current counter from a targeted handler', () => {
    const r = rig();
    r.gate.setDirective([A], 500);
    r.setGop(4);
    expect(r.gate.request(A, 4)).toBe('honoured');
    expect(r.produced()).toBe(1);
    expect(r.count(PRODUCED, { source: 'request' })).toBe(1);
  });

  it('a counter AHEAD of current is honoured and counted as honoured_counter_ahead', () => {
    const r = rig();
    r.gate.setDirective([A], 500);
    r.setGop(4);
    expect(r.gate.request(A, 9)).toBe('honoured_counter_ahead');
    expect(r.count(RECEIVED, { outcome: 'honoured_counter_ahead' })).toBe(1);
    expect(r.produced()).toBe(1);
  });

  it('a counter BEHIND is stale_counter and produces nothing', () => {
    const r = rig();
    r.gate.setDirective([A], 500);
    r.setGop(4);
    expect(r.gate.request(A, 3)).toBe('stale_counter');
    expect(r.produced()).toBe(0);
  });

  it.each([
    ['a handler not in the CURRENT target set', [B], false],
    ['an undirected stream (empty targets: unwatched or server-muted)', [], false],
    ['a client-muted stream', [A], true],
  ] as const)(
    '%s is not_sending — a request never starts capture or makes it send',
    (_n, targets, muted) => {
      const r = rig();
      r.gate.setDirective(targets, 500);
      r.gate.setMuted(muted);
      r.setGop(0);
      expect(r.gate.request(A, 0)).toBe('not_sending');
      expect(r.produced()).toBe(0);
      expect(r.count(RECEIVED, { outcome: 'not_sending' })).toBe(1);
    },
  );

  it('a burst of stale and duplicate requests at one counter yields ONE keyframe (multi-handler coalescing)', () => {
    const r = rig();
    r.gate.setDirective([A, B], 500);
    r.setGop(7);
    r.gate.request(A, 7);
    r.emitKey(); // gop 8
    for (let i = 0; i < 10; i += 1) {
      r.gate.request(A, 7);
      r.gate.request(B, 7);
    }
    r.time.advance(10_000);
    expect(r.produced()).toBe(1);
    expect(r.count(PRODUCED)).toBe(1);
  });
});

describe('KeyframeGate — the effective interval', () => {
  it('is max(directed, compiled floor); a below-floor value is clamped, counted per directive and WARNed once', () => {
    const r = rig();
    r.gate.setDirective([A], 50);
    r.gate.setDirective([A], 50);
    expect(r.gate.effectiveIntervalMs).toBe(KEYFRAME_MIN_INTERVAL_FLOOR_MS);
    expect(r.count(CLAMPED)).toBe(2);
    expect(r.warnings).toHaveLength(1);
    expect(r.warnings[0]).toMatch(/directed min_keyframe_interval_ms 50 .*floor 100/);
    r.gate.setDirective([A], 700);
    expect(r.gate.effectiveIntervalMs).toBe(700);
    expect(r.count(CLAMPED)).toBe(2);
  });

  it('the clamp actually bounds keying at the floor', () => {
    const r = rig();
    r.gate.setDirective([A], 1);
    r.setGop(0);
    r.gate.request(A, 0);
    r.emitKey();
    expect(r.gate.request(A, 1)).toBe('min_interval');
    r.time.advance(KEYFRAME_MIN_INTERVAL_FLOOR_MS - 1);
    expect(r.produced()).toBe(1);
    r.time.advance(1);
    expect(r.produced()).toBe(2);
  });

  it('an UNSOLICITED key chunk restarts the interval: a held deferral cannot fire right behind it (security S2)', () => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A], 500);
    r.setGop(0);
    r.gate.request(A, 0);
    r.emitKey(); // t=0, gop 1
    r.time.advance(400);
    expect(r.gate.request(A, 1)).toBe('min_interval'); // held, would fire at 500
    r.time.advance(50); // t=450: the platform emits an unsolicited key
    r.emitKey(); // gop 2, clock restarts at 450
    r.time.advance(100); // t=550: original fire time passed
    expect(r.produced()).toBe(1);
    // The held request is now stale (gop 1 < 2): dropped at fire, zero extra.
    r.time.advance(1_000);
    expect(r.produced()).toBe(1);
  });
});

describe('KeyframeGate — ONE deferral per stream', () => {
  it('a request inside the interval is held (min_interval) and fires once the interval elapses', () => {
    const r = rig();
    r.gate.setDirective([A], 500);
    r.setGop(0);
    r.gate.request(A, 0);
    r.emitKey();
    expect(r.gate.request(A, 1)).toBe('min_interval');
    expect(r.gate.hasDeferral).toBe(true);
    expect(r.time.pending()).toBe(1);
    r.time.advance(500);
    expect(r.produced()).toBe(2);
    expect(r.count(PRODUCED, { source: 'request' })).toBe(2);
  });

  it('a request and a too-soon self-key share the one deferral: ONE keyframe, counted ONCE under the creating cause', () => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A], 500);
    r.setGop(0);
    r.gate.request(A, 0);
    r.emitKey();
    r.gate.request(A, 1); // creates the deferral
    r.gate.selfKey('abandonment'); // merges
    r.gate.selfKey('directive_start'); // merges
    expect(r.time.pending()).toBe(1);
    r.time.advance(500);
    expect(r.produced()).toBe(2);
    expect(r.count(PRODUCED, { source: 'request' })).toBe(2);
    expect(r.count(PRODUCED, { source: 'abandonment' })).toBe(0);
    expect(r.count(PRODUCED, { source: 'directive_start' })).toBe(0);
  });

  it('when the creator lapses at fire time, a surviving merged cause fires it, counted under the survivor', () => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A], 500);
    r.setGop(0);
    r.gate.request(A, 0);
    r.emitKey(); // gop 1
    r.gate.request(A, 1); // creator
    r.gate.selfKey('abandonment'); // merged
    r.setGop(5); // another group advanced the counter: the request is now stale
    r.time.advance(500);
    expect(r.produced()).toBe(2);
    expect(r.count(PRODUCED, { source: 'abandonment' })).toBe(1);
  });

  it('a held request whose counter fell behind by fire time is dropped: zero extra keyframes', () => {
    const r = rig();
    r.gate.setDirective([A], 500);
    r.setGop(0);
    r.gate.request(A, 0);
    r.emitKey(); // gop 1
    r.gate.request(A, 1);
    r.setGop(2);
    r.time.advance(500);
    expect(r.produced()).toBe(1);
  });

  it('a held self-key with no target still awaiting a group at fire time is dropped', () => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A], 500);
    r.gate.selfKey('directive_start');
    r.emitKey();
    r.gate.selfKey('abandonment'); // held
    r.setAwaiting(0);
    r.time.advance(500);
    expect(r.produced()).toBe(1);
  });

  it.each([
    ['withdrawal (empty targets)', (r: ReturnType<typeof rig>) => r.gate.setDirective([], 500)],
    ['client mute', (r: ReturnType<typeof rig>) => r.gate.setMuted(true)],
  ])('is DISCARDED on %s', (_n, edge) => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A], 500);
    r.setGop(0);
    r.gate.request(A, 0);
    r.emitKey();
    r.gate.request(A, 1);
    edge(r);
    expect(r.gate.hasDeferral).toBe(false);
    expect(r.time.pending()).toBe(0);
    r.gate.setMuted(false);
    r.gate.setDirective([A], 500);
    r.time.advance(10_000);
    expect(r.produced()).toBe(1);
  });

  it('a target-set change drops a held request whose handler LEFT; a request from a still-targeted handler survives', () => {
    const r = rig();
    r.gate.setDirective([A, B], 500);
    r.setGop(0);
    r.gate.request(A, 0);
    r.emitKey();
    r.gate.request(A, 1);
    r.gate.setDirective([B], 500);
    expect(r.gate.hasDeferral).toBe(false);
    r.gate.setDirective([A, B], 500);
    r.gate.request(A, 1);
    r.gate.request(B, 1);
    r.gate.setDirective([B], 500);
    expect(r.gate.hasDeferral).toBe(true);
    r.time.advance(500);
    expect(r.produced()).toBe(2);
  });
});

describe('KeyframeGate — self-keying (a target with no group in progress)', () => {
  it('a directed start keys with no request, counted directive_start', () => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A], 500);
    r.gate.selfKey('directive_start');
    expect(r.produced()).toBe(1);
    expect(r.count(PRODUCED, { source: 'directive_start' })).toBe(1);
  });

  it('a stream whose targets all have a group in progress never self-keys', () => {
    const r = rig({ awaiting: 0 });
    r.gate.setDirective([A], 500);
    r.gate.selfKey('directive_start');
    r.gate.selfKey('abandonment');
    r.time.advance(10_000);
    expect(r.produced()).toBe(0);
  });

  it('a publisher-decided abandonment counts as abandonment, NEVER directive_start, and is held (never dropped) inside the interval', () => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A], 500);
    r.gate.selfKey('directive_start');
    r.emitKey();
    r.time.advance(100);
    r.gate.selfKey('abandonment');
    expect(r.produced()).toBe(1);
    r.time.advance(400);
    expect(r.produced()).toBe(2);
    expect(r.count(PRODUCED, { source: 'abandonment' })).toBe(1);
    expect(r.count(PRODUCED, { source: 'directive_start' })).toBe(1);
  });

  it('a muted or undirected stream never self-keys', () => {
    const r = rig({ awaiting: 1 });
    r.gate.selfKey('directive_start');
    r.gate.setDirective([A], 500);
    r.gate.setMuted(true);
    r.gate.selfKey('directive_start');
    expect(r.produced()).toBe(0);
  });

  it('close() stops every timer', () => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A], 500);
    r.gate.selfKey('directive_start');
    r.gate.selfKey('abandonment');
    r.gate.close();
    expect(r.time.pending()).toBe(0);
    expect(r.gate.request(A, 0)).toBe('not_sending');
  });
});

describe('KeyframeGate — Gate-3 additions', () => {
  it('multi-handler coalescing BEFORE the key chunk lands: the in-interval requests are held, then dropped stale at fire', () => {
    const r = rig();
    r.gate.setDirective([A, B], 500);
    r.setGop(7);
    expect(r.gate.request(A, 7)).toBe('honoured');
    // The key has been asked for but not yet encoded: every handler requests
    // at the SAME counter.
    expect(r.gate.request(B, 7)).toBe('min_interval');
    for (let i = 0; i < 5; i += 1) expect(r.gate.request(A, 7)).toBe('min_interval');
    expect(r.gate.hasDeferral).toBe(true);
    r.emitKey(); // gop 8: the one keyframe reaches every target
    r.time.advance(10_000);
    expect(r.produced()).toBe(1);
    expect(r.count(PRODUCED)).toBe(1);
  });

  it('a held self-key across an unsolicited key chunk RE-ARMS and still FIRES once, counted under its cause', () => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A], 500);
    r.gate.selfKey('directive_start');
    r.emitKey(); // t=0
    r.time.advance(100);
    r.gate.selfKey('abandonment'); // held, due at 500
    r.time.advance(300); // t=400
    r.emitKey(); // unsolicited: interval restarts, due at 900
    r.time.advance(100); // t=500: the original fire time
    expect(r.produced()).toBe(1);
    r.time.advance(399); // t=899
    expect(r.produced()).toBe(1);
    r.time.advance(1); // t=900
    expect(r.produced()).toBe(2);
    expect(r.count(PRODUCED, { source: 'abandonment' })).toBe(1);
  });

  it('a request FLOOD inside one interval keeps the deferral bounded and yields ONE keyframe (security F2)', () => {
    const r = rig({ awaiting: 1 });
    r.gate.setDirective([A, B], 500);
    r.setGop(0);
    r.gate.request(A, 0);
    r.emitKey(); // gop 1
    for (let i = 0; i < 10_000; i += 1) r.gate.request(A, 1 + (i % 3));
    for (let i = 0; i < 100; i += 1) r.gate.selfKey('abandonment');
    expect(r.gate.deferredCauseCount).toBeLessThanOrEqual(2);
    r.time.advance(500);
    expect(r.produced()).toBe(2);
  });

  it('caps the effective interval at the compiled ceiling (no timer overflow spin)', () => {
    const r = rig();
    r.gate.setDirective([A], 2 ** 32 - 1);
    expect(r.gate.effectiveIntervalMs).toBe(10_000);
  });
});

describe('KeyframeGate — producedBySource (story 3 C-C1, the test bus sendCounts)', () => {
  it('counts every produced keyframe under its cause, EQUAL to the metric, and returns a copy', () => {
    const r = rig({ awaiting: 1 });
    expect(r.gate.producedBySource()).toEqual({ directive_start: 0, request: 0, abandonment: 0 });
    r.gate.setDirective([A], 500);
    r.gate.selfKey('directive_start');
    r.emitKey();
    r.time.advance(500);
    r.gate.selfKey('abandonment');
    r.emitKey();
    r.time.advance(500);
    r.gate.request(A, 1);
    const snapshot = r.gate.producedBySource();
    expect(snapshot).toEqual({ directive_start: 1, request: 1, abandonment: 1 });
    // The same call site as the metric: the two cannot disagree.
    for (const [source, n] of Object.entries(snapshot)) {
      expect(r.count(PRODUCED, { source })).toBe(n);
    }
    // A copy: a reader holding it never sees it move.
    r.time.advance(500);
    r.gate.request(A, 2);
    expect(snapshot.request).toBe(1);
    expect(r.gate.producedBySource().request).toBe(2);
  });

  it('a request that produces nothing (stale, held, not sending) counts nothing', () => {
    const r = rig({ awaiting: 1 });
    r.gate.request(A, 0); // not directed: not_sending
    r.gate.setDirective([A], 500);
    r.setGop(5);
    r.gate.request(A, 4); // stale
    expect(r.gate.producedBySource()).toEqual({ directive_start: 0, request: 0, abandonment: 0 });
  });
});
