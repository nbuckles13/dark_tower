// File: packages/sdk-core/src/media/__tests__/media-transport.test.ts
//
// R-41: unit tests for `MediaTransport` against `MockWebTransport` — the
// handshake, envelope, redaction, deadline, metric and datagram contracts,
// driven through the handler-set reconcile (`applyHandlerSet` + `arm`, ADR-0039
// §4). The reconcile gates themselves (diff, removal, close codes, park, loss
// reports, empty set) live in `reconcile.test.ts`. Envelopes are built at
// runtime via protobuf-es; outbound frames are decoded by stripping the reused
// 4-byte BE prefix.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { context, trace } from '@opentelemetry/api';
import { InMemoryMetricsSink, type MockWebTransport } from '@darktower/test-utils';

import { MediaTransport } from '../MediaTransport.js';
import type { MediaTransportOptions } from '../events.js';
import { handlerRef } from '../handlerSet.js';
import type { MhConnectionStatusReport } from '../../signaling/events.js';
import {
  MediaConnectionError,
  MediaConnectionErrorCode,
} from '../../errors/MediaConnectionError.js';
import {
  configureTelemetry,
  getMeter,
  getTracer,
  resetTelemetryForTest,
} from '../../telemetry/telemetryConfig.js';
import { exportCalls } from '../../telemetry/__tests__/otlpExporterMock.js';
import { decodeMhClientMessages, intakeOf, makeConnect, waitFor } from './helpers.js';

const URL_A = 'https://mh-a.example:4433';
const URL_B = 'https://mh-b.example:4433';
const URL_C = 'https://mh-c.example:4433';
const REF_A = handlerRef(1);
const REF_B = handlerRef(2);
const REF_C = handlerRef(3);
const JWT = 'meeting-jwt-value';

afterEach(async () => {
  await resetTelemetryForTest();
  vi.restoreAllMocks();
});

/**
 * A transport whose join-time set is `urls` (refs 1..n in order), armed with
 * `JWT` — the reconcile's join path.
 */
function joinTransport(
  urls: readonly string[],
  options: MediaTransportOptions = {},
): {
  mt: MediaTransport;
  mocks: Map<string, MockWebTransport>;
  reports: MhConnectionStatusReport[];
  armed: () => Promise<void>;
} {
  const mocks = new Map<string, MockWebTransport>();
  const mt = new MediaTransport({ connect: makeConnect(mocks), ...options });
  const reports: MhConnectionStatusReport[] = [];
  mt.on('status', (r) => reports.push(r));
  mt.applyHandlerSet(
    intakeOf(
      1n,
      urls.map((url, i) => ({ ref: i + 1, url })),
    ),
  );
  let armedPromise: Promise<void> | undefined;
  return {
    mt,
    mocks,
    reports,
    armed: () => (armedPromise ??= mt.arm(JWT)),
  };
}

describe('MediaTransport.arm — the join-time dials (R-20/R-21)', () => {
  it('all connect → fires connected per ref, resolves, reports CONNECTED per ref', async () => {
    const { mt, mocks, reports, armed } = joinTransport([URL_A, URL_B]);
    const connected: string[] = [];
    mt.on('connected', (v) => connected.push(v.url));

    const p = armed();
    mocks.get(URL_A)!.simulateReady();
    mocks.get(URL_B)!.simulateReady();
    await p;

    expect(connected.sort()).toEqual([URL_A, URL_B].sort());
    expect(reports.map((r) => [r.handlerRef, r.state]).sort()).toEqual([
      [REF_A, 'connected'],
      [REF_B, 'connected'],
    ]);
    expect(mt.getState(REF_A)).toBe('connected');
  });

  it('partial → resolves, fires failed with MediaConnectionError, reports FAILED with bounded fields', async () => {
    const { mt, mocks, reports, armed } = joinTransport([URL_A, URL_B, URL_C]);
    const failures: MediaConnectionError[] = [];
    mt.on('failed', (err) => failures.push(err));

    const p = armed();
    mocks.get(URL_A)!.simulateReady();
    mocks.get(URL_B)!.simulateError(new Error('mh-b refused'));
    mocks.get(URL_C)!.simulateReady();
    await p;

    // A failed join-time dial is LOST (a redial is pending), never `connected`.
    expect(mt.getState(REF_B)).toBe('lost');
    expect(failures).toHaveLength(1);
    expect(failures[0]).toBeInstanceOf(MediaConnectionError);
    expect(failures[0]!.mediaCode).toBe(MediaConnectionErrorCode.Transport);
    expect(failures[0]!.mhUrl).toBe(URL_B);
    // The FAILED report carries the bounded (non-raw) failure fields.
    const bReport = reports.find((r) => r.handlerRef === REF_B)!;
    expect(bReport.state).toBe('failed');
    expect(bReport.failureCode).toBe(MediaConnectionErrorCode.Transport);
    expect(typeof bReport.failureReason).toBe('string');
    mt.disconnect();
  });

  it('a SYNCHRONOUSLY throwing connect is a bounded FAILED dial, never a ref stuck connecting', async () => {
    // e.g. no WebTransport constructor, a URL the constructor rejects, or a
    // refused dial.
    const mocks = new Map<string, MockWebTransport>();
    const inner = makeConnect(mocks);
    const failures: MediaConnectionError[] = [];
    const reports: MhConnectionStatusReport[] = [];
    const mt = new MediaTransport({
      connect: (url) => {
        if (url === URL_B) throw new Error('raw platform message with secret-ish detail');
        return inner(url);
      },
    });
    mt.on('failed', (err) => failures.push(err));
    mt.on('status', (r) => reports.push(r));
    mt.applyHandlerSet(
      intakeOf(1n, [
        { ref: 1, url: URL_A },
        { ref: 2, url: URL_B },
      ]),
    );

    const p = mt.arm(JWT);
    mocks.get(URL_A)!.simulateReady();
    await p;

    expect(mt.getState(REF_B)).toBe('lost');
    expect(mt.getState(REF_A)).toBe('connected');
    const bReport = reports.find((r) => r.handlerRef === REF_B)!;
    expect(bReport).toMatchObject({
      state: 'failed',
      failureCode: MediaConnectionErrorCode.Transport,
    });
    expect(bReport.failureReason).not.toContain('secret-ish');
    expect(failures).toHaveLength(1);
    expect(failures[0]!.message).not.toContain('secret-ish');
    mt.disconnect();
  });

  it('a NON-EMPTY join set that connects nowhere → rejects MediaConnectionError(ALL_FAILED)', async () => {
    const { mt, mocks, reports, armed } = joinTransport([URL_A, URL_B]);

    const p = armed();
    mocks.get(URL_A)!.simulateError(new Error('a'));
    mocks.get(URL_B)!.simulateError(new Error('b'));

    await expect(p).rejects.toBeInstanceOf(MediaConnectionError);
    await p.catch((e: MediaConnectionError) => {
      expect(e.mediaCode).toBe(MediaConnectionErrorCode.AllFailed);
      // The aggregate error carries NO mhUrl (R-23: static message, no per-MH concat).
      expect(e.mhUrl).toBeUndefined();
      expect(e.toJSON().mhUrl).toBeUndefined();
    });
    expect(reports.every((r) => r.state === 'failed')).toBe(true);
    expect(reports).toHaveLength(2);
    mt.disconnect();
  });

  it('default-constructs with prod defaults (no options)', () => {
    const mt = new MediaTransport();
    expect(mt.getState(REF_A)).toBeUndefined();
  });

  it('buckets the 3rd handler index as "2+" in mh_connection_total', async () => {
    const sink = new InMemoryMetricsSink();
    const { mocks, armed } = joinTransport([URL_A, URL_B, URL_C], { metricsSink: sink });
    const p = armed();
    [URL_A, URL_B, URL_C].forEach((u) => mocks.get(u)!.simulateReady());
    await p;
    sink.assertCounter('dt_client_mh_connection_total', { mh_index_bucket: '2+' }, 1);
  });

  it('an EMPTY join set → resolves with nothing dialed ("media connecting")', async () => {
    const { mocks, armed, reports } = joinTransport([]);
    await expect(armed()).resolves.toBeUndefined();
    expect(mocks.size).toBe(0);
    expect(reports).toEqual([]);
  });
});

describe('MediaTransport — MhClientMessage envelope (R-20/R-58)', () => {
  it('writes one connectRequest{joinToken, mediaDialToken} envelope; trace EMPTY when telemetry unconfigured', async () => {
    const { mocks, armed } = joinTransport([URL_A]);
    const p = armed();
    mocks.get(URL_A)!.simulateReady();
    await p;

    const envs = decodeMhClientMessages(mocks.get(URL_A)!.getOutboundBidiWrites(0));
    expect(envs).toHaveLength(1);
    expect(envs[0]!.message.case).toBe('connectRequest');
    const request = envs[0]!.message.case === 'connectRequest' ? envs[0]!.message.value : undefined;
    expect(request?.joinToken).toBe(JWT);
    expect(request?.mediaDialToken).toBe('tok-1');
    expect(envs[0]!.traceParent).toBe('');
    expect(envs[0]!.traceState).toBe('');
  });

  it('populates trace via runInContext under a real active span — trace_id EQUALS the span (R-58)', async () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    const span = getTracer()!.startSpan('dt_client.join');
    const traceId = span.spanContext().traceId;

    const { mocks, armed } = joinTransport([URL_A], {
      // Mirrors SignalingClient.runInJoinContext: synchronous wrap at the inject site.
      runInContext: (fn) => context.with(trace.setSpan(context.active(), span), fn),
    });
    const p = armed();
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
  it('MediaConnectionError from a real failed handshake never exposes the JWT or the dial token', async () => {
    const { mt, mocks, armed } = joinTransport([URL_A]);
    const failures: MediaConnectionError[] = [];
    mt.on('failed', (e) => failures.push(e));

    const p = armed();
    const mock = mocks.get(URL_A)!;
    // Simulate a transport error whose message echoes the written bytes (incl. the
    // credentials) — the worst-case leak vector — in the createBidi step.
    vi.spyOn(mock, 'createBidirectionalStream').mockRejectedValue(
      new Error(`handshake echoed token=${JWT} dial=tok-1`),
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
    expect(serialized).not.toContain('tok-1');
    mt.disconnect();
  });
});

describe('MediaTransport — teardown + deadline (@operations)', () => {
  it('disconnect() is idempotent and tears down an in-flight never-ready transport', async () => {
    const { mt, mocks, armed, reports } = joinTransport([URL_A], { connectTimeoutMs: 10_000 });
    const p = armed();
    // URL_A never becomes ready; the dial holds it for teardown before awaiting ready.
    await waitFor(() => mocks.has(URL_A));
    const closeSpy = vi.spyOn(mocks.get(URL_A)!, 'close');

    mt.disconnect();
    mt.disconnect(); // idempotent — second call is a no-op
    expect(closeSpy).toHaveBeenCalledTimes(1);

    // arm still settles (the aborted dial early-returns on teardown) and reports nothing.
    await p;
    expect(reports).toEqual([]);
  });

  it('per-dial connect deadline → never-ready handler FAILED(CONNECT_TIMEOUT); arm still resolves if another connects', async () => {
    const { mt, mocks, armed, reports } = joinTransport([URL_A, URL_B], { connectTimeoutMs: 25 });
    const p = armed();
    mocks.get(URL_A)!.simulateReady(); // A connects
    // B is never made ready → its deadline fires.
    await p;

    expect(mt.getState(REF_A)).toBe('connected');
    expect(mt.getState(REF_B)).toBe('lost');
    const bReport = reports.find((r) => r.handlerRef === REF_B)!;
    expect(bReport.failureCode).toBe(MediaConnectionErrorCode.ConnectTimeout);
    mt.disconnect();
  });

  it('timeout-path error message MATCHES its mediaCode (not the generic transport message) — F1', async () => {
    const { mt, armed } = joinTransport([URL_A], { connectTimeoutMs: 25 });
    const failures: MediaConnectionError[] = [];
    mt.on('failed', (e) => failures.push(e));

    await armed().catch(() => {}); // all-fail rejects; we only care about the per-dial error

    expect(failures).toHaveLength(1);
    expect(failures[0]!.mediaCode).toBe(MediaConnectionErrorCode.ConnectTimeout);
    // The .message reflects the TIMEOUT reason, not the hardcoded transport reason.
    expect(failures[0]!.message).toBe('media handler connect deadline exceeded');
    expect(failures[0]!.message).not.toBe('media handler connection failed');
    mt.disconnect();
  });

  it('preserves the transport.ready rejection cause on the MediaConnectionError (symmetric with write-path)', async () => {
    const { mt, mocks, armed } = joinTransport([URL_A]);
    const failures: MediaConnectionError[] = [];
    mt.on('failed', (e) => failures.push(e));

    const readyCause = new Error('quic handshake refused');
    const p = armed();
    mocks.get(URL_A)!.simulateError(readyCause); // rejects transport.ready
    await p.catch(() => {});

    expect(failures).toHaveLength(1);
    expect(failures[0]!.mediaCode).toBe(MediaConnectionErrorCode.Transport);
    // The ready-rejection cause is retained on the (non-enumerable) Error.cause...
    expect(failures[0]!.cause).toBe(readyCause);
    // ...but never crosses the redaction line into the serialized shape.
    expect(JSON.stringify(failures[0]!.toJSON())).not.toContain('quic handshake refused');
    mt.disconnect();
  });
});

describe('MediaTransport — metrics (R-25 M1)', () => {
  it('emits dt_client_mh_connection_total per MH with status + mh_index_bucket + label set', async () => {
    const sink = new InMemoryMetricsSink();
    // ---------------------------------------------------------------------
    // `meeting_id_hash` HERE IS CORRECT. DO NOT "FIX" IT WHILE APPLYING R3.
    // ---------------------------------------------------------------------
    //
    // `dt_client_mh_connection_total` is an ADR-0028 join-flow metric that
    // carried the join label set from birth, and ADR-0036 §11 grandfathers that
    // set AS A SET: a closed, enumerated exception that is not extended and that
    // nothing joins. The frozen roster is named in
    // `docs/observability/metrics/client.md`.
    //
    // THE TEST IS WHAT A METRIC OBSERVES, NOT WHICH DIRECTORY IT LIVES IN.
    // Connect-lifecycle — once per MH, at handshake, before any frame exists —
    // is grandfathered. Media-carrying is under the bar. Neither §11 argument
    // reaches this counter: the trace argument concerns the time-ordered
    // sequence of sizes for ONE stream, and the oracle argument concerns
    // `reason` mirroring which crypto layer rejected a frame.
    //
    // This assertion and the negative one in
    // `media/pipeline/__tests__/ingress.attribution.test.ts` — that NO
    // `dt_client_media_*` series carries `meeting_id_hash` — assert opposite
    // things and BOTH ARE RIGHT. Unannotated they read as a contradiction that
    // someone resolves in whichever direction they meet first.
    const labels = { client_version: '0.0.0-test', meeting_id_hash: 'abc123', org_id: 'demo' };
    const { mt, mocks, armed } = joinTransport([URL_A, URL_B], {
      metricsSink: sink,
      metricLabels: labels,
    });
    const p = armed();
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
    mt.disconnect();
  });
});

describe('MediaTransport — datagram I/O (ADR-0036 §1, §11)', () => {
  it("CHOOSES the datagram queue depths rather than inheriting the user agent's", async () => {
    // "The default being adequate is not the same as the default being chosen"
    // (ADR-0036 §1). Asserted on the VALUES, not on the fact that a write
    // happened: an unset knob is the hazard, and it looks identical to a set one
    // unless the number is read back.
    const { mt, mocks, armed } = joinTransport([URL_A], {
      datagramQueue: {
        outgoingHighWaterMark: 2,
        outgoingMaxAgeMs: 500,
        incomingHighWaterMark: 32,
      },
    });
    const p = armed();
    mocks.get(URL_A)!.simulateReady();
    await p;

    expect(mocks.get(URL_A)!.getDatagramQueueSettings()).toEqual({
      outgoingHighWaterMark: 2,
      outgoingMaxAge: 500,
      incomingHighWaterMark: 32,
    });
    mt.disconnect();
  });

  it('sends a datagram over a connected handler and reports the transport maximum', async () => {
    const { mt, mocks, armed } = joinTransport([URL_A]);
    const p = armed();
    mocks.get(URL_A)!.simulateReady();
    await p;

    mocks.get(URL_A)!.setMaxDatagramSize(1200);
    const channel = mt.getDatagramChannel(REF_A);
    expect(channel).toBeDefined();
    expect(channel!.maxDatagramSize).toBe(1200);
    expect(channel!.isOpen).toBe(true);

    await channel!.send(Uint8Array.of(1, 2, 3));
    expect([...(mocks.get(URL_A)!.getOutboundDatagrams()[0] ?? [])]).toEqual([1, 2, 3]);
    mt.disconnect();
  });

  it('receives datagrams on the channel readable', async () => {
    const { mt, mocks, armed } = joinTransport([URL_A]);
    const p = armed();
    mocks.get(URL_A)!.simulateReady();
    await p;

    const channel = mt.getDatagramChannel(REF_A)!;
    const reader = channel.readable.getReader();
    mocks.get(URL_A)!.simulateIncomingDatagram(Uint8Array.of(9, 9));
    const { value } = await reader.read();
    expect([...(value ?? [])]).toEqual([9, 9]);
    reader.releaseLock();
    mt.disconnect();
  });

  it('reports a closed transport as not open, so a sender counts connection_closed', async () => {
    const { mt, mocks, armed } = joinTransport([URL_A]);
    const p = armed();
    mocks.get(URL_A)!.simulateReady();
    await p;

    const channel = mt.getDatagramChannel(REF_A)!;
    mocks.get(URL_A)!.simulateClose();
    await waitFor(() => channel.isOpen === false);
    expect(channel.isOpen).toBe(false);
    mt.disconnect();
  });

  it('returns no channel for a handler that never connected', async () => {
    // The caller counts that as `not_connected` rather than being handed a
    // channel that cannot send.
    const { mt, mocks, armed } = joinTransport([URL_A]);
    const p = armed();
    mocks.get(URL_A)!.simulateError(new Error('nope'));
    await p.catch(() => {});
    expect(mt.getDatagramChannel(REF_A)).toBeUndefined();
    // ...nor for a ref that is not in the set at all.
    expect(mt.getDatagramChannel(REF_C)).toBeUndefined();
    mt.disconnect();
  });

  it('emits no dt_client_media_* series — media metrics belong to the pipeline', async () => {
    // The boundary this file's grandfathering comment describes, made
    // checkable: `MediaTransport` is connect-lifecycle, so its one metric keeps
    // the join label set, and it must not grow a media counter that would then
    // inherit it.
    const sink = new InMemoryMetricsSink();
    const { mt, mocks, armed } = joinTransport([URL_A], {
      metricsSink: sink,
      metricLabels: { client_version: 'v', meeting_id_hash: 'h', org_id: 'o' },
    });
    const p = armed();
    mocks.get(URL_A)!.simulateReady();
    await p;
    const channel = mt.getDatagramChannel(REF_A)!;
    await channel.send(Uint8Array.of(1));

    expect(sink.getRecordedMetrics().filter((m) => m.name.startsWith('dt_client_media_'))).toEqual(
      [],
    );
    mt.disconnect();
  });
});

describe('the unit tier never reaches a real OTLP exporter', () => {
  // Positive control for the tier-wide mock (`setupFiles` -> otlpExporterSetup.ts):
  // a dropped setup entry or a wrong module path would otherwise look exactly like
  // "no egress, no error". This file has no mock of its own, so it proves the wiring.
  it("resetTelemetryForTest's shutdown export lands on the shared mock", async () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    getMeter()!.createCounter('reset_probe').add(1);
    const before = exportCalls.length;
    await resetTelemetryForTest();
    expect(exportCalls.length).toBe(before + 1);
  });
});
