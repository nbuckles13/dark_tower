// File: packages/sdk-core/src/signaling/mediaKindStates.ts
//
// THE ONE decoder for MC's kind-keyed SNAPSHOT LISTS (docs/protocol/CONVENTIONS.md
// §6; ADR-0036 §5 amendment 2026-10-25): `Participant.media_states` (roster
// entries on `JoinResponse.existing_participants` and `ParticipantJoined`),
// `ParticipantMuteUpdate.media_states`, and `JoinResponse.receive_slot_caps`.
// Every per-kind fact MC sends this client arrives through here.
//
// ---------------------------------------------------------------------------
// ONE COMPLETENESS RULE (CONVENTIONS §6), ONE CHECKER
// ---------------------------------------------------------------------------
//
// A server snapshot list carries EXACTLY ONE entry per non-UNSPECIFIED
// `MediaKind` this bundle knows — `KNOWN_WIRE_MEDIA_KINDS`, DERIVED from the
// generated enum, never hand-listed, so a kind added to the proto and
// regenerated joins the rule with no edit here. `VIDEO_SCREEN` is one of them:
// MC sends it (NOT_PUBLISHED, cap 0) and it is validated like any kind, but it
// is not projected — this SDK has no screen kind (`MediaKindToken`) — and its
// VALUE is not checked (MC owns that invariant).
//
// MALFORMED, and session-fatal: an empty list, an UNSPECIFIED or unknown kind
// number, a duplicate, a known kind absent, or an UNSPECIFIED / unknown
// `client_state`. Client and MC are co-deployed (no compatibility window), so
// none of these has a benign reading: each is an MC bug. The caller ends the
// session (`SignalingErrorCode.ContractFault`) and applies NOTHING from the
// message. There is no "unknown" state to fall back to — that is the point of
// the rule — and dropping the message would hide a participant or a mute.
//
// Kind -> public token goes through `slotStateMap.ts:mediaKindToken`, the one
// wire -> token map; nothing here switches on the wire kind a second time.

import {
  ClientMediaState,
  MediaKind,
  type MediaKindState,
  type ReceiveSlotCap,
} from '../proto/dark_tower/signaling/v1/signaling_pb.js';

import { perMediaKind, type MediaKindToken } from '../media/mediaKind.js';
import {
  MAX_RELAYED_PARTICIPANT_ID_LENGTH,
  type ClientMediaStateToken,
  type KindMediaState,
  type ParticipantMediaStates,
  type ReceiveSlotCaps,
} from './events.js';
import { mediaKindToken } from './slotStateMap.js';

/**
 * The bounded reason a contract fault is LOGGED with (a closed log-field
 * vocabulary: no identifiers, no wire content) — which snapshot list was
 * malformed.
 */
export const CONTRACT_FAULT_REASONS = {
  MediaStatesMalformed: 'media_states_malformed',
  ReceiveSlotCapsMalformed: 'receive_slot_caps_malformed',
  UnmuteRequestMalformed: 'unmute_request_malformed',
} as const;

/** One of {@link CONTRACT_FAULT_REASONS}. */
export type ContractFaultReason =
  (typeof CONTRACT_FAULT_REASONS)[keyof typeof CONTRACT_FAULT_REASONS];

/**
 * Every non-UNSPECIFIED wire `MediaKind` this bundle knows, DERIVED from the
 * generated (numeric TS) enum: its reverse mapping makes `Object.values` yield
 * names and numbers, and only the numbers are kinds.
 */
export const KNOWN_WIRE_MEDIA_KINDS: readonly MediaKind[] = Object.freeze(
  Object.values(MediaKind).filter(
    (v): v is MediaKind => typeof v === 'number' && v !== MediaKind.UNSPECIFIED,
  ),
);

/**
 * Wire client state -> token. Exhaustive over the wire enum (a new member that
 * is not mapped fails to compile). `UNSPECIFIED` maps to `undefined`: it is
 * MALFORMED, never a display state; so is any number the table does not name.
 */
const CLIENT_MEDIA_STATE_TOKENS: Record<ClientMediaState, ClientMediaStateToken | undefined> = {
  [ClientMediaState.UNSPECIFIED]: undefined,
  [ClientMediaState.NOT_PUBLISHED]: 'not_published',
  [ClientMediaState.LIVE]: 'live',
  [ClientMediaState.CLIENT_MUTED]: 'client_muted',
};

/** A relayed participant id, bounded; an empty string reads as absent. */
export function boundRelayedId(raw: string): string | undefined {
  return raw === '' ? undefined : raw.slice(0, MAX_RELAYED_PARTICIPANT_ID_LENGTH);
}

/**
 * THE completeness check (module header): the projected value, or `undefined`
 * when the list is MALFORMED. `valid` judges one entry of a KNOWN
 * kind; `project` builds the public value from the entries.
 */
function decodeKindList<E extends { readonly mediaKind: MediaKind }, T>(
  entries: readonly E[],
  valid: (entry: E) => boolean,
  project: (byKind: ReadonlyMap<MediaKind, E>) => T,
): T | undefined {
  const byKind = new Map<MediaKind, E>();
  for (const entry of entries) {
    const kind = entry.mediaKind;
    if (!KNOWN_WIRE_MEDIA_KINDS.includes(kind) || byKind.has(kind) || !valid(entry)) {
      return undefined;
    }
    byKind.set(kind, entry);
  }
  return byKind.size === KNOWN_WIRE_MEDIA_KINDS.length ? project(byKind) : undefined;
}

/** The entry of `token`'s kind, from a COMPLETE list (guaranteed by the checker). */
function entryFor<E>(byKind: ReadonlyMap<MediaKind, E>, token: MediaKindToken): E {
  for (const [kind, entry] of byKind) {
    if (mediaKindToken(kind) === token) return entry;
  }
  // Unreachable: every token's kind is in KNOWN_WIRE_MEDIA_KINDS, and the list is complete.
  throw new Error(`media-kind snapshot has no ${token} entry after the completeness check`);
}

function kindMediaState(entry: MediaKindState): KindMediaState {
  // `valid` already refused an unnamed state; `!` is that guarantee.
  const clientState = CLIENT_MEDIA_STATE_TOKENS[entry.clientState]!;
  const serverMutedBy = entry.serverMuted ? boundRelayedId(entry.serverMutedBy) : undefined;
  return {
    clientState,
    serverMuted: entry.serverMuted,
    ...(serverMutedBy !== undefined ? { serverMutedBy } : {}),
  };
}

/** Decode a `media_states` list (roster entry or `ParticipantMuteUpdate`). */
export function decodeMediaStates(
  entries: readonly MediaKindState[],
): ParticipantMediaStates | undefined {
  return decodeKindList(
    entries,
    (e) => CLIENT_MEDIA_STATE_TOKENS[e.clientState] !== undefined,
    (byKind) => perMediaKind((kind) => kindMediaState(entryFor(byKind, kind))),
  );
}

/**
 * Decode the kinds of MC's `UnmuteRequest` relay to a host: a NON-EMPTY SET of
 * kinds this SDK serves (a map by kind, CONVENTIONS §6 — MC re-encodes it from
 * the requester's parsed, server-muted kinds). Empty, a duplicate, or any kind
 * outside {@link MediaKindToken} (screen, UNSPECIFIED, an unknown number) is
 * MALFORMED: `undefined`. Order follows the wire.
 */
export function decodeRelayedKinds(kinds: readonly MediaKind[]): MediaKindToken[] | undefined {
  const tokens: MediaKindToken[] = [];
  for (const kind of kinds) {
    const token = mediaKindToken(kind);
    if (token === 'unknown' || tokens.includes(token)) return undefined;
    tokens.push(token);
  }
  return tokens.length > 0 ? tokens : undefined;
}

/** Decode `JoinResponse.receive_slot_caps`. A present 0 stays 0. */
export function decodeReceiveSlotCaps(
  entries: readonly ReceiveSlotCap[],
): ReceiveSlotCaps | undefined {
  return decodeKindList(
    entries,
    () => true,
    (byKind) => perMediaKind((kind) => entryFor(byKind, kind).maxSlots),
  );
}
