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
//   * server mute   <- `ParticipantMuteUpdate`, PER KIND (who, and who muted them);
//   * client mute   <- the participant's own slot of THAT kind reading
//                      `avatar_client_muted` while MC reports no server mute of
//                      that kind (video: also the verified live render, R-5).
//
// FOUR INDICATORS PER PARTICIPANT (story 3 R-15): client audio, client video,
// server audio, server video. CLIENT MUTE IS TRI-STATE per kind, deliberately:
// self-mute does not fan out on the roster, so the ONLY wire signal is a slot's
// `avatar_client_muted`. A participant no slot of that kind of ours carries has
// no wire fact either way, and `false` there would assert "not muted" from
// absence. That case is `unknown`. So is a kind that is server-muted: the slot
// reads `avatar_server_muted` (server wins), which says nothing about their own
// device.
//
// NAMES ARE BOUNDED AND TEXT ONLY. Every rendered name — the roster's, and the
// server-muted-by name (the only "reason" a muted participant sees: the proto's
// `ServerMuteRequest.reason` is not relayed) — is truncated to the SDK's own
// display-name bound (`MAX_DISPLAY_NAME_LENGTH`) and interpolated as text,
// never markup. An id the roster does not know renders as a bounded placeholder,
// never as the raw wire string (@security M4).

import { MAX_DISPLAY_NAME_LENGTH } from '@darktower/sdk-core';
import type {
  MediaCaptureFailureOutcome,
  PublishKind,
  RosterParticipant,
  StreamAssignmentEvent,
  VideoRenderState,
} from '@darktower/sdk-core';
import type { MediaStore } from '@darktower/sdk-svelte';

/** A roster row's reachability token: the wire's `source_unreachable`, or reachable. */
export type ReachabilityToken = 'source_unreachable' | 'reachable';

/** Client mute of another participant, as far as the wire can say. */
export type ClientMuteToken = 'true' | 'false' | 'unknown';

/** The two kinds a participant has indicators for: the SDK's media-kind vocabulary. */
export type IndicatorKind = PublishKind;

/**
 * Every media kind, in the order the UI lists them — DERIVED from an
 * exhaustive `Record` over the SDK's kind union, so a new kind fails to compile
 * here rather than silently missing from the indicators, host controls and
 * resume notices that iterate this.
 */
const KIND_ORDER: Readonly<Record<PublishKind, number>> = { audio: 0, video: 1 };
export const MEDIA_KINDS: readonly PublishKind[] = (Object.keys(KIND_ORDER) as PublishKind[]).sort(
  (a, b) => KIND_ORDER[a] - KIND_ORDER[b],
);

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
 * The four indicators' text, by kind and cause, per tri-state value. One home:
 * a tile and a row render the same participant with the same words.
 */
export const INDICATOR_TEXT = {
  client: {
    audio: { true: 'mic: muted by them', false: 'mic: on', unknown: 'mic: unknown' },
    video: { true: 'camera: off by them', false: 'camera: on', unknown: 'camera: unknown' },
  },
  server: {
    audio: { true: 'mic: muted by host', false: 'mic: not host-muted' },
    video: { true: 'camera: off by host', false: 'camera: not host-blocked' },
  },
} as const satisfies Record<
  'client' | 'server',
  Record<IndicatorKind, Readonly<Record<string, string>>>
>;

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

/** One participant's server mute, as the store's participant cell reports it. */
export interface ServerMuteFacts {
  readonly audioServerMuted: boolean;
  readonly videoServerMuted: boolean;
  readonly serverMutedBy: string | undefined;
}

/** The media facts one participant's indicators read (that participant's cells, passed in). */
export interface MediaFacts {
  /** MC's assignment of this participant's paired AUDIO slot, if any. */
  readonly audioSlot: StreamAssignmentEvent | undefined;
  /** MC's assignment of this participant's paired VIDEO slot, if any. */
  readonly videoSlot: StreamAssignmentEvent | undefined;
  /** What that video slot renders (the SDK's verified live sender, R-5). */
  readonly videoRender: VideoRenderState | undefined;
  readonly unreachableSenderIds: readonly number[];
  readonly serverMute: ServerMuteFacts;
}

/** Everything a row or tile renders about one other participant's mute state. */
export interface ParticipantIndicators {
  readonly reachability: ReachabilityToken;
  readonly clientMuted: Readonly<Record<IndicatorKind, ClientMuteToken>>;
  readonly serverMuted: Readonly<Record<IndicatorKind, boolean>>;
  /** Resolved (bounded) name of whoever applied a server mute; only while server-muted. */
  readonly serverMutedByName: string | undefined;
}

function clientMuteFrom(
  slot: StreamAssignmentEvent | undefined,
  serverMuted: boolean,
  liveForThem: boolean,
): ClientMuteToken {
  if (serverMuted) return 'unknown';
  if (slot?.slotState === 'avatar_client_muted') return 'true';
  if (slot?.slotState === 'active' || liveForThem) return 'false';
  return 'unknown';
}

/**
 * Read ONE participant's media facts from the store's CELLS — their server-mute
 * cell, their pair, and the two slots the pair names — so a `$derived` over it
 * tracks only that participant (story 3 granularity). Pairing is by sender id
 * (R-3), never slot index.
 */
export function mediaFactsFor(media: MediaStore, participant: RosterParticipant): MediaFacts {
  const pair = participant.senderId === undefined ? undefined : media.pairOf(participant.senderId);
  const audioSlotId = pair?.audioSlotId;
  const videoSlotId = pair?.videoSlotId;
  const video = videoSlotId === undefined ? undefined : media.slot(videoSlotId);
  return {
    audioSlot: audioSlotId === undefined ? undefined : media.slot(audioSlotId).assignment,
    videoSlot: video?.assignment,
    videoRender: video?.render,
    unreachableSenderIds: media.unreachableSenderIds,
    serverMute: media.participant(participant.participantId),
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
  const { audioServerMuted, videoServerMuted, serverMutedBy } = media.serverMute;
  const liveVideo =
    senderId !== undefined &&
    media.videoRender?.kind === 'active' &&
    media.videoRender.senderId === senderId;
  return {
    reachability,
    clientMuted: {
      audio: clientMuteFrom(media.audioSlot, audioServerMuted, false),
      video: clientMuteFrom(media.videoSlot, videoServerMuted, liveVideo),
    },
    serverMuted: { audio: audioServerMuted, video: videoServerMuted },
    serverMutedByName:
      audioServerMuted || videoServerMuted ? nameForParticipantId(serverMutedBy, ctx) : undefined,
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
