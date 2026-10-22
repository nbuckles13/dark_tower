// Positive-control tests for the uni-stream, STOP_SENDING / reset, backpressure
// and bidi-frame-splitting control points of `MockWebTransport`. The mock is
// shared infrastructure: a knob that silently no-ops would make every SDK test
// relying on it vacuous, so each knob is proven to DO something here.

import { describe, expect, it } from 'vitest';
import { MockWebTransport } from '../MockWebTransport.js';

/** Let pending promise reactions run (no timers, no wall clock). */
async function flush(): Promise<void> {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
}

function framed(payload: number[]): Uint8Array {
  const out = new Uint8Array(4 + payload.length);
  new DataView(out.buffer).setUint32(0, payload.length, false);
  out.set(payload, 4);
  return out;
}

describe('MockWebTransport — outbound uni streams', () => {
  it('captures writes per stream in their own ordinal space and records the SDK finishing', async () => {
    const wt = new MockWebTransport();
    await wt.createBidirectionalStream(); // bidi index 0 — must not shift uni indices
    const a = await wt.createUnidirectionalStream();
    const b = await wt.createUnidirectionalStream();
    const wa = a.getWriter();
    const wb = b.getWriter();
    await wa.write(new Uint8Array([1]));
    await wb.write(new Uint8Array([2, 3]));
    await wa.close();

    const info = wt.getOutboundUniStreams();
    expect(info.map((s) => s.index)).toEqual([0, 1]);
    expect(info[0]!.chunks.map((c) => [...c])).toEqual([[1]]);
    expect(info[1]!.chunks.map((c) => [...c])).toEqual([[2, 3]]);
    expect(info[0]!.ending).toBe('closed');
    expect(info[1]!.ending).toBe('open');
    expect(info[0]!.stopSendingCode).toBeUndefined();
  });

  it('records an SDK abort (RESET) as `aborted`, distinct from a finish', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createUnidirectionalStream();
    await s.getWriter().abort(new Error('abandon'));
    expect(wt.getOutboundUniStreams()[0]!.ending).toBe('aborted');
  });

  it('peer STOP_SENDING rejects the next write with the platform error shape and is not the SDK ending', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createUnidirectionalStream();
    const w = s.getWriter();
    await w.write(new Uint8Array([9]));
    wt.simulateOutboundUniStopSending(0, 42);
    await expect(w.write(new Uint8Array([10]))).rejects.toMatchObject({
      name: 'WebTransportError',
      source: 'stream',
      streamErrorCode: 42,
    });
    const info = wt.getOutboundUniStreams()[0]!;
    expect(info.stopSendingCode).toBe(42);
    expect(info.ending).toBe('open');
    expect(info.chunks).toHaveLength(1);
  });

  it('a paused stream holds the write out of the inspector until it is released', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createUnidirectionalStream();
    wt.pauseUniStreamWrites(0);
    const w = s.getWriter();
    let settled = false;
    const write = w.write(new Uint8Array([7])).then(() => {
      settled = true;
    });
    await flush();
    expect(settled).toBe(false);
    expect(wt.getOutboundUniStreams()[0]!.chunks).toHaveLength(0);
    expect(wt.getOutboundUniStreams()[0]!.paused).toBe(true);
    // Backpressure is visible to the writer while the write is stalled.
    expect(w.desiredSize).toBeLessThanOrEqual(0);

    wt.resumeUniStreamWrites(0);
    await write;
    expect(settled).toBe(true);
    expect(wt.getOutboundUniStreams()[0]!.chunks.map((c) => [...c])).toEqual([[7]]);
  });

  it('pauses per stream: another stream keeps flowing', async () => {
    const wt = new MockWebTransport();
    await wt.createUnidirectionalStream();
    const other = await wt.createUnidirectionalStream();
    wt.pauseUniStreamWrites(0);
    await other.getWriter().write(new Uint8Array([1]));
    expect(wt.getOutboundUniStreams()[1]!.chunks).toHaveLength(1);
  });

  it('a STOP_SENDING wakes a write stalled on a pause gate with a rejection', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createUnidirectionalStream();
    wt.pauseUniStreamWrites(0);
    const write = s.getWriter().write(new Uint8Array([1]));
    await flush();
    wt.simulateOutboundUniStopSending(0, 7);
    await expect(write).rejects.toMatchObject({ streamErrorCode: 7 });
    expect(wt.getOutboundUniStreams()[0]!.chunks).toHaveLength(0);
  });

  it('rejects createUnidirectionalStream on a closed transport, as bidi does', async () => {
    const wt = new MockWebTransport();
    wt.simulateClose();
    await expect(wt.createUnidirectionalStream()).rejects.toThrow(/closed/);
  });

  it('errors open outbound uni streams when the session ends', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createUnidirectionalStream();
    const w = s.getWriter();
    wt.simulateClose();
    await expect(w.write(new Uint8Array([1]))).rejects.toMatchObject({ name: 'WebTransportError' });
  });

  it('clearInspector clears uni captures and keeps stream state', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createUnidirectionalStream();
    await s.getWriter().write(new Uint8Array([1]));
    wt.clearInspector();
    expect(wt.getOutboundUniStreams()[0]!.chunks).toHaveLength(0);
    expect(wt.getOutboundUniStreams()).toHaveLength(1);
  });
});

describe('MockWebTransport — inbound uni streams', () => {
  it('buffers a stream simulated before the SDK reads, then delivers its chunks in order', async () => {
    const wt = new MockWebTransport();
    const h = wt.simulateIncomingUniStream();
    h.enqueue(new Uint8Array([1]));
    h.enqueue(new Uint8Array([2]));
    h.close();

    const streams = wt.incomingUnidirectionalStreams.getReader();
    const { value: stream } = await streams.read();
    const reader = stream!.getReader();
    expect([...(await reader.read()).value!]).toEqual([1]);
    expect([...(await reader.read()).value!]).toEqual([2]);
    expect((await reader.read()).done).toBe(true);
    expect(wt.getInboundUniStreams()[0]).toMatchObject({ index: 0, peerEnding: 'closed' });
  });

  it('a peer reset rejects the next read with the code, in the platform shape', async () => {
    const wt = new MockWebTransport();
    const h = wt.simulateIncomingUniStream();
    const { value: stream } = await wt.incomingUnidirectionalStreams.getReader().read();
    const reader = stream!.getReader();
    h.reset(5);
    await expect(reader.read()).rejects.toMatchObject({
      name: 'WebTransportError',
      source: 'stream',
      streamErrorCode: 5,
    });
    expect(wt.getInboundUniStreams()[0]!.peerEnding).toBe('reset');
  });

  it('records the SDK cancelling its reader separately from any peer ending', async () => {
    const wt = new MockWebTransport();
    wt.simulateIncomingUniStream();
    const { value: stream } = await wt.incomingUnidirectionalStreams.getReader().read();
    await stream!.cancel();
    expect(wt.getInboundUniStreams()[0]).toMatchObject({ peerEnding: 'open', cancelled: true });
  });

  it('uses its own ordinal space, independent of outbound uni and bidi', async () => {
    const wt = new MockWebTransport();
    await wt.createBidirectionalStream();
    await wt.createUnidirectionalStream();
    expect(wt.simulateIncomingUniStream().index).toBe(0);
    expect(wt.simulateIncomingUniStream().index).toBe(1);
  });

  it('errors open inbound streams and the incoming list on a session error', async () => {
    const wt = new MockWebTransport();
    // `simulateError` also rejects `ready` and `closed`; observe both.
    const settled = Promise.allSettled([wt.ready, wt.closed]);
    wt.simulateIncomingUniStream();
    const incoming = wt.incomingUnidirectionalStreams.getReader();
    const { value: stream } = await incoming.read();
    const reader = stream!.getReader();
    wt.simulateError(new Error('boom'));
    await expect(reader.read()).rejects.toThrow('boom');
    await expect(incoming.read()).rejects.toThrow('boom');
    await settled;
  });

  it('ends the incoming list cleanly on a session close', async () => {
    const wt = new MockWebTransport();
    const incoming = wt.incomingUnidirectionalStreams.getReader();
    wt.simulateClose();
    expect((await incoming.read()).done).toBe(true);
  });
});

describe('MockWebTransport — getOutboundBidiFrames', () => {
  it('splits concatenated length-prefixed writes, including frames split across writes', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createBidirectionalStream();
    const w = s.writable.getWriter();
    const two = new Uint8Array([...framed([1, 2]), ...framed([3])]);
    await w.write(two.slice(0, 3));
    await w.write(two.slice(3));
    expect(wt.getOutboundBidiFrames(0).map((f) => [...f])).toEqual([[1, 2], [3]]);
  });

  it('throws on a truncated trailing frame rather than dropping it', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createBidirectionalStream();
    await s.writable.getWriter().write(framed([1, 2, 3]).slice(0, 6));
    expect(() => wt.getOutboundBidiFrames(0)).toThrow(/truncated frame/);
  });

  it('throws on a leftover partial length prefix', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createBidirectionalStream();
    await s.writable.getWriter().write(new Uint8Array([...framed([1]), 0, 0]));
    expect(() => wt.getOutboundBidiFrames(0)).toThrow(/partial length prefix/);
  });
});

describe('MockWebTransport — bidi peer endings', () => {
  it('a peer FIN makes the reader read done', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createBidirectionalStream();
    const reader = s.readable.getReader();
    wt.simulateBidiStreamEnd(0);
    expect((await reader.read()).done).toBe(true);
  });

  it('a peer reset rejects the next read with the platform error shape', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createBidirectionalStream();
    const reader = s.readable.getReader();
    wt.simulateBidiStreamReset(0, 9);
    await expect(reader.read()).rejects.toMatchObject({
      name: 'WebTransportError',
      source: 'stream',
      streamErrorCode: 9,
    });
  });
});

describe('MockWebTransport — SDK abort of a write stalled on backpressure', () => {
  it('rejects the stalled write, records `aborted`, keeps no chunk and no injected stop', async () => {
    const wt = new MockWebTransport();
    const s = await wt.createUnidirectionalStream();
    wt.pauseUniStreamWrites(0);
    const w = s.getWriter();
    const write = w.write(new Uint8Array([1]));
    await flush();
    const abort = w.abort(new Error('abandon group'));
    await expect(write).rejects.toBeDefined();
    await abort;
    const info = wt.getOutboundUniStreams()[0]!;
    expect(info.ending).toBe('aborted');
    expect(info.chunks).toHaveLength(0);
    expect(info.stopSendingCode).toBeUndefined();
  });
});
