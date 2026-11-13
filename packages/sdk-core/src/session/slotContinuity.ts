// File: packages/sdk-core/src/session/slotContinuity.ts
//
// PER-SLOT CONTINUITY across a media-handler swap (ADR-0039 §4 "Slot states";
// story 4 C2, R-20 and the `dt_client_media_resume_ms` media-dark window). ONE
// record per declared slot drives BOTH derived facts, so they cannot disagree:
//
//   * the client-local RECONNECTING overlay (no wire SlotState): set when this
//     client OBSERVES the loss of the handler its slot's latest assignment names
//     (`MediaTransport` `handlerLost` — the one `isHandlerLoss` predicate), at
//     once. Never inferred from silence (ADR-0036 §6). The slot keeps its
//     previous occupant (the verified live sender, else MC's assigned sender —
//     never anything frame-borne and unverified). It clears when the slot's
//     latest assignment names a CONNECTED handler (on that assignment, or when
//     the handler connects), or at `reconnectingCapMs` — "until the slot has a
//     source again or a clientConfig cap expires" (§4).
//   * A REMOVAL ("told to drop") is DECIDED BY THE NEXT ASSIGNMENT, because MC
//     sends the set WITHOUT the old handler FIRST and the moved assignments
//     right after (`crates/mc-service/src/actors/meeting/health.rs`
//     `remove_handler_for`: `broadcast_handler_set` then `flush_views`). So at
//     the removal a slot still names the removed ref, and the overlay is held
//     PENDING — but only when the removed ref was still CONNECTED and another
//     member is connected (a planned transfer's shape; lead ruling
//     2026-11-13); otherwise the slot reconnects at once. The next decisive
//     assignment settles a hold — a CONNECTED handler (a planned transfer:
//     nothing is ever shown), an unconnected one (reconnecting), or no source
//     (the latest assignment renders). With no decisive assignment within
//     `removalGraceMs` (`media.handlers.removalGraceMs`) the slot reconnects.
//     The membership re-check SignalingClient emits at the set change (the
//     slot ACTIVE with its stale ref resolved to none) is not decisive.
//   * the RESUME EPISODE (`dt_client_media_resume_ms{outcome}`): opened at the
//     loss or the removal itself (by contract — so a planned transfer records
//     its blip, removal → first frame via the new handler; the client cannot
//     split planned from crash, `mc_media_handler_migrations_total{trigger}`
//     gives the mix), only for a slot ACTIVE at that instant (so a client with
//     no active slot starts none). Ends `resumed` at the first frame the slot
//     presents via a CONNECTED transport (another handler, or the same ref
//     after it reconnects: a frame still draining from a lost or removed
//     transport never ends it), `reassigned_away` when the latest assignment
//     gives no source or a different sender, and `abandoned` at session end or
//     `resumeAbandonMs`.
//
// REMOVAL CLEANUP (ADR-0039 §4, lead ruling 2026-11-13): no per-REF state is
// kept here at all — "is this transport live" is the transport's answer
// (`isConnected`), asked when a frame is presented — while a slot that still
// names a removed ref is reconnecting until reassigned to a connected handler
// or the cap.
//
// NOT HOSTED BY `media/lifecycle/unfulfilledSlots.ts` (deliberate non-collapse,
// DRY review 2026-11-13): that tracker is VIDEO-only and DECODE-driven, per
// kind inside the slot pipelines; this one spans audio AND video and is driven
// by transport lifecycle and set changes. Different subject, different inputs.
//
// No credential, no URL, no metric label beyond the per-kind handle's
// allow-listed bag. Timers capture the slot record, never a token.

import { DEFAULT_TRANSPORT_SCHEDULER, type TransportScheduler } from '../media/events.js';
import type { HandlerSetConfig } from '../config/clientConfig.js';
import type { MediaTransport } from '../media/MediaTransport.js';
import type { SignalingClient } from '../signaling/SignalingClient.js';
import type { HandlerRef } from '../media/handlerSet.js';
import type { MediaKindToken } from '../media/mediaKind.js';
import type { VideoRenderState } from '../media/pipeline/renderDecision.js';
import type { MediaResumeOutcome } from '../media/setup/mediaMetrics.js';
import type { StreamAssignmentEvent, StreamAssignmentsEvent } from '../signaling/events.js';

/** Payload of the session's `slotReconnectingChanged` event (transitions only). */
export interface SlotReconnectingChangedEvent {
  readonly slotId: number;
  readonly reconnecting: boolean;
  /**
   * The occupant the slot showed when it began reconnecting (the verified live
   * video sender, else MC's assigned sender), rendered as its avatar; or
   * `undefined` when none was known. Absent meaning on a clear.
   */
  readonly previousSenderId: number | undefined;
}

/** The per-kind resume histogram (`MediaKindMetrics.resumeMs`). */
export interface ResumeMetrics {
  resumeMs(outcome: MediaResumeOutcome, ms: number): void;
}

/** Construction options for {@link SlotContinuity}. */
export interface SlotContinuityOptions {
  /** `media.handlers.reconnectingCapMs`. */
  readonly reconnectingCapMs: number;
  /** `media.handlers.resumeAbandonMs`. */
  readonly resumeAbandonMs: number;
  /**
   * How long a planned-shape removal holds the overlay for MC's follow-up
   * assignment (`media.handlers.removalGraceMs`).
   */
  readonly removalGraceMs: number;
  readonly clock: () => number;
  readonly scheduler: TransportScheduler;
  /** Whether `ref` has a CONNECTED transport now (`MediaTransport.getState`). */
  readonly isConnected: (ref: HandlerRef) => boolean;
  /** The resume histogram for a kind, or `undefined` (no telemetry). */
  readonly metricsFor: (kind: MediaKindToken) => ResumeMetrics | undefined;
  readonly onReconnectingChanged: (event: SlotReconnectingChangedEvent) => void;
}

interface Overlay {
  readonly previousSenderId: number | undefined;
  timer: unknown;
}

/** A removal awaiting MC's follow-up assignment (see the module header). */
interface PendingDrop {
  readonly previousSenderId: number | undefined;
  timer: unknown;
}

interface Episode {
  readonly kind: MediaKindToken;
  readonly startedAt: number;
  /** The sender the slot showed at the start; a different one is `reassigned_away`. */
  readonly senderId: number | undefined;
  timer: unknown;
}

interface SlotRecord {
  assignment: StreamAssignmentEvent | undefined;
  /** The verified sender of the slot's live video, while it renders live. */
  liveSender: number | undefined;
  overlay: Overlay | undefined;
  pending: PendingDrop | undefined;
  episode: Episode | undefined;
}

/** One per media-carrying join attempt. See the module header. */
export class SlotContinuity {
  readonly #o: SlotContinuityOptions;
  readonly #slots = new Map<number, SlotRecord>();
  #ended = false;

  constructor(
    options: Omit<SlotContinuityOptions, 'scheduler'> & {
      readonly scheduler?: TransportScheduler;
    },
  ) {
    this.#o = { ...options, scheduler: options.scheduler ?? DEFAULT_TRANSPORT_SCHEDULER };
  }

  /** MC's latest assignments (REPLACE, never merge). */
  assignments(event: StreamAssignmentsEvent): void {
    if (this.#ended) return;
    const present = new Set<number>();
    for (const a of event.assignments) {
      present.add(a.slotId);
      const rec = this.#record(a.slotId);
      rec.assignment = a;
      const ep = rec.episode;
      if (ep) {
        if (a.slotState !== 'active') this.#endEpisode(rec, 'reassigned_away');
        else if (
          a.senderId !== undefined &&
          ep.senderId !== undefined &&
          a.senderId !== ep.senderId
        ) {
          this.#endEpisode(rec, 'reassigned_away');
        }
      }
      const connected = a.handlerRef !== undefined && this.#o.isConnected(a.handlerRef);
      if (rec.pending && (connected || (a.handlerRef === undefined && a.slotState !== 'active'))) {
        // A DECISIVE follow-up resolves the hold with NO overlay: a CONNECTED
        // handler, or no source. A named but not-yet-connected handler keeps
        // holding (its `connected` may still land in time); the membership
        // re-check (ACTIVE, stale ref resolved to none) is not decisive.
        this.#clearPending(rec);
      }
      if (rec.overlay && connected) this.#clearOverlay(a.slotId, rec);
    }
    for (const [slotId, rec] of this.#slots) {
      if (present.has(slotId)) continue;
      rec.assignment = undefined;
      if (rec.episode) this.#endEpisode(rec, 'reassigned_away');
      this.#clearPending(rec);
      if (rec.overlay) this.#clearOverlay(slotId, rec);
    }
  }

  /** A video slot's render state (its verified live sender, for the avatar). */
  videoRender(slotId: number, state: VideoRenderState): void {
    if (this.#ended) return;
    const rec = this.#record(slotId);
    if (state.kind === 'active') rec.liveSender = state.senderId;
    else if (!rec.overlay && !rec.pending) rec.liveSender = undefined;
  }

  /** This client observed the LOSS of `ref` (`MediaTransport` `handlerLost`): reconnecting at once. */
  handlerLost(ref: HandlerRef): void {
    this.#forSlotsNaming(ref, (slotId, rec, occupant) => {
      const pending = rec.pending;
      this.#clearPending(rec);
      this.#setOverlay(slotId, rec, pending?.previousSenderId ?? occupant);
    });
    // A loss DURING a planned handover breaks its premise: every held slot
    // reconnects now rather than waiting out the hold.
    for (const [slotId, rec] of this.#slots) {
      const pending = rec.pending;
      if (!pending) continue;
      this.#clearPending(rec);
      this.#setOverlay(slotId, rec, pending.previousSenderId);
    }
  }

  /**
   * `ref` LEFT THE SET while slots still name it: their episode starts now. The
   * overlay is HELD for MC's follow-up assignment only in a planned transfer's
   * shape — the ref was still connected and another member is — else it is
   * set at once (see the module header).
   */
  handlerRemoved(
    ref: HandlerRef,
    shape: { readonly wasConnected: boolean; readonly otherConnected: boolean } = {
      wasConnected: true,
      otherConnected: true,
    },
  ): void {
    const hold = shape.wasConnected && shape.otherConnected;
    this.#forSlotsNaming(ref, (slotId, rec, occupant) => {
      if (rec.overlay || rec.pending) return;
      if (!hold) {
        this.#setOverlay(slotId, rec, occupant);
        return;
      }
      const pending: PendingDrop = { previousSenderId: occupant, timer: undefined };
      pending.timer = this.#o.scheduler.setTimeout(() => {
        if (rec.pending !== pending) return;
        rec.pending = undefined;
        this.#setOverlay(slotId, rec, pending.previousSenderId);
      }, this.#o.removalGraceMs);
      rec.pending = pending;
    });
  }

  /** `ref` (re)connected: slots naming it have a source on a live handler again. */
  handlerConnected(ref: HandlerRef): void {
    if (this.#ended) return;
    for (const [slotId, rec] of this.#slots) {
      if (rec.assignment?.handlerRef !== ref) continue;
      this.#clearPending(rec);
      if (rec.overlay) this.#clearOverlay(slotId, rec);
    }
  }

  /**
   * A slot presented a frame from verified `senderId` via `ref` (audio played /
   * video decoded). Only a frame via a CONNECTED transport ends an episode: one
   * still draining from a lost or removed transport does not.
   */
  presented(slotId: number, ref: HandlerRef, senderId: number): void {
    const rec = this.#slots.get(slotId);
    const ep = rec?.episode;
    if (!rec || !ep || this.#ended || !this.#o.isConnected(ref)) return;
    this.#endEpisode(
      rec,
      ep.senderId !== undefined && senderId !== ep.senderId ? 'reassigned_away' : 'resumed',
    );
  }

  /** The session ended: open episodes are `abandoned`; the tracker goes inert. Emits no event. */
  end(): void {
    if (this.#ended) return;
    for (const rec of this.#slots.values()) {
      if (rec.episode) this.#endEpisode(rec, 'abandoned');
      this.#clearPending(rec);
      if (rec.overlay) this.#o.scheduler.clearTimeout(rec.overlay.timer);
      rec.overlay = undefined;
    }
    this.#ended = true;
  }

  /** Whether slot `slotId` is reconnecting now. */
  isReconnecting(slotId: number): boolean {
    return this.#slots.get(slotId)?.overlay !== undefined;
  }

  #record(slotId: number): SlotRecord {
    let rec = this.#slots.get(slotId);
    if (!rec) {
      rec = {
        assignment: undefined,
        liveSender: undefined,
        overlay: undefined,
        pending: undefined,
        episode: undefined,
      };
      this.#slots.set(slotId, rec);
    }
    return rec;
  }

  /**
   * For each slot whose latest assignment names `ref`: open its resume episode
   * (if ACTIVE and none is open), then run `then` with the slot's occupant.
   */
  #forSlotsNaming(
    ref: HandlerRef,
    then: (slotId: number, rec: SlotRecord, occupant: number | undefined) => void,
  ): void {
    if (this.#ended) return;
    for (const [slotId, rec] of this.#slots) {
      const a = rec.assignment;
      if (a?.handlerRef !== ref) continue;
      const occupant = rec.liveSender ?? a.senderId;
      if (!rec.episode && a.slotState === 'active' && a.mediaKind !== 'unknown') {
        const episode: Episode = {
          kind: a.mediaKind,
          startedAt: this.#o.clock(),
          senderId: occupant,
          timer: undefined,
        };
        episode.timer = this.#o.scheduler.setTimeout(() => {
          if (rec.episode !== episode) return;
          this.#endEpisode(rec, 'abandoned');
        }, this.#o.resumeAbandonMs);
        rec.episode = episode;
      }
      then(slotId, rec, occupant);
    }
  }

  #setOverlay(slotId: number, rec: SlotRecord, previousSenderId: number | undefined): void {
    if (rec.overlay) return;
    const overlay: Overlay = { previousSenderId, timer: undefined };
    overlay.timer = this.#o.scheduler.setTimeout(() => {
      if (rec.overlay !== overlay) return;
      this.#clearOverlay(slotId, rec);
    }, this.#o.reconnectingCapMs);
    rec.overlay = overlay;
    this.#o.onReconnectingChanged({ slotId, reconnecting: true, previousSenderId });
  }

  #clearPending(rec: SlotRecord): void {
    const pending = rec.pending;
    if (!pending) return;
    rec.pending = undefined;
    this.#o.scheduler.clearTimeout(pending.timer);
  }

  #clearOverlay(slotId: number, rec: SlotRecord): void {
    const overlay = rec.overlay;
    if (!overlay) return;
    rec.overlay = undefined;
    this.#o.scheduler.clearTimeout(overlay.timer);
    this.#o.onReconnectingChanged({ slotId, reconnecting: false, previousSenderId: undefined });
  }

  #endEpisode(rec: SlotRecord, outcome: MediaResumeOutcome): void {
    const ep = rec.episode;
    if (!ep) return;
    rec.episode = undefined;
    this.#o.scheduler.clearTimeout(ep.timer);
    this.#o.metricsFor(ep.kind)?.resumeMs(outcome, Math.max(0, this.#o.clock() - ep.startedAt));
  }
}

/**
 * Build the session's ONE tracker and feed it the transport's per-ref
 * lifecycle (`handlerLost`, set-driven `handlerRemoved`, `connected`) and MC's
 * assignments. The session ends it (`end()`) BEFORE the transport's teardown
 * removals, which then reach an inert tracker; presentation evidence and video
 * render states are fed by `startMedia`'s outlets.
 */
export function bindSlotContinuity(
  media: MediaTransport,
  signaling: SignalingClient,
  deps: {
    readonly handlers: HandlerSetConfig;
    readonly clock: () => number;
    readonly scheduler: TransportScheduler | undefined;
    readonly metricsFor: (kind: MediaKindToken) => ResumeMetrics | undefined;
    readonly onReconnectingChanged: (event: SlotReconnectingChangedEvent) => void;
  },
): SlotContinuity {
  const continuity = new SlotContinuity({
    reconnectingCapMs: deps.handlers.reconnectingCapMs,
    resumeAbandonMs: deps.handlers.resumeAbandonMs,
    removalGraceMs: deps.handlers.removalGraceMs,
    clock: deps.clock,
    ...(deps.scheduler ? { scheduler: deps.scheduler } : {}),
    isConnected: (ref) => media.getState(ref) === 'connected',
    metricsFor: deps.metricsFor,
    onReconnectingChanged: deps.onReconnectingChanged,
  });
  media.on('handlerLost', (ref) => continuity.handlerLost(ref));
  // Set-driven removals only (never session teardown), with their shape.
  media.on('handlerDropped', (d) => continuity.handlerRemoved(d.ref, d));
  media.on('connected', ({ ref }) => continuity.handlerConnected(ref));
  signaling.on('streamAssignments', (e) => continuity.assignments(e));
  return continuity;
}
