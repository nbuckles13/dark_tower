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
  StreamAssignmentEvent,
  StreamAssignmentsEvent,
} from '@darktower/sdk-core';

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
   * state and the one the loopback is diagnosing. **Observed, never gated**
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
   * `$lastMediaFault` — the most recent bounded, SDK-authored media fault.
   *
   * Held so a UI can show that the pipeline broke instead of showing silence.
   * Distinct from `MeetingStore.lastError`, which is terminal for the session; a
   * media fault leaves signaling joined and healthy.
   */
  #lastMediaFault = $state<MediaFault | undefined>(undefined);

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

  /** Reactive getter for `$lastMediaFault`. */
  get lastMediaFault(): MediaFault | undefined {
    return this.#lastMediaFault;
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
  }

  /** Apply a `mediaFault` event. Bounded and SDK-authored; never a platform string. */
  applyMediaFault(fault: MediaFault): void {
    this.#lastMediaFault = fault;
  }
}
