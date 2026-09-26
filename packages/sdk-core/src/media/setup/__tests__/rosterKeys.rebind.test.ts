// File: packages/sdk-core/src/media/setup/__tests__/rosterKeys.rebind.test.ts
//
// Story 2 R-18: a roster update that rebinds a LIVE sender id to a DIFFERENT
// identity key purges that sender's cached transmit keys — and never its replay
// state. This suite pins the DECISION (what counts as a rebind) and its
// ordering against concurrent imports. The end-to-end consequence — a purged
// sender's frames, and the rebind-BACK replay the retained window must reject —
// is in `media/lifecycle/__tests__/audioPipeline.multiSender.test.ts`.
//
// The roster key stays trust-on-first-use; nothing here makes it more than that.

import { describe, expect, it } from 'vitest';

import { generateIdentityKeyPair } from '../../frame/ed25519.js';
import { RosterIdentityKeys, type TransmitKeyInvalidation } from '../rosterKeys.js';

/** A fresh, genuinely distinct Ed25519 public key. */
async function publicKey(): Promise<Uint8Array> {
  return (await generateIdentityKeyPair()).publicKey;
}

function rig(maxEntries = 8) {
  const roster = new RosterIdentityKeys(maxEntries);
  const events: { senderId: number; cause: TransmitKeyInvalidation }[] = [];
  roster.setTransmitKeyInvalidationListener((senderId, cause) => events.push({ senderId, cause }));
  return { roster, events, rebinds: () => events.filter((e) => e.cause === 'rebind') };
}

const SENDER = 258;

describe('what is and is not a rebind', () => {
  it('a FIRST binding of a new sender is not a rebind', async () => {
    const { roster, events } = rig();
    await roster.upsert({ senderId: SENDER, identityPublicKey: await publicKey() });
    expect(events).toEqual([]);
    expect(roster.identityKeyFor(SENDER)).toBeDefined();
  });

  it('EQUAL bytes are a no-op: no purge, and the key stays usable throughout', async () => {
    const { roster, events } = rig();
    const k = await publicKey();
    await roster.upsert({ senderId: SENDER, identityPublicKey: k });
    const before = roster.identityKeyFor(SENDER);
    // A fresh copy of the same bytes, as a re-sent roster entry would be.
    await roster.upsert({ senderId: SENDER, identityPublicKey: Uint8Array.from(k) });
    expect(events).toEqual([]);
    expect(roster.identityKeyFor(SENDER)).toBe(before);
  });

  it('a first key for a KEYLESS entry is a first binding, not a rebind', async () => {
    // A joiner that published no key, then does. Nothing was ever unwrapped
    // under a verified binding for it, so there is nothing to purge.
    const { roster, events } = rig();
    await roster.upsert({ senderId: SENDER, identityPublicKey: new Uint8Array(0) });
    await roster.upsert({ senderId: SENDER, identityPublicKey: await publicKey() });
    expect(events).toEqual([]);
    expect(roster.identityKeyFor(SENDER)).toBeDefined();
  });

  it('DIFFERENT bytes for a live sender are a rebind, reported exactly once', async () => {
    const { roster, rebinds } = rig();
    await roster.upsert({ senderId: SENDER, identityPublicKey: await publicKey() });
    await roster.upsert({ senderId: SENDER, identityPublicKey: await publicKey() });
    expect(rebinds()).toEqual([{ senderId: SENDER, cause: 'rebind' }]);
  });

  it.each([
    ['empty', new Uint8Array(0)],
    ['wrong-width', new Uint8Array(31).fill(3)],
  ])(
    'a DOWNGRADE of a live key to %s invalidates, reported as a downgrade (not a rebind), and leaves the entry keyless',
    async (_l, bad) => {
      const { roster, events, rebinds } = rig();
      await roster.upsert({ senderId: SENDER, identityPublicKey: await publicKey() });
      await roster.upsert({ senderId: SENDER, identityPublicKey: bad });
      // Reported apart from a rebind: MC publishing an empty key is a documented
      // occurrence, so it must not share a counter arm with a true rebind.
      expect(events).toEqual([{ senderId: SENDER, cause: 'downgrade' }]);
      expect(rebinds()).toHaveLength(0);
      // Fail closed: never the previous key.
      expect(roster.identityKeyFor(SENDER)).toBeUndefined();
    },
  );
});

describe('the decision is synchronous, and the import is sequence-guarded', () => {
  it('goes KEYLESS the instant a rebind is decided, before the new key imports', async () => {
    // Nothing may verify against the OLD key while the new one is importing:
    // those frames fail closed as `no_roster_entry` instead.
    const { roster, rebinds } = rig();
    await roster.upsert({ senderId: SENDER, identityPublicKey: await publicKey() });
    const pending = roster.upsert({ senderId: SENDER, identityPublicKey: await publicKey() });
    expect(rebinds()).toHaveLength(1);
    expect(roster.identityKeyFor(SENDER)).toBeUndefined();
    await pending;
    expect(roster.identityKeyFor(SENDER)).toBeDefined();
  });

  it('never lets an OLDER import that resolves LATE overwrite a newer binding', async () => {
    // Two updates for one sender, started in order. Whatever order their
    // imports complete in, the later update's key is the one held.
    const { roster } = rig();
    const k1 = await publicKey();
    const k2 = await publicKey();
    const first = roster.upsert({ senderId: SENDER, identityPublicKey: k1 });
    const second = roster.upsert({ senderId: SENDER, identityPublicKey: k2 });
    await Promise.all([second, first]);
    const held = roster.identityKeyFor(SENDER)!;
    const raw = new Uint8Array(await crypto.subtle.exportKey('raw', held));
    expect(raw).toEqual(k2);
  });

  it('discards an import that resolves after the sender was REMOVED', async () => {
    const { roster } = rig();
    const pending = roster.upsert({ senderId: SENDER, identityPublicKey: await publicKey() });
    roster.remove(SENDER);
    await pending;
    expect(roster.identityKeyFor(SENDER)).toBeUndefined();
  });
});

describe('forgetting a sender invalidates its transmit keys, uncounted', () => {
  it('remove() reports a forget, never a rebind', async () => {
    const { roster, events } = rig();
    await roster.upsert({ senderId: SENDER, identityPublicKey: await publicKey() });
    roster.remove(SENDER);
    expect(events).toEqual([{ senderId: SENDER, cause: 'forgotten' }]);
  });

  it('LRU eviction reports a forget, so a later re-add cannot slip past the rebind check', async () => {
    // Once a sender's bytes are evicted, a re-add is indistinguishable from a
    // first binding. Purging on eviction is what stops that from being a way
    // around the rebind purge.
    const { roster, events } = rig(2);
    await roster.upsert({ senderId: 1, identityPublicKey: await publicKey() });
    await roster.upsert({ senderId: 2, identityPublicKey: await publicKey() });
    await roster.upsert({ senderId: 3, identityPublicKey: await publicKey() });
    expect(events).toEqual([{ senderId: 1, cause: 'forgotten' }]);
  });

  it('remove() of a sender never on the roster reports nothing', () => {
    const { roster, events } = rig();
    roster.remove(999);
    expect(events).toEqual([]);
  });
});
