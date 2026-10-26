// File: packages/sdk-svelte/src/stores/mediaCells.svelte.ts
//
// The FINE-GRAINED reactive cells behind `MediaStore` (story 3 C-C1): one per
// slot, one per participant, one per paired sender.
//
// ---------------------------------------------------------------------------
// WHY CELLS, NOT ONE MAP IN ONE `$state`
// ---------------------------------------------------------------------------
//
// Svelte 5 tracks per `$state` CELL. A map of every participant's server mute in
// ONE cell (the story-2 shape) invalidates every reader whenever ANY entry
// changes, so one participant's mute re-rendered every roster row and tile. Here
// each participant, slot and pair owns its own cells, and a tile or row reads
// only its own — so "a slot's state change re-renders only that tile" and "a
// participant's mute change re-renders only their tile and row" hold by
// construction (proved by `__tests__/MediaStore.test.ts`, counted).
//
// WRITES ARE CHANGE-ONLY: each setter compares before assigning, because an
// unconditional write of an equal value still invalidates readers of a fresh
// object (a new assignment object per `StreamAssignments`).
//
// READERS NEVER CONSTRUCT (`CellRegistry`): a cell is created only by a
// writer, outside any reaction. A reader of a not-yet-created key gets an empty
// view and re-runs when the cell appears (Svelte does not track a source created
// inside the reaction reading it, so lazy creation on read would freeze it).
//
// LIFETIME = the owning `MediaStore` = one `MeetingRoom` join attempt. Cells are
// never dropped mid-session: their number is bounded by the participant ids MC
// relays (the roster) plus the N+M declared slot ids, and the store itself is
// discarded with the attempt.
//
// No metric, log or trace is emitted here; nothing frame-rate touches a cell.

import type {
  ClientMediaStateToken,
  KindMediaState,
  PublishKind,
  ParticipantMediaStates,
  StreamAssignmentEvent,
  VideoRenderState,
} from '@darktower/sdk-core';
import { MEDIA_KIND_TOKENS, perMediaKind } from '@darktower/sdk-core';

/** What a component reads about ONE slot. */
export interface SlotView {
  /** MC's latest assignment for the slot; `undefined` until MC names it. */
  readonly assignment: StreamAssignmentEvent | undefined;
  /**
   * What a VIDEO slot renders (the SDK's `videoSlotRenderChanged`); `undefined`
   * before the SDK reported any. Audio slots never carry one.
   */
  readonly render: VideoRenderState | undefined;
}

/**
 * What a component reads about ONE kind of ONE participant (story 3 R-15; wire
 * `MediaKindState`). Client state and server mute are independent.
 */
export interface KindMediaView {
  /**
   * The participant's own state for this kind, as MC reports it. `undefined`
   * ONLY for a participant MC has sent no state for — never a roster
   * participant (its cell is seeded from its roster entry before the roster
   * lists it, `MeetingStore`), only this client's OWN cell before a self
   * update. It is NOT a display state: there is no "unknown" to render, and a
   * reader that meets it has a wiring bug (`participantState.ts` fails loudly).
   */
  readonly clientState: ClientMediaStateToken | undefined;
  /** Whether this kind is server-muted. */
  readonly serverMuted: boolean;
  /** Participant id of whoever server-muted this kind; only while `serverMuted`. */
  readonly serverMutedBy: string | undefined;
}

/**
 * What a component reads about ONE participant's mute state: per kind, the ONE
 * home of all of it (seeded by the roster entry, moved by every
 * `ParticipantMuteUpdate` — each a FULL snapshot).
 */
export interface ParticipantMediaView {
  /** One kind's cells; a reader of one kind tracks only that kind. */
  kind(kind: PublishKind): KindMediaView;
  /** Whether MC reports this participant server-muted in ANY kind. */
  readonly serverMutedAny: boolean;
  /** This participant asked the host to lift their server mute (host clients only). */
  readonly unmuteRequested: boolean;
}

/** The slots showing ONE sender, paired by publisher (story 3 R-3), never by slot index. */
export interface PairView {
  readonly audioSlotId: number | undefined;
  readonly videoSlotId: number | undefined;
}

function sameAssignment(
  a: StreamAssignmentEvent | undefined,
  b: StreamAssignmentEvent | undefined,
): boolean {
  if (a === b) return true;
  if (a === undefined || b === undefined) return false;
  return (
    a.slotId === b.slotId &&
    a.mediaKind === b.mediaKind &&
    a.senderId === b.senderId &&
    a.mediaHandlerUrl === b.mediaHandlerUrl &&
    a.slotState === b.slotState
  );
}

function sameRender(a: VideoRenderState | undefined, b: VideoRenderState | undefined): boolean {
  if (a === b) return true;
  if (a === undefined || b === undefined) return false;
  if (a.kind !== b.kind) return false;
  if (a.kind === 'active' && b.kind === 'active') return a.senderId === b.senderId;
  if (a.kind === 'avatar' && b.kind === 'avatar') {
    return a.reason === b.reason && a.participantSenderId === b.participantSenderId;
  }
  return true;
}

/** One slot's cells. */
export class SlotCell implements SlotView {
  #assignment = $state<StreamAssignmentEvent | undefined>(undefined);
  #render = $state<VideoRenderState | undefined>(undefined);

  get assignment(): StreamAssignmentEvent | undefined {
    return this.#assignment;
  }

  get render(): VideoRenderState | undefined {
    return this.#render;
  }

  /** Change-only write. */
  setAssignment(next: StreamAssignmentEvent | undefined): void {
    if (!sameAssignment(this.#assignment, next)) this.#assignment = next;
  }

  /** Change-only write. */
  setRender(next: VideoRenderState): void {
    if (!sameRender(this.#render, next)) this.#render = next;
  }
}

/** One kind's cells of one participant. */
export class KindMediaCell implements KindMediaView {
  #clientState = $state<ClientMediaStateToken | undefined>(undefined);
  #serverMuted = $state(false);
  #serverMutedBy = $state<string | undefined>(undefined);

  get clientState(): ClientMediaStateToken | undefined {
    return this.#clientState;
  }

  get serverMuted(): boolean {
    return this.#serverMuted;
  }

  get serverMutedBy(): string | undefined {
    return this.#serverMutedBy;
  }

  /** Change-only write of one kind's snapshot. Client and server never touch each other. */
  set(state: KindMediaState): void {
    if (this.#clientState !== state.clientState) this.#clientState = state.clientState;
    if (this.#serverMuted !== state.serverMuted) this.#serverMuted = state.serverMuted;
    const nextBy = state.serverMuted ? state.serverMutedBy : undefined;
    if (this.#serverMutedBy !== nextBy) this.#serverMutedBy = nextBy;
  }
}

/** One participant's mute cells, per kind. */
export class ParticipantMediaCell implements ParticipantMediaView {
  /** One cell per kind, over the SDK's one kind list. */
  readonly #kinds: Readonly<Record<PublishKind, KindMediaCell>> = perMediaKind(
    () => new KindMediaCell(),
  );
  #unmuteRequested = $state(false);

  kind(kind: PublishKind): KindMediaCell {
    return this.#kinds[kind];
  }

  get unmuteRequested(): boolean {
    return this.#unmuteRequested;
  }

  get serverMutedAny(): boolean {
    return Object.values(this.#kinds).some((k) => k.serverMuted);
  }

  /**
   * THE one write, for every carrier (roster seed and `ParticipantMuteUpdate`):
   * every kind, from a FULL snapshot (the SDK refuses anything else, so there
   * is no absent kind to skip). Change-only per cell.
   */
  apply(states: ParticipantMediaStates): void {
    for (const kind of MEDIA_KIND_TOKENS) this.#kinds[kind].set(states[kind]);
  }

  setUnmuteRequested(requested: boolean): void {
    if (this.#unmuteRequested !== requested) this.#unmuteRequested = requested;
  }
}

/** One sender's pairing cells. */
export class PairCell implements PairView {
  #audioSlotId = $state<number | undefined>(undefined);
  #videoSlotId = $state<number | undefined>(undefined);

  get audioSlotId(): number | undefined {
    return this.#audioSlotId;
  }

  get videoSlotId(): number | undefined {
    return this.#videoSlotId;
  }

  /** Change-only writes. */
  set(audio: number | undefined, video: number | undefined): void {
    if (this.#audioSlotId !== audio) this.#audioSlotId = audio;
    if (this.#videoSlotId !== video) this.#videoSlotId = video;
  }
}

/**
 * A keyed registry of cells.
 *
 * READ and WRITE are deliberately different paths, because Svelte does not
 * register a dependency on a `$state` source CREATED inside the reaction that
 * reads it. A cell constructed lazily inside a template or `$derived` would
 * therefore never re-run that reader when written later — a silently frozen
 * tile. So:
 *
 *   * {@link read} NEVER constructs. For a key with no cell yet it tracks ONE
 *     registry-level `$state` counter and returns the shared `empty` view; the
 *     reader re-runs when a cell is created, and from then on tracks only that
 *     cell. (Creation happens once per participant / slot / sender, so this
 *     coarse edge is bounded by the same N+M + roster bound.)
 *   * {@link ensure} constructs — from the store's `apply*` mutators, which run
 *     from SDK events, never inside a reaction — and bumps the counter.
 */
export class CellRegistry<K, C extends V, V> {
  // Deliberately NOT reactive: a lookup must track nothing but the cell (and,
  // for a missing key, `#created`). A SvelteMap would make every reader of any
  // key re-run on every insertion.
  /* eslint-disable svelte/prefer-svelte-reactivity */
  readonly #cells = new Map<K, C>();
  /* eslint-enable svelte/prefer-svelte-reactivity */
  readonly #make: () => C;
  readonly #empty: V;
  /** Bumped on every construction; read only by readers of a not-yet-created key. */
  #created = $state(0);

  constructor(make: () => C, empty: V) {
    this.#make = make;
    this.#empty = empty;
  }

  /** The cell for `key` for a READER: never constructs (see the class doc). */
  read(key: K): V {
    const cell = this.#cells.get(key);
    if (cell !== undefined) return cell;
    void this.#created;
    // Re-check after tracking the counter (the cell may exist by now).
    return this.#cells.get(key) ?? this.#empty;
  }

  /** The cell for `key` for a WRITER: constructed on first use. Never call inside a reaction. */
  ensure(key: K): C {
    let cell = this.#cells.get(key);
    if (cell === undefined) {
      cell = this.#make();
      this.#cells.set(key, cell);
      this.#created += 1;
    }
    return cell;
  }

  /** Every cell created so far, with its key. */
  entries(): IterableIterator<[K, C]> {
    return this.#cells.entries();
  }
}

/** The view of a slot MC has never named and the SDK has never rendered. */
export const EMPTY_SLOT: SlotView = Object.freeze({ assignment: undefined, render: undefined });

/** One kind of a participant MC has sent no state for. */
const EMPTY_KIND: KindMediaView = Object.freeze({
  clientState: undefined,
  serverMuted: false,
  serverMutedBy: undefined,
});

/** The view of a participant MC has sent no state for (see {@link KindMediaView.clientState}). */
export const EMPTY_PARTICIPANT: ParticipantMediaView = Object.freeze({
  kind: () => EMPTY_KIND,
  serverMutedAny: false,
  unmuteRequested: false,
});

/** The view of a sender no slot shows. */
export const EMPTY_PAIR: PairView = Object.freeze({
  audioSlotId: undefined,
  videoSlotId: undefined,
});
