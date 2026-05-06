import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '../InMemoryMetricsSink.js';

describe('InMemoryMetricsSink', () => {
  it('subset-filter assertions sum across matching label combinations and ignore extra labels', () => {
    const sink = new InMemoryMetricsSink();
    sink.counter('dt_client_test_join_total', { outcome: 'success', client_version: '0.1.0' });
    sink.counter('dt_client_test_join_total', { outcome: 'success', mh_index: '0' });
    sink.counter('dt_client_test_join_total', { outcome: 'failure', mh_index: '0' });

    // Subset filter: 'outcome=success' matches the first two regardless of extras.
    sink.assertCounter('dt_client_test_join_total', { outcome: 'success' }, 2);
    sink.assertCounterAtLeast('dt_client_test_join_total', { outcome: 'success' }, 1);

    // Read API returns 0 for no match (never throws).
    expect(sink.getCounter('dt_client_test_join_total', { outcome: 'unknown' })).toBe(0);

    // Histogram and gauge happy-paths.
    sink.histogram('dt_client_test_handshake_duration_seconds', { mh: 'mh-0' }, 0.05);
    sink.histogram('dt_client_test_handshake_duration_seconds', { mh: 'mh-0' }, 0.07);
    sink.assertHistogramObserved('dt_client_test_handshake_duration_seconds', { mh: 'mh-0' }, 2);
    expect(
      sink.getHistogramObservations('dt_client_test_handshake_duration_seconds', { mh: 'mh-0' }),
    ).toEqual([0.05, 0.07]);

    sink.gauge('dt_client_test_active_streams', { kind: 'mc' }, 3);
    sink.gauge('dt_client_test_active_streams', { kind: 'mc' }, 5);
    sink.assertGaugeInRange('dt_client_test_active_streams', { kind: 'mc' }, { min: 1, max: 10 });
    expect(sink.getGauge('dt_client_test_active_streams', { kind: 'mc' })).toBe(5);

    // Assertion APIs throw on missing target.
    expect(() => sink.assertCounter('dt_client_test_missing', {}, 0)).toThrow(/no counter entries/);
    expect(() => sink.assertGaugeInRange('dt_client_test_missing', {}, { min: 0, max: 1 })).toThrow(
      /no gauge entries/,
    );
  });

  it('stable label-key handling: insertion order of label keys does not affect lookup', () => {
    const sink = new InMemoryMetricsSink();
    sink.counter('dt_client_test_join_total', { a: '1', b: '2' });
    sink.counter('dt_client_test_join_total', { b: '2', a: '1' });
    // Two distinct emissions sharing the same logical label set.
    sink.assertCounter('dt_client_test_join_total', { a: '1', b: '2' }, 2);
    sink.assertCounter('dt_client_test_join_total', { b: '2', a: '1' }, 2);
  });

  it('subset-filter on both read and assertion APIs: empty labels matches all; partial labels aggregate across extras', () => {
    const sink = new InMemoryMetricsSink();
    sink.counter('dt_client_test_join_total', { outcome: 'success' });
    sink.counter('dt_client_test_join_total', { outcome: 'failure' });
    sink.counter('dt_client_test_join_total', { outcome: 'success', mh_index: '2' });

    // Assertion API: empty {} subset matches all three; partial label
    // {outcome:'success'} matches the two success entries (one without
    // mh_index, one with mh_index=2).
    sink.assertCounter('dt_client_test_join_total', {}, 3);
    sink.assertCounter('dt_client_test_join_total', { outcome: 'success' }, 2);

    // Read API: same subset semantics — single coherent semantic per @observability Path A.
    expect(sink.getCounter('dt_client_test_join_total', {})).toBe(3);
    expect(sink.getCounter('dt_client_test_join_total', { outcome: 'success' })).toBe(2);
    expect(sink.getCounter('dt_client_test_join_total', { outcome: 'failure' })).toBe(1);

    // clear() resets all state.
    sink.clear();
    expect(sink.getRecordedMetrics()).toHaveLength(0);
    expect(sink.getCounter('dt_client_test_join_total', { outcome: 'success' })).toBe(0);
  });
});
