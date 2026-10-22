// File: packages/sdk-core/src/signaling/rosterKeyFeed.ts
//
// Turns roster signalling into operations on the media path's identity-key
// resolver: participants joining (with their published keys) and participants
// leaving. Split out of `SignalingClient` as one responsibility — the dispatch
// loop decodes messages; this decides what each means for the key resolver.

import type { RosterKeySink } from './events.js';

/** One roster participant, as the key feed needs it. */
export interface RosterKeyParticipant {
  readonly participantId: string;
  /** Absent when MC has allocated no sender id: no key id can name it. */
  readonly senderId?: number | undefined;
  readonly identityPublicKey: Uint8Array;
}

/** Feeds the roster into a `RosterKeySink`, and forgets leavers. */
export class RosterKeyFeed {
  readonly #sink: RosterKeySink | undefined;
  /**
   * participant id -> sender id, from the roster this client saw. A leave names
   * only the participant, so this is how it finds the key to forget. Bounded by
   * MC's roster: an entry is deleted on leave.
   */
  readonly #senderByParticipant = new Map<string, number>();
  /**
   * The SAME index, reversed: sender id -> participant id. Written ONLY by the
   * two private helpers below, together with the forward map, so the two
   * directions are one index and cannot disagree. O(1) because the video
   * receive path reads it on EVERY video frame at arrival (story 3 C-B: the
   * server-mute check by verified sender) and at every draw.
   */
  readonly #participantBySender = new Map<number, string>();

  constructor(sink: RosterKeySink | undefined) {
    this.#sink = sink;
  }

  /**
   * Feed roster identity keys into the media path's resolver.
   *
   * Fire-and-forget: `importKey` is async and the dispatch loop is not, and a
   * roster update must not block the read loop. A key that has not landed yet
   * simply means the next frame from that sender is dropped and counted as
   * `no_roster_entry` — which is the correct transient, and is why the roster
   * update must travel the same signalling path as the KEK and land first.
   */
  joined(participants: readonly RosterKeyParticipant[]): void {
    const sink = this.#sink;
    for (const p of participants) {
      if (p.senderId === undefined) continue;
      // A participant id reappearing under a DIFFERENT sender id (a reconnect
      // rotate-and-reissue) retires the old one: forget its key now, or it would
      // survive until LRU eviction — defeating the leaver cutoff below, and
      // leaving this map bounded by history rather than by the roster.
      const previous = this.#senderByParticipant.get(p.participantId);
      if (previous !== undefined && previous !== p.senderId) sink?.remove(previous);
      // Maintained with or without a sink: it is also the roster's one
      // participant -> sender index (`senderIdOf`).
      this.#bind(p.participantId, p.senderId);
      if (!sink) continue;
      void sink
        .upsert({ senderId: p.senderId, identityPublicKey: p.identityPublicKey })
        .catch(() => {
          // The resolver already fails closed on an unusable key by recording the
          // absence; there is nothing further to report and nothing safe to log
          // about a participant's key material.
        });
    }
  }

  /**
   * The sender id the roster published for `participantId`, or `undefined`.
   * THE roster's participant <-> sender index. Never ATTRIBUTION (which is the
   * frame's verified key id): it serves presentation and, through
   * {@link participantIdOf}, the video never-render guard that looks up a
   * VERIFIED sender's server-mute state (story 3 C-B, security S2).
   */
  senderIdOf(participantId: string): number | undefined {
    return this.#senderByParticipant.get(participantId);
  }

  /**
   * The participant the roster published `senderId` for, or `undefined`. The
   * reverse direction of the ONE index (written only with the forward map, by
   * `#bind`/`#unbind`), so it follows every rebind and leave. Maintained by the
   * same `joined()` entry that feeds the sender's identity key, so a sender whose
   * frame VERIFIES (a peer roster key) is mapped by construction. O(1): read on
   * EVERY video frame at arrival and at every draw.
   */
  participantIdOf(senderId: number): string | undefined {
    return this.#participantBySender.get(senderId);
  }

  /**
   * A participant left: forget its roster key.
   *
   * A second, INDEPENDENT cutoff for a leaver, beside the slot-edge gate:
   * forgetting its roster key drops its frames at `no_roster_entry` BEFORE
   * verify, and the resolver invalidates the transmit keys it could still open
   * frames with. Replay state is kept. A leave for a participant this client
   * never saw is a no-op.
   */
  left(participantId: string): void {
    const senderId = this.#senderByParticipant.get(participantId);
    this.#unbind(participantId);
    if (senderId !== undefined) this.#sink?.remove(senderId);
  }

  /** Bind participant <-> sender in BOTH directions (the one write path). */
  #bind(participantId: string, senderId: number): void {
    this.#unbind(participantId);
    this.#senderByParticipant.set(participantId, senderId);
    this.#participantBySender.set(senderId, participantId);
  }

  /** Forget a participant in BOTH directions. */
  #unbind(participantId: string): void {
    const senderId = this.#senderByParticipant.get(participantId);
    this.#senderByParticipant.delete(participantId);
    if (senderId !== undefined && this.#participantBySender.get(senderId) === participantId) {
      this.#participantBySender.delete(senderId);
    }
  }
}
