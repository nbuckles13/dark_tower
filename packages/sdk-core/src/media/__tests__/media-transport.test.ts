// File: packages/sdk-core/src/media/__tests__/media-transport.test.ts
//
// R-41: unit tests for `MediaTransport` (active/active MH handshake) against
// `MockWebTransport`. Envelopes are built at runtime via protobuf-es; outbound
// frames are decoded by stripping the reused 4-byte BE prefix.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { context, propagation, trace } from '@opentelemetry/api';
import { InMemoryMetricsSink, type MockWebTransport } from '@darktower/test-utils';

import { MediaTransport } from '../MediaTransport.js';
import {
  MediaConnectionError,
  MediaConnectionErrorCode,
} from '../../errors/MediaConnectionError.js';
import {
  configureTelemetry,
  getTracer,
  resetTelemetryForTest,
} from '../../telemetry/telemetryConfig.js';
import { decodeMhClientMessages, makeConnect, waitFor } from './helpers.js';

const URL_A = 'https://mh-a.example:4433';
const URL_B = 'https://mh-b.example:4433';
const URL_C = 'https://mh-c.example:4433';
const JWT = 'meeting-jwt-value';

afterEach(() => {
  resetTelemetryForTest();
  context.disable();
  propagation.disable();
  vi.restoreAllMocks();
});

describe('MediaTransport.connectAll — resolver (R-20/R-21)', () => {
  it('all connect → fires connected per MH, resolves, reports all connected in input order', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const connected: string[] = [];
    const mt = new MediaTransport({ connect: makeConnect(mocks) });
    mt.on('connected', (url) => connected.push(url));

    const p = mt.connectAll([URL_A, URL_B], JWT);
    mocks.get(URL_A)!.simulateReady();
    mocks.get(URL_B)!.simulateReady();
    const reports = await p;

    expect(connected.sort()).toEqual([URL_A, URL_B].sort());
    expect(reports.map((r) => [r.mhUrl, r.state])).toEqual([
      [URL_A, 'connected'],
      [URL_B, 'connected'],
    ]);
    expect(mt.getState(URL_A)).toBe('connected');
  });

  it('partial → resolves, fires failed with MediaConnectionError, reports index-stable', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const failures: MediaConnectionError[] = [];
    const mt = new MediaTransport({ connect: makeConnect(mocks) });
    mt.on('failed', (err) => failures.push(err));

    const p = mt.connectAll([URL_A, URL_B, URL_C], JWT);
    mocks.get(URL_A)!.simulateReady();
    mocks.get(URL_B)!.simulateError(new Error('mh-b refused'));
    mocks.get(URL_C)!.simulateReady();
    const reports = await p;

    // G1: order mirrors input [A,B,C] regardless of settle order.
    expect(reports.map((r) => [r.mhUrl, r.state])).toEqual([
      [URL_A, 'connected'],
      [URL_B, 'failed'],
      [URL_C, 'connected'],
    ]);
    expect(mt.getState(URL_B)).toBe('failed');
    expect(failures).toHaveLength(1);
    expect(failures[0]).toBeInstanceOf(MediaConnectionError);
    expect(failures[0]!.mediaCode).toBe(MediaConnectionErrorCode.Transport);
    expect(failures[0]!.mhUrl).toBe(URL_B);
    // Failed report carries the bounded (non-raw) failure fields.
    const bReport = reports.find((r) => r.mhUrl === URL_B)!;
    expect(bReport.failureCode).toBe(MediaConnectionErrorCode.Transport);
    expect(typeof bReport.failureReason).toBe('string');
  });

  it('all fail → rejects MediaConnectionError(ALL_FAILED); getStatusReports all failed', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({ connect: makeConnect(mocks) });

    const p = mt.connectAll([URL_A, URL_B], JWT);
    mocks.get(URL_A)!.simulateError(new Error('a'));
    mocks.get(URL_B)!.simulateError(new Error('b'));

    await expect(p).rejects.toBeInstanceOf(MediaConnectionError);
    await p.catch((e: MediaConnectionError) => {
      expect(e.mediaCode).toBe(MediaConnectionErrorCode.AllFailed);
      // The aggregate error carries NO mhUrl (R-23: static message, no per-MH concat).
      expect(e.mhUrl).toBeUndefined();
      expect(e.toJSON().mhUrl).toBeUndefined();
    });
    expect(mt.getStatusReports().every((r) => r.state === 'failed')).toBe(true);
  });

  it('default-constructs with prod defaults (no options)', () => {
    const mt = new MediaTransport();
    expect(mt.getState('x')).toBeUndefined();
    expect(mt.getStatusReports()).toEqual([]);
  });

  it('buckets the 3rd MH index as "2+" in mh_connection_total', async () => {
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({ connect: makeConnect(mocks), metricsSink: sink });
    const p = mt.connectAll([URL_A, URL_B, URL_C], JWT);
    [URL_A, URL_B, URL_C].forEach((u) => mocks.get(u)!.simulateReady());
    await p;
    sink.assertCounter('dt_client_mh_connection_total', { mh_index_bucket: '2+' }, 1);
  });

  it('empty media_servers → resolves with []', async () => {
    const mt = new MediaTransport({ connect: makeConnect(new Map()) });
    await expect(mt.connectAll([], JWT)).resolves.toEqual([]);
  });
});

describe('MediaTransport — MhClientMessage envelope (R-20/R-58)', () => {
  it('writes one connectRequest{joinToken} envelope; trace EMPTY when telemetry unconfigured', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({ connect: makeConnect(mocks) });
    const p = mt.connectAll([URL_A], JWT);
    mocks.get(URL_A)!.simulateReady();
    await p;

    const envs = decodeMhClientMessages(mocks.get(URL_A)!.getOutboundBidiWrites(0));
    expect(envs).toHaveLength(1);
    expect(envs[0]!.message.case).toBe('connectRequest');
    expect(envs[0]!.message.case === 'connectRequest' ? envs[0]!.message.value.joinToken : '').toBe(
      JWT,
    );
    expect(envs[0]!.traceParent).toBe('');
    expect(envs[0]!.traceState).toBe('');
  });

  it('populates trace via runInContext under a real active span — trace_id EQUALS the span (R-58)', async () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    const span = getTracer()!.startSpan('dt_client.join');
    const traceId = span.spanContext().traceId;

    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({
      connect: makeConnect(mocks),
      // Mirrors SignalingClient.runInJoinContext: synchronous wrap at the inject site.
      runInContext: (fn) => context.with(trace.setSpan(context.active(), span), fn),
    });
    const p = mt.connectAll([URL_A], JWT);
    mocks.get(URL_A)!.simulateReady();
    await p;
    span.end();

    const env = decodeMhClientMessages(mocks.get(URL_A)!.getOutboundBidiWrites(0))[0]!;
    expect(env.traceParent).toMatch(/^00-[0-9a-f]{32}-[0-9a-f]{16}-0[01]$/);
    // Real-path equality: the MH envelope's trace-id is the active join span's.
    expect(env.traceParent).toContain(traceId);
  });
});

describe('MediaTransport — token redaction (R-23, G3)', () => {
  it('MediaConnectionError from a real failed handshake never exposes the JWT', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({ connect: makeConnect(mocks) });
    const failures: MediaConnectionError[] = [];
    mt.on('failed', (e) => failures.push(e));

    const p = mt.connectAll([URL_A], JWT);
    const mock = mocks.get(URL_A)!;
    // Simulate a transport error whose message echoes the written bytes (incl. the JWT)
    // — the worst-case leak vector — in the createBidi step (the catch path captures it).
    vi.spyOn(mock, 'createBidirectionalStream').mockRejectedValue(
      new Error(`handshake echoed token=${JWT}`),
    );
    mock.simulateReady();
    await expect(p).rejects.toBeInstanceOf(MediaConnectionError);

    const err = failures[0]!;
    const serialized = [
      JSON.stringify(err.toJSON()),
      JSON.stringify(err),
      err.message,
      err.stack ?? '',
    ].join('\n');
    expect(serialized).not.toContain(JWT);
  });
});

describe('MediaTransport — teardown + deadline (@operations)', () => {
  it('disconnect() is idempotent and tears down an in-flight never-ready transport', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({ connect: makeConnect(mocks), connectTimeoutMs: 10_000 });
    const p = mt.connectAll([URL_A], JWT);
    // URL_A never becomes ready; it is registered for teardown before await ready.
    await waitFor(() => mocks.has(URL_A));
    const closeSpy = vi.spyOn(mocks.get(URL_A)!, 'close');

    mt.disconnect();
    mt.disconnect(); // idempotent — second call is a no-op
    expect(closeSpy).toHaveBeenCalledTimes(1);

    // connectAll still settles (the in-flight connectOne early-returns on terminate).
    mocks.get(URL_A)!.simulateError(new Error('closed'));
    await p.catch(() => {});

    // The url stayed 'connecting' (terminate bailed before marking it) → reported
    // FAILED with no failure-info fields.
    const report = mt.getStatusReports()[0]!;
    expect(report.state).toBe('failed');
    expect(report.failureCode).toBeUndefined();
  });

  it('per-MH connect deadline → never-ready MH FAILED(CONNECT_TIMEOUT); connectAll still resolves if another connects', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({ connect: makeConnect(mocks), connectTimeoutMs: 25 });
    const p = mt.connectAll([URL_A, URL_B], JWT);
    mocks.get(URL_A)!.simulateReady(); // A connects
    // B is never made ready → its deadline fires.
    const reports = await p;

    expect(mt.getState(URL_A)).toBe('connected');
    expect(mt.getState(URL_B)).toBe('failed');
    const bReport = reports.find((r) => r.mhUrl === URL_B)!;
    expect(bReport.failureCode).toBe(MediaConnectionErrorCode.ConnectTimeout);
  });

  it('timeout-path error message MATCHES its mediaCode (not the generic transport message) — F1', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({ connect: makeConnect(mocks), connectTimeoutMs: 25 });
    const failures: MediaConnectionError[] = [];
    mt.on('failed', (e) => failures.push(e));

    const p = mt.connectAll([URL_A], JWT); // never made ready → deadline fires
    await p.catch(() => {}); // all-fail rejects; we only care about the per-MH error

    expect(failures).toHaveLength(1);
    expect(failures[0]!.mediaCode).toBe(MediaConnectionErrorCode.ConnectTimeout);
    // The .message reflects the TIMEOUT reason, not the hardcoded transport reason.
    expect(failures[0]!.message).toBe('media handler connect deadline exceeded');
    expect(failures[0]!.message).not.toBe('media handler connection failed');
  });

  it('preserves the transport.ready rejection cause on the MediaConnectionError (symmetric with write-path)', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({ connect: makeConnect(mocks) });
    const failures: MediaConnectionError[] = [];
    mt.on('failed', (e) => failures.push(e));

    const readyCause = new Error('quic handshake refused');
    const p = mt.connectAll([URL_A], JWT);
    mocks.get(URL_A)!.simulateError(readyCause); // rejects transport.ready
    await p.catch(() => {});

    expect(failures).toHaveLength(1);
    expect(failures[0]!.mediaCode).toBe(MediaConnectionErrorCode.Transport);
    // The ready-rejection cause is retained on the (non-enumerable) Error.cause...
    expect(failures[0]!.cause).toBe(readyCause);
    // ...but never crosses the redaction line into the serialized shape.
    expect(JSON.stringify(failures[0]!.toJSON())).not.toContain('quic handshake refused');
  });
});

describe('MediaTransport — metrics (R-25 M1)', () => {
  it('emits dt_client_mh_connection_total per MH with status + mh_index_bucket + label set', async () => {
    const sink = new InMemoryMetricsSink();
    const labels = { client_version: '0.0.0-test', meeting_id_hash: 'abc123', org_id: 'demo' };
    const mocks = new Map<string, MockWebTransport>();
    const mt = new MediaTransport({
      connect: makeConnect(mocks),
      metricsSink: sink,
      metricLabels: labels,
    });
    const p = mt.connectAll([URL_A, URL_B], JWT);
    mocks.get(URL_A)!.simulateReady();
    mocks.get(URL_B)!.simulateError(new Error('b'));
    await p;

    sink.assertCounter(
      'dt_client_mh_connection_total',
      { status: 'success', mh_index_bucket: '0', ...labels },
      1,
    );
    sink.assertCounter(
      'dt_client_mh_connection_total',
      { status: 'failure', mh_index_bucket: '1', ...labels },
      1,
    );
    // No PII labels.
    const keys = new Set(sink.getRecordedMetrics().flatMap((m) => Object.keys(m.labels)));
    expect(keys.has('email')).toBe(false);
    expect(keys.has('user_id')).toBe(false);
  });
});
