// File: packages/web-app/src/lib/debugStats.ts
//
// The R-35 statistics overlay's arithmetic (story 3): rows projected from the
// SDK's LOCAL snapshot (`localStatsOf(session)`), and rates computed HERE
// from two successive snapshots on the overlay's own clock.
//
// LOCAL ONLY, NEVER EXPORTED. The per-slot bit rates are per-stream size
// series, which ADR-0036 §11 forbids exporting. This module imports only SDK
// TYPES, holds no state (the caller keeps exactly ONE previous sample), and
// writes nowhere: no metric, no telemetry, no test bus, no console, no global.
// `packages/web-app/tests/debugNeverExported.test.ts` enforces that
// mechanically.
//
// EXPLICIT ALLOW-LIST PROJECTION. Every rendered value is named below; nothing
// is spread or iterated generically out of an SDK object, so a field added to
// the snapshot later never reaches the screen by accident.

import type {
  LocalMediaStats,
  LocalReceiveSlotStats,
  LocalSendStreamStats,
} from '@darktower/sdk-core';

/** The overlay's sampling period. Its own constant (not any test-bus cadence). */
export const DEBUG_STATS_SAMPLE_INTERVAL_MS = 1_000;

/** One timed sample: the snapshot and when the overlay read it. */
export interface StatsSample {
  readonly atMs: number;
  readonly stats: LocalMediaStats;
}

/** Packet and bit rate. `undefined` when no honest rate exists (see {@link rates}). */
export interface Rates {
  readonly packetsPerSec: number | undefined;
  readonly bitsPerSec: number | undefined;
}

const NO_RATE: Rates = { packetsPerSec: undefined, bitsPerSec: undefined };

/**
 * THE ONE rate computation, for every row. From two cumulative readings
 * `dtMs` apart (the MEASURED gap, never the nominal period). No rate — never
 * NaN, Infinity or a negative number — when there is no previous reading,
 * when `dtMs <= 0`, or when a counter went backwards (a reset: a recreated
 * egress or lane).
 */
export function rates(
  prev: { readonly packets: number; readonly bytes: number } | undefined,
  next: { readonly packets: number; readonly bytes: number },
  dtMs: number,
): Rates {
  if (prev === undefined || !(dtMs > 0)) return NO_RATE;
  const dPackets = next.packets - prev.packets;
  const dBytes = next.bytes - prev.bytes;
  if (dPackets < 0 || dBytes < 0) return NO_RATE;
  return {
    packetsPerSec: (dPackets * 1_000) / dtMs,
    bitsPerSec: (dBytes * 8 * 1_000) / dtMs,
  };
}

/** One drop reason with a non-zero count. */
export interface DropCount {
  readonly reason: string;
  readonly count: number;
}

/** One receive-slot row, as rendered. */
export interface ReceiveRow extends Rates {
  readonly key: string;
  readonly kind: LocalReceiveSlotStats['kind'];
  readonly slotId: number;
  readonly queueDepth: number;
  readonly framesDecoded: number;
  readonly drops: readonly DropCount[];
  /** `undefined` where not measured (audio). */
  readonly localDrops: number | undefined;
  readonly keyframesSeen: number | undefined;
  readonly unfulfilledMs: number | undefined;
}

/** Keyframes produced for one cause (the SDK's closed `VideoSendCounts['keyframes']` keys). */
export interface KeyframeCount {
  readonly cause: string;
  readonly count: number;
}

/** One sent-stream row, as rendered. */
export interface SendRow extends Rates {
  readonly key: string;
  readonly kind: LocalSendStreamStats['kind'];
  /** Keyframes produced, one entry per cause in the SDK's closed cause set (video only). */
  readonly keyframes: readonly KeyframeCount[] | undefined;
  readonly groupsAbandoned: number | undefined;
}

/** The overlay's whole view. */
export interface StatsView {
  readonly receive: readonly ReceiveRow[];
  readonly send: readonly SendRow[];
}

const receiveKey = (r: LocalReceiveSlotStats): string => `${r.kind}:${r.slotId}`;

function drops(r: LocalReceiveSlotStats): DropCount[] {
  const out: DropCount[] = [];
  // The reason set is the SDK's closed reject vocabulary (a typed record).
  for (const [reason, count] of Object.entries(r.drops)) {
    if (count > 0) out.push({ reason, count });
  }
  return out;
}

/**
 * Project `next` into rows, with rates against `prev` (the ONE previous
 * sample). A slot or stream absent from `prev` (new, or the previous sample is
 * missing) has no rate yet.
 */
export function statsView(prev: StatsSample | undefined, next: StatsSample): StatsView {
  const dtMs = prev === undefined ? 0 : next.atMs - prev.atMs;
  const prevReceive = new Map(prev?.stats.receive.map((r) => [receiveKey(r), r] as const));
  const prevSend = new Map(prev?.stats.send.map((s) => [s.kind, s] as const));
  return {
    receive: next.stats.receive.map((r) => {
      const key = receiveKey(r);
      return {
        key,
        kind: r.kind,
        slotId: r.slotId,
        ...rates(prevReceive.get(key), r, dtMs),
        queueDepth: r.queueDepth,
        framesDecoded: r.framesDecoded,
        drops: drops(r),
        localDrops: r.localDrops,
        keyframesSeen: r.keyframesSeen,
        unfulfilledMs: r.unfulfilledMs,
      };
    }),
    send: next.stats.send.map((s: LocalSendStreamStats) => ({
      key: s.kind,
      kind: s.kind,
      ...rates(prevSend.get(s.kind), s, dtMs),
      // Every cause the SDK reports, so a new cause is shown without an edit here.
      keyframes:
        s.keyframes === undefined
          ? undefined
          : Object.entries(s.keyframes).map(([cause, count]) => ({ cause, count })),
      groupsAbandoned: s.groupsAbandoned,
    })),
  };
}

/** Render a rate for the overlay; "—" when there is none. */
export function formatRate(value: number | undefined, unit: 'pkt/s' | 'kbit/s'): string {
  if (value === undefined || !Number.isFinite(value)) return '—';
  return unit === 'kbit/s' ? `${(value / 1_000).toFixed(1)} kbit/s` : `${value.toFixed(1)} pkt/s`;
}
