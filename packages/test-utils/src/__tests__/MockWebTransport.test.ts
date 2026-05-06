import { describe, expect, it } from 'vitest';
import { MockWebTransport } from '../MockWebTransport.js';

describe('MockWebTransport', () => {
  it('resolves ready/closed promises via simulate control points and rejects new bidi streams once closed', async () => {
    const wt = new MockWebTransport();
    let isReady = false;
    void wt.ready.then(() => {
      isReady = true;
    });
    wt.simulateReady();
    await wt.ready;
    expect(isReady).toBe(true);

    wt.simulateClose(1000, 'normal');
    const info = await wt.closed;
    expect(info.closeCode).toBe(1000);
    expect(info.reason).toBe('normal');

    await expect(wt.createBidirectionalStream()).rejects.toThrow(/closed/);
  });

  it('captures outbound datagrams and bidi writes via inspector helpers', async () => {
    const wt = new MockWebTransport();
    wt.simulateReady();
    await wt.ready;

    // Datagrams.
    const dg = new Uint8Array([1, 2, 3, 4]);
    const dgWriter = wt.datagrams.writable.getWriter();
    await dgWriter.write(dg);
    dgWriter.releaseLock();
    expect(wt.getOutboundDatagrams()).toHaveLength(1);
    expect(Array.from(wt.getOutboundDatagrams()[0]!)).toEqual([1, 2, 3, 4]);

    // Bidi.
    const stream = await wt.createBidirectionalStream();
    const bidiWriter = stream.writable.getWriter();
    await bidiWriter.write(new Uint8Array([9, 8]));
    await bidiWriter.write(new Uint8Array([7, 6, 5]));
    bidiWriter.releaseLock();
    expect(wt.getOpenedBidiStreams()).toHaveLength(1);
    const writes = wt.getOutboundBidiWrites(0);
    expect(writes).toHaveLength(2);
    expect(Array.from(writes[0]!)).toEqual([9, 8]);
    expect(Array.from(writes[1]!)).toEqual([7, 6, 5]);
  });

  it('routes simulateBidiStream and simulateIncomingDatagram bytes to the corresponding readable streams', async () => {
    const wt = new MockWebTransport();
    wt.simulateReady();
    await wt.ready;

    const stream = await wt.createBidirectionalStream();
    const bidiReader = stream.readable.getReader();
    wt.simulateBidiStream(0, new Uint8Array([10, 20, 30]));
    const bidiResult = await bidiReader.read();
    expect(bidiResult.done).toBe(false);
    expect(Array.from(bidiResult.value!)).toEqual([10, 20, 30]);
    bidiReader.releaseLock();

    const dgReader = wt.datagrams.readable.getReader();
    wt.simulateIncomingDatagram(new Uint8Array([42]));
    const dgResult = await dgReader.read();
    expect(dgResult.done).toBe(false);
    expect(Array.from(dgResult.value!)).toEqual([42]);
    dgReader.releaseLock();

    // Out-of-range stream index throws.
    expect(() => wt.simulateBidiStream(99, new Uint8Array([0]))).toThrow(/no bidi stream/);
  });
});
