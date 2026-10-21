// File: packages/web-app/e2e/slotOracle.ts
//
// Story 3 R-32 layer 1: the TEST'S OWN slot oracle. It gives the occupant each
// subscriber's every slot SHOULD hold, from the join order and the fill rule.
// Pure (no Playwright, no `./env`, no clock), so `tests/slotOracle.test.ts` runs
// it on the node tier.
//
// ---------------------------------------------------------------------------
// WHY IT EXISTS
// ---------------------------------------------------------------------------
//
// MC never names the source of a live video slot: video ACTIVE carries no
// sender_id (R-5), while audio ACTIVE keeps it. So "the key-id sender rendered
// in this video slot is the right one" needs an expectation that does not come
// from MC. This module is that expectation.
//
// ---------------------------------------------------------------------------
// RULE SOURCE: THE STORY TEXT, NEVER MC'S CODE
// ---------------------------------------------------------------------------
//
// Written from R-3 in `docs/user-stories/2026-10-07-see-each-other.md` and the
// MC-A SLOT TABLE bullets there (including the 2026-10-21 KIND ORDER
// amendment). It imports nothing from `crates/mc-service` and shares no code
// with MC's fill (`media_routing/slots.rs`) or MC's in-module model check. A
// shared bug would cancel out. `tests/slotOracle.test.ts` enforces the import
// rule. Where MC and this oracle disagree, the env-test fails, and that is a
// finding about one of them, never a reason to align the oracle to MC's
// output.
//
// R-3, as implemented:
//   * Slots of each kind fill per subscriber S by:
//       (1) the earliest joiner already holding a slot of the OTHER kind with S
//           but none of this kind (pair completion), else
//       (2) the earliest eligible joiner holding no slot of this kind.
//   * Eligible: never S itself; shares at least one connected handler with S
//     (otherwise the pair is unreachable and consumes no slot); for AUDIO the
//     candidate publishes audio; for VIDEO, any participant (receive-only and
//     audio-only render as AVATAR_NO_VIDEO).
//   * STABILITY BEATS PAIRING: no join, leave, mute or redeclaration of the
//     other kind moves an occupied slot. A leave frees only the leaver's slots.
//     A connectivity change frees only the slots of pairs that became
//     unreachable. A redeclaration that keeps a kind's count keeps its
//     occupants by ordinal; growing appends empty slots; shrinking truncates
//     the tail.
//   * Over-cap declarations truncate: slots past the per-kind cap are
//     UNAVAILABLE.
//   * KIND ORDER (lead ruling 2026-10-21): when S has empty slots of both kinds
//     at one event, fill AUDIO first, then VIDEO, ONE slot at a time, repeating
//     to a fixed point. See {@link KIND_FILL_ORDER}.
//
// Output states (the wire names the bus carries):
//   video occupant: active (publishes video, not muted) | avatar_no_video
//                   (publishes no video) | avatar_client_muted |
//                   avatar_server_muted (wins over client-muted);
//   audio occupant: active | avatar_client_muted | avatar_server_muted (wins);
//   no occupant:    fewer_sources; past the cap: unavailable.
// A participant that publishes no video has nothing to mute, so
// avatar_no_video takes precedence over the mute avatars on a video slot.

import type { SlotStateToken } from '../src/lib/slotState.js';
import type { PublishSet } from './cohort.js';

/** The two slot kinds. */
export type Kind = 'audio' | 'video';

/**
 * The kind order when a subscriber has empty slots of BOTH kinds at one event.
 * Audio first: rule (1) exists to complete pairs, and audio-first is the order
 * under which a both-kinds publisher pairs. Lead ruling at T-A planning,
 * 2026-10-21, mirrored in the story's MC-A SLOT TABLE "Fill order" bullets.
 * MC-A implements the same order. The order is pinned by
 * `tests/slotOracle.test.ts`.
 */
export const KIND_FILL_ORDER: readonly Kind[] = ['audio', 'video'];

/**
 * Every kind, for loops that visit each kind with no ordering meaning (leave,
 * redeclare, handler changes). Only the refill uses {@link KIND_FILL_ORDER}.
 */
const KINDS: readonly Kind[] = ['audio', 'video'];

/** The states the SDK's union already carries, checked against it now. */
export const SHARED_SLOT_STATES = [
  'active',
  'fewer_sources',
] as const satisfies readonly SlotStateToken[];

/**
 * PENDING SSoT. These five wire tokens are not yet in the SDK's
 * `StreamAssignmentEvent['slotState']` union (`src/lib/slotState.ts`
 * `SlotStateToken`). C-A1 (task 11) adds them there. When it does, C-C1 (task
 * 17) replaces this local list with
 * `as const satisfies readonly SlotStateToken[]`, as for
 * {@link SHARED_SLOT_STATES}, so the oracle and the app share one vocabulary.
 */
export const PENDING_SLOT_STATES = [
  'avatar_no_video',
  'avatar_client_muted',
  'avatar_server_muted',
  'unavailable',
] as const;

/** Every state the oracle can expect. */
export type ExpectedSlotState =
  (typeof SHARED_SLOT_STATES)[number] | (typeof PENDING_SLOT_STATES)[number];

/** One slot's expectation. */
export interface SlotExpectation {
  readonly state: ExpectedSlotState;
  /** The participant the slot holds; absent on fewer_sources / unavailable. */
  readonly occupant?: string;
}

/** A subscriber's expected slots, by kind, in declaration order. */
export interface SubscriberExpectation {
  readonly audio: readonly SlotExpectation[];
  readonly video: readonly SlotExpectation[];
  /** Participants unreachable to this subscriber (disjoint handler sets). */
  readonly unreachable: readonly string[];
}

/** Per-kind slot counts. */
export interface PerKind<T> {
  readonly audio: T;
  readonly video: T;
}

/** A join, as the test drives it. */
export interface OracleJoin {
  readonly publish: PublishSet;
  /** Declared receive slots per kind (before truncation). */
  readonly slots: PerKind<number>;
  /** The handlers this participant is connected to. */
  readonly handlers: readonly string[];
}

interface Mute {
  client: boolean;
  server: boolean;
}

interface Member {
  readonly id: string;
  readonly rank: number;
  readonly publish: PublishSet;
  declared: { audio: number; video: number };
  handlers: Set<string>;
  readonly mutes: { audio: Mute; video: Mute };
  /** Filled slots per kind (`null` = empty), sized to min(declared, cap). */
  readonly slots: { audio: (string | null)[]; video: (string | null)[] };
}

const publishes = (p: PublishSet, kind: Kind): boolean =>
  p === 'both' || (kind === 'audio' ? p === 'audio' : p === 'video');

const otherKind = (kind: Kind): Kind => (kind === 'audio' ? 'video' : 'audio');

function checkCount(n: number, what: string): void {
  if (!Number.isInteger(n) || n < 0) {
    throw new RangeError(`slot oracle: ${what} must be a non-negative integer (got ${n})`);
  }
}

/**
 * The event-sourced R-3 model of one meeting. Every mutation refills every
 * subscriber's empty slots to the fixed point.
 */
export class SlotOracle {
  private readonly members = new Map<string, Member>();
  private nextRank = 0;

  /** `caps`: MC's per-kind receive-slot caps, as the cluster's MC config declares them. */
  constructor(private readonly caps: PerKind<number>) {
    checkCount(caps.audio, 'the audio cap');
    checkCount(caps.video, 'the video cap');
  }

  /** A participant joins (last in join order). */
  join(id: string, join: OracleJoin): void {
    if (this.members.has(id)) throw new Error(`slot oracle: ${id} already joined`);
    checkCount(join.slots.audio, `${id}'s audio slots`);
    checkCount(join.slots.video, `${id}'s video slots`);
    const member: Member = {
      id,
      rank: this.nextRank++,
      publish: join.publish,
      declared: { audio: join.slots.audio, video: join.slots.video },
      handlers: new Set(join.handlers),
      mutes: { audio: { client: false, server: false }, video: { client: false, server: false } },
      slots: {
        audio: new Array<string | null>(Math.min(join.slots.audio, this.caps.audio)).fill(null),
        video: new Array<string | null>(Math.min(join.slots.video, this.caps.video)).fill(null),
      },
    };
    this.members.set(id, member);
    this.refillAll();
  }

  /** A participant leaves: only its slots are freed, then refilled. */
  leave(id: string): void {
    this.member(id);
    this.members.delete(id);
    for (const s of this.members.values()) {
      for (const kind of KINDS) {
        s.slots[kind] = s.slots[kind].map((o) => (o === id ? null : o));
      }
    }
    this.refillAll();
  }

  /** Set one kind's client and/or server mute. Never moves a slot. */
  setMute(id: string, kind: Kind, mute: { client?: boolean; server?: boolean }): void {
    const m = this.member(id);
    if (mute.client !== undefined) m.mutes[kind].client = mute.client;
    if (mute.server !== undefined) m.mutes[kind].server = mute.server;
  }

  /**
   * Redeclare one subscriber's receive slots. A kind whose count is unchanged
   * keeps its occupants; growing appends empty slots; shrinking truncates.
   */
  redeclare(id: string, slots: Partial<PerKind<number>>): void {
    const m = this.member(id);
    for (const kind of KINDS) {
      const n = slots[kind];
      if (n === undefined) continue;
      checkCount(n, `${id}'s ${kind} slots`);
      m.declared[kind] = n;
      const size = Math.min(n, this.caps[kind]);
      const current = m.slots[kind];
      m.slots[kind] =
        size <= current.length
          ? current.slice(0, size)
          : [...current, ...new Array<string | null>(size - current.length).fill(null)];
    }
    this.refillAll();
  }

  /**
   * A participant's connected handler set changes. Only slots of pairs that no
   * longer share a handler are freed, in both directions. Then everything refills.
   */
  setHandlers(id: string, handlers: readonly string[]): void {
    const m = this.member(id);
    m.handlers = new Set(handlers);
    for (const other of this.members.values()) {
      if (other.id === id || this.shareHandler(m, other)) continue;
      for (const kind of KINDS) {
        m.slots[kind] = m.slots[kind].map((o) => (o === other.id ? null : o));
        other.slots[kind] = other.slots[kind].map((o) => (o === id ? null : o));
      }
    }
    this.refillAll();
  }

  /** What `subscriber` should see right now. */
  expected(subscriber: string): SubscriberExpectation {
    const s = this.member(subscriber);
    const project = (kind: Kind): SlotExpectation[] => {
      const filled = s.slots[kind].map((occupant): SlotExpectation => {
        if (occupant === null) return { state: 'fewer_sources' };
        return { state: this.occupantState(this.member(occupant), kind), occupant };
      });
      const truncated = Math.max(0, s.declared[kind] - this.caps[kind]);
      return [...filled, ...new Array<SlotExpectation>(truncated).fill({ state: 'unavailable' })];
    };
    const unreachable = [...this.members.values()]
      .filter((o) => o.id !== subscriber && !this.shareHandler(s, o))
      .sort((a, b) => a.rank - b.rank)
      .map((o) => o.id);
    return { audio: project('audio'), video: project('video'), unreachable };
  }

  private occupantState(o: Member, kind: Kind): ExpectedSlotState {
    // No video published wins over both mutes (story MC-A slot table, NO-VIDEO PRECEDENCE amendment 2026-10-21).
    if (kind === 'video' && !publishes(o.publish, 'video')) return 'avatar_no_video';
    if (o.mutes[kind].server) return 'avatar_server_muted';
    if (o.mutes[kind].client) return 'avatar_client_muted';
    return 'active';
  }

  private member(id: string): Member {
    const m = this.members.get(id);
    if (m === undefined) throw new Error(`slot oracle: no participant ${id}`);
    return m;
  }

  private shareHandler(a: Member, b: Member): boolean {
    for (const h of a.handlers) if (b.handlers.has(h)) return true;
    return false;
  }

  private eligible(s: Member, c: Member, kind: Kind): boolean {
    if (c.id === s.id || !this.shareHandler(s, c)) return false;
    return kind === 'video' || publishes(c.publish, 'audio');
  }

  /** The candidate for one empty slot of `kind` at `s`, by rules (1) then (2). */
  private candidate(s: Member, kind: Kind): string | null {
    const byRank = [...this.members.values()]
      .filter((c) => this.eligible(s, c, kind) && !s.slots[kind].includes(c.id))
      .sort((a, b) => a.rank - b.rank);
    const pair = byRank.find((c) => s.slots[otherKind(kind)].includes(c.id));
    return (pair ?? byRank[0])?.id ?? null;
  }

  /** Refill every subscriber to the fixed point, one slot at a time in KIND_FILL_ORDER. */
  private refillAll(): void {
    for (const s of this.members.values()) {
      let changed = true;
      while (changed) {
        changed = false;
        for (const kind of KIND_FILL_ORDER) {
          const empty = s.slots[kind].indexOf(null);
          if (empty < 0) continue;
          const c = this.candidate(s, kind);
          if (c === null) continue;
          s.slots[kind][empty] = c;
          changed = true;
        }
      }
    }
  }
}
