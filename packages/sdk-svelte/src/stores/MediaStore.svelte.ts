// File: packages/sdk-svelte/src/stores/MediaStore.svelte.ts
//
// The reactive container for the media path's ABSENCE-SIGNALS — the
// conditions ADR-0036 says a client must be TOLD about rather than infer from
// frames not arriving — plus the story-3 per-slot, per-participant and pairing
// cells (`mediaCells.svelte.ts`) and the R-18 notice latch.
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
  CameraHealthChangedEvent,
  CaptureResumedAfterServerMuteEvent,
  ClientMuteFacts,
  MediaCaptureFailureOutcome,
  MediaFault,
  MuteSnapshot,
  ParticipantMuteEvent,
  PublishKind,
  StreamAssignmentEvent,
  StreamAssignmentsEvent,
  UnmuteRequestedEvent,
  VideoSlotRenderChangedEvent,
} from '@darktower/sdk-core';
import { pairSlotsBySender } from '@darktower/sdk-core';
import {
  CellRegistry,
  EMPTY_PAIR,
  EMPTY_PARTICIPANT,
  EMPTY_SLOT,
  PairCell,
  ParticipantMediaCell,
  SlotCell,
  type PairView,
  type ParticipantMediaView,
  type SlotView,
} from './mediaCells.svelte.js';

/** The kinds a resume notice can be about (the SDK's publish kinds). */
type NoticeKind = PublishKind;

export class MediaStore {
  /**
   * `$audioMuted` / `$videoMuted` — client mute, as the SDK reports it.
   *
   * **The only values a mute indicator may render from.** Start `false` because
   * `MuteState` starts unmuted; not "unknown", so there is no third state for a
   * UI to mishandle. Separate cells: an audio toggle does not invalidate video
   * readers.
   */
  #audioMuted = $state(false);
  #videoMuted = $state(false);

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
   * The WHOLE list, for layout-level readers (how many rows, which ids). A
   * reader that renders ONE slot reads {@link slot} instead, whose cells change
   * only when that slot does.
   */
  #slots = $state<readonly StreamAssignmentEvent[]>([]);

  /**
   * `$unreachableSenderIds` — roster participants this subscriber cannot reach.
   *
   * Its OWN `$state` cell. Carried through from the wire (ADR-0036 §9) and never
   * derived from having noticed that frames stopped.
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
   * `$cameraFailure` — this participant's camera gate's LATCHED failure (story 3
   * R-19), from `cameraHealthChanged`. `undefined` while healthy. A closed token.
   */
  #cameraFailure = $state<MediaCaptureFailureOutcome | undefined>(undefined);

  /**
   * The R-18 NOTICE LATCH, one cell per kind: set by the SDK's
   * `captureResumedAfterServerMute` and cleared ONLY by
   * {@link acknowledgeResume}. Held HERE rather than in a component so a
   * re-mount or a grid rebuild can never drop a pending notice (security C-C1).
   * A second resume of the same kind while pending keeps ONE pending notice; a
   * resume of the other kind is its own notice. Nothing else clears it — no
   * timer, no mute event, no setting.
   */
  #audioResumeNotice = $state(false);
  #videoResumeNotice = $state(false);

  /** Per-slot cells (assignment, video render). */
  readonly #slotCells = new CellRegistry<number, SlotCell, SlotView>(
    () => new SlotCell(),
    EMPTY_SLOT,
  );
  /** Per-participant server-mute cells, keyed by participant id. */
  readonly #participantCells = new CellRegistry<string, ParticipantMediaCell, ParticipantMediaView>(
    () => new ParticipantMediaCell(),
    EMPTY_PARTICIPANT,
  );
  /** Per-sender pairing cells. */
  readonly #pairCells = new CellRegistry<number, PairCell, PairView>(
    () => new PairCell(),
    EMPTY_PAIR,
  );
  /**
   * Video slot id -> the VERIFIED sender its live slot shows, from the SDK's
   * render events. PLAIN (not reactive): it only feeds the pairing recompute,
   * whose cells are what readers track.
   */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- deliberately NOT reactive (see above)
  readonly #liveVideo = new Map<number, number>();

  /** Reactive getter for `$audioMuted`. */
  get audioMuted(): boolean {
    return this.#audioMuted;
  }

  /** Reactive getter for `$videoMuted`. */
  get videoMuted(): boolean {
    return this.#videoMuted;
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

  /** Reactive getter for `$cameraFailure` (R-19). */
  get cameraFailure(): MediaCaptureFailureOutcome | undefined {
    return this.#cameraFailure;
  }

  /** ONE slot's cells. Reading it tracks only that slot. */
  slot(slotId: number): SlotView {
    return this.#slotCells.read(slotId);
  }

  /** ONE participant's mute cells (all four facts). Reading it tracks only that participant. */
  participant(participantId: string): ParticipantMediaView {
    return this.#participantCells.read(participantId);
  }

  /** ONE sender's pairing (audio slot + video slot, by publisher). */
  pairOf(senderId: number): PairView {
    return this.#pairCells.read(senderId);
  }

  /** Whether an R-18 notice for `kind` is pending acknowledgement. */
  resumeNoticePending(kind: NoticeKind): boolean {
    return kind === 'audio' ? this.#audioResumeNotice : this.#videoResumeNotice;
  }

  /**
   * The participant acknowledged the R-18 notice for `kind` (an explicit OK, or
   * the inline client-mute action). The ONLY way a pending notice clears.
   */
  acknowledgeResume(kind: NoticeKind): void {
    if (kind === 'audio') this.#audioResumeNotice = false;
    else this.#videoResumeNotice = false;
  }

  /**
   * Create `participantId`'s cells now, from a roster event (outside any
   * reaction). Called by `MeetingStore` on join / participant-joined, so a
   * roster row's cells exist before the row first renders and a LATER first
   * server mute re-renders only that participant's row and tile.
   */
  registerParticipant(participantId: string): void {
    this.#participantCells.ensure(participantId);
  }

  /**
   * Seed `participant`'s client mute from its roster entry (`joined`'s
   * `existingParticipants`, `participantJoined`), through the SAME setter as
   * {@link applyParticipantMute}: an absent kind is no fact and changes nothing.
   */
  applyRosterEntry(participant: ClientMuteFacts & { readonly participantId: string }): void {
    this.#participantCells
      .ensure(participant.participantId)
      .setClientMute(participant.audioClientMuted, participant.videoClientMuted);
  }

  /**
   * The participant left: their client-mute facts are forgotten (back to no
   * fact), so a later roster entry under the same id starts UNKNOWN rather than
   * inheriting a stale value. The cell itself stays (readers hold it).
   */
  applyParticipantLeft(participantId: string): void {
    this.#participantCells.ensure(participantId).clearClientMute();
  }

  /** Apply a `captureResumedAfterServerMute` event: latch the notice for its kind. */
  applyCaptureResumed(event: CaptureResumedAfterServerMuteEvent): void {
    if (event.kind === 'audio') this.#audioResumeNotice = true;
    else this.#videoResumeNotice = true;
  }

  /** Apply a `cameraHealthChanged` event. */
  applyCameraHealth(event: CameraHealthChangedEvent): void {
    this.#cameraFailure = event.failure;
  }

  /**
   * Apply a `participantMuteChanged` event to THAT participant's cells only. A
   * lift of every kind also clears their pending unmute request (R-10: the
   * request itself lifts nothing).
   */
  applyParticipantMute(event: ParticipantMuteEvent): void {
    const cell = this.#participantCells.ensure(event.participantId);
    cell.setServerMute(event.audioServerMuted, event.videoServerMuted, event.serverMutedBy);
    cell.setClientMute(event.audioClientMuted, event.videoClientMuted);
    if (!cell.serverMutedAny) cell.setUnmuteRequested(false);
  }

  /**
   * Apply an `unmuteRequested` event. Recorded only while MC says the requester
   * IS server-muted (either kind) — a stale relay for someone already unmuted is
   * not a pending request.
   */
  applyUnmuteRequested(event: UnmuteRequestedEvent): void {
    const cell = this.#participantCells.ensure(event.participantId);
    if (!cell.serverMutedAny) return;
    cell.setUnmuteRequested(true);
  }

  /** Apply a `muteChanged` event. The SDK's snapshot, unmodified. */
  applyMuteChanged(snapshot: MuteSnapshot): void {
    if (this.#audioMuted !== snapshot.audioMuted) this.#audioMuted = snapshot.audioMuted;
    if (this.#videoMuted !== snapshot.videoMuted) this.#videoMuted = snapshot.videoMuted;
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
   * Per-slot cells change only for slots whose assignment changed; a slot MC
   * dropped reads `undefined` again.
   */
  applyStreamAssignments(event: StreamAssignmentsEvent): void {
    this.#slots = [...event.assignments];
    // A local, per-call scratch set: never read reactively.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const present = new Set<number>();
    for (const a of event.assignments) {
      present.add(a.slotId);
      this.#slotCells.ensure(a.slotId).setAssignment(a);
    }
    for (const [slotId, cell] of this.#slotCells.entries()) {
      if (!present.has(slotId)) cell.setAssignment(undefined);
    }
    // REPLACED, not merged, for the same reason and with sharper stakes: a
    // merged unreachable set latches a transient into a permanent "can't hear X"
    // badge on a meeting whose audio is fine, which is indistinguishable to the
    // user from the real fault. A peer that becomes reachable simply stops
    // appearing in the set, and needs no "now reachable" signal of its own.
    this.#unreachableSenderIds = [...event.unreachableSenderIds];
    this.#recomputePairs();
  }

  /**
   * Apply a `videoSlotRenderChanged` event: THAT slot's render cell, and the
   * pairing view's live half (a live video slot names no sender on the wire; it
   * pairs by the verified key-id sender of what it renders, R-5).
   */
  applyVideoRender(event: VideoSlotRenderChangedEvent): void {
    this.#slotCells.ensure(event.slotId).setRender(event.state);
    const before = this.#liveVideo.get(event.slotId);
    if (event.state.kind === 'active') this.#liveVideo.set(event.slotId, event.state.senderId);
    else this.#liveVideo.delete(event.slotId);
    if (before !== this.#liveVideo.get(event.slotId)) this.#recomputePairs();
  }

  /** Apply a `mediaFault` event. Bounded and SDK-authored; never a platform string. */
  applyMediaFault(fault: MediaFault): void {
    this.#lastMediaFault = fault;
  }

  /**
   * Recompute the pairing view with the SDK's ONE pairing function
   * (`pairSlotsBySender`), writing only the pair cells that changed.
   */
  #recomputePairs(): void {
    const pairs = pairSlotsBySender(this.#slots, this.#liveVideo);
    for (const [senderId, pair] of pairs) {
      this.#pairCells.ensure(senderId).set(pair.audioSlotId, pair.videoSlotId);
    }
    for (const [senderId, cell] of this.#pairCells.entries()) {
      if (!pairs.has(senderId)) cell.set(undefined, undefined);
    }
  }
}
