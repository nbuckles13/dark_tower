// File: packages/sdk-core/src/media/lifecycle/__tests__/recoveryRequestQueue.test.ts
//
// The receiver's recovery-request queue (story 3 R-13): one queued per (handler,
// slot) with replace-on-newer, one in flight per handler, no timer, no retry,
// counted on a successful write only.

import { describe, expect, it } from 'vitest';

import type { MhRecoveryRequest } from '../../events.js';
import type { MediaRecoverySource } from '../../setup/mediaMetrics.js';
import { RecoveryRequestQueue } from '../recoveryRequestQueue.js';

interface Pending {
  readonly url: string;
  readonly request: MhRecoveryRequest;
  resolve(): void;
  reject(): void;
}

function harness() {
  const sends: Pending[] = [];
  const sent: MediaRecoverySource[] = [];
  let failed = 0;
  const queue = new RecoveryRequestQueue({
    send: (url, request) =>
      new Promise<void>((resolve, reject) => {
        sends.push({ url, request, resolve, reject: () => reject(new Error('write failed')) });
      }),
    sent: (reason) => sent.push(reason),
    failed: () => {
      failed += 1;
    },
  });
  return { queue, sends, sent, failed: () => failed };
}

const req = (slotId: number, senderId: number, gopCounter: number): MhRecoveryRequest => ({
  slotId,
  senderId,
  gopCounter,
});

const tick = async (): Promise<void> => {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
};

describe('RecoveryRequestQueue', () => {
  it('sends a request at once when nothing is in flight, carrying exactly the three fields', () => {
    const h = harness();
    h.queue.request('u', req(2, 7, 9), 'verify_failed');
    expect(h.sends.map((s) => s.request)).toEqual([{ slotId: 2, senderId: 7, gopCounter: 9 }]);
  });

  it('a newer request REPLACES a queued one for the same slot; nothing obsolete reaches the transport', async () => {
    const h = harness();
    h.queue.request('u', req(2, 7, 9), 'verify_failed'); // in flight
    h.queue.request('u', req(2, 7, 10), 'decrypt_failed'); // queued
    h.queue.request('u', req(2, 7, 11), 'local_drop'); // replaces it
    expect(h.queue.pendingFor('u')).toBe(1);
    h.sends[0]?.resolve();
    await tick();
    expect(h.sends.map((s) => s.request.gopCounter)).toEqual([9, 11]);
    h.sends[1]?.resolve();
    await tick();
    expect(h.sent).toEqual(['verify_failed', 'local_drop']);
  });

  it('one in flight per handler; different slots queue in order; handlers are independent', async () => {
    const h = harness();
    h.queue.request('u', req(2, 7, 1), 'verify_failed');
    h.queue.request('u', req(3, 8, 1), 'verify_failed');
    h.queue.request('v', req(4, 9, 1), 'verify_failed');
    expect(h.sends.map((s) => [s.url, s.request.slotId])).toEqual([
      ['u', 2],
      ['v', 4],
    ]);
    h.sends[0]?.resolve();
    await tick();
    expect(h.sends.map((s) => s.request.slotId)).toEqual([2, 4, 3]);
  });

  it('a failed write is surfaced, NOT counted and NOT retried; the queue moves on', async () => {
    const h = harness();
    h.queue.request('u', req(2, 7, 1), 'verify_failed');
    h.queue.request('u', req(3, 7, 1), 'verify_failed');
    h.sends[0]?.reject();
    await tick();
    expect(h.failed()).toBe(1);
    expect(h.sent).toEqual([]);
    expect(h.sends.map((s) => s.request.slotId)).toEqual([2, 3]);
  });

  it('stop drops everything queued and counts nothing after', async () => {
    const h = harness();
    h.queue.request('u', req(2, 7, 1), 'verify_failed');
    h.queue.request('u', req(3, 7, 1), 'verify_failed');
    h.queue.stop();
    h.sends[0]?.resolve();
    await tick();
    expect(h.sends).toHaveLength(1);
    expect(h.sent).toEqual([]);
    h.queue.request('u', req(4, 7, 1), 'verify_failed');
    expect(h.sends).toHaveLength(1);
  });
});
