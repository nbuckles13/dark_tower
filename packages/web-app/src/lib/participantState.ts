// File: packages/web-app/src/lib/participantState.ts
//
// The single place this demo turns per-participant WIRE facts (story 2 R-10,
// R-11, R-33; ADR-0036 §5, §6, §9) into what a person reads — the sibling of
// `slotState.ts`, which owns the slot-state copy this module reuses.
//
// Every value here is derived from state MC put on the wire and the stores hold
// verbatim, never from frames arriving or not:
//
//   * reachability  <- `StreamAssignments.unreachable_sender_ids` (per subscriber);
//   * server mute   <- `ParticipantMuteUpdate` (who, and who muted them);
//   * client mute   <- the slot carrying that participant reading `source_muted`
//                      while MC reports no server mute.
//
// CLIENT MUTE IS TRI-STATE, deliberately. Self-mute does not fan out on the
// roster (MC's rule; `ParticipantMuteUpdate`'s self-mute booleans are stale
// snapshots), so the ONLY wire signal is a slot's `source_muted`. A participant
// no slot of ours carries (over-subscription, unreachable) has no wire fact either
// way, and `false` there would assert "not muted" from absence — the collapse
// `slotState.ts` exists to prevent. That case is `unknown`. So is a server-muted
// participant: their slot reads `source_muted` because of the server mute, which
// says nothing about their own microphone.
//
// ORDERING WINDOW: MC may push the slot's `source_muted` a moment before the
// `ParticipantMuteUpdate` for the same server mute lands, so a server mute can
// briefly read as a client mute. That is transient and self-correcting; the
// browser suite polls to the settled state rather than reading once.
//
// Identities are resolved against the roster and rendered as TEXT only. An id the
// roster does not know renders as a bounded placeholder, never as the raw wire
// string (@security M4).

import type { RosterParticipant, StreamAssignmentEvent } from '@darktower/sdk-core';
import { SLOT_STATE_TEXT, type SlotStateToken } from './slotState.js';

/** A roster row's reachability token: the wire's `source_unreachable`, or reachable. */
export type ReachabilityToken = Extract<SlotStateToken, 'source_unreachable'> | 'reachable';

/** Client mute of another participant, as far as the wire can say. */
export type ClientMuteToken = 'true' | 'false' | 'unknown';

/** Every user-facing string about mute and identity. Tests assert tokens, never these. */
export const PARTICIPANT_TEXT = {
  unknownParticipant: 'unknown participant',
  you: 'you',
  unreachable: SLOT_STATE_TEXT.source_unreachable,
  mutedThemselves: 'muted their microphone',
  muteForEveryone: 'Mute for everyone',
  unmuteForEveryone: 'Unmute for everyone',
  asksToBeUnmuted: 'asks to be unmuted',
  ownNotServerMuted: 'not muted by the host',
  requestUnmute: 'Ask the host to unmute you',
  unmuteRequestSent: 'Asked the host to unmute you',
} as const;

/** "muted for everyone by <name>" — the one spelling of the server-mute indicator. */
export function serverMutedByText(name: string): string {
  return `muted for everyone by ${name}`;
}

/** The roster and this participant's own id, for resolving names. */
export interface NameContext {
  readonly roster: readonly RosterParticipant[];
  readonly selfParticipantId: string | undefined;
}

/** Name for a participant id: "you", a roster name, or the bounded placeholder. */
export function nameForParticipantId(id: string | undefined, ctx: NameContext): string {
  if (id === undefined) return PARTICIPANT_TEXT.unknownParticipant;
  if (id === ctx.selfParticipantId) return PARTICIPANT_TEXT.you;
  return (
    ctx.roster.find((p) => p.participantId === id)?.name ?? PARTICIPANT_TEXT.unknownParticipant
  );
}

/** Name for a sender id (a slot's assigned source). Never the raw number. */
export function nameForSenderId(senderId: number | undefined, ctx: NameContext): string {
  if (senderId === undefined) return PARTICIPANT_TEXT.unknownParticipant;
  return (
    ctx.roster.find((p) => p.senderId === senderId)?.name ?? PARTICIPANT_TEXT.unknownParticipant
  );
}

/** Everything a roster row renders about one other participant. */
export interface ParticipantIndicators {
  readonly reachability: ReachabilityToken;
  readonly serverMuted: boolean;
  /** Resolved name of whoever applied the server mute; only when server-muted. */
  readonly serverMutedByName: string | undefined;
  readonly clientMuted: ClientMuteToken;
}

/** The media facts a roster row reads (the store's cells, passed in). */
export interface MediaFacts {
  readonly slots: readonly StreamAssignmentEvent[];
  readonly unreachableSenderIds: readonly number[];
  readonly serverMutes: ReadonlyMap<string, { readonly serverMutedBy?: string | undefined }>;
}

/** Derive a roster row's indicators. Pure. */
export function participantIndicators(
  participant: RosterParticipant,
  media: MediaFacts,
  ctx: NameContext,
): ParticipantIndicators {
  const senderId = participant.senderId;
  const reachability: ReachabilityToken =
    senderId !== undefined && media.unreachableSenderIds.includes(senderId)
      ? 'source_unreachable'
      : 'reachable';
  const serverMute = media.serverMutes.get(participant.participantId);
  const serverMuted = serverMute !== undefined;
  const slot =
    senderId === undefined ? undefined : media.slots.find((s) => s.senderId === senderId);
  let clientMuted: ClientMuteToken = 'unknown';
  if (slot !== undefined && !serverMuted) {
    if (slot.slotState === 'source_muted') clientMuted = 'true';
    else if (slot.slotState === 'active') clientMuted = 'false';
  }
  return {
    reachability,
    serverMuted,
    serverMutedByName: serverMuted
      ? nameForParticipantId(serverMute.serverMutedBy, ctx)
      : undefined,
    clientMuted,
  };
}
