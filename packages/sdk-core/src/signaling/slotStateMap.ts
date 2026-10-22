// File: packages/sdk-core/src/signaling/slotStateMap.ts
//
// THE ONE mapping from the wire `SlotState` / `MediaKind` of a
// `StreamAssignment` onto the public tokens (story 3 R-4, wire freeze). Same
// shape and reasoning as `codecMap.ts`:
//
//   * KEYED ON THE WIRE ENUM. `Record<SlotState, …>` must list every member, so
//     a value added to the proto is a COMPILE ERROR here rather than a slot that
//     silently renders as something it is not.
//   * OPEN-ENUM FAIL-CLOSED. protobuf-es decodes a number the schema does not
//     name as that number; it is not in the table and maps to `unspecified`.
//     NOTHING but the wire's `ACTIVE` maps to `active` — the security property
//     (an unknown or legacy state never renders media).
//
// `SOURCE_MUTED` is the one wire value with no direct token: the flat
// vocabulary removed it (story 3 wire freeze). Until MC-A stops emitting it,
// `legacySourceMutedShim.ts` resolves it to an avatar token. Here it maps to
// `null`, which callers must hand to that shim — never default.

import { MediaKind, SlotState } from '../proto/dark_tower/signaling/v1/signaling_pb.js';

import type { SlotStateToken } from './events.js';
import type { MediaKindToken, OpenMediaKindToken } from '../media/mediaKind.js';

/**
 * Exhaustive wire -> token table. `null` = "no direct token": the legacy
 * `SOURCE_MUTED`, resolved by the transitional shim.
 */
// TRANSITIONAL (C-A1 shim b) — deleted by C-A2: the `| null` arm of this type.
const SLOT_STATE_TOKENS: Record<SlotState, SlotStateToken | null> = {
  [SlotState.UNSPECIFIED]: 'unspecified',
  [SlotState.ACTIVE]: 'active',
  // TRANSITIONAL (C-A1 shim b) — deleted by C-A2: legacy SOURCE_MUTED has no token.
  [SlotState.SOURCE_MUTED]: null,
  [SlotState.WITHHELD_BY_CONGESTION]: 'withheld_by_congestion',
  [SlotState.FEWER_SOURCES_THAN_SLOTS]: 'fewer_sources_than_slots',
  [SlotState.SOURCE_UNREACHABLE]: 'source_unreachable',
  [SlotState.SWITCH_PENDING]: 'switch_pending',
  [SlotState.AVATAR_NO_VIDEO]: 'avatar_no_video',
  [SlotState.UNAVAILABLE]: 'unavailable',
  [SlotState.AVATAR_CLIENT_MUTED]: 'avatar_client_muted',
  [SlotState.AVATAR_SERVER_MUTED]: 'avatar_server_muted',
};

/**
 * Map a wire slot state onto its token. Returns `null` ONLY for the legacy
 * `SOURCE_MUTED` (hand it to the transitional shim); any number the table does
 * not name is `unspecified`, never `active`.
 */
export function slotStateToken(state: SlotState): SlotStateToken | null {
  return Object.prototype.hasOwnProperty.call(SLOT_STATE_TOKENS, state)
    ? SLOT_STATE_TOKENS[state]
    : 'unspecified';
}

/**
 * THE wire -> public media-kind map, for slot assignments AND send-directive
 * streams alike. Screen share, unspecified and unknown numbers are `unknown`,
 * which renders no media and is never sent.
 */
export function mediaKindToken(kind: MediaKind): OpenMediaKindToken {
  switch (kind) {
    case MediaKind.AUDIO:
      return 'audio';
    case MediaKind.VIDEO_CAMERA:
      return 'video';
    default:
      return 'unknown';
  }
}

/**
 * The send direction for the two kinds this client declares and publishes:
 * the closed wire values only (`MEDIA_KIND_AUDIO`, `MEDIA_KIND_VIDEO_CAMERA`).
 */
export function wireMediaKind(kind: MediaKindToken): MediaKind {
  return kind === 'audio' ? MediaKind.AUDIO : MediaKind.VIDEO_CAMERA;
}
