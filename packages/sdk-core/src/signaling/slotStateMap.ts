// File: packages/sdk-core/src/signaling/slotStateMap.ts
//
// THE ONE mapping from the wire `SlotState` / `MediaKind` of a
// `StreamAssignment` onto the public tokens (story 3 R-4, wire freeze). Same
// shape and reasoning as `codecMap.ts`:
//
//   * KEYED ON THE WIRE ENUM, EXHAUSTIVELY: a new wire member that is not
//     mapped is a compile error here.
//   * OPEN-ENUM FAIL-CLOSED. protobuf-es decodes a number the schema does not
//     name as that number; it is not in the table and maps to `unspecified`.
//     NOTHING but the wire's `ACTIVE` maps to `active` — the security property
//     (an unknown or legacy state never renders media).
//
// A wire value the table does not name — an unknown number from a newer or
// older peer, e.g. a reserved number such as story 2's deleted SOURCE_MUTED —
// maps to `unspecified`, never `active`.

import { MediaKind, SlotState } from '../proto/dark_tower/signaling/v1/signaling_pb.js';

import type { SlotStateToken } from './events.js';
import type { MediaKindToken, OpenMediaKindToken } from '../media/mediaKind.js';

/**
 * Wire -> token table. An exhaustive `Record<SlotState, …>`: adding a wire
 * member without mapping it fails to compile (`__tests__/slotStateMap.test.ts`
 * also pins "every member is mapped").
 */
const SLOT_STATE_TOKENS: Record<SlotState, SlotStateToken> = {
  [SlotState.UNSPECIFIED]: 'unspecified',
  [SlotState.ACTIVE]: 'active',
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
 * Map a wire slot state onto its token. Any number the table does not name is
 * `unspecified`, never `active` (fail closed).
 */
export function slotStateToken(state: SlotState): SlotStateToken {
  return SLOT_STATE_TOKENS[state] ?? 'unspecified';
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
