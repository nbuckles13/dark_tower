// File: packages/sdk-core/src/media/setup/__tests__/rosterKeys.failClosed.test.ts
//
// THE EMPTY-KEY BRANCH, EXERCISED SPECIFICALLY.
//
// MC ADMITS a joiner whose `JoinRequest.identity_public_key` is empty and
// publishes an EMPTY `Participant.identity_public_key` — length 0, never a
// zero-filled placeholder. `signaling.proto` states the consumer rule: consumers
// MUST fail closed and MUST NOT fall back to accepting unsigned frames.
//
// Rejecting empty at admission was considered and REVERSED: validation is
// length-only with no proof of possession and no `cnf` binding, so a hostile
// client is admitted anyway with 32 random bytes, and the reject would exclude
// only honest clients that had not implemented the field. What that reversal
// gave up is exactly this — reject made "a keyless participant on the roster"
// unreachable, so no consumer could fail open on one. **That protection now
// lives in this class, and nothing in MC will catch a violation.**
//
// The dangerous implementation is not "forgot to check". It is reading empty as
// *"no key, so skip verification"*, which fails open and is the natural reading
// of an absent field. Present-key coverage does not discharge this file.

import { describe, expect, it } from 'vitest';

import { generateIdentityKeyPair } from '../../frame/ed25519.js';
import { RosterIdentityKeys } from '../rosterKeys.js';

describe('RosterIdentityKeys fails closed', () => {
  it('returns no key for a participant whose published key is EMPTY', async () => {
    const roster = new RosterIdentityKeys(8);
    await roster.upsert({ senderId: 258, identityPublicKey: new Uint8Array(0) });
    // `undefined` is what the ingress turns into a counted `no_roster_entry`
    // drop. There is deliberately no third state a caller could read as
    // "unverifiable, therefore accepted".
    expect(roster.identityKeyFor(258)).toBeUndefined();
  });

  it('returns no key for a MALFORMED (wrong-width) published key', async () => {
    const roster = new RosterIdentityKeys(8);
    await roster.upsert({ senderId: 258, identityPublicKey: new Uint8Array(31).fill(7) });
    expect(roster.identityKeyFor(258)).toBeUndefined();
  });

  it('returns no key for a participant that is not on the roster at all', () => {
    expect(new RosterIdentityKeys(8).identityKeyFor(258)).toBeUndefined();
  });

  it('makes the three absent cases INDISTINGUISHABLE to the caller', async () => {
    // Deliberate. All three mean "this frame cannot be verified", and a caller
    // that could tell them apart would be a caller that could treat one of them
    // permissively.
    const roster = new RosterIdentityKeys(8);
    await roster.upsert({ senderId: 1, identityPublicKey: new Uint8Array(0) });
    await roster.upsert({ senderId: 2, identityPublicKey: new Uint8Array(31) });
    expect(roster.identityKeyFor(1)).toBe(roster.identityKeyFor(2));
    expect(roster.identityKeyFor(2)).toBe(roster.identityKeyFor(3));
  });

  it('resolves a well-formed key, so the fail-closed cases are not vacuous', async () => {
    const roster = new RosterIdentityKeys(8);
    const identity = await generateIdentityKeyPair();
    await roster.upsert({ senderId: 258, identityPublicKey: identity.publicKey });
    expect(roster.identityKeyFor(258)).toBeDefined();
  });

  it('ignores a participant with no sender id — no key id can name them', async () => {
    const roster = new RosterIdentityKeys(8);
    const identity = await generateIdentityKeyPair();
    await roster.upsert({ senderId: 0, identityPublicKey: identity.publicKey });
    expect(roster.identityKeyFor(0)).toBeUndefined();
  });
});

describe('RosterIdentityKeys is bounded', () => {
  it('evicts least-recently-updated entries rather than growing without limit', async () => {
    // Roster membership's bar is a MEETING LINK, so anything unbounded and keyed
    // by `sender_id` is attacker-influenced memory growth. Same per-sender
    // bounded shape as `ReplayWindow` and `TransmitKeyCache`, deliberately —
    // not a third eviction idiom.
    const roster = new RosterIdentityKeys(2);
    const a = await generateIdentityKeyPair();
    await roster.upsert({ senderId: 1, identityPublicKey: a.publicKey });
    await roster.upsert({ senderId: 2, identityPublicKey: a.publicKey });
    await roster.upsert({ senderId: 3, identityPublicKey: a.publicKey });
    expect(roster.identityKeyFor(1)).toBeUndefined();
    expect(roster.identityKeyFor(3)).toBeDefined();
  });

  it('imports each sender key ONCE, never per frame', async () => {
    // `crypto.subtle.importKey` at 50 frames a second is waste with no
    // corresponding safety; the resolver holds the imported `CryptoKey`.
    const roster = new RosterIdentityKeys(8);
    const identity = await generateIdentityKeyPair();
    await roster.upsert({ senderId: 258, identityPublicKey: identity.publicKey });
    expect(roster.identityKeyFor(258)).toBe(roster.identityKeyFor(258));
  });

  it('forgets a participant on removal', async () => {
    const roster = new RosterIdentityKeys(8);
    const identity = await generateIdentityKeyPair();
    await roster.upsert({ senderId: 258, identityPublicKey: identity.publicKey });
    roster.remove(258);
    expect(roster.identityKeyFor(258)).toBeUndefined();
  });
});
