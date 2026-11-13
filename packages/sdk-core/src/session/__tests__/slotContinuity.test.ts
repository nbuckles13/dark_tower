// File: packages/sdk-core/src/session/__tests__/slotContinuity.test.ts
//
// Per-slot continuity across a handler swap (ADR-0039 §4 "Slot states";
// story 4 C2 R-20 and `dt_client_media_resume_ms`), on the pure tracker with a
// hand-driven `FakeScheduler`. Every negative has its positive arm on the SAME
// fixture, so a broken fixture cannot make a negative pass vacuously.
//
// EVENT ORDER IS MC's, READ FROM MC's CODE: `remove_handler_for` in
// `crates/mc-service/src/actors/meeting/health.rs` calls
// `broadcast_handler_set` (the set WITHOUT the old handler) and THEN
// `flush_views` (the moved assignments) — so a removal reaches the client while
// its slots still name the removed ref, and the follow-up assignment comes
// after. SignalingClient's membership re-check (the slot ACTIVE with its stale
// ref resolved to none) is emitted between the two.

import { describe, expect, it } from 'vitest';
import { FakeScheduler } from '@darktower/test-utils';

import { SlotContinuity, type SlotReconnectingChangedEvent } from '../slotContinuity.js';
import { handlerRef, type HandlerRef } from '../../media/handlerSet.js';
import type { MediaKindToken } from '../../media/mediaKind.js';
import type { MediaResumeOutcome } from '../../media/setup/mediaMetrics.js';
import type { SlotStateToken, StreamAssignmentEvent } from '../../signaling/events.js';

const CAP = 30_000;
const ABANDON = 60_000;
const GRACE = 2_000;
const OLD = handlerRef(1);
const NEW = handlerRef(2);
const AUDIO_SLOT = 0;
const VIDEO_SLOT = 4;
const BEA = 77;
const CAL = 78;

interface Obs {
  readonly kind: MediaKindToken;
  readonly outcome: MediaResumeOutcome;
  readonly ms: number;
}

function rig(connected: HandlerRef[] = [OLD]) {
  const sched = new FakeScheduler();
  const live = new Set<HandlerRef>(connected);
  const events: SlotReconnectingChangedEvent[] = [];
  const resumes: Obs[] = [];
  const sc = new SlotContinuity({
    reconnectingCapMs: CAP,
    resumeAbandonMs: ABANDON,
    removalGraceMs: GRACE,
    clock: sched.clock,
    scheduler: sched,
    isConnected: (ref) => live.has(ref),
    metricsFor: (kind) => ({ resumeMs: (outcome, ms) => resumes.push({ kind, outcome, ms }) }),
    onReconnectingChanged: (e) => events.push(e),
  });
  const reconnecting = (slotId: number): boolean => sc.isReconnecting(slotId);
  /** The transport's view of a ref leaving the set (`handlerDropped`): its shape, then removed. */
  const remove = (ref: HandlerRef): void => {
    const wasConnected = live.has(ref);
    live.delete(ref);
    sc.handlerRemoved(ref, { wasConnected, otherConnected: live.size > 0 });
  };
  /** The transport's view of a crash: no longer connected, then the loss event. */
  const lose = (ref: HandlerRef): void => {
    live.delete(ref);
    sc.handlerLost(ref);
  };
  return { sched, live, sc, events, resumes, reconnecting, remove, lose };
}

function assignment(
  slotId: number,
  mediaKind: MediaKindToken,
  handler: HandlerRef | undefined,
  senderId: number | undefined,
  slotState: SlotStateToken = 'active',
): StreamAssignmentEvent {
  return { slotId, mediaKind, handlerRef: handler, senderId, slotState };
}

/** Audio slot 0 live from Bea and video slot 4 live (verified Bea), both on `ref`. */
function assignBoth(sc: SlotContinuity, ref: HandlerRef | undefined): void {
  sc.assignments({
    assignments: [
      assignment(AUDIO_SLOT, 'audio', ref, BEA),
      assignment(VIDEO_SLOT, 'video', ref, undefined),
    ],
    unreachableSenderIds: [],
  });
  sc.videoRender(VIDEO_SLOT, { kind: 'active', senderId: BEA });
}

/** SignalingClient's membership re-check after a removal: the stale ref resolves to none. */
function recheck(sc: SlotContinuity): void {
  sc.assignments({
    assignments: [
      assignment(AUDIO_SLOT, 'audio', undefined, BEA),
      assignment(VIDEO_SLOT, 'video', undefined, undefined),
    ],
    unreachableSenderIds: [],
  });
}

describe("PLANNED transfer, in MC's REAL order (set without OLD first, then the moved assignments)", () => {
  it('ZERO reconnecting events, ONE resumed blip per active slot (removal → first frame via NEW); a frame via the removed OLD does not end it', () => {
    const r = rig([OLD]);
    assignBoth(r.sc, OLD);
    r.live.add(NEW); // make-before-break: NEW connected before the removal
    r.sc.handlerConnected(NEW);
    r.remove(OLD); // set without OLD — slots still name OLD here
    recheck(r.sc); // the membership re-check: not decisive
    r.sched.advance(GRACE - 1);
    assignBoth(r.sc, NEW); // flush_views: edges moved to the CONNECTED NEW
    r.sched.advance(300);
    r.sc.presented(AUDIO_SLOT, OLD, BEA); // a frame still draining from OLD
    r.sc.presented(VIDEO_SLOT, OLD, BEA);
    expect(r.resumes).toEqual([]);
    r.sc.presented(AUDIO_SLOT, NEW, BEA);
    r.sc.presented(VIDEO_SLOT, NEW, BEA);
    r.sched.advance(CAP * 3);
    expect(r.events).toEqual([]); // not even a set/clear pair
    expect(r.resumes).toEqual([
      { kind: 'audio', outcome: 'resumed', ms: GRACE - 1 + 300 },
      { kind: 'video', outcome: 'resumed', ms: GRACE - 1 + 300 },
    ]);
  });

  it('(3a) NO other member connected: the removal is not a planned shape — reconnecting AT ONCE', () => {
    const r = rig([OLD]);
    assignBoth(r.sc, OLD);
    r.remove(OLD); // nothing else connected
    expect(r.events.map((e) => [e.slotId, e.reconnecting])).toEqual([
      [AUDIO_SLOT, true],
      [VIDEO_SLOT, true],
    ]);
    expect(r.events[0]!.previousSenderId).toBe(BEA);
    r.live.add(NEW);
    assignBoth(r.sc, NEW);
    expect(r.reconnecting(AUDIO_SLOT)).toBe(false);
  });

  it('(3b) OLD already lost (crash, then removal): reconnecting at the loss, no hold', () => {
    const r = rig([OLD, NEW]);
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    expect(r.events).toHaveLength(2);
    r.remove(OLD); // wasConnected=false
    expect(r.events).toHaveLength(2); // no second set
  });

  it('(4) a loss DURING the hold sets the overlay at once (no waiting out the hold)', () => {
    const r = rig([OLD, NEW]);
    assignBoth(r.sc, OLD);
    r.remove(OLD);
    recheck(r.sc);
    expect(r.events).toEqual([]);
    r.lose(NEW);
    expect(r.events.map((e) => e.reconnecting)).toEqual([true, true]);
  });

  it('(5) a reassignment to a NOT-connected member is not resolved: the hold expires into reconnecting; a connect in time resolves it', () => {
    const r = rig([OLD, NEW]);
    const THIRD = handlerRef(3);
    assignBoth(r.sc, OLD);
    r.remove(OLD);
    assignBoth(r.sc, THIRD); // in the set, still dialling
    r.sched.advance(GRACE - 1);
    expect(r.events).toEqual([]);
    r.sched.advance(1);
    expect(r.events.map((e) => e.reconnecting)).toEqual([true, true]);
    // Positive arm on a fresh rig: THIRD connects within the hold → nothing shown.
    const q = rig([OLD, NEW]);
    assignBoth(q.sc, OLD);
    q.remove(OLD);
    assignBoth(q.sc, THIRD);
    q.sched.advance(GRACE - 1);
    q.live.add(THIRD);
    q.sc.handlerConnected(THIRD);
    q.sched.advance(GRACE * 5);
    expect(q.events).toEqual([]);
  });

  it('(6) session end during the hold: no event, and the timer is inert afterwards', () => {
    const r = rig([OLD, NEW]);
    assignBoth(r.sc, OLD);
    r.remove(OLD);
    r.sc.end();
    r.sched.advance(GRACE * 5);
    expect(r.events).toEqual([]);
  });

  it('grace: a follow-up at grace-1 decides; NO follow-up by the grace → reconnecting', () => {
    const r = rig([OLD, NEW]);
    assignBoth(r.sc, OLD);
    r.remove(OLD);
    recheck(r.sc);
    r.sched.advance(GRACE - 1);
    expect(r.events).toEqual([]);
    r.sched.advance(1);
    expect(r.events.map((e) => e.reconnecting)).toEqual([true, true]);
    expect(r.events[1]!.previousSenderId).toBe(BEA); // the verified live sender
  });

  it('a follow-up with NO source decides "no overlay" (the latest assignment renders)', () => {
    const r = rig([OLD, NEW]);
    assignBoth(r.sc, OLD);
    r.remove(OLD);
    r.sc.assignments({
      assignments: [
        assignment(AUDIO_SLOT, 'audio', undefined, undefined, 'fewer_sources_than_slots'),
        assignment(VIDEO_SLOT, 'video', undefined, undefined, 'fewer_sources_than_slots'),
      ],
      unreachableSenderIds: [],
    });
    r.sched.advance(GRACE * 2);
    expect(r.events).toEqual([]);
    expect(r.resumes.map((o) => o.outcome)).toEqual(['reassigned_away', 'reassigned_away']);
  });
});

describe('the reconnecting overlay on an OBSERVED loss — at once, never inferred', () => {
  it('crash (no code → handlerLost) sets it at once, keeping the previous occupant', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    expect(r.events).toEqual([]); // negative before the loss
    r.lose(OLD);
    expect(r.events).toEqual([
      { slotId: AUDIO_SLOT, reconnecting: true, previousSenderId: BEA },
      { slotId: VIDEO_SLOT, reconnecting: true, previousSenderId: BEA },
    ]);
  });

  it('a loss DURING a pending removal sets it at once (no waiting for the grace)', () => {
    const r = rig([OLD, NEW]);
    assignBoth(r.sc, OLD);
    r.sc.handlerRemoved(OLD, { wasConnected: true, otherConnected: true });
    expect(r.events).toEqual([]);
    r.sc.handlerLost(OLD);
    expect(r.events.map((e) => e.reconnecting)).toEqual([true, true]);
  });

  it('a loss of a handler NO slot names sets nothing (positive arm: the named one does)', () => {
    const r = rig([OLD, NEW]);
    assignBoth(r.sc, OLD);
    r.lose(NEW);
    expect(r.events).toEqual([]);
    r.lose(OLD);
    expect(r.events).toHaveLength(2);
  });

  it('reassigned to an in-set but NOT-YET-connected ref keeps it; that ref connecting clears it', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    assignBoth(r.sc, NEW);
    expect(r.reconnecting(AUDIO_SLOT)).toBe(true);
    r.live.add(NEW);
    r.sc.handlerConnected(NEW); // no new assignment needed
    expect(r.reconnecting(AUDIO_SLOT)).toBe(false);
    expect(r.reconnecting(VIDEO_SLOT)).toBe(false);
    expect(r.events.at(-1)).toEqual({
      slotId: VIDEO_SLOT,
      reconnecting: false,
      previousSenderId: undefined,
    });
  });

  it('the SAME ref reconnecting (a redial) clears it', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    r.live.add(OLD);
    r.sc.handlerConnected(OLD);
    expect(r.reconnecting(AUDIO_SLOT)).toBe(false);
  });

  it('cap: at cap-1 still reconnecting; at cap it clears (the latest assignment renders)', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    r.sched.advance(CAP - 1);
    expect(r.reconnecting(AUDIO_SLOT)).toBe(true);
    r.sched.advance(1);
    expect(r.reconnecting(AUDIO_SLOT)).toBe(false);
    expect(r.events.filter((e) => !e.reconnecting)).toHaveLength(2);
  });

  it('a sourceless slot never had a handler: nothing to reconnect', () => {
    const r = rig();
    r.sc.assignments({
      assignments: [
        assignment(AUDIO_SLOT, 'audio', undefined, undefined, 'fewer_sources_than_slots'),
      ],
      unreachableSenderIds: [],
    });
    r.lose(OLD);
    expect(r.events).toEqual([]);
  });
});

describe('removal cleanup (lead ruling 2026-11-13: ADR-0039 §4 "until the slot has a source again")', () => {
  it('loss then removal: a slot still naming the removed ref STAYS reconnecting; a frame via the removed ref never ends its episode', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    r.sched.advance(500);
    r.remove(OLD);
    recheck(r.sc);
    expect(r.reconnecting(AUDIO_SLOT)).toBe(true);
    expect(r.reconnecting(VIDEO_SLOT)).toBe(true);
    r.sc.presented(AUDIO_SLOT, OLD, BEA); // negative: removed transport
    expect(r.resumes).toEqual([]);
    r.live.add(NEW);
    r.sched.advance(1_500);
    r.sc.presented(AUDIO_SLOT, NEW, BEA); // positive: a connected transport
    expect(r.resumes).toEqual([{ kind: 'audio', outcome: 'resumed', ms: 2_000 }]);
  });

  it('removal FIRST (told to drop): the episode starts at the removal; OLD frames never end it, NEW ends it', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.remove(OLD);
    r.sched.advance(800);
    r.sc.presented(AUDIO_SLOT, OLD, BEA);
    expect(r.resumes).toEqual([]);
    r.live.add(NEW);
    r.sc.presented(AUDIO_SLOT, NEW, BEA);
    expect(r.resumes).toEqual([{ kind: 'audio', outcome: 'resumed', ms: 800 }]);
  });
});

describe('dt_client_media_resume_ms — one episode per ACTIVE slot', () => {
  it('resumed: the first frame via another CONNECTED handler ends it; frames via the LOST one do not', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    r.sched.advance(1_000);
    r.sc.presented(AUDIO_SLOT, OLD, BEA); // a trailing frame via the lost transport
    expect(r.resumes).toEqual([]);
    r.live.add(NEW);
    r.sched.advance(3_000);
    r.sc.presented(AUDIO_SLOT, NEW, BEA);
    r.sc.presented(VIDEO_SLOT, NEW, BEA);
    expect(r.resumes).toEqual([
      { kind: 'audio', outcome: 'resumed', ms: 4_000 },
      { kind: 'video', outcome: 'resumed', ms: 4_000 },
    ]);
  });

  it('after the lost ref RECONNECTS (a new transport), a frame via it ends the episode', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    r.sched.advance(2_500);
    r.live.add(OLD);
    r.sc.handlerConnected(OLD);
    r.sc.presented(AUDIO_SLOT, OLD, BEA);
    expect(r.resumes).toEqual([{ kind: 'audio', outcome: 'resumed', ms: 2_500 }]);
  });

  it('reassigned_away: the next assignment gives NO source', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    r.sc.assignments({
      assignments: [
        assignment(AUDIO_SLOT, 'audio', undefined, undefined, 'fewer_sources_than_slots'),
        assignment(VIDEO_SLOT, 'video', NEW, undefined),
      ],
      unreachableSenderIds: [],
    });
    expect(r.resumes).toEqual([{ kind: 'audio', outcome: 'reassigned_away', ms: 0 }]);
  });

  it('reassigned_away: a DIFFERENT sender (audio by assignment, video by the verified frame)', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    r.live.add(NEW);
    r.sc.assignments({
      assignments: [
        assignment(AUDIO_SLOT, 'audio', NEW, CAL),
        assignment(VIDEO_SLOT, 'video', NEW, undefined),
      ],
      unreachableSenderIds: [],
    });
    r.sc.presented(VIDEO_SLOT, NEW, CAL);
    expect(r.resumes.map((o) => [o.kind, o.outcome])).toEqual([
      ['audio', 'reassigned_away'],
      ['video', 'reassigned_away'],
    ]);
  });

  it('abandoned at the cap: cap-1 open, cap observed', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    r.sched.advance(ABANDON - 1);
    expect(r.resumes).toEqual([]);
    r.sched.advance(1);
    expect(r.resumes.map((o) => [o.outcome, o.ms])).toEqual([
      ['abandoned', ABANDON],
      ['abandoned', ABANDON],
    ]);
  });

  it('abandoned at session end; the tracker is inert afterwards (no event, no observation)', () => {
    const r = rig();
    assignBoth(r.sc, OLD);
    r.lose(OLD);
    r.sched.advance(900);
    const eventsBefore = r.events.length;
    r.sc.end();
    expect(r.resumes.map((o) => [o.outcome, o.ms])).toEqual([
      ['abandoned', 900],
      ['abandoned', 900],
    ]);
    r.sc.handlerRemoved(OLD); // the transport's teardown removals
    r.live.add(NEW);
    r.sc.presented(AUDIO_SLOT, NEW, BEA);
    r.sched.advance(ABANDON * 2);
    expect(r.resumes).toHaveLength(2);
    expect(r.events).toHaveLength(eventsBefore);
  });

  it('a client with NO active slot starts no episode; an active-slot client on the same loss does', () => {
    const r = rig([OLD, NEW]);
    r.sc.assignments({
      assignments: [assignment(AUDIO_SLOT, 'audio', OLD, BEA, 'avatar_client_muted')],
      unreachableSenderIds: [],
    });
    r.lose(OLD);
    r.sched.advance(ABANDON * 2);
    expect(r.resumes).toEqual([]);
    // Positive arm: the slot goes active on NEW, which is then lost; a frame via OLD (reconnected) ends it.
    r.sc.assignments({
      assignments: [assignment(AUDIO_SLOT, 'audio', NEW, BEA)],
      unreachableSenderIds: [],
    });
    r.lose(NEW);
    r.live.add(OLD);
    r.sc.presented(AUDIO_SLOT, OLD, BEA);
    expect(r.resumes.map((o) => o.outcome)).toEqual(['resumed']);
  });
});
