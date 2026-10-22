// File: packages/sdk-core/src/signaling/legacySourceMutedShim.ts
//
// TRANSITIONAL (C-A1 shim b) — deleted by C-A2 (story 3 task 13), with its two
// call sites in `SignalingClient.ts`.
//
// WHY IT EXISTS. The flat slot vocabulary (story 3 wire freeze) has no
// `source_muted`: a muted source is `avatar_client_muted` or
// `avatar_server_muted`, server winning. The controller in service before MC-A
// (task 12) still emits the legacy `SLOT_STATE_SOURCE_MUTED` for both causes. So
// that the UI and tests see ONLY the flat vocabulary across that window, this
// resolves the legacy value from the latest `ParticipantMuteUpdate` for the
// slot's sender:
//
//   * that sender server-muted for the SLOT'S kind  -> `avatar_server_muted`;
//   * otherwise — including no update yet, an unknown sender, an absent sender
//     id, or an unknown slot kind                    -> `avatar_client_muted`.
//
// It NEVER yields `active` (fail-closed: a legacy or unknown state never renders
// media). `avatar_client_muted` is the fail-closed DEFAULT, not a claim about
// the cause; a server mute, once its update is seen, always wins.
//
// LATE UPDATES. MC may send the assignments before the mute update. The holder
// keeps the latest WIRE assignments, so `SignalingClient` re-derives and
// re-emits them when a mute update arrives for a sender holding a legacy slot —
// otherwise a server-muted slot would stay on the client-muted default.

import { SlotState } from '../proto/dark_tower/signaling/v1/signaling_pb.js';

import type { SlotMediaKind, SlotStateToken } from './events.js';

/** Per-kind server-mute flags for one sender, from its latest update. */
interface ServerMuteFlags {
  readonly audio: boolean;
  readonly video: boolean;
}

/** The fields of a wire assignment this shim reads. */
export interface LegacyAssignmentInput {
  readonly slotState: SlotState;
  readonly senderId: number | undefined;
  readonly mediaKind: SlotMediaKind;
}

/** Resolve a legacy `SOURCE_MUTED` slot. Pure. Never `active`. */
export function resolveLegacySourceMuted(
  assignment: LegacyAssignmentInput,
  serverMuted: ServerMuteFlags | undefined,
): SlotStateToken {
  if (assignment.senderId === undefined || serverMuted === undefined) return 'avatar_client_muted';
  switch (assignment.mediaKind) {
    case 'audio':
      return serverMuted.audio ? 'avatar_server_muted' : 'avatar_client_muted';
    case 'video':
      return serverMuted.video ? 'avatar_server_muted' : 'avatar_client_muted';
    default:
      return 'avatar_client_muted';
  }
}

/**
 * The shim's state: latest per-sender server-mute flags. Keyed by SENDER id
 * because slots name senders; the participant -> sender lookup is the roster's
 * own index (`RosterKeyFeed.senderIdOf`), passed in, never a second map.
 */
export class LegacySourceMutedShim {
  readonly #bySender = new Map<number, ServerMuteFlags>();

  /**
   * Record a `ParticipantMuteUpdate`. Returns whether a re-derive could change
   * anything: true when the sender is known.
   */
  recordMuteUpdate(senderId: number | undefined, audio: boolean, video: boolean): boolean {
    if (senderId === undefined) return false;
    this.#bySender.set(senderId, { audio, video });
    return true;
  }

  /** A sender left the roster: forget its flags (bounded by the roster). */
  forget(senderId: number | undefined): void {
    if (senderId !== undefined) this.#bySender.delete(senderId);
  }

  /** Resolve one legacy slot against the recorded flags. */
  resolve(assignment: LegacyAssignmentInput): SlotStateToken {
    const flags =
      assignment.senderId === undefined ? undefined : this.#bySender.get(assignment.senderId);
    return resolveLegacySourceMuted(assignment, flags);
  }

  /** Whether any assignment in `wire` is a legacy slot held by `senderId`. */
  static holdsLegacySlot(wire: readonly LegacyAssignmentInput[], senderId: number): boolean {
    return wire.some((a) => a.slotState === SlotState.SOURCE_MUTED && a.senderId === senderId);
  }
}
