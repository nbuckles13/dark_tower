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
    if (!sink) return;
    for (const p of participants) {
      if (p.senderId === undefined) continue;
      // A participant id reappearing under a DIFFERENT sender id (a reconnect
      // rotate-and-reissue) retires the old one: forget its key now, or it would
      // survive until LRU eviction — defeating the leaver cutoff below, and
      // leaving this map bounded by history rather than by the roster.
      const previous = this.#senderByParticipant.get(p.participantId);
      if (previous !== undefined && previous !== p.senderId) sink.remove(previous);
      this.#senderByParticipant.set(p.participantId, p.senderId);
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
    this.#senderByParticipant.delete(participantId);
    if (senderId !== undefined) this.#sink?.remove(senderId);
  }
}
