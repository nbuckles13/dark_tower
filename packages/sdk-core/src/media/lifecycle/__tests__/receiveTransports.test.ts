// File: packages/sdk-core/src/media/lifecycle/__tests__/receiveTransports.test.ts
//
// ADR-0039 §4 removal cleanup on the DATAGRAM read loops: `stop(ref)` ends that
// ref's loop QUIETLY and clears ITS hop monitor only; the other refs' loops and
// monitors are untouched; and a ref whose transport ended gets a fresh loop
// (and monitor) on the next `open` — the redial case.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { MockWebTransport } from '@darktower/test-utils';

import { ReceiveTransports } from '../receiveTransports.js';
import { HopSequenceMonitor } from '../../pipeline/hopSequenceMonitor.js';
import { handlerRef, type HandlerRef } from '../../handlerSet.js';
import { flushMicrotasks } from '../../__tests__/helpers.js';

const R1 = handlerRef(1);
const R2 = handlerRef(2);

afterEach(() => {
  vi.restoreAllMocks();
});

function rig() {
  const mocks = new Map<HandlerRef, MockWebTransport>([
    [R1, new MockWebTransport()],
    [R2, new MockWebTransport()],
  ]);
  const monitorOf = new Map<HandlerRef, HopSequenceMonitor>();
  const faults: string[] = [];
  const transports = new ReceiveTransports({
    readableFor: (ref) => mocks.get(ref)?.datagrams.readable,
    declaredSlotIds: [0],
    hopRestartBackwardJumpFrames: 64,
    accept: (datagram, monitor) => {
      // The datagram's first byte names the ref it was delivered on.
      monitorOf.set(handlerRef(datagram[0]!), monitor);
      return Promise.resolve();
    },
    isStopped: () => false,
    register: () => undefined,
    transportFault: (m) => faults.push(m),
    frameFault: (m) => faults.push(m),
  });
  return { transports, mocks, monitorOf, faults };
}

describe('ReceiveTransports.stop(ref)', () => {
  it('ends that loop quietly and clears ONLY its hop monitor', async () => {
    const r = rig();
    r.transports.open([R1, R2]);
    r.mocks.get(R1)!.simulateIncomingDatagram(Uint8Array.of(1));
    r.mocks.get(R2)!.simulateIncomingDatagram(Uint8Array.of(2));
    await flushMicrotasks(50);
    const m1 = r.monitorOf.get(R1)!;
    const m2 = r.monitorOf.get(R2)!;
    expect(m1).not.toBe(m2);
    const cleared = vi.spyOn(HopSequenceMonitor.prototype, 'clear');

    r.transports.stop(R1);
    await flushMicrotasks(50);

    expect(cleared.mock.contexts).toEqual([m1]);
    expect(r.faults).toEqual([]);
    // Positive arm: R2 still reads, on its own monitor.
    r.mocks.get(R2)!.simulateIncomingDatagram(Uint8Array.of(2));
    await flushMicrotasks(50);
    expect(r.monitorOf.get(R2)).toBe(m2);
  });

  it('a loop whose transport ENDED frees its slot: the next open reads the new transport', async () => {
    const r = rig();
    r.transports.open([R1]);
    r.mocks.get(R1)!.simulateDatagramReadableEnd();
    await flushMicrotasks(50);
    // An in-set end is surfaced (positive control for the quiet stop above).
    expect(r.faults).toHaveLength(1);
    // The redial: a NEW transport for the same ref.
    const fresh = new MockWebTransport();
    r.mocks.set(R1, fresh);
    r.transports.open([R1]);
    expect(fresh.datagrams.readable.locked).toBe(true);
  });
});
