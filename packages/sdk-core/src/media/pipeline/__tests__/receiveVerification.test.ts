// File: packages/sdk-core/src/media/pipeline/__tests__/receiveVerification.test.ts
//
// The recorder's BOUNDS and key discipline (story 2 R-30). Both the slot and the
// pre-verify sender are attacker-influenced, so state must stop growing at the
// cap and every event past it must be COUNTED rather than dropped silently.

import { describe, expect, it } from 'vitest';

import {
  BoundedReceiveVerificationRecorder,
  RECEIVE_VERIFICATION_MAX_KEYS,
} from '../receiveVerification.js';

describe('BoundedReceiveVerificationRecorder', () => {
  it('keys layers by (slot, sender) and drops by (slot, sender, reason), cumulatively', () => {
    const r = new BoundedReceiveVerificationRecorder();
    r.keyed(0, 7);
    r.keyed(0, 7);
    r.verified(0, 7);
    r.keyed(1, 7);
    r.dropped(0, 7, 'decrypt_failed');
    r.dropped(0, 7, 'decrypt_failed');
    r.dropped('undeclared', 9, 'no_roster_entry');
    r.dropped('unparsed', undefined, 'truncated');
    const snap = r.snapshot();
    expect(snap.layers).toEqual([
      { slot: 0, senderId: 7, keyed: 2, verified: 1 },
      { slot: 1, senderId: 7, keyed: 1, verified: 0 },
    ]);
    expect(snap.drops).toEqual([
      { slot: 0, senderId: 7, reason: 'decrypt_failed', count: 2 },
      { slot: 'undeclared', senderId: 9, reason: 'no_roster_entry', count: 1 },
      { slot: 'unparsed', senderId: null, reason: 'truncated', count: 1 },
    ]);
    expect(snap.overflow).toBe(0);
  });

  it('keeps the extreme ids distinct (stream 65535, sender 0 and 65535)', () => {
    const r = new BoundedReceiveVerificationRecorder();
    r.keyed(65_535, 65_535);
    r.keyed(65_535, 0);
    r.keyed(0, 65_535);
    expect(r.snapshot().layers).toEqual([
      { slot: 65_535, senderId: 65_535, keyed: 1, verified: 0 },
      { slot: 65_535, senderId: 0, keyed: 1, verified: 0 },
      { slot: 0, senderId: 65_535, keyed: 1, verified: 0 },
    ]);
  });

  it('creates NO new key past the cap and counts every refused event on overflow', () => {
    const cap = 3;
    const r = new BoundedReceiveVerificationRecorder(cap);
    r.keyed(0, 1);
    r.keyed(0, 2);
    r.dropped(0, 1, 'decrypt_failed');
    // At the cap: three more NEW keys are refused and counted.
    r.keyed(0, 3);
    r.verified(0, 4);
    r.dropped(0, 5, 'decrypt_failed');
    // Existing keys still count normally.
    r.keyed(0, 1);
    r.dropped(0, 1, 'decrypt_failed');
    const snap = r.snapshot();
    expect(snap.layers.length + snap.drops.length).toBe(cap);
    expect(snap.overflow).toBe(3);
    expect(snap.layers).toContainEqual({ slot: 0, senderId: 1, keyed: 2, verified: 0 });
    expect(snap.drops).toEqual([{ slot: 0, senderId: 1, reason: 'decrypt_failed', count: 2 }]);
  });

  it('defaults to a hard cap', () => {
    const r = new BoundedReceiveVerificationRecorder();
    for (let sender = 1; sender <= RECEIVE_VERIFICATION_MAX_KEYS + 10; sender += 1) {
      r.keyed(0, sender);
    }
    const snap = r.snapshot();
    expect(snap.layers).toHaveLength(RECEIVE_VERIFICATION_MAX_KEYS);
    expect(snap.overflow).toBe(10);
  });
});
