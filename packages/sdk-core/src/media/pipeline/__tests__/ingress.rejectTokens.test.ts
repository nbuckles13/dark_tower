// File: packages/sdk-core/src/media/pipeline/__tests__/ingress.rejectTokens.test.ts
//
// THE EIGHT STRUCTURAL CODEC TOKENS ARE EMITTED INDIVIDUALLY.
//
// ---------------------------------------------------------------------------
// WHY THIS IS A SEPARATE, LOUD TEST
// ---------------------------------------------------------------------------
//
// R-25 settles the granularity question verbatim: *"'bucket' names a FAMILY of
// `reason` values, not one collapsed `decode_reject` label"*. Two things break
// if the tokens are collapsed, and only one of them is obvious:
//
//   * `sum by(reason)` stops comparing between the media handler and the client,
//     so the two ends of one hop can no longer be reconciled on an incident
//     timeline; and
//   * **`unknown_version` stops being individually visible, which is R-31's ONLY
//     lever for detecting a version-skewed rollback.** Rollback for this feature
//     is redeploy-only with no finer-grained control, so that counter is the
//     whole detection story.
//
// ---------------------------------------------------------------------------
// DRIVEN BY THE VECTORS, NOT BY A HAND-WRITTEN TABLE
// ---------------------------------------------------------------------------
//
// The frames are the SSoT's own `frame_hex` bytes and the expected tokens are
// the SSoT's own `reject_reason` values. Nothing is retyped here: a third
// hand-written list of the sixteen spellings would have no guard behind it, and
// the whole point of `rejectReason.ts`'s set-equality test is that exactly ONE
// in-tree mirror earns that status.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { ReplayWindow, TransmitKeyCache } from '../../frame/receivePath.js';
import { hexToBytes } from '../../frame/hex.js';
import { JoinResponseKekSource } from '../../setup/kekSource.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { FirstMediaObserver } from '../../setup/measurement.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import { HopSequenceMonitor } from '../hopSequenceMonitor.js';
import { IngressPipeline } from '../ingress.js';
import { loadFrameVectors } from '../../frame/__tests__/frameVectors.js';

const file = loadFrameVectors();

/** Every `decode_reject`-kind row, driven straight off the SSoT. */
const decodeRejectRows = file.vectors.filter((row) => row.kind === 'decode_reject');

/** The tokens the SSoT marks as belonging to the codec layer. */
const structuralTokens = file.reject_reasons
  .filter((r) => r.layer === 'codec' && r.has_vector)
  .map((r) => r.token);

function makeIngress(): { sink: InMemoryMetricsSink; ingress: IngressPipeline } {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: '0.0.0-test', orgId: 'demo' }, sink);
  const kek = new JoinResponseKekSource();
  const ingress = new IngressPipeline({
    metrics,
    roster: new RosterIdentityKeys(8),
    keys: kek,
    cache: new TransmitKeyCache(8),
    replay: new ReplayWindow(8, 64),
    hopMonitor: new HopSequenceMonitor([0]),
    firstMedia: new FirstMediaObserver(metrics, () => 0),
  });
  return { sink, ingress };
}

function reasonsFrom(sink: InMemoryMetricsSink): string[] {
  return sink
    .getRecordedMetrics()
    .filter((m) => m.name === 'dt_client_media_frames_dropped_total')
    .map((m) => String(m.labels.reason));
}

describe('structural codec rejects reach the drop counter individually', () => {
  it('exercises every structural token the vectors declare — the scope fails itself', () => {
    // A parameterised suite that silently covered three of eight would report
    // green while five tokens shipped unobservable.
    expect(decodeRejectRows.length).toBeGreaterThan(0);
    const covered = new Set(decodeRejectRows.map((row) => row.reject_reason));
    for (const token of structuralTokens) {
      expect(covered.has(token), `no vector row exercises the ${token} token`).toBe(true);
    }
  });

  for (const row of decodeRejectRows) {
    it(`emits ${String(row.reject_reason)} verbatim for the ${row.name} vector`, async () => {
      const { sink, ingress } = makeIngress();
      await ingress.accept(hexToBytes(row.frame_hex));

      // VERBATIM. No mapping table, no bucket, no `other`, no layer-based
      // collapse: the label value IS `FrameRejectedError.rejectReason`, and the
      // expected value IS the SSoT's `reject_reason` for this row.
      expect(reasonsFrom(sink)).toEqual([row.reject_reason]);
      // Counted at the wire regardless, so the identity holds.
      expect(
        sink.getRecordedMetrics().filter((m) => m.name === 'dt_client_media_frames_received_total')
          .length,
      ).toBe(1);
      expect(
        sink.getRecordedMetrics().filter((m) => m.name === 'dt_client_media_frames_accepted_total')
          .length,
      ).toBe(0);
    });
  }

  it('keeps unknown_version distinguishable from every other structural token', () => {
    // Named explicitly rather than left to the loop above, because this is the
    // one whose collapse has a consequence no dashboard would reveal: a
    // version-skewed rollback presenting as an undifferentiated decode failure.
    const rows = decodeRejectRows.filter((row) => row.reject_reason === 'unknown_version');
    expect(rows).toHaveLength(1);
    const others = new Set(
      decodeRejectRows.map((row) => row.reject_reason).filter((r) => r !== 'unknown_version'),
    );
    expect(others.has('unknown_version')).toBe(false);
    expect(others.size).toBeGreaterThan(1);
  });
});
