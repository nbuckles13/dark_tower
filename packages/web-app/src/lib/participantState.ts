// File: packages/web-app/src/lib/participantState.ts
//
// The single place this demo turns per-participant WIRE facts (story 2 R-10,
// R-11, R-33; story 3 R-15, R-17, R-19; ADR-0036 §5, §6, §9) into what a person
// reads — the sibling of `slotState.ts`, which owns the slot-state copy.
//
// Every value here is derived from state MC put on the wire (or the SDK
// reported) and the stores hold verbatim, never from frames arriving or not:
//
//   * reachability  <- `StreamAssignments.unreachable_sender_ids` (per subscriber);
//   * per-kind state <- the participant's ROSTER entry `media_states`, moved by
//                       every later `ParticipantMuteUpdate` (the store's
//                       participant cell, one `KindMediaView` per kind): their
//                       own client state, and the server mute (and who applied
//                       it). NEVER from slots.
//
// FOUR INDICATORS PER PARTICIPANT (story 3 R-15): client audio, client video,
// server audio, server video. THE CLIENT STATE per kind is MC's
// `ClientMediaState`: `not_published` (they joined without that kind),
// `live`, or `client_muted`. There is NO "unknown" (ADR-0036 §5 amendment
// 2026-10-25): every roster entry carries every kind, and a snapshot that does
// not is a contract fault the SDK ends the session on. The client state is
// shown whatever this client's slots carry — a participant no slot of ours
// shows still has a known state — and whatever their server mute: the server
// indicators say what the host did, the client ones what the participant did,
// and a server mute still wins where it decides what is SEEN (the slot reads
// `avatar_server_muted`, `slotState.ts`). History: until 2026-10-24 client mute
// was read from slots and was "unknown" outside them; until 2026-10-25 it was
// "unknown" until the participant's post-join report, and for any kind they did
// not publish.
//
// NAMES ARE BOUNDED AND TEXT ONLY. Every rendered name — the roster's, and the
// server-muted-by name (the only "reason" a muted participant sees: the proto's
// `ServerMuteRequest.reason` is not relayed) — is truncated to the SDK's own
// display-name bound (`MAX_DISPLAY_NAME_LENGTH`) and interpolated as text,
// never markup. An id the roster does not know renders as a bounded placeholder,
// never as the raw wire string (@security M4).

import { MAX_DISPLAY_NAME_LENGTH, MEDIA_KIND_TOKENS, perMediaKind } from '@darktower/sdk-core';
import type {
  ClientMediaStateToken,
  MediaCaptureFailureOutcome,
  PublishKind,
  RosterParticipant,
} from '@darktower/sdk-core';
import type { KindMediaView, MediaStore } from '@darktower/sdk-svelte';

/** A roster row's reachability token: the wire's `source_unreachable`, or reachable. */
export type ReachabilityToken = 'source_unreachable' | 'reachable';

/**
 * Another participant's own state for one kind — the SDK's closed
 * `ClientMediaStateToken`, rendered verbatim (DOM `data-client-state*`).
 */
export type ClientStateToken = ClientMediaStateToken;

/** The two kinds a participant has indicators for: the SDK's media-kind vocabulary. */
export type IndicatorKind = PublishKind;

/**
 * Every media kind, in the order the UI lists them: the SDK's ONE derived list
 * (`MEDIA_KIND_TOKENS`), so a new kind reaches the indicators, host controls
 * and resume notices that iterate this without an edit here.
 */
export const MEDIA_KINDS: readonly PublishKind[] = MEDIA_KIND_TOKENS;

/** Every user-facing string about mute and identity. Tests assert tokens, never these. */
export const PARTICIPANT_TEXT = {
  unknownParticipant: 'unknown participant',
  you: 'you',
  unreachable: 'cannot be reached from your connection',
  muteForEveryone: { audio: 'Mute microphone for everyone', video: 'Turn camera off for everyone' },
  // A server UNMUTE resumes the participant's capture unless they client-muted
  // it (R-18): the host's label says so plainly, never "allow".
  unmuteForEveryone: {
    audio: 'Turn microphone back on for everyone',
    video: 'Turn camera back on for everyone',
  },
  asksToBeUnmuted: 'asks to be unmuted',
  ownNotServerMuted: {
    audio: 'the host has not muted your microphone',
    video: 'the host has not turned your camera off',
  },
  requestUnmute: 'Ask the host to unmute you',
  unmuteRequestSent: 'Asked the host to unmute you',
} as const;

/**
 * The four indicators' text, by kind and cause, per state. One home: a tile
 * and a row render the same participant with the same words. The client table
 * is exhaustive over the SDK's `ClientMediaStateToken` (a new state fails to
 * compile here).
 */
export const INDICATOR_TEXT = {
  client: {
    audio: {
      not_published: 'mic: not publishing',
      live: 'mic: on',
      client_muted: 'mic: muted by them',
    },
    video: {
      not_published: 'camera: not publishing',
      live: 'camera: on',
      client_muted: 'camera: off by them',
    },
  } satisfies Record<IndicatorKind, Readonly<Record<ClientStateToken, string>>>,
  server: {
    audio: { true: 'mic: muted by host', false: 'mic: not host-muted' },
    video: { true: 'camera: off by host', false: 'camera: not host-blocked' },
  } satisfies Record<IndicatorKind, Readonly<Record<'true' | 'false', string>>>,
} as const;

/** "… turned off for everyone by <name>" — the one spelling of the server-mute indicator. */
export function serverMutedByText(name: string, kind: IndicatorKind): string {
  return kind === 'audio'
    ? `microphone muted for everyone by ${name}`
    : `camera turned off for everyone by ${name}`;
}

/** A name bounded to the SDK's display-name limit before it is rendered. */
export function boundedName(name: string): string {
  return name.length > MAX_DISPLAY_NAME_LENGTH
    ? `${name.slice(0, MAX_DISPLAY_NAME_LENGTH)}…`
    : name;
}

/** The roster and this participant's own id, for resolving names. */
export interface NameContext {
  readonly roster: readonly RosterParticipant[];
  readonly selfParticipantId: string | undefined;
}

/** Name for a participant id: "you", a bounded roster name, or the bounded placeholder. */
export function nameForParticipantId(id: string | undefined, ctx: NameContext): string {
  if (id === undefined) return PARTICIPANT_TEXT.unknownParticipant;
  if (id === ctx.selfParticipantId) return PARTICIPANT_TEXT.you;
  const name = ctx.roster.find((p) => p.participantId === id)?.name;
  return name === undefined ? PARTICIPANT_TEXT.unknownParticipant : boundedName(name);
}

/** Name for a sender id (a slot's source). Never the raw number. */
export function nameForSenderId(senderId: number | undefined, ctx: NameContext): string {
  return nameForParticipantId(participantIdForSenderId(senderId, ctx), ctx);
}

/** The roster participant behind a sender id, when the roster knows it (ONE roster scan). */
export function participantForSenderId(
  senderId: number | undefined,
  ctx: NameContext,
): RosterParticipant | undefined {
  if (senderId === undefined) return undefined;
  return ctx.roster.find((p) => p.senderId === senderId);
}

/** The roster participant id behind a sender id, when the roster knows it. */
export function participantIdForSenderId(
  senderId: number | undefined,
  ctx: NameContext,
): string | undefined {
  return participantForSenderId(senderId, ctx)?.participantId;
}

/** The media facts one participant's indicators read (that participant's cells, passed in). */
export interface MediaFacts {
  readonly unreachableSenderIds: readonly number[];
  /** Their per-kind cells (client state, server mute, who applied it). */
  readonly kind: (kind: IndicatorKind) => KindMediaView;
}

/** Everything a row or tile renders about one other participant's mute state. */
export interface ParticipantIndicators {
  readonly reachability: ReachabilityToken;
  readonly clientState: Readonly<Record<IndicatorKind, ClientStateToken>>;
  readonly serverMuted: Readonly<Record<IndicatorKind, boolean>>;
  /** Resolved (bounded) name of whoever applied each kind's server mute; only while muted. */
  readonly serverMutedByName: Readonly<Record<IndicatorKind, string | undefined>>;
}

/**
 * A listed participant's client state. Its cell is seeded from its roster
 * entry BEFORE the roster lists it (`MeetingStore`), so `undefined` here is a
 * wiring bug, never a display state — failed loudly, never rendered as a guess.
 */
function clientStateOf(view: KindMediaView, kind: IndicatorKind): ClientStateToken {
  if (view.clientState === undefined) {
    throw new Error(`a listed participant has no ${kind} state: its media cell was never seeded`);
  }
  return view.clientState;
}

/**
 * Read ONE participant's media facts from the store's CELLS — their per-kind
 * cells and the meeting's unreachable set — so a `$derived` over it tracks only
 * that participant (story 3 granularity). No slot is read: what a slot shows is
 * `slotState.ts`'s business, never a mute fact.
 */
export function mediaFactsFor(media: MediaStore, participant: RosterParticipant): MediaFacts {
  const cell = media.participant(participant.participantId);
  return {
    unreachableSenderIds: media.unreachableSenderIds,
    kind: (kind) => cell.kind(kind),
  };
}

/** Derive one participant's indicators. Pure. */
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
  return {
    reachability,
    clientState: perMediaKind((k) => clientStateOf(media.kind(k), k)),
    serverMuted: perMediaKind((k) => media.kind(k).serverMuted),
    serverMutedByName: perMediaKind((k) => {
      const view = media.kind(k);
      return view.serverMuted ? nameForParticipantId(view.serverMutedBy, ctx) : undefined;
    }),
  };
}

// ---------------------------------------------------------------------------
// This participant's OWN camera (story 3 R-17, R-19)
// ---------------------------------------------------------------------------

/**
 * What this participant's own camera status says. ONE vocabulary: the DOM's
 * `data-camera-state` carries exactly these, and the e2e side reads this type.
 *
 *   * `not_publishing` — the join's fixed publish set has no camera;
 *   * `failed`         — the camera gate latched a failure (R-19): shown with
 *                        the reason; others see this participant video-muted;
 *   * `host_off`       — the host turned it off (server video mute);
 *   * `muted`          — this participant turned it off (client video mute);
 *   * `on`             — on whenever someone is watching (the SDK lights the
 *                        camera only while MC directs it, R-8).
 */
export const SELF_CAMERA_STATES = ['not_publishing', 'failed', 'host_off', 'muted', 'on'] as const;
export type SelfCameraState = (typeof SELF_CAMERA_STATES)[number];

/** Copy per state — exhaustive over {@link SelfCameraState}. */
export const SELF_CAMERA_TEXT: Record<SelfCameraState, string> = {
  not_publishing: 'you joined without a camera',
  failed: 'your camera stopped working; others see your camera as off',
  host_off: 'the host turned your camera off',
  muted: 'you turned your camera off',
  on: 'your camera is on whenever someone is watching',
};

/** The R-19 failure reason, per the SDK's closed capture-failure vocabulary. */
export const CAMERA_FAILURE_TEXT: Record<MediaCaptureFailureOutcome, string> = {
  permission_denied: 'camera permission was denied',
  no_device: 'no camera was found',
  device_unavailable: 'the camera is in use or unavailable',
  unsupported: 'the camera does not support the requested format',
  platform_unsupported: 'this browser cannot use a camera here',
  unknown: 'the camera failed for an unknown reason',
  ended: 'the camera stopped (unplugged or revoked)',
};

/** Derive this participant's own camera state. Precedence: not publishing, failed, host, self. */
export function selfCameraState(facts: {
  readonly publishesVideo: boolean;
  readonly cameraFailure: MediaCaptureFailureOutcome | undefined;
  readonly videoServerMuted: boolean;
  readonly videoMuted: boolean;
}): SelfCameraState {
  if (!facts.publishesVideo) return 'not_publishing';
  if (facts.cameraFailure !== undefined) return 'failed';
  if (facts.videoServerMuted) return 'host_off';
  if (facts.videoMuted) return 'muted';
  return 'on';
}
