// File: packages/sdk-svelte/src/stores/MediaStore.svelte.ts
//
// The reactive container for the media path's four ABSENCE-SIGNALS — the
// conditions ADR-0036 says a client must be TOLD about rather than infer from
// frames not arriving.
//
// ---------------------------------------------------------------------------
// WHY THIS IS A SEPARATE CLASS FROM `MeetingStore`
// ---------------------------------------------------------------------------
//
// Two independent reasons, and both would be lost by folding these fields into
// the roster store:
//
//   1. **Re-render granularity.** A mute toggle happens during a call, on a page
//      showing a participant grid. It must not invalidate the roster
//      projection. Svelte 5 tracks per `$state` CELL, so the isolation is real
//      only if mute lives in its own cell — which it does here, and which
//      `MeetingStore` hangs off a plain (non-reactive) field so that reading
//      `store.media` tracks nothing.
//   2. **One cell per field, never one cell holding an object.** Replacing an
//      object invalidates every reader of it, so `{audioMuted, slots, ...}` in a
//      single `$state` would make an unmute re-render the slot list and vice
//      versa — the exact coupling (1) exists to remove, reintroduced one level
//      down. Each field below is its own cell.
//
// ---------------------------------------------------------------------------
// THIS STORE HOLDS PROJECTIONS, NEVER DERIVATIONS
// ---------------------------------------------------------------------------
//
// Every value here is written ONLY by `subscribeSession` from an SDK event, and
// the SDK is the single authoritative holder of each. In particular
// `audioMuted` is written only from `muteChanged`, which the SDK emits from its
// own `MuteState` — the same state that gates capture. Nothing here, and nothing
// downstream of here, may derive mute from frame counts, from a timer, or from
// the click that requested it:
//
//   * from frame absence, it inverts DANGEROUSLY — a transport stall would
//     display "muted" while the microphone is live (ADR-0036 §5);
//   * from the click, the indicator would show what the user asked for rather
//     than what the SDK did, which is a lie the moment the request is refused.
//
// No metric, log or trace is emitted here. Media-path telemetry is the SDK's
// (ADR-0036 §11), and none of these values may become a metric dimension.

import type {
  MediaFault,
  MuteSnapshot,
  ParticipantMuteEvent,
  StreamAssignmentEvent,
  StreamAssignmentsEvent,
  UnmuteRequestedEvent,
} from '@darktower/sdk-core';
import { emptyMap, emptySet, withEntry, withMember } from './snapshots.js';

/** One server-muted participant, as MC last reported it. */
export interface ServerMuteState {
  /** Participant id of whoever applied it, when MC said. Resolve via the roster to render. */
  readonly serverMutedBy?: string | undefined;
}

export class MediaStore {
  /**
   * `$audioMuted` — client mute, as the SDK reports it.
   *
   * **The only value a mute indicator may render from.** Starts `false` because
   * `MuteState` starts unmuted; it is not "unknown", so there is no third state
   * for a UI to mishandle.
   */
  #audioMuted = $state(false);

  /**
   * `$firstMediaFrameMs` — ms from media start to the first frame arriving.
   *
   * `undefined` means "no media has come back yet", which is a real, renderable
   * state (no sender has reached this receiver yet). **Observed, never gated**
   * (ADR-0036 §10): no threshold may be compared against this, here or in a UI.
   */
  #firstMediaFrameMs = $state<number | undefined>(undefined);

  /**
   * `$slots` — MC's current slot assignments, each carrying its own wire state.
   *
   * Empty means MC has not sent assignments yet, which is DIFFERENT from a slot
   * assigned with nothing in it — see `slotState.ts` in the web app for why that
   * distinction is rendered rather than collapsed.
   */
  #slots = $state<readonly StreamAssignmentEvent[]>([]);

  /**
   * `$unreachableSenderIds` — roster participants this subscriber cannot reach.
   *
   * Its OWN `$state` cell, for the same reason every other field here has one: a
   * connectivity change must not invalidate readers of the slot list or the mute
   * state. Carried through from the wire (ADR-0036 §9) and never derived from
   * having noticed that frames stopped.
   *
   * Rendering it is task 15's (the multi-participant UI owns the roster surface).
   * This field is the seam that stops the wire fact being dropped at the SDK
   * boundary in the meantime — `signaling.proto` states client marking as an
   * obligation, and until it is rendered the gap is a known unmet one rather than
   * an invisible one.
   */
  #unreachableSenderIds = $state<readonly number[]>([]);

  /**
   * `$lastMediaFault` — the most recent bounded, SDK-authored media fault.
   *
   * Held so a UI can show that the pipeline broke instead of showing silence.
   * Distinct from `MeetingStore.lastError`, which is terminal for the session; a
   * media fault leaves signaling joined and healthy.
   */
  #lastMediaFault = $state<MediaFault | undefined>(undefined);

  /**
   * `$serverMutes` — every participant MC currently reports as SERVER-muted
   * (this participant included), keyed by participant id (story 2 R-11).
   *
   * Written ONLY from `participantMuteChanged` (the wire's `ParticipantMuteUpdate`).
   * Nothing sets it from a click — not the host's mute button and not a muted
   * participant's own unmute request (R-10: that only notifies the host). Its own
   * cell, REPLACED on every change, so reactivity stays per-cell.
   */
  #serverMutes = $state<ReadonlyMap<string, ServerMuteState>>(emptyMap());

  /**
   * `$unmuteRequests` — participants who asked the host to lift their server mute
   * (only a host receives these). A request is cleared when MC reports that
   * participant's server mute lifted; it never lifts anything itself.
   */
  #unmuteRequests = $state<ReadonlySet<string>>(emptySet());

  /** Reactive getter for `$audioMuted`. */
  get audioMuted(): boolean {
    return this.#audioMuted;
  }

  /** Reactive getter for `$firstMediaFrameMs`. */
  get firstMediaFrameMs(): number | undefined {
    return this.#firstMediaFrameMs;
  }

  /** Whether any media has come back from the handler yet. */
  get hasReceivedMedia(): boolean {
    return this.#firstMediaFrameMs !== undefined;
  }

  /** Reactive getter for `$slots` (readonly view). */
  get slots(): readonly StreamAssignmentEvent[] {
    return this.#slots;
  }

  /** Reactive getter for `$unreachableSenderIds` (readonly view). */
  get unreachableSenderIds(): readonly number[] {
    return this.#unreachableSenderIds;
  }

  /** Reactive getter for `$lastMediaFault`. */
  get lastMediaFault(): MediaFault | undefined {
    return this.#lastMediaFault;
  }

  /** Reactive getter for `$serverMutes` (readonly view). */
  get serverMutes(): ReadonlyMap<string, ServerMuteState> {
    return this.#serverMutes;
  }

  /** Reactive getter for `$unmuteRequests` (readonly view). */
  get unmuteRequests(): ReadonlySet<string> {
    return this.#unmuteRequests;
  }

  /** Apply a `participantMuteChanged` event: replace the map; unmute also clears any request. */
  applyParticipantMute(event: ParticipantMuteEvent): void {
    this.#serverMutes = withEntry(
      this.#serverMutes,
      event.participantId,
      event.audioServerMuted ? { serverMutedBy: event.serverMutedBy } : undefined,
    );
    if (!event.audioServerMuted && this.#unmuteRequests.has(event.participantId)) {
      this.#unmuteRequests = withMember(this.#unmuteRequests, event.participantId, false);
    }
  }

  /**
   * Apply an `unmuteRequested` event. Recorded only while MC says the requester
   * IS server-muted — a stale relay for someone already unmuted is not a pending
   * request.
   */
  applyUnmuteRequested(event: UnmuteRequestedEvent): void {
    if (!this.#serverMutes.has(event.participantId)) return;
    if (this.#unmuteRequests.has(event.participantId)) return;
    this.#unmuteRequests = withMember(this.#unmuteRequests, event.participantId, true);
  }

  /** Apply a `muteChanged` event. The SDK's snapshot, unmodified. */
  applyMuteChanged(snapshot: MuteSnapshot): void {
    this.#audioMuted = snapshot.audioMuted;
  }

  /**
   * Apply a `firstMediaFrame` event.
   *
   * First one wins: the SDK observes this at most once per session, and a second
   * write would turn a startup measurement into something that moves.
   */
  applyFirstMediaFrame(elapsedMs: number): void {
    if (this.#firstMediaFrameMs !== undefined) return;
    this.#firstMediaFrameMs = elapsedMs;
  }

  /**
   * Apply a `streamAssignments` event.
   *
   * REPLACES rather than merges: the message is MC's complete current view of
   * this subscriber's slots, so merging would resurrect a slot MC has dropped.
   */
  applyStreamAssignments(event: StreamAssignmentsEvent): void {
    this.#slots = [...event.assignments];
    // REPLACED, not merged, for the same reason and with sharper stakes: a
    // merged unreachable set latches a transient into a permanent "can't hear X"
    // badge on a meeting whose audio is fine, which is indistinguishable to the
    // user from the real fault. A peer that becomes reachable simply stops
    // appearing in the set, and needs no "now reachable" signal of its own.
    this.#unreachableSenderIds = [...event.unreachableSenderIds];
  }

  /** Apply a `mediaFault` event. Bounded and SDK-authored; never a platform string. */
  applyMediaFault(fault: MediaFault): void {
    this.#lastMediaFault = fault;
  }
}
