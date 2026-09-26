// File: packages/sdk-core/src/signaling/__tests__/rosterKeyFeed.test.ts
//
// Roster signalling -> identity-key resolver operations. The leave path is the
// SECOND cutoff for a departed participant, beside the slot-edge gate, so it
// must hold across a participant being re-issued a new sender id.

import { describe, expect, it } from 'vitest';

import { RosterKeyFeed } from '../rosterKeyFeed.js';

function recordingSink() {
  const upserts: number[] = [];
  const removes: number[] = [];
  return {
    upserts,
    removes,
    sink: {
      upsert: async (e: { senderId: number }) => void upserts.push(e.senderId),
      remove: (id: number) => void removes.push(id),
    },
  };
}

const key = new Uint8Array(32).fill(1);

describe('RosterKeyFeed', () => {
  it('forgets a leaver by the sender id the roster gave it', () => {
    const { sink, removes } = recordingSink();
    const feed = new RosterKeyFeed(sink);
    feed.joined([{ participantId: 'p1', senderId: 77, identityPublicKey: key }]);
    feed.left('p1');
    expect(removes).toEqual([77]);
  });

  it('RETIRES the old sender id when a participant reappears under a new one', () => {
    // A reconnect with rotate-and-reissue. Without this the old sender's key
    // survived until LRU eviction — the old id could still verify frames, which
    // defeats the leaver cutoff this feed exists to provide.
    const { sink, removes } = recordingSink();
    const feed = new RosterKeyFeed(sink);
    feed.joined([{ participantId: 'p1', senderId: 77, identityPublicKey: key }]);
    feed.joined([{ participantId: 'p1', senderId: 88, identityPublicKey: key }]);
    expect(removes).toEqual([77]);
    // And the eventual leave forgets the CURRENT id, not the retired one.
    feed.left('p1');
    expect(removes).toEqual([77, 88]);
  });

  it('does not remove anything on a re-announcement under the SAME sender id', () => {
    const { sink, removes } = recordingSink();
    const feed = new RosterKeyFeed(sink);
    feed.joined([{ participantId: 'p1', senderId: 77, identityPublicKey: key }]);
    feed.joined([{ participantId: 'p1', senderId: 77, identityPublicKey: key }]);
    expect(removes).toEqual([]);
  });

  it('ignores a leave for a participant it never saw, and a participant with no sender id', () => {
    const { sink, removes, upserts } = recordingSink();
    const feed = new RosterKeyFeed(sink);
    feed.joined([{ participantId: 'p2', identityPublicKey: key }]);
    feed.left('p2');
    feed.left('stranger');
    expect(removes).toEqual([]);
    expect(upserts).toEqual([]);
  });
});
